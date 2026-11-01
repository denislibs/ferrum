//! Булевы группы в ядре worker'а (подсистема 3, этап 4): команды раскладки
//! через протокол, строка слоя с операцией, сводка выделения с операцией и
//! смена операции выделенной группы.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_wasm::core::Core;
use ferrum_wasm::layers::NodeType;
use ferrum_wasm::protocol::*;
use ferrum_wasm::summary::BooleanOpDto;
use kurbo::{Shape, Size};

fn core() -> Core {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::Demo { nodes: 0 });
    c.take_events();
    c
}

fn drag(c: &mut Core, from: (f64, f64), to: (f64, f64), t: f64) {
    let ev = |p: (f64, f64), dt: f64| PointerEvent {
        point: p.into(),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t + dt,
    };
    c.handle(Input::PointerDown(ev(from, 0.0)));
    c.handle(Input::PointerMove(ev(to, 10.0)));
    c.handle(Input::PointerUp(ev(to, 20.0)));
}

fn summary(ev: &[CoreEvent]) -> Option<Summary> {
    ev.iter().rev().find_map(|e| match e {
        CoreEvent::Properties { summary } => Some(summary.clone()),
        _ => None,
    })
}

fn run(c: &mut Core, command: &str) -> Vec<CoreEvent> {
    c.command(Command::Execute {
        command: command.into(),
    });
    c.take_events()
}

/// Два прямоугольника инструментом R, оба выделены. Зум демо — 0,5: в мире
/// это 200 × 200 с перекрытием 100.
fn two_rects(c: &mut Core) {
    let before: Vec<String> = c
        .editor()
        .scene()
        .iter()
        .map(|(_, n)| n.id.to_string())
        .collect();
    for (i, x) in [100.0, 150.0].into_iter().enumerate() {
        c.command(Command::SetTool { tool: Tool::Rect });
        drag(c, (x, 100.0), (x + 100.0, 200.0), 1000.0 * (i + 1) as f64);
    }
    // Только созданные здесь узлы, а не всё, что есть в сцене.
    let ids: Vec<String> = c
        .editor()
        .scene()
        .iter()
        .map(|(_, n)| n.id.to_string())
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(ids.len(), 2);
    c.command(Command::Select {
        ids,
        mode: SelectMode::Replace,
    });
    c.take_events();
}

#[test]
fn boolean_commands_reach_the_editor_and_the_summary_shows_the_op() {
    let mut c = core();
    two_rects(&mut c);
    let ev = run(&mut c, "object.boolean_subtract");
    let s = summary(&ev).expect("сводка");
    assert_eq!(s.kind, Some(NodeType::Boolean));
    assert_eq!(s.boolean_op, Some(BooleanOpDto::Subtract));
    assert_eq!(s.name.as_deref(), Some("Вычитание"));
    assert_eq!(s.w.value, 100.0, "рамка — по результату");
    // Та же команда на выделенной группе меняет операцию.
    let ev = run(&mut c, "object.boolean_union");
    let s = summary(&ev).expect("сводка");
    assert_eq!(s.boolean_op, Some(BooleanOpDto::Union));
    assert_eq!(s.w.value, 300.0);
    // Строка слоя несёт операцию для иконки.
    c.command(Command::LayersWindow {
        start: 0,
        count: 10,
    });
    let ev = c.take_events();
    let rows = ev
        .iter()
        .rev()
        .find_map(|e| match e {
            CoreEvent::Layers { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .expect("строки слоёв");
    let row = rows.iter().find(|r| r.kind == NodeType::Boolean).unwrap();
    assert_eq!(row.boolean_op, Some(BooleanOpDto::Union));
    assert!(rows
        .iter()
        .filter(|r| r.kind == NodeType::Rect)
        .all(|r| r.boolean_op.is_none()));
}

#[test]
fn boolean_and_outline_commands_are_in_the_keymap() {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    let ev = c.take_events();
    let commands = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Keymap { commands, .. } => Some(commands.clone()),
            _ => None,
        })
        .expect("раскладка");
    assert_eq!(commands.len(), 54);
    for (key, binding) in [
        ("object.boolean_union", "Alt+Shift+KeyU"),
        ("object.boolean_subtract", "Alt+Shift+KeyS"),
        ("object.boolean_intersect", "Alt+Shift+KeyI"),
        ("object.boolean_exclude", "Alt+Shift+KeyE"),
        ("object.outline_stroke", "Ctrl+Alt+KeyO"),
    ] {
        let e = commands.iter().find(|e| e.key == key).expect(key);
        assert_eq!(e.bindings, vec![binding.to_string()], "{key}");
    }
}

fn font_file(rel: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const INTER: [&str; 2] = [
    "inter/Inter-Variable.ttf",
    "inter/Inter-Italic-Variable.ttf",
];

/// Ядро с каталогом вшитого Inter, но без байтов шрифта.
fn core_with_inter_catalog() -> Core {
    let mut c = core();
    let faces = INTER
        .iter()
        .flat_map(|r| ferrum_text::read_faces(&font_file(r), r))
        .collect::<Vec<_>>();
    let families = ferrum_text::group(faces)
        .into_iter()
        .map(|f| FontFamilyDto {
            family: f.family,
            faces: f
                .faces
                .into_iter()
                .map(|x| FontFaceDto {
                    style: x.style,
                    weight: Some(x.weight as f64),
                    italic: Some(x.italic),
                    stretch: Some(x.stretch as f64),
                    file: x.file,
                    index: Some(x.index),
                })
                .collect(),
        })
        .collect();
    c.command(Command::FontsAvailable {
        source: FontSource::Bundled,
        families,
    });
    c.take_events();
    c
}

fn boolean_area(c: &Core) -> f64 {
    c.editor()
        .scene()
        .iter()
        .find_map(|(_, n)| n.boolean.as_ref())
        .expect("булева группа")
        .contours
        .to_path()
        .area()
        .abs()
}

/// Ядро с булевой группой «прямоугольник + текст» (шрифт загружен).
fn core_with_boolean_text() -> Core {
    let mut a = core_with_inter_catalog();
    a.command(Command::TestCreateText {
        x: 600.0,
        y: 450.0,
        text: "Ok".into(),
        font: None,
        size: Some(64.0),
        sizing: None,
        width: None,
        height: None,
    });
    let ev = a.take_events();
    for e in &ev {
        if let CoreEvent::FontRequest { family, files, .. } = e {
            for f in files {
                a.load_font_for_tests(family, f, font_file(f));
            }
        }
    }
    a.take_events();
    // Прямоугольник левее текста.
    a.command(Command::SetTool { tool: Tool::Rect });
    drag(&mut a, (300.0, 260.0), (320.0, 280.0), 1000.0);
    let ids: Vec<String> = a
        .editor()
        .scene()
        .iter()
        .map(|(_, n)| n.id.to_string())
        .collect();
    assert_eq!(ids.len(), 2);
    a.command(Command::Select {
        ids,
        mode: SelectMode::Replace,
    });
    a.take_events();
    run(&mut a, "object.boolean_union");
    a
}

/// Шрифт пришёл через `LoadFont` — булева группа с текстом получает глифы
/// сразу, без ввода и команд, и сводка выделенной группы обновляется.
#[test]
fn font_arrival_recomputes_boolean_with_text_and_refreshes_summary() {
    // Документ с булевой группой «прямоугольник + текст» (шрифт загружен).
    let mut a = core_with_boolean_text();
    let full_area = boolean_area(&a);
    a.command(Command::Snapshot { thumbnail: false });
    let bytes = a
        .take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Snapshot { bytes, .. } => Some(bytes),
            _ => None,
        })
        .expect("снимок");

    // Тот же документ в ядре, где байтов шрифта ещё нет.
    let mut c = core_with_inter_catalog();
    c.command(Command::Open { bytes });
    let requests = c.take_events();
    let gid = c
        .editor()
        .scene()
        .iter()
        .find(|(_, n)| n.boolean.is_some())
        .map(|(_, n)| n.id.to_string())
        .expect("булева группа");
    c.command(Command::Select {
        ids: vec![gid],
        mode: SelectMode::Replace,
    });
    let before = summary(&c.take_events()).expect("сводка");
    let area_before = boolean_area(&c);
    assert!(
        area_before < full_area * 0.7,
        "без шрифта — только прямоугольник: {area_before} из {full_area}"
    );

    // Шрифт приходит по настоящему пути LoadFont — и больше ничего.
    let mut after = None;
    for e in &requests {
        if let CoreEvent::FontRequest { family, files, .. } = e {
            for f in files {
                c.load_font_for_tests(family, f, font_file(f));
                after = summary(&c.take_events()).or(after);
            }
        }
    }
    let area_after = boolean_area(&c);
    assert!(
        (area_after - full_area).abs() < full_area * 0.01,
        "глифы в результате: {area_after} против {full_area}"
    );
    let after = after.expect("сводка после прихода шрифта");
    assert!(
        after.w.value > before.w.value + 20.0,
        "ширина в сводке: {} -> {}",
        before.w.value,
        after.w.value
    );
}

fn thumbnail(c: &mut Core) -> Vec<u8> {
    c.command(Command::Snapshot { thumbnail: true });
    c.take_events()
        .into_iter()
        .find_map(|e| match e {
            CoreEvent::Snapshot {
                thumbnail: Some(t), ..
            } => Some(t.into_vec()),
            _ => None,
        })
        .expect("превью")
}

/// Превью первой страницы, когда открыта другая, строится по контурам глифов:
/// текст в булевой группе не пропадает.
#[test]
fn thumbnail_of_another_page_keeps_text_in_booleans() {
    let mut c = core_with_boolean_text();
    let current = thumbnail(&mut c);
    let first = c.editor().page();
    c.command(Command::AddPage);
    c.take_events();
    assert_ne!(c.editor().page(), first, "открыта вторая страница");
    assert!(
        current == thumbnail(&mut c),
        "превью то же, что с открытой первой"
    );
}
