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
            ids: None,
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
        ids: None,
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
        ids: None,
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
            ids: None,
        });
        assert_eq!(errors(&core.take_events()).len(), 1);
    }
    core.command(Command::EditSelection {
        edit: Edit::Fills {
            fills: vec![red; 65],
        },
        phase: EditPhase::Commit,
        ids: None,
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
            ids: None,
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

// ——— Проводка панелей: кэш слоёв, страницы, пределы ———

fn pages(ev: &[CoreEvent]) -> Vec<(Vec<PageInfo>, String, Option<String>)> {
    ev.iter()
        .filter_map(|e| match e {
            CoreEvent::Pages {
                pages,
                current,
                background,
            } => Some((pages.clone(), current.clone(), background.clone())),
            _ => None,
        })
        .collect()
}

fn pointer(x: f64, y: f64, t: f64) -> PointerEvent {
    PointerEvent {
        point: (x, y).into(),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    }
}

fn drag(core: &mut Core, from: (f64, f64), to: (f64, f64), t: f64) {
    core.handle(Input::PointerDown(pointer(from.0, from.1, t)));
    for i in 1..=5 {
        let k = i as f64 / 5.0;
        let (x, y) = (from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
        core.handle(Input::PointerMove(pointer(x, y, t + i as f64 * 16.0)));
    }
    core.handle(Input::PointerUp(pointer(to.0, to.1, t + 100.0)));
}

fn row_id(rows: &[LayerRow], name: &str) -> String {
    rows.iter().find(|r| r.name == name).unwrap().id.clone()
}

/// Каждая применённая к сцене правка (команда панели, отмена, жест холста)
/// доходит до кэша имён: при активном поиске страница не обходится заново.
#[test]
fn search_name_cache_stays_warm_across_every_kind_of_edit() {
    let mut core = demo(1000);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    let (one, two) = (row_id(&rows, "Экран 1"), row_id(&rows, "Экран 2"));
    core.command(Command::FilterLayers {
        query: "экран 1".into(),
    });
    let (_, total, rows, _) = layers(&core.take_events()).unwrap();
    assert_eq!((total, rows[0].name.as_str()), (1, "Экран 1"));
    assert_eq!(core.layers().name_scans(), 1, "первый поиск — полный обход");
    let stamp = core.editor().scene().structure_stamp();

    // Переименование во время поиска: совпадения пересчитаны, обхода нет.
    core.command(Command::RenameLayer {
        id: two.clone(),
        name: "Экран 12".into(),
    });
    let (_, total, _, _) = layers(&core.take_events()).unwrap();
    assert_eq!(total, 2, "новое имя совпало с запросом");
    // Отмена.
    core.command(Command::Execute {
        command: "edit.undo".into(),
    });
    let (_, total, _, _) = layers(&core.take_events()).unwrap();
    assert_eq!(total, 1, "отмена вернула прежнее имя");
    // Правка из панели свойств.
    core.command(Command::Select {
        ids: vec![one.clone()],
        mode: SelectMode::Replace,
    });
    core.command(Command::EditSelection {
        edit: Edit::Number {
            field: Field::X,
            value: 10.0,
            relative: true,
        },
        phase: EditPhase::Commit,
        ids: None,
    });
    // Жест холста (перенос «Слой 1» в «Экран 0»): правка приходит из ввода.
    drag(&mut core, (140.0, 64.0), (160.0, 64.0), 1000.0);
    core.take_events();
    assert_eq!(
        core.editor().scene().structure_stamp(),
        stamp + 4,
        "четыре правки дошли до сцены"
    );
    core.command(Command::FilterLayers {
        query: "экран 12".into(),
    });
    let (_, total, _, _) = layers(&core.take_events()).unwrap();
    assert_eq!(total, 0);
    assert_eq!(core.layers().name_scans(), 1, "кэш имён не сбрасывался");

    // Другая страница — другая сцена: тот же счётчик показывает новый обход.
    core.command(Command::AddPage);
    core.command(Command::FilterLayers {
        query: "экран".into(),
    });
    core.take_events();
    assert_eq!(core.layers().name_scans(), 2);
}

/// Команда `PrepareSearch` строит кэш имён заранее: первый поиск после неё
/// обходить страницу не должен.
#[test]
fn prepare_search_warms_the_name_cache() {
    let mut core = demo(1000);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    core.take_events();
    core.command(Command::PrepareSearch);
    core.take_events();
    assert_eq!(core.layers().name_scans(), 1, "кэш построен заранее");
    core.command(Command::FilterLayers {
        query: "экран 1".into(),
    });
    let (_, total, _, _) = layers(&core.take_events()).unwrap();
    assert_eq!(total, 1);
    assert_eq!(core.layers().name_scans(), 1, "поиск обхода не потребовал");
}

/// Черновик жеста не попадает в строки, готовая фигура — попадает; отмена и
/// другая страница пересобирают строки.
#[test]
fn layer_rows_follow_drafts_undo_and_page_switch() {
    let mut core = demo(1000);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    assert_eq!(layers(&core.take_events()).unwrap().1, 10);
    core.command(Command::SetTool { tool: Tool::Rect });
    // Пустое место под фреймами: мир y 1042..1092.
    core.handle(Input::PointerDown(pointer(100.0, 565.0, 0.0)));
    core.handle(Input::PointerMove(pointer(150.0, 580.0, 16.0)));
    core.handle(Input::PointerMove(pointer(200.0, 590.0, 32.0)));
    core.command(Command::LayersWindow {
        start: 1,
        count: 40,
    });
    let (_, total, rows, _) = layers(&core.take_events()).unwrap();
    assert_eq!((total, rows.len()), (10, 9), "черновика в строках нет");
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    core.handle(Input::PointerUp(pointer(200.0, 590.0, 48.0)));
    let (_, total, rows, reveal) = layers(&core.take_events()).unwrap();
    assert_eq!(total, 11);
    assert_eq!(rows[0].kind, NodeType::Rect, "новая фигура — сверху");
    assert!(rows[0].selected);
    assert_eq!(reveal, Some(0));
    core.command(Command::Execute {
        command: "edit.undo".into(),
    });
    let (_, total, _, _) = layers(&core.take_events()).unwrap();
    assert_eq!(total, 10, "отмена убрала строку");
    core.command(Command::AddPage);
    let (_, total, rows, _) = layers(&core.take_events()).unwrap();
    assert_eq!((total, rows.len()), (0, 0), "новая страница пуста");
}

/// Имена страниц и документа из интерфейса: без управляющих символов, не длиннее
/// 256 символов; строка больше 4096 байт — ошибка, имя не меняется.
#[test]
fn page_and_document_names_are_cleaned_and_capped() {
    let mut core = demo(10);
    core.take_events();
    let page = core.editor().page().to_string();
    let name_of = |ev: &[CoreEvent]| pages(ev).last().unwrap().0[0].name.clone();
    core.command(Command::RenamePage {
        id: page.clone(),
        name: "  Новая\tстраница\n".into(),
    });
    assert_eq!(name_of(&core.take_events()), "Новая страница");
    core.command(Command::RenamePage {
        id: page.clone(),
        name: "я".repeat(300),
    });
    assert_eq!(name_of(&core.take_events()).chars().count(), 256);
    core.command(Command::RenamePage {
        id: page.clone(),
        name: "я".repeat(3000),
    });
    let ev = core.take_events();
    assert_eq!(errors(&ev), ["Слишком длинная строка в команде"]);
    assert_eq!(name_of(&ev), "я".repeat(256), "имя прежнее");
    core.command(Command::SetDocumentName {
        name: "\u{7}Лендинг\r\n".into(),
    });
    assert!(core.take_events().contains(&CoreEvent::DocumentName {
        name: "Лендинг".into()
    }));
    core.command(Command::RenamePage {
        id: "x".repeat(65),
        name: "Фон".into(),
    });
    assert_eq!(errors(&core.take_events()), ["Страница не найдена"]);
}

/// Список страниц уходит после переименования и смены фона — по одному событию.
#[test]
fn pages_event_after_rename_and_background() {
    let mut core = demo(10);
    core.take_events();
    let page = core.editor().page().to_string();
    core.command(Command::RenamePage {
        id: page.clone(),
        name: "Главная".into(),
    });
    let p = pages(&core.take_events());
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].0[0].name, "Главная");
    core.command(Command::SetPageBackground {
        color: Some("#ffffff".into()),
        phase: EditPhase::Commit,
    });
    assert_eq!(
        pages(&core.take_events())[0].2.as_deref(),
        Some("#ffffffff")
    );
    core.command(Command::SetPageBackground {
        color: None,
        phase: EditPhase::Commit,
    });
    let p = pages(&core.take_events());
    assert_eq!((p.len(), p[0].2.clone()), (1, None), "фон снят");
}

/// Жест палитры фона: сотни превью — ни одной записи в документ, одна запись при
/// отпускании; отмена с мусором вместо цвета ошибки не даёт и превью убирает.
#[test]
fn page_background_gesture_writes_once_on_release() {
    let mut core = demo(10);
    core.take_events();
    for i in 0..200u32 {
        core.command(Command::SetPageBackground {
            color: Some(format!("#{:02x}2030", i % 256)),
            phase: EditPhase::Preview,
        });
    }
    let ev = core.take_events();
    assert!(pages(&ev).is_empty() && errors(&ev).is_empty());
    assert_eq!(core.editor().page_background(), None, "документ не тронут");
    assert_eq!(core.background().to_rgba8().r, 199);
    core.command(Command::SetPageBackground {
        color: Some("#c72030".into()),
        phase: EditPhase::Commit,
    });
    assert_eq!(pages(&core.take_events()).len(), 1);
    assert_eq!(
        core.editor().page_background().map(|c| c.to_hex()),
        Some("#c72030ff".into())
    );
    core.command(Command::SetPageBackground {
        color: Some("#000000".into()),
        phase: EditPhase::Preview,
    });
    core.command(Command::SetPageBackground {
        color: Some("мусор".into()),
        phase: EditPhase::Cancel,
    });
    assert!(errors(&core.take_events()).is_empty());
    assert_eq!(core.background().to_rgba8().r, 0xc7, "превью снято");
}

/// Пределы новых команд и событий: списки, строки, обводки, байты, имена в
/// событиях — ошибка на русском, документ не меняется.
#[test]
fn panel_command_and_event_caps() {
    let mut core = demo(100);
    core.take_events();
    let steps = document_steps(&core);
    let many: Vec<String> = (0..1001).map(|i| format!("{i}@1")).collect();
    let stroke = StrokeDto {
        color: "#000000".into(),
        width: 1.0,
        align: Align::Inside,
    };
    let cases = [
        Command::SetLayerFlags {
            ids: many.clone(),
            visible: Some(false),
            locked: None,
        },
        Command::MoveLayers {
            ids: Some(many),
            target: "1@1".into(),
            position: DropPosition::Into,
        },
        Command::HoverLayer {
            id: Some("x".repeat(100)),
        },
        Command::EditSelection {
            edit: Edit::Strokes {
                strokes: vec![stroke.clone(); 65],
            },
            phase: EditPhase::Commit,
            ids: None,
        },
        Command::EditSelection {
            edit: Edit::Strokes {
                strokes: vec![StrokeDto {
                    width: f64::NAN,
                    ..stroke
                }],
            },
            phase: EditPhase::Commit,
            ids: None,
        },
        Command::EditSelection {
            edit: Edit::Size {
                w: f64::NAN,
                h: 1.0,
            },
            phase: EditPhase::Preview,
            ids: None,
        },
        Command::SetDocumentName {
            name: "я".repeat(3000),
        },
        Command::Unbind {
            command: "tool.rect".into(),
            shortcut: "K".repeat(65),
        },
        Command::ResetKeymap {
            command: Some("нет.такой".repeat(1000)),
        },
        Command::ImportKeymap {
            json: " ".repeat(64 * 1024 + 1),
        },
        Command::PasteImage {
            bytes: ByteBuf::from(vec![0u8; (64 << 20) + 1]),
        },
    ];
    let n = cases.len();
    for c in cases {
        core.command(c);
    }
    let errs = errors(&core.take_events());
    assert_eq!(errs.len(), n, "{errs:?}");
    for e in &errs {
        assert!(e.chars().count() < 120, "{e}");
        assert!(
            e.chars().any(|c| ('\u{400}'..='\u{4ff}').contains(&c)),
            "по-русски: {e}"
        );
    }
    assert_eq!(document_steps(&core), steps);

    // Имена из файла в событиях — не длиннее 256 символов.
    let mut doc = ferrum_doc::Document::new();
    let page = doc.pages()[0].0;
    doc.rename_page(page, &"с".repeat(5000)).unwrap();
    doc.set_name(&"д".repeat(5000)).unwrap();
    let bytes = ferrum_doc::container::encode(&doc).unwrap();
    core.command(Command::Open {
        bytes: ByteBuf::from(bytes),
    });
    let ev = core.take_events();
    let Some(CoreEvent::Opened { name, .. }) =
        ev.iter().find(|e| matches!(e, CoreEvent::Opened { .. }))
    else {
        panic!("{ev:?}")
    };
    assert_eq!(name.chars().count(), 256);
    assert_eq!(pages(&ev)[0].0[0].name.chars().count(), 256);
}

#[test]
fn edit_with_explicit_ids_applies_to_them_not_to_selection() {
    let mut core = demo(200);
    core.command(Command::LayersWindow {
        start: 0,
        count: 40,
    });
    let (_, _, rows, _) = layers(&core.take_events()).unwrap();
    let x_of = |core: &mut Core, id: &str| {
        core.command(Command::Select {
            ids: vec![id.into()],
            mode: SelectMode::Replace,
        });
        summary(&core.take_events()).unwrap().x.value
    };
    let (a, b) = (rows[0].id.clone(), rows[1].id.clone());
    let (xa, xb) = (x_of(&mut core, &a), x_of(&mut core, &b));
    // Выделен b, а правка адресована a: превью и шаг — у a.
    let steps = document_steps(&core);
    for phase in [EditPhase::Preview, EditPhase::Commit] {
        core.command(Command::EditSelection {
            edit: Edit::Number {
                field: Field::X,
                value: 30.0,
                relative: true,
            },
            phase,
            ids: Some(vec![a.clone()]),
        });
    }
    assert_eq!(document_steps(&core), steps + 1.0);
    assert_eq!(x_of(&mut core, &a), xa + 30.0);
    assert_eq!(x_of(&mut core, &b), xb, "выделенный не тронут");
    // Мёртвый узел отфильтровывается; пустой список — правки нет; лишний — ошибка.
    core.take_events();
    let steps = document_steps(&core);
    for ids in [vec!["999999@1".to_string()], vec![]] {
        core.command(Command::EditSelection {
            edit: Edit::Number {
                field: Field::X,
                value: 5.0,
                relative: true,
            },
            phase: EditPhase::Commit,
            ids: Some(ids),
        });
    }
    assert_eq!(document_steps(&core), steps);
    core.take_events();
    core.command(Command::EditSelection {
        edit: Edit::ClipContent { value: true },
        phase: EditPhase::Commit,
        ids: Some(vec![a.clone(); 1001]),
    });
    assert!(!errors(&core.take_events()).is_empty());
    assert_eq!(document_steps(&core), steps);
}
