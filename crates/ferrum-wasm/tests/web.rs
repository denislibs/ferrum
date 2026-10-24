//! Код, которому нужен wasm32: часы `performance.now()`, разбор команд из
//! JS-объектов и события в JS, ядро кадра без `std::time::Instant` (он в wasm32
//! паникует). Запуск в Node:
//! `cargo test --target wasm32-unknown-unknown -p ferrum-wasm --test web`.
#![cfg(target_arch = "wasm32")]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_render::{Clock, CpuBackend};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::{Backend, Command, CoreEvent};
use ferrum_wasm::web::{events_to_js, parse_command, PerformanceClock};
use js_sys::{Object, Reflect, Uint8Array};
use kurbo::{Point, Size};
use serde_bytes::ByteBuf;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

fn obj(pairs: &[(&str, JsValue)]) -> JsValue {
    let o = Object::new();
    for (k, v) in pairs {
        Reflect::set(&o, &(*k).into(), v).unwrap();
    }
    o.into()
}

#[wasm_bindgen_test]
fn performance_clock_runs() {
    let c = PerformanceClock::new().expect("в Node есть performance");
    let a = c.now_ms();
    let mut x = 0u64;
    for i in 0..2_000_000u64 {
        x = x.wrapping_add(i * i);
    }
    let b = c.now_ms();
    assert!(x > 0 && b >= a && b - a < 10_000.0, "{a} → {b}");
}

#[wasm_bindgen_test]
fn commands_parse_from_js_objects() {
    let bytes = Uint8Array::from(&[1u8, 2, 3][..]);
    let open = obj(&[("type", "open".into()), ("bytes", bytes.into())]);
    assert_eq!(
        parse_command(open),
        Ok(Command::Open {
            bytes: ByteBuf::from(vec![1, 2, 3])
        })
    );
    let demo = obj(&[("type", "demo".into()), ("nodes", 100.into())]);
    assert_eq!(parse_command(demo), Ok(Command::Demo { nodes: 100 }));
    for bad in [
        obj(&[("type", "demo".into()), ("nodes", "много".into())]),
        obj(&[("type", "erase".into())]),
        JsValue::from_str("save"),
        JsValue::NULL,
    ] {
        let e = parse_command(bad).unwrap_err();
        assert!(e.starts_with("Неверная команда"), "{e}");
    }
}

#[wasm_bindgen_test]
fn events_become_js_objects_with_byte_arrays() {
    let js = events_to_js(&[
        CoreEvent::Clipboard {
            bytes: ByteBuf::from(vec![7, 8]),
        },
        CoreEvent::Document {
            can_undo: true,
            can_redo: false,
            steps: 2.0,
        },
    ]);
    let arr: js_sys::Array = js.dyn_into().unwrap();
    assert_eq!(arr.length(), 2);
    let clip = arr.get(0);
    assert_eq!(Reflect::get(&clip, &"type".into()).unwrap(), "clipboard");
    let bytes = Reflect::get(&clip, &"bytes".into()).unwrap();
    let bytes: Uint8Array = bytes.dyn_into().expect("байты — Uint8Array");
    assert_eq!(bytes.to_vec(), [7, 8]);
    let doc = arr.get(1);
    assert_eq!(Reflect::get(&doc, &"canUndo".into()).unwrap(), true);
}

#[wasm_bindgen_test]
fn core_frame_runs_in_wasm_without_instant() {
    let mut core = Core::new(Platform::Linux, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    core.renderer_mut()
        .set_clock(PerformanceClock::new().map(|c| Box::new(c) as _));
    core.command(Command::Demo { nodes: 2000 });
    let (w, h) = core.device_size();
    let mut b = CpuBackend::new(w, h);
    core.draw(&mut b);
    // Перетаскивание и отмена — те же пути, что в браузере.
    let p = |x: f64, y: f64, t: f64| PointerEvent {
        point: Point::new(x, y),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    };
    // Камера примера: центр (720, 512), зум 0.5, холст 400 × 300.
    let screen = |x: f64, y: f64| ((x - 720.0) * 0.5 + 200.0, (y - 512.0) * 0.5 + 150.0);
    // «Слой 25» первого фрейма: (700, 200) – (820, 280).
    let (sx, sy) = screen(760.0, 240.0);
    core.handle(Input::PointerDown(p(sx, sy, 0.0)));
    core.handle(Input::PointerMove(p(sx + 20.0, sy + 10.0, 5.0)));
    core.handle(Input::PointerUp(p(sx + 20.0, sy + 10.0, 9.0)));
    core.draw(&mut b);
    let live = core.live();
    assert_eq!(
        (live.steps, live.selection_x, live.selection_y),
        (1.0, 740.0, 220.0)
    );
    // Отдаление: растры фреймов с бюджетом по часам `performance.now()`.
    core.command(Command::Execute {
        command: "view.fit_all".into(),
    });
    for _ in 0..3 {
        core.draw(&mut b);
    }
    let pm = b.render();
    assert_eq!((pm.width(), pm.height()), (400, 300));
}

/// Команда с полем `bytes` произвольного JS-типа.
fn open_with(bytes: JsValue) -> JsValue {
    obj(&[("type", "open".into()), ("bytes", bytes)])
}

#[wasm_bindgen_test]
fn malformed_commands_never_trap() {
    // Форма не та: ошибка с русским текстом, не паника.
    let nodes = |v: JsValue| obj(&[("type", "demo".into()), ("nodes", v)]);
    let cases: Vec<(&str, JsValue)> = vec![
        ("nodes: 1e300", nodes(1e300.into())),
        ("nodes: -1", nodes((-1.0).into())),
        ("nodes: NaN", nodes(f64::NAN.into())),
        ("nodes: 1.5", nodes(1.5.into())),
        ("nodes: null", nodes(JsValue::NULL)),
        ("число", JsValue::from_f64(5.0)),
        ("undefined", JsValue::UNDEFINED),
        ("массив", js_sys::Array::new().into()),
        ("функция", js_sys::Function::new_no_args("return 1").into()),
        ("bytes: объект", open_with(Object::new().into())),
        ("bytes: число", open_with(7.into())),
        ("bytes: null", open_with(JsValue::NULL)),
        (
            "bytes: массив с мусором",
            open_with(js_sys::Array::of2(&1.into(), &"x".into()).into()),
        ),
        (
            "bytes: массив с числом вне u8",
            open_with(js_sys::Array::of2(&1.into(), &300.into()).into()),
        ),
    ];
    for (name, case) in cases {
        let e = parse_command(case).expect_err(name);
        assert!(e.starts_with("Неверная команда"), "{name}: {e}");
    }
}

#[wasm_bindgen_test]
fn demo_nodes_float_with_integer_value_is_accepted() {
    // `1.0` в JS — то же число, что `1`: целое по значению принимается.
    let demo = obj(&[("type", "demo".into()), ("nodes", 1.0.into())]);
    assert_eq!(parse_command(demo), Ok(Command::Demo { nodes: 1 }));
}

#[wasm_bindgen_test]
fn bytes_accepted_as_uint8array_and_as_number_array() {
    let want = Ok(Command::Open {
        bytes: ByteBuf::from(vec![1, 2, 3]),
    });
    let u8s = Uint8Array::from(&[1u8, 2, 3][..]);
    assert_eq!(parse_command(open_with(u8s.into())), want);
    let arr = js_sys::Array::of3(&1.into(), &2.into(), &3.into());
    assert_eq!(parse_command(open_with(arr.into())), want);
}

#[wasm_bindgen_test]
fn bytes_as_string_is_taken_as_utf8() {
    // Строка вместо байтов: serde_bytes принимает её как UTF-8 (не ошибка). Дальше
    // `open` отвергнет не-файл событием `error`, но разбор не падает.
    let r = parse_command(open_with("abc".into()));
    assert_eq!(
        r,
        Ok(Command::Open {
            bytes: ByteBuf::from(b"abc".to_vec())
        })
    );
    let mut core = Core::new(Platform::Linux, Size::new(100.0, 100.0), 1.0, Backend::Cpu);
    core.command(r.unwrap());
    assert!(core
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
}

#[wasm_bindgen_test]
fn rejected_command_is_clipped_in_error_event() {
    let mut core = Core::new(Platform::Linux, Size::new(100.0, 100.0), 1.0, Backend::Cpu);
    core.command(Command::Execute {
        command: "ы".repeat(500),
    });
    let ev = core.take_events();
    let Some(CoreEvent::Error { message }) = ev.last() else {
        panic!("{ev:?}")
    };
    assert!(message.chars().count() < 120, "{}", message.chars().count());
}
