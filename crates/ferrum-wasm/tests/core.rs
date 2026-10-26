//! Ядро worker'а без браузера: ввод из кольцевого буфера доходит до редактора,
//! кадр рисуется только когда нужен, команды и события протокола, живое
//! состояние. Рендер — `vello_cpu`, как запасной бэкенд в браузере.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_render::{CpuBackend, Pixmap};
use ferrum_wasm::core::{Core, CANVAS_DARK};
use ferrum_wasm::input::{encode, Record};
use ferrum_wasm::protocol::{Backend, Command, CoreEvent, Tool};
use kurbo::{Point, Size};
use serde_bytes::ByteBuf;

const VIEW: Size = Size::new(800.0, 600.0);

/// Ядро со страницей-примером: камера на первом фрейме, зум 0.5.
fn demo(nodes: u32) -> Core {
    let mut core = Core::new(Platform::Mac, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes });
    core
}

/// Экранная точка мира при камере примера (центр (720, 512), зум 0.5).
fn screen(x: f64, y: f64) -> Point {
    Point::new((x - 720.0) * 0.5 + 400.0, (y - 512.0) * 0.5 + 300.0)
}

fn frame(core: &mut Core) -> Pixmap {
    let (w, h) = core.device_size();
    let mut b = CpuBackend::new(w, h);
    core.draw(&mut b);
    b.render()
}

fn px(p: &Pixmap, at: Point) -> [u8; 4] {
    let i = (at.y as usize * p.width() as usize + at.x as usize) * 4;
    p.data_as_u8_slice()[i..i + 4].try_into().unwrap()
}

fn near(a: [u8; 4], b: [u8; 3]) -> bool {
    (0..3).all(|i| a[i].abs_diff(b[i]) <= 3)
}

fn pointer(at: Point, t: f64) -> PointerEvent {
    PointerEvent {
        point: at,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    }
}

/// Перетаскивание записями кольцевого буфера, как их пишет основной поток.
fn drag_records(from: Point, by: (f64, f64), t: f64) -> Vec<u8> {
    let mut recs = vec![Record::Input(Input::PointerDown(pointer(from, t)))];
    for i in 1..=10 {
        let k = i as f64 / 10.0;
        let p = Point::new(from.x + by.0 * k, from.y + by.1 * k);
        recs.push(Record::Input(Input::PointerMove(pointer(p, t + i as f64))));
    }
    let to = Point::new(from.x + by.0, from.y + by.1);
    recs.push(Record::Input(Input::PointerUp(pointer(to, t + 20.0))));
    recs.iter().flat_map(encode).collect()
}

#[test]
fn demo_page_is_drawn_once_and_shows_the_document() {
    let mut core = demo(1000);
    let ev = core.take_events();
    assert!(
        ev.contains(&CoreEvent::Opened {
            pages: 1,
            nodes: 1000,
            warnings: vec![],
            name: "Без названия".into(),
        }),
        "{ev:?}"
    );
    // История примера пуста: отмена не стирает страницу.
    assert!(ev.contains(&CoreEvent::Document {
        can_undo: false,
        can_redo: false,
        steps: 0.0
    }));
    assert!(core.wants_frame());
    let img = frame(&mut core);
    assert_eq!((img.width(), img.height()), (800, 600));
    // Прямоугольник «Слой 1» первого фрейма: (140, 0) – (260, 80), заливка (1, 120, 200).
    assert!(near(px(&img, screen(200.0, 40.0)), [1, 120, 200]));
    // Вне фреймов — фон холста.
    let bg = CANVAS_DARK.to_rgba8();
    assert!(near(px(&img, Point::new(5.0, 5.0)), [bg.r, bg.g, bg.b]));
    // Ничего не изменилось — кадр не нужен.
    assert!(!core.wants_frame());
    let live = core.live();
    assert_eq!((live.frames, live.nodes, live.zoom), (1.0, 1000.0, 0.5));
    assert_eq!(
        (live.device_w, live.device_h, live.backend),
        (800.0, 600.0, 3.0)
    );
}

#[test]
fn drag_from_ring_records_moves_the_node_and_undo_returns_it() {
    let mut core = demo(1000);
    frame(&mut core);
    core.take_events();
    let n = core.apply_input(&drag_records(screen(200.0, 40.0), (50.0, 30.0), 1000.0));
    assert_eq!(n, 12);
    let ev = core.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::Selection { count: 1, .. })),
        "{ev:?}"
    );
    assert!(ev.contains(&CoreEvent::Document {
        can_undo: true,
        can_redo: false,
        steps: 1.0
    }));
    // Один шаг отмены — одно событие документа, хоть изменений и много.
    assert_eq!(
        ev.iter()
            .filter(|e| matches!(e, CoreEvent::Document { .. }))
            .count(),
        1
    );
    assert!(core.wants_frame());
    let img = frame(&mut core);
    let live = core.live();
    // Сдвиг на (50, 30) CSS-пикселей при зуме 0.5 — (100, 60) в мире.
    assert_eq!(
        (live.selection_count, live.selection_x, live.selection_y),
        (1.0, 240.0, 60.0)
    );
    assert_eq!((live.selection_w, live.selection_h), (120.0, 80.0));
    assert_eq!((live.steps, live.can_undo, live.inputs), (1.0, 1.0, 12.0));
    // На новом месте — прямоугольник.
    assert!(near(px(&img, screen(300.0, 100.0)), [1, 120, 200]));

    core.command(Command::Execute {
        command: "edit.undo".into(),
    });
    frame(&mut core);
    let live = core.live();
    assert_eq!((live.selection_x, live.selection_y), (140.0, 0.0));
    assert_eq!((live.can_undo, live.can_redo), (0.0, 1.0));
}

#[test]
fn resize_record_changes_device_size_and_requests_a_frame() {
    let mut core = demo(100);
    frame(&mut core);
    let rec = encode(&Record::Resize {
        width: 640.0,
        height: 480.0,
        dpr: 2.0,
    });
    core.apply_input(&rec);
    assert!(core.wants_frame());
    assert_eq!(core.device_size(), (1280, 960));
    let img = frame(&mut core);
    assert_eq!((img.width(), img.height()), (1280, 960));
    assert_eq!(core.live().dpr, 2.0);
}

#[test]
fn garbage_input_is_counted_and_ignored() {
    let mut core = demo(100);
    frame(&mut core);
    let mut bytes = vec![0xffu8; 64 * 3];
    bytes.extend_from_slice(&encode(&Record::Input(Input::Blur)));
    bytes.extend_from_slice(&[1, 2]);
    assert_eq!(core.apply_input(&bytes), 1);
    let live = core.live();
    assert_eq!((live.inputs, live.skipped), (1.0, 4.0));
    // Нечисловые координаты отбрасывает редактор: документ цел.
    core.handle(Input::PointerDown(pointer(Point::new(f64::NAN, 1.0), 0.0)));
    assert_eq!(core.live().nodes, 100.0);
}

#[test]
fn bad_commands_report_errors_and_keep_the_document() {
    let mut core = demo(100);
    core.take_events();
    core.command(Command::Execute {
        command: "edit.format_disk".into(),
    });
    core.command(Command::Open {
        bytes: ByteBuf::from(b"not a ferrum file".to_vec()),
    });
    core.command(Command::Demo { nodes: 10_000_000 });
    core.command(Command::Paste {
        bytes: ByteBuf::from(vec![1, 2, 3]),
    });
    core.command(Command::PlaceImage {
        bytes: ByteBuf::from(vec![0; 10]),
    });
    let errors: Vec<CoreEvent> = core
        .take_events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::Error { .. }))
        .collect();
    assert_eq!(errors.len(), 5, "{errors:?}");
    assert_eq!(core.live().nodes, 100.0);
}

#[test]
fn save_and_open_round_trip() {
    let mut core = demo(300);
    core.command(Command::Save);
    let Some(CoreEvent::Saved { bytes }) = core
        .take_events()
        .into_iter()
        .find(|e| matches!(e, CoreEvent::Saved { .. }))
    else {
        panic!("нет события Saved");
    };
    let mut other = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    other.command(Command::Open { bytes });
    assert!(other.take_events().contains(&CoreEvent::Opened {
        pages: 1,
        nodes: 300,
        warnings: vec![],
        name: "Без названия".into(),
    }));
}

#[test]
fn copy_as_png_exports_the_selection() {
    let mut core = demo(100);
    frame(&mut core);
    // Клик по «Слою 1» выделяет его.
    let at = screen(200.0, 40.0);
    core.handle(Input::PointerDown(pointer(at, 0.0)));
    core.handle(Input::PointerUp(pointer(at, 10.0)));
    core.take_events();
    core.command(Command::Execute {
        command: "edit.copy_png".into(),
    });
    let png = core.take_events().into_iter().find_map(|e| match e {
        CoreEvent::ClipboardPng { bytes } => Some(bytes),
        _ => None,
    });
    let png = png.expect("нет PNG");
    let img = ferrum_render::decode_png(&png).expect("PNG не читается");
    assert_eq!((img.width(), img.height()), (120, 80));
}

#[test]
fn placed_image_is_drawn() {
    let mut core = demo(100);
    frame(&mut core);
    let mut src = Pixmap::new(40, 20);
    for p in src.data_as_u8_slice_mut().chunks_exact_mut(4) {
        p.copy_from_slice(&[255, 0, 0, 255]);
    }
    let png = src.into_png().unwrap();
    core.command(Command::PlaceImage {
        bytes: ByteBuf::from(png),
    });
    // Клик в пустом месте ставит картинку в исходном размере.
    let at = screen(720.0, 1100.0);
    core.handle(Input::PointerDown(pointer(at, 0.0)));
    core.handle(Input::PointerUp(pointer(at, 10.0)));
    let ev = core.take_events();
    assert!(ev.contains(&CoreEvent::Tool { tool: Tool::Move }), "{ev:?}");
    let img = frame(&mut core);
    let live = core.live();
    assert_eq!((live.selection_w, live.selection_h), (40.0, 20.0));
    let centre = screen(
        live.selection_x + live.selection_w / 2.0,
        live.selection_y + live.selection_h / 2.0,
    );
    assert!(
        near(px(&img, centre), [255, 0, 0]),
        "{:?}",
        px(&img, centre)
    );
    assert!(!core.wants_frame(), "картинка уже декодирована");
}

#[test]
fn zoomed_out_page_keeps_drawing_until_rasters_are_warm() {
    let mut core = demo(20_000);
    // Счётный предел растров за кадр: число кадров не зависит от скорости машины.
    core.renderer_mut().set_raster_build_time(None);
    frame(&mut core);
    core.command(Command::Execute {
        command: "view.fit_all".into(),
    });
    let mut frames = 0;
    let mut builds = 0;
    while core.wants_frame() {
        let (w, h) = core.device_size();
        builds += core.draw(&mut CpuBackend::new(w, h)).raster_builds;
        frames += 1;
        assert!(frames < 100, "кадры не кончаются");
    }
    // Растры строятся не больше 32 за кадр: прогрев занимает несколько кадров,
    // после него кадр больше не нужен.
    assert!(
        frames >= 2 && builds > 32,
        "кадров {frames}, растров {builds}"
    );
}

#[test]
fn frame_statistics() {
    let mut core = demo(100);
    for i in 0..10 {
        core.frame_done(i as f64 * 200.0, i as f64);
    }
    let live = core.live();
    assert_eq!(live.frame_ms, 9.0);
    assert_eq!((live.frame_p50, live.frame_p95), (5.0, 9.0));
    // За последнюю секунду (800..1800 мс) — 6 кадров.
    assert_eq!(live.fps, 6.0);
}

#[test]
fn oversized_demo_is_rejected_without_building_anything() {
    let mut core = demo(100);
    frame(&mut core);
    core.take_events();
    for nodes in [u32::MAX, ferrum_wasm::protocol::MAX_DEMO_NODES + 1] {
        core.command(Command::Demo { nodes });
        // Отказ до генерации: страница не заменяется (число узлов то же).
        let ev = core.take_events();
        assert_eq!(ev.len(), 1, "{ev:?}");
        assert!(matches!(ev[0], CoreEvent::Error { .. }), "{ev:?}");
        assert_eq!(core.live().nodes, 100.0);
        assert!(!core.wants_frame(), "документ не менялся — кадр не нужен");
    }
}

/// Щелчок записями кольцевого буфера.
fn click_records(at: Point, t: f64) -> Vec<Record> {
    vec![
        Record::Input(Input::PointerDown(pointer(at, t))),
        Record::Input(Input::PointerUp(pointer(at, t + 5.0))),
    ]
}

/// Идентификаторы единственного события выделения после пачки `recs`.
fn selection_after(core: &mut Core, recs: &[Record]) -> Vec<CoreEvent> {
    let bytes: Vec<u8> = recs.iter().flat_map(encode).collect();
    core.apply_input(&bytes);
    core.take_events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::Selection { .. }))
        .collect()
}

fn selected_ids(ev: &[CoreEvent]) -> Vec<String> {
    match ev {
        [CoreEvent::Selection { ids, .. }] => ids.clone(),
        _ => panic!("ожидалось одно событие выделения: {ev:?}"),
    }
}

#[test]
fn selection_changes_in_one_batch_give_one_selection_event() {
    let mut core = demo(1000);
    frame(&mut core);
    core.take_events();
    // «Слой 1» — (140, 0)–(260, 80), «Слой 2» — (280, 0)–(400, 80).
    let (one, two) = (screen(200.0, 40.0), screen(340.0, 40.0));
    let id_two = selected_ids(&selection_after(&mut core, &click_records(two, 100.0)));
    let id_one = selected_ids(&selection_after(&mut core, &click_records(one, 200.0)));
    assert_ne!(id_one, id_two);
    // Одной пачкой: снять выделение, «Слой 1», снять, «Слой 1», «Слой 2». Первое
    // изменение — пустое выделение, последнее — «Слой 2».
    let mut recs = click_records(Point::new(5.0, 5.0), 1000.0);
    recs.extend(click_records(one, 1100.0));
    recs.extend(click_records(Point::new(5.0, 5.0), 1200.0));
    recs.extend(click_records(one, 1300.0));
    recs.extend(click_records(two, 1400.0));
    let sel = selection_after(&mut core, &recs);
    assert_eq!(
        sel.len(),
        1,
        "в пачке важно только последнее выделение: {sel:?}"
    );
    assert_eq!(
        selected_ids(&sel),
        id_two,
        "осталось последнее выделение, а не первое"
    );
}

#[test]
fn selection_after_open_replaces_the_one_from_opened_in_the_same_batch() {
    // `Opened` даёт выделение 0; щелчок до отправки событий — то же окно событий.
    let mut core = Core::new(Platform::Mac, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes: 1000 });
    frame(&mut core);
    core.apply_input(
        &click_records(screen(200.0, 40.0), 1000.0)
            .iter()
            .flat_map(encode)
            .collect::<Vec<u8>>(),
    );
    let sel: Vec<_> = core
        .take_events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::Selection { .. }))
        .collect();
    assert_eq!(sel.len(), 1, "{sel:?}");
    assert!(
        matches!(sel[0], CoreEvent::Selection { count: 1, .. }),
        "{sel:?}"
    );
}

// Страница в 110 тыс. узлов в отладочной сборке строится десятки секунд; предел
// ID проверяет и быстрый модульный тест `selection_event` в core.rs.
#[test]
#[cfg_attr(debug_assertions, ignore)]
fn selection_event_ids_are_capped_but_count_is_exact() {
    // select_all берёт фреймы верхнего уровня: их больше предела при ~110 тыс. узлов.
    let mut core = demo(110_000);
    core.take_events();
    core.command(Command::Execute {
        command: "edit.select_all".into(),
    });
    let ev = core.take_events();
    let Some(CoreEvent::Selection { count, ids, .. }) = ev
        .into_iter()
        .find(|e| matches!(e, CoreEvent::Selection { .. }))
    else {
        panic!("нет события Selection");
    };
    assert!(count > ferrum_wasm::protocol::SELECTION_IDS_MAX as u32);
    assert_eq!(ids.len(), ferrum_wasm::protocol::SELECTION_IDS_MAX);
}

#[test]
fn copy_png_of_undrawn_images_has_their_pixels() {
    let mut core = demo(100);
    frame(&mut core);
    for (i, rgba) in [[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]]
        .into_iter()
        .enumerate()
    {
        let mut src = Pixmap::new(40, 20);
        for p in src.data_as_u8_slice_mut().chunks_exact_mut(4) {
            p.copy_from_slice(&rgba);
        }
        core.command(Command::PlaceImage {
            bytes: ByteBuf::from(src.into_png().unwrap()),
        });
        let at = screen(300.0 + 200.0 * i as f64, 1100.0);
        core.handle(Input::PointerDown(pointer(at, 0.0)));
        core.handle(Input::PointerUp(pointer(at, 10.0)));
    }
    core.command(Command::Save);
    let Some(CoreEvent::Saved { bytes }) = core
        .take_events()
        .into_iter()
        .find(|e| matches!(e, CoreEvent::Saved { .. }))
    else {
        panic!("нет события Saved");
    };
    // Свежее ядро: кэш картинок пуст, ни одного кадра не нарисовано.
    let mut other = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    other.command(Command::Open { bytes });
    other.command(Command::Execute {
        command: "edit.select_all".into(),
    });
    other.take_events();
    other.command(Command::Execute {
        command: "edit.copy_png".into(),
    });
    let ev = other.take_events();
    let png = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::ClipboardPng { bytes } => Some(bytes.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("нет PNG: {ev:?}"));
    let img = ferrum_render::decode_png(&png).unwrap();
    let has = |c: [u8; 3]| {
        img.data_as_u8_slice()
            .chunks_exact(4)
            .any(|p| near([p[0], p[1], p[2], p[3]], c))
    };
    // Три картинки: больше счётного бюджета кадра (2), экспорт его не знает.
    assert!(
        has([255, 0, 0]) && has([0, 0, 255]) && has([0, 255, 0]),
        "нет цветов картинок"
    );
}

#[test]
fn unknown_command_error_is_truncated() {
    let mut core = demo(100);
    core.take_events();
    core.command(Command::Execute {
        command: "x".repeat(100_000),
    });
    let ev = core.take_events();
    let [CoreEvent::Error { message }] = ev.as_slice() else {
        panic!("{ev:?}")
    };
    assert!(message.chars().count() < 120, "{}", message.len());
}

#[test]
fn failed_open_keeps_undo_and_redo_of_the_old_document() {
    let mut core = demo(1000);
    frame(&mut core);
    core.apply_input(&drag_records(screen(200.0, 40.0), (50.0, 30.0), 1000.0));
    frame(&mut core);
    core.command(Command::Open {
        bytes: ByteBuf::from(b"not a ferrum file".to_vec()),
    });
    assert!(matches!(
        core.take_events().last(),
        Some(CoreEvent::Error { .. })
    ));
    assert_eq!(core.live().can_undo, 1.0);
    core.command(Command::Execute {
        command: "edit.undo".into(),
    });
    frame(&mut core);
    assert_eq!((core.live().can_undo, core.live().can_redo), (0.0, 1.0));
    core.command(Command::Execute {
        command: "edit.redo".into(),
    });
    frame(&mut core);
    assert_eq!(core.live().can_undo, 1.0);
    assert_eq!(core.live().nodes, 1000.0);
}

#[test]
fn reference_frame_matches_screen_frame_and_leaves_state_alone() {
    let mut core = demo(300);
    let screen_px = frame(&mut core);
    let frames_before = core.live().frames;
    let reference = core.draw_reference();
    assert_eq!(
        (reference.width(), reference.height()),
        (screen_px.width(), screen_px.height())
    );
    assert_eq!(
        reference.data_as_u8_slice(),
        screen_px.data_as_u8_slice(),
        "эталон — тот же кадр vello_cpu"
    );
    assert_eq!(core.live().frames, frames_before, "эталон кадры не считает");
}

#[test]
fn request_frame_and_push_event_reach_the_loop() {
    let mut core = demo(10);
    frame(&mut core);
    // Кадр дорисован: до запроса новый не нужен.
    while core.wants_frame() {
        frame(&mut core);
    }
    core.request_frame();
    assert!(core.wants_frame());
    core.push_event(CoreEvent::Error {
        message: "x".into(),
    });
    assert!(core
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { message } if message == "x")));
}

mod present {
    use ferrum_wasm::present::{
        Action, PresentError, PresentWatch, Soft, SoftErrors, MAX_SKIPS, MAX_SOFT_ERRORS,
    };

    fn skip() -> PresentError {
        PresentError::Skipped("таймаут".into())
    }

    #[test]
    fn transient_skips_redraw_until_the_limit_then_pause_once() {
        let mut w = PresentWatch::default();
        for _ in 1..MAX_SKIPS {
            assert_eq!(w.error(skip()), Action::Redraw);
        }
        let Action::Pause(Some(m)) = w.error(skip()) else {
            panic!("после предела — сообщение и пауза")
        };
        assert!(m.contains("таймаут"), "{m}");
        // То же сообщение повторно не отдаётся, кадров не просим.
        let Action::Pause(None) = w.error(skip()) else {
            panic!()
        };
    }

    #[test]
    fn success_resets_the_skip_counter() {
        let mut w = PresentWatch::default();
        for _ in 1..MAX_SKIPS {
            w.error(skip());
        }
        w.ok();
        assert_eq!(w.error(skip()), Action::Redraw);
    }

    #[test]
    fn persistent_failure_is_reported_once_and_kills_the_surface() {
        let mut w = PresentWatch::default();
        let f = || PresentError::Failed("шейдер".into());
        let Action::Dead(Some(m)) = w.error(f()) else {
            panic!()
        };
        assert!(m.starts_with("Не удалось показать кадр"), "{m}");
        assert_eq!(w.error(f()), Action::Dead(None));
        let Action::Dead(Some(_)) = w.error(PresentError::Failed("другое".into())) else {
            panic!("другое сообщение — новое событие")
        };
    }

    #[test]
    fn too_large_frame_pauses_with_one_message_and_recovers() {
        let mut w = PresentWatch::default();
        let big =
            |w: u32| PresentError::TooLarge(format!("{w} × 720 больше предела устройства (16384)"));
        let Action::Pause(Some(m)) = w.error(big(16385)) else {
            panic!("слишком большой кадр — пауза, не смерть поверхности")
        };
        assert!(m.starts_with("Кадр не показан: 16385 × 720"), "{m}");
        // Окно растёт дальше: размеры другие, но сообщение одно.
        assert_eq!(w.error(big(16400)), Action::Pause(None));
        assert_eq!(w.error(big(16500)), Action::Pause(None));
        // Размер снова влез — кадр показан; следующий выход за предел — новое сообщение.
        w.ok();
        assert!(matches!(w.error(big(16385)), Action::Pause(Some(_))));
    }

    #[test]
    fn soft_errors_are_reported_once_per_message() {
        let mut s = SoftErrors::default();
        let e = || "Ошибка WebGPU: текстура".to_string();
        assert_eq!(s.error(e()), Soft::Report(e()));
        for _ in 0..5 {
            assert_eq!(
                s.error(e()),
                Soft::Quiet,
                "тот же текст на следующих кадрах — тихо"
            );
        }
        let other = "Ошибка WebGPU: буфер".to_string();
        assert_eq!(s.error(other.clone()), Soft::Report(other));
    }

    #[test]
    fn too_many_soft_errors_kill_the_surface_with_one_message() {
        let mut s = SoftErrors::default();
        for i in 0..MAX_SOFT_ERRORS {
            let a = s.error(format!("Ошибка WebGPU: {}", i % 3));
            assert!(!matches!(a, Soft::Dead(_)), "{i}: {a:?}");
        }
        let Soft::Dead(m) = s.error("Ошибка WebGPU: 0".into()) else {
            panic!("после {MAX_SOFT_ERRORS} ошибок поверхность останавливается")
        };
        assert!(
            m.contains(&MAX_SOFT_ERRORS.to_string()) && m.contains("Ошибка WebGPU: 0"),
            "{m}"
        );
        // Дальше — тишина: сообщение об остановке одно.
        assert_eq!(s.error("Ошибка WebGPU: новая".into()), Soft::Quiet);
    }
}

#[test]
fn read_measurements_reports_alt_measurements() {
    use ferrum_editor::KeyEvent;
    let mut core = demo(1000);
    let read = |core: &mut Core| {
        core.take_events();
        core.command(Command::ReadMeasurements);
        core.take_events()
            .into_iter()
            .find_map(|e| match e {
                CoreEvent::Measurements { values, guides } => Some((values, guides)),
                _ => None,
            })
            .expect("событие measurements")
    };
    // «Слой 0» (0, 0, 120 × 80) выделен, Alt, наведение на «Слой 1» (140, 0): зазор 20.
    let a = screen(60.0, 40.0);
    core.handle(Input::PointerDown(pointer(a, 0.0)));
    core.handle(Input::PointerUp(pointer(a, 10.0)));
    assert_eq!(read(&mut core), (vec![], 0), "без Alt");
    core.handle(Input::KeyDown(KeyEvent::new("AltLeft", Modifiers::ALT)));
    let mut e = pointer(screen(200.0, 40.0), 20.0);
    e.modifiers = Modifiers::ALT;
    core.handle(Input::PointerMove(e));
    assert_eq!(read(&mut core), (vec![20.0], 0));
    core.handle(Input::KeyUp(KeyEvent::new("AltLeft", Modifiers::NONE)));
    assert_eq!(read(&mut core), (vec![], 0), "Alt отпущен");
}
