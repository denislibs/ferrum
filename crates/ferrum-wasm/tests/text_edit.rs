//! Правка текста в ядре worker'а (подсистема 3, этап 2): команды ввода, клавиш,
//! стиля и свойств, события курсора и сводки стиля, пределы недоверенного ввода,
//! очередь правок, ждущих шрифтов, мигание курсора.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use ferrum_editor::Platform;
use ferrum_text::read_faces;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const FILES: [&str; 4] = [
    "inter/Inter-Variable.ttf",
    "inter/Inter-Italic-Variable.ttf",
    "roboto/Roboto-Variable.ttf",
    "roboto/Roboto-Italic-Variable.ttf",
];

/// Ядро со списком вшитых Inter и Roboto; Inter загружен, на странице — текст
/// «Привет» (выделен).
fn core() -> (Core, String) {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(400.0, 300.0),
        1.0,
        Backend::Cpu,
    );
    c.command(Command::Demo { nodes: 0 });
    let faces = FILES
        .iter()
        .flat_map(|r| read_faces(&font_file(r), r))
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
    for f in &FILES[..2] {
        c.load_font_for_tests("Inter", f, font_file(f));
    }
    c.command(Command::TestCreateText {
        x: 20.0,
        y: 20.0,
        text: "Привет".into(),
        font: None,
        size: Some(24.0),
        sizing: None,
        width: None,
        height: None,
    });
    let id = c.editor().selection()[0].to_string();
    c.take_events();
    (c, id)
}

/// Последнее `TextCaret` пачки: правка, выделение, прямоугольник.
type CaretState = (bool, Option<[u32; 2]>, Option<[f64; 4]>);

fn caret(events: &[CoreEvent]) -> Option<CaretState> {
    events.iter().rev().find_map(|e| match e {
        CoreEvent::TextCaret {
            editing,
            selection,
            rect,
            ..
        } => Some((*editing, *selection, *rect)),
        _ => None,
    })
}

fn summary(events: &[CoreEvent]) -> Option<Option<TextStyleDto>> {
    events.iter().rev().find_map(|e| match e {
        CoreEvent::TextStyleSummary { summary } => Some(summary.clone()),
        _ => None,
    })
}

fn text(c: &Core) -> String {
    let id = c.editor().selection()[0];
    c.editor()
        .scene()
        .node(id)
        .unwrap()
        .props
        .text
        .as_ref()
        .unwrap()
        .content
        .text()
}

fn input(c: &mut Core, kind: TextInputKindDto, data: &str) {
    c.command(Command::TextInput {
        kind,
        data: Some(data.into()),
    });
}

fn key(c: &mut Core, key: &str, mods: KeyMods) {
    c.command(Command::TextKey {
        key: key.into(),
        mods,
    });
}

#[test]
fn edit_start_input_and_end_report_the_caret() {
    let (mut c, id) = core();
    c.command(Command::TextEditStart { id });
    let (editing, sel, rect) = caret(&c.take_events()).unwrap();
    assert!(editing);
    assert_eq!(sel, Some([0, 6]));
    let r = rect.unwrap();
    // Камера примера — зум 0.5: строка 29 px мира — 14,5 px экрана.
    assert!(r[3] > 12.0, "высота курсора — строка: {r:?}");
    key(&mut c, "End", KeyMods::default());
    input(&mut c, TextInputKindDto::Insert, "!");
    let events = c.take_events();
    assert_eq!(caret(&events).unwrap().1, Some([7, 7]));
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Document { .. })));
    assert_eq!(text(&c), "Привет!\n");
    key(&mut c, "Escape", KeyMods::default());
    let (editing, sel, _) = caret(&c.take_events()).unwrap();
    assert!(!editing && sel.is_none());
}

#[test]
fn untrusted_text_commands_are_bounded() {
    let (mut c, id) = core();
    c.command(Command::TextEditStart { id });
    c.take_events();
    input(&mut c, TextInputKindDto::Insert, &"я".repeat(40_000));
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { message } if message.contains("64 КБ"))));
    assert_eq!(text(&c), "Привет\n");
    key(&mut c, &"K".repeat(100), KeyMods::default());
    assert!(c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
    // Неизвестный ключ правки стиля не разбирается.
    let bad = serde_json::json!({
        "type": "setTextStyle", "phase": "commit", "patch": {"size": 12, "weight": 700}
    });
    assert!(serde_json::from_value::<Command>(bad).is_err());
    // Не число — ошибка, документ тот же.
    c.command(Command::SetTextStyle {
        patch: TextStylePatchDto {
            size: Some(f64::NAN),
            ..Default::default()
        },
        phase: EditPhase::Commit,
        ids: None,
    });
    assert!(c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { message } if message.contains("не число"))));
}

#[test]
fn style_and_props_commands_edit_and_report_the_summary() {
    let (mut c, _) = core();
    c.command(Command::SetTextStyle {
        patch: TextStylePatchDto {
            size: Some(40.0),
            line_height: Some(SpacingDto {
                unit: SpacingUnit::Percent,
                value: 150.0,
            }),
            ..Default::default()
        },
        phase: EditPhase::Commit,
        ids: None,
    });
    let s = summary(&c.take_events()).unwrap().unwrap();
    assert_eq!(s.size.value, 40.0);
    assert_eq!(s.line_height.value.unit, SpacingUnit::Percent);
    assert!(!s.range);
    c.command(Command::SetTextProps {
        patch: TextPropsPatchDto {
            align_h: Some(AlignHDto::Right),
            ..Default::default()
        },
        phase: EditPhase::Commit,
        ids: None,
    });
    let s = summary(&c.take_events()).unwrap().unwrap();
    assert_eq!(s.align_h.value, "right");
    // Без выделения сводки нет.
    c.command(Command::Select {
        ids: vec![],
        mode: SelectMode::Replace,
    });
    assert_eq!(summary(&c.take_events()), Some(None));
}

#[test]
fn style_waiting_for_a_font_keeps_the_order_of_typing() {
    let (mut c, id) = core();
    c.command(Command::TextEditStart { id });
    c.command(Command::SetTextStyle {
        patch: TextStylePatchDto {
            family: Some("Roboto".into()),
            style: Some("Regular".into()),
            ..Default::default()
        },
        phase: EditPhase::Commit,
        ids: None,
    });
    // Roboto ещё не загружен: правка ждёт, набор за ней — тоже.
    input(&mut c, TextInputKindDto::Insert, "!");
    assert_eq!(text(&c), "Привет\n");
    let events = c.take_events();
    let req = events
        .iter()
        .find_map(|e| match e {
            CoreEvent::FontRequest { family, files, .. } if family == "Roboto" => {
                Some(files.clone())
            }
            _ => None,
        })
        .expect("запрос Roboto");
    for f in req {
        c.load_font_for_tests("Roboto", &f, font_file(&f));
    }
    assert_eq!(
        text(&c),
        "!\n",
        "сначала стиль, затем набор поверх выделения"
    );
    let s = summary(&c.take_events()).unwrap().unwrap();
    assert_eq!(s.family.value, "Roboto");
}

#[test]
fn plain_paste_outside_editing_creates_text() {
    let (mut c, _) = core();
    let before = c.editor().scene().len();
    input(&mut c, TextInputKindDto::Paste, "Из буфера");
    assert_eq!(c.editor().scene().len(), before + 1);
    assert_eq!(text(&c), "Из буфера\n");
}

#[test]
fn caret_blink_requests_frames() {
    let (mut c, id) = core();
    c.set_time(1000.0);
    c.command(Command::TextEditStart { id });
    key(&mut c, "End", KeyMods::default());
    let mut b = ferrum_render::CpuBackend::new(400, 300);
    c.draw(&mut b);
    assert!(!c.wants_frame());
    c.set_time(1100.0);
    assert!(!c.wants_frame(), "курсор ещё горит — кадр не нужен");
    c.set_time(1600.0);
    assert!(c.wants_frame(), "курсор погас — нужен кадр");
}

#[test]
fn text_tool_and_editing_show_the_text_cursor() {
    let (mut c, _) = core();
    c.command(Command::SetTool { tool: Tool::Text });
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Tool { tool: Tool::Text })));
    assert!(events.iter().any(|e| matches!(
        e,
        CoreEvent::Cursor {
            cursor: CursorKind::Text,
            ..
        }
    )));
}

/// Документ и шаги отмены до и после недопустимой правки не отличаются.
fn assert_rejected(c: &mut Core, cmd: Command, needle: &str) {
    let steps = c.editor().step_count();
    let before = text(c);
    let size = c
        .editor()
        .scene()
        .node(c.editor().selection()[0])
        .unwrap()
        .props
        .text
        .as_ref()
        .unwrap()
        .props
        .style
        .size;
    c.command(cmd);
    let events = c.take_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, CoreEvent::Error { message } if message.contains(needle))),
        "ждали ошибку «{needle}»: {events:?}"
    );
    assert!(!events
        .iter()
        .any(|e| matches!(e, CoreEvent::Document { .. })));
    assert_eq!(c.editor().step_count(), steps);
    assert_eq!(text(c), before);
    let after = c
        .editor()
        .scene()
        .node(c.editor().selection()[0])
        .unwrap()
        .props
        .text
        .as_ref()
        .unwrap()
        .props
        .style
        .size;
    assert_eq!(after, size);
}

fn style(patch: TextStylePatchDto) -> Command {
    Command::SetTextStyle {
        patch,
        phase: EditPhase::Commit,
        ids: None,
    }
}

#[test]
fn style_patch_bounds_are_errors_and_leave_the_document() {
    let (mut c, _) = core();
    let sp = |unit, value| SpacingDto { unit, value };
    for size in [0.0, 0.5, -3.0, 10_001.0, f64::INFINITY] {
        assert_rejected(
            &mut c,
            style(TextStylePatchDto {
                size: Some(size),
                ..Default::default()
            }),
            "Размер шрифта",
        );
    }
    for v in [f64::NAN, -1.0, 10_001.0] {
        assert_rejected(
            &mut c,
            style(TextStylePatchDto {
                line_height: Some(sp(SpacingUnit::Px, v)),
                ..Default::default()
            }),
            "Интерлиньяж",
        );
    }
    assert_rejected(
        &mut c,
        style(TextStylePatchDto {
            line_height: Some(sp(SpacingUnit::Percent, 10_001.0)),
            ..Default::default()
        }),
        "Интерлиньяж",
    );
    for v in [f64::NEG_INFINITY, -10_001.0, 10_001.0] {
        assert_rejected(
            &mut c,
            style(TextStylePatchDto {
                letter_spacing: Some(sp(SpacingUnit::Px, v)),
                ..Default::default()
            }),
            "Трекинг",
        );
    }
    assert_rejected(
        &mut c,
        style(TextStylePatchDto {
            letter_spacing: Some(sp(SpacingUnit::Percent, 1001.0)),
            ..Default::default()
        }),
        "Трекинг",
    );
    assert_rejected(
        &mut c,
        style(TextStylePatchDto {
            letter_spacing: Some(sp(SpacingUnit::Auto, 0.0)),
            ..Default::default()
        }),
        "Трекинг",
    );
    // Имена: длиннее 256 байт, пустые, начертание без семейства.
    let long = "Я".repeat(129);
    assert_eq!(long.len(), 258);
    for (family, style_name) in [
        (Some(long.clone()), None),
        (Some("Inter".to_string()), Some(long)),
        (Some(String::new()), None),
        (None, Some("Bold".to_string())),
    ] {
        assert_rejected(
            &mut c,
            style(TextStylePatchDto {
                family,
                style: style_name,
                ..Default::default()
            }),
            "Неверная правка",
        );
    }
    // Границы допустимого проходят.
    c.command(style(TextStylePatchDto {
        size: Some(10_000.0),
        ..Default::default()
    }));
    assert!(!c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
    c.command(style(TextStylePatchDto {
        size: Some(1.0),
        ..Default::default()
    }));
    assert!(!c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
}

#[test]
fn props_patch_bounds_and_unknown_keys_are_rejected() {
    let (mut c, _) = core();
    for (spacing, indent) in [
        (Some(f64::NAN), None),
        (Some(-1.0), None),
        (Some(10_001.0), None),
        (None, Some(f64::INFINITY)),
        (None, Some(-10_001.0)),
    ] {
        assert_rejected(
            &mut c,
            Command::SetTextProps {
                patch: TextPropsPatchDto {
                    paragraph_spacing: spacing,
                    paragraph_indent: indent,
                    ..Default::default()
                },
                phase: EditPhase::Commit,
                ids: None,
            },
            "Неверная правка",
        );
    }
    for bad in [
        serde_json::json!({"type": "setTextProps", "phase": "commit", "patch": {"truncate": true}}),
        serde_json::json!({"type": "setTextProps", "phase": "commit", "patch": {"alignH": "left", "x": 1}}),
        serde_json::json!({"type": "textKey", "key": "End", "mods": {"shift": true, "super": true}}),
    ] {
        assert!(serde_json::from_value::<Command>(bad).is_err());
    }
}

#[test]
fn ids_are_bounded() {
    let (mut c, _) = core();
    let too_many = vec!["0@0".to_string(); 1001];
    assert_rejected(
        &mut c,
        Command::SetTextProps {
            patch: TextPropsPatchDto {
                align_h: Some(AlignHDto::Right),
                ..Default::default()
            },
            phase: EditPhase::Commit,
            ids: Some(too_many),
        },
        "слоёв",
    );
    assert_rejected(
        &mut c,
        Command::SetTextProps {
            patch: TextPropsPatchDto {
                align_h: Some(AlignHDto::Right),
                ..Default::default()
            },
            phase: EditPhase::Commit,
            ids: Some(vec!["не идентификатор".into()]),
        },
        "идентификатор",
    );
}
