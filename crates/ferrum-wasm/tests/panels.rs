//! Команды и события панелей (этап 5) в ядре worker'а: окно слоёв, выделение из
//! панели и с холста, правки свойств одним шагом, страницы, экспорт, раскладка,
//! настройки. Неверные данные — событие `Error`, а не паника и не правка.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_render::CpuBackend;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

const VIEW: Size = Size::new(800.0, 600.0);

fn demo(nodes: u32) -> Core {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes });
    core
}

fn layers(ev: &[CoreEvent]) -> Option<(u32, u32, Vec<LayerRow>, Option<u32>)> {
    ev.iter().rev().find_map(|e| match e {
        CoreEvent::Layers {
            start,
            total,
            rows,
            reveal,
        } => Some((*start, *total, rows.clone(), *reveal)),
        _ => None,
    })
}

fn summary(ev: &[CoreEvent]) -> Option<Summary> {
    ev.iter().rev().find_map(|e| match e {
        CoreEvent::Properties { summary } => Some(summary.clone()),
        _ => None,
    })
}

fn errors(ev: &[CoreEvent]) -> Vec<String> {
    ev.iter()
        .filter_map(|e| match e {
            CoreEvent::Error { message } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

fn click(core: &mut Core, x: f64, y: f64, t: f64) {
    let e = PointerEvent {
        point: (x, y).into(),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    };
    core.handle(Input::PointerDown(e));
    core.handle(Input::PointerUp(e));
}

fn draw(core: &mut Core) {
    let (w, h) = core.device_size();
    core.draw(&mut CpuBackend::new(w, h));
}

fn document_steps(core: &Core) -> f64 {
    core.live().steps
}

#[test]
fn keymap_and_presets_arrive_before_any_document() {
    let mut core = Core::new(Platform::Mac, VIEW, 1.0, Backend::Cpu);
    let ev = core.take_events();
    let Some(CoreEvent::Keymap {
        commands,
        overrides,
        reserved,
        ..
    }) = ev.iter().find(|e| matches!(e, CoreEvent::Keymap { .. }))
    else {
        panic!("нет раскладки: {ev:?}");
    };
    let rect = commands.iter().find(|c| c.key == "tool.rect").unwrap();
    assert_eq!(
        (rect.name.as_str(), rect.group.as_str()),
        ("Прямоугольник", "tool")
    );
    assert_eq!(
        (rect.bindings.clone(), rect.labels.clone()),
        (vec!["KeyR".to_string()], vec!["R".to_string()])
    );
    let undo = commands.iter().find(|c| c.key == "edit.undo").unwrap();
    assert_eq!(undo.labels, ["⌘Z"], "подпись macOS");
    assert!(overrides.contains("ferrum-keymap"));
    assert!(reserved.contains(&"Meta+KeyW".to_string()));
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Presets { presets } if presets.len() >= 11)));
}

#[test]
fn opening_sends_layers_properties_and_pages() {
    let mut core = demo(1000);
    let ev = core.take_events();
    let (start, total, rows, _) = layers(&ev).expect("строки слоёв");
    // Окно по умолчанию — пустое, пока панель не попросит.
    assert_eq!((start, total, rows.len()), (0, 10, 0));
    assert_eq!(summary(&ev).unwrap().count, 0);
    assert!(ev.iter().any(
        |e| matches!(e, CoreEvent::Pages { pages, background: None, .. } if pages.len() == 1)
    ));
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0].name, "Экран 9");
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    assert!(
        layers(&core.take_events()).is_none(),
        "то же окно не повторяется"
    );
}

#[test]
fn select_from_panel_does_not_reveal_but_canvas_selection_does() {
    let mut core = demo(200);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    let frame = rows[1].id.clone();
    core.command(Command::Select {
        ids: vec![frame.clone()],
        mode: SelectMode::Replace,
    });
    let ev = core.take_events();
    let (_, _, rows, reveal) = layers(&ev).unwrap();
    assert!(rows[1].selected && reveal.is_none());
    let s = summary(&ev).unwrap();
    assert_eq!(
        (s.count, s.kind, s.w.value),
        (1, Some(NodeType::Frame), 1440.0)
    );
    // Щелчок на холсте по «Слой 1» в «Экран 0» (мир (200, 40), экран (140, 64)):
    // фрейм раскрывается, панель получает строку для прокрутки — сверху «Экран 1»,
    // «Экран 0», затем дети сверху вниз от «Слой 98».
    click(&mut core, 140.0, 64.0, 1000.0);
    let (_, total, _, reveal) = layers(&core.take_events()).unwrap();
    assert_eq!((total, reveal), (2 + 99, Some(2 + 97)));
}

#[test]
fn edit_selection_is_one_step_and_updates_summary() {
    let mut core = demo(200);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    core.command(Command::Select {
        ids: vec![rows[0].id.clone()],
        mode: SelectMode::Replace,
    });
    core.take_events();
    let steps = document_steps(&core);
    for v in [10.0, 20.0, 30.0] {
        core.command(Command::EditSelection {
            edit: Edit::Number {
                field: Field::X,
                value: v,
                relative: true,
            },
            phase: EditPhase::Preview,
        });
    }
    assert_eq!(document_steps(&core), steps, "превью — без шагов");
    core.command(Command::EditSelection {
        edit: Edit::Number {
            field: Field::X,
            value: 30.0,
            relative: true,
        },
        phase: EditPhase::Commit,
    });
    assert_eq!(document_steps(&core), steps + 1.0);
    let s = summary(&core.take_events()).expect("сводка обновилась");
    assert_eq!(s.x.value, 1600.0 + 30.0);
    draw(&mut core);
    assert_eq!(
        core.live().selection_x,
        1630.0,
        "живое состояние — те же значения"
    );
    core.command(Command::Execute {
        command: "edit.undo".into(),
    });
    assert_eq!(summary(&core.take_events()).unwrap().x.value, 1600.0);
}

#[test]
fn fills_from_panel_are_validated() {
    let mut core = demo(100);
    core.command(Command::Execute {
        command: "edit.select_all".into(),
    });
    core.take_events();
    let steps = document_steps(&core);
    let red = FillDto {
        kind: PaintKind::Solid,
        color: "#ff0000".into(),
        opacity: 0.5,
        visible: true,
        image_hash: None,
        scale_mode: None,
    };
    core.command(Command::EditSelection {
        edit: Edit::Fills {
            fills: vec![red.clone()],
        },
        phase: EditPhase::Commit,
    });
    let s = summary(&core.take_events()).unwrap();
    assert_eq!(s.fills.unwrap().list[0].color, "#ff0000ff");
    for bad in [
        FillDto {
            color: "красный".into(),
            ..red.clone()
        },
        FillDto {
            opacity: f64::NAN,
            ..red.clone()
        },
        FillDto {
            kind: PaintKind::Image,
            image_hash: Some("zz".into()),
            ..red.clone()
        },
    ] {
        core.command(Command::EditSelection {
            edit: Edit::Fills { fills: vec![bad] },
            phase: EditPhase::Commit,
        });
        assert_eq!(errors(&core.take_events()).len(), 1);
    }
    core.command(Command::EditSelection {
        edit: Edit::Fills {
            fills: vec![red; 65],
        },
        phase: EditPhase::Commit,
    });
    assert_eq!(errors(&core.take_events()).len(), 1, "предел заливок");
    assert_eq!(document_steps(&core), steps + 1.0, "неверное не правит");
}

#[test]
fn rename_move_and_flags_from_layers() {
    let mut core = demo(200);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    let (top, bottom) = (rows[0].id.clone(), rows[1].id.clone());
    core.command(Command::RenameLayer {
        id: top.clone(),
        name: "Главная".into(),
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    assert_eq!(rows[0].name, "Главная");
    core.command(Command::RenameLayer {
        id: top.clone(),
        name: "   ".into(),
    });
    let (_, _, rows, _) =
        layers(&core.take_events()).expect("строки вернулись после пустого имени");
    assert_eq!(rows[0].name, "Главная");
    core.command(Command::MoveLayers {
        ids: Some(vec![top.clone()]),
        target: bottom.clone(),
        position: DropPosition::Below,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    assert_eq!(
        (rows[0].id.as_str(), rows[1].id.as_str()),
        (bottom.as_str(), top.as_str())
    );
    core.command(Command::SetLayerFlags {
        ids: vec![top.clone()],
        visible: Some(false),
        locked: Some(true),
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    assert!(!rows[1].visible && rows[1].locked);
    core.command(Command::MoveLayers {
        ids: None,
        target: top.clone(),
        position: DropPosition::Into,
    });
    assert!(
        errors(&core.take_events()).is_empty(),
        "пустое выделение — без ошибки и без правки"
    );
}

#[test]
fn pages_add_switch_rename_background_delete() {
    let mut core = demo(100);
    core.take_events();
    core.command(Command::AddPage);
    let ev = core.take_events();
    let Some(CoreEvent::Pages { pages, current, .. }) = ev
        .iter()
        .rev()
        .find(|e| matches!(e, CoreEvent::Pages { .. }))
    else {
        panic!("{ev:?}")
    };
    assert_eq!(pages.len(), 2);
    assert_eq!(*current, pages[1].id, "новая — текущая");
    assert_eq!(layers(&ev).unwrap().1, 0, "на новой странице слоёв нет");
    let first = pages[0].id.clone();
    let second = pages[1].id.clone();
    // Превью палитры — только в кадре: документ и список страниц не меняются.
    core.command(Command::SetPageBackground {
        color: Some("#102030".into()),
        phase: EditPhase::Preview,
    });
    assert_eq!(core.background().to_rgba8().r, 0x10);
    assert!(!core
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Pages { .. })));
    core.command(Command::SetPageBackground {
        color: None,
        phase: EditPhase::Cancel,
    });
    assert_eq!(core.background(), ferrum_wasm::core::CANVAS_DARK);
    core.command(Command::SetPageBackground {
        color: Some("#336699".into()),
        phase: EditPhase::Commit,
    });
    let ev = core.take_events();
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Pages { background: Some(b), .. } if b == "#336699ff")));
    assert!(core.wants_frame());
    core.command(Command::RenamePage {
        id: second.clone(),
        name: "Черновик".into(),
    });
    core.command(Command::SetPage { id: first.clone() });
    let ev = core.take_events();
    assert_eq!(layers(&ev).unwrap().1, 1, "первая страница: один фрейм");
    core.command(Command::DeletePage { id: first });
    let ev = core.take_events();
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Pages { pages, current, .. }
        if pages.len() == 1 && pages[0].name == "Черновик" && *current == second)));
    core.command(Command::DeletePage { id: second });
    assert_eq!(
        errors(&core.take_events()),
        ["Нельзя удалить единственную страницу"]
    );
    core.command(Command::SetPage { id: "1@1".into() });
    assert_eq!(errors(&core.take_events()), ["Страница не найдена"]);
}

#[test]
fn export_selection_and_page() {
    let mut core = demo(100);
    core.command(Command::Export { scale: 2.0 });
    let ev = core.take_events();
    let Some(CoreEvent::Exported { name, bytes }) =
        ev.iter().find(|e| matches!(e, CoreEvent::Exported { .. }))
    else {
        panic!("{ev:?}")
    };
    assert_eq!(name, "Страница 1@2x.png");
    assert_eq!(&bytes[1..4], b"PNG");
    // 1440 × 1024 при 2× — 2880 × 2048 (IHDR: ширина и высота big-endian).
    assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 2880);
    core.command(Command::Export { scale: 4.0 });
    assert_eq!(
        errors(&core.take_events()),
        ["Масштаб экспорта — 1×, 2× или 3×"]
    );
    core.command(Command::AddPage);
    core.command(Command::Export { scale: 1.0 });
    assert_eq!(
        errors(&core.take_events()),
        ["Нечего экспортировать: страница пуста"]
    );
}

#[test]
fn bind_conflict_replace_reset_and_import() {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    core.take_events();
    core.command(Command::Bind {
        command: "tool.rect".into(),
        shortcut: "KeyE".into(),
        replace: false,
    });
    let ev = core.take_events();
    let Some(CoreEvent::Keymap {
        commands,
        overrides,
        ..
    }) = ev.iter().find(|e| matches!(e, CoreEvent::Keymap { .. }))
    else {
        panic!("{ev:?}")
    };
    let rect = commands.iter().find(|c| c.key == "tool.rect").unwrap();
    assert_eq!(rect.bindings, ["KeyR", "KeyE"]);
    assert!(rect.overridden && overrides.contains("KeyE"));
    let saved = overrides.clone();
    core.command(Command::Bind {
        command: "tool.ellipse".into(),
        shortcut: "KeyR".into(),
        replace: false,
    });
    let ev = core.take_events();
    assert!(ev.contains(&CoreEvent::BindConflict {
        command: "tool.ellipse".into(),
        shortcut: "KeyR".into(),
        commands: vec!["tool.rect".into()],
    }));
    core.command(Command::Bind {
        command: "tool.ellipse".into(),
        shortcut: "KeyR".into(),
        replace: true,
    });
    core.command(Command::ResetKeymap { command: None });
    core.command(Command::ImportKeymap { json: saved });
    let ev = core.take_events();
    assert!(ev.contains(&CoreEvent::KeymapImported { warnings: vec![] }));
    let Some(CoreEvent::Keymap { commands, .. }) = ev
        .iter()
        .rev()
        .find(|e| matches!(e, CoreEvent::Keymap { .. }))
    else {
        panic!()
    };
    assert_eq!(
        commands
            .iter()
            .find(|c| c.key == "tool.ellipse")
            .unwrap()
            .bindings,
        ["KeyO"]
    );
    core.command(Command::ImportKeymap { json: "{".into() });
    assert_eq!(
        errors(&core.take_events()),
        ["Не удалось импортировать раскладку: это не файл раскладки Ferrum (JSON)"],
        "без английского текста разбора"
    );
    core.command(Command::Bind {
        command: "tool.pen".into(),
        shortcut: "KeyP".into(),
        replace: true,
    });
    core.command(Command::Bind {
        command: "tool.rect".into(),
        shortcut: "Hyper+KeyP".into(),
        replace: true,
    });
    assert_eq!(errors(&core.take_events()).len(), 2);
}

#[test]
fn malformed_panel_commands_are_errors() {
    let mut core = demo(100);
    core.take_events();
    let steps = document_steps(&core);
    let many: Vec<String> = (0..1001).map(|i| format!("{i}@1")).collect();
    let cases = [
        Command::Select {
            ids: many,
            mode: SelectMode::Replace,
        },
        Command::Select {
            ids: vec!["узел".into()],
            mode: SelectMode::Toggle,
        },
        Command::RenameLayer {
            id: "1@1".into(),
            name: "я".repeat(5000),
        },
        Command::FilterLayers {
            query: "x".repeat(5000),
        },
        Command::ExpandLayer {
            id: "x".repeat(100),
            expanded: true,
        },
        Command::EditSelection {
            edit: Edit::Number {
                field: Field::W,
                value: f64::INFINITY,
                relative: false,
            },
            phase: EditPhase::Commit,
        },
        Command::SetPageBackground {
            color: Some("#12".into()),
            phase: EditPhase::Commit,
        },
        Command::AddFramePreset { index: 999 },
        Command::SetZoom { zoom: -1.0 },
        Command::PasteImage {
            bytes: ByteBuf::from(vec![1, 2, 3]),
        },
    ];
    let n = cases.len();
    for c in cases {
        core.command(c);
    }
    assert_eq!(errors(&core.take_events()).len(), n);
    assert_eq!(document_steps(&core), steps);
}

#[test]
fn settings_zoom_preset_and_paste_image() {
    let mut core = demo(100);
    core.command(Command::SetSettings { pixel_snap: false });
    assert!(!core.editor().settings().pixel_snap);
    core.command(Command::SetZoom { zoom: 2.0 });
    assert_eq!(core.live().zoom, 2.0);
    core.command(Command::AddFramePreset { index: 0 });
    let s = summary(&core.take_events()).unwrap();
    assert_eq!((s.count, s.w.value, s.h.unwrap().value), (1, 393.0, 852.0));
    // PNG 2 × 1 из ferrum-render.
    let png = {
        let mut p = ferrum_render::Pixmap::new(2, 1);
        p.data_as_u8_slice_mut().fill(255);
        p.into_png().unwrap()
    };
    core.command(Command::PasteImage {
        bytes: ByteBuf::from(png),
    });
    let s = summary(&core.take_events()).unwrap();
    assert_eq!(
        (s.kind, s.w.value, s.h.unwrap().value),
        (Some(NodeType::Image), 2.0, 1.0)
    );
}

#[test]
fn keymap_and_settings_survive_a_new_document() {
    let mut core = demo(100);
    core.command(Command::Bind {
        command: "tool.rect".into(),
        shortcut: "KeyQ".into(),
        replace: true,
    });
    core.command(Command::SetSettings { pixel_snap: false });
    core.command(Command::Demo { nodes: 0 });
    let km = core.editor().keymap();
    let rect = ferrum_editor::CommandId::ToolRect;
    assert_eq!(
        km.bindings(rect)
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ["KeyR", "KeyQ"]
    );
    assert!(
        !core.editor().settings().pixel_snap,
        "настройки — пользователя, не документа"
    );
}
