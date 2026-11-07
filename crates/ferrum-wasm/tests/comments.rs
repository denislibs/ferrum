//! Комментарии в ядре worker'а (спека подсистемы 4, раздел 10): метки от интерфейса проверяются
//! ядром, события редактора уходят событиями протокола, без `CommentsAvailable` (отдельная сборка)
//! режима нет, команды комментариев не берут удержание чужого превью, метки не попадают ни в
//! документ и журнал, ни в превью файла.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, KeyEvent, Modifiers, Platform, PointerEvent};
use ferrum_render::CpuBackend;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_json::json;

/// Ядро; камера при открытии — центр в начале мира: экран (x, y) — мир (x − 400, y − 300).
fn core() -> Core {
    let mut c = Core::new(
        Platform::Windows,
        Size::new(800.0, 600.0),
        1.0,
        Backend::Cpu,
    );
    c.take_events();
    c
}

fn page(c: &Core) -> String {
    c.editor().page().to_string()
}

fn at(x: f64, y: f64, t: f64) -> PointerEvent {
    PointerEvent::new((x, y), Modifiers::NONE, t)
}

fn click(c: &mut Core, x: f64, y: f64, t: f64) {
    c.handle(Input::PointerDown(at(x, y, t)));
    c.handle(Input::PointerUp(at(x, y, t + 50.0)));
}

/// Прямоугольник инструментом от экрана (100, 100) до (200, 200); его идентификатор.
fn rect(c: &mut Core) -> String {
    c.command(Command::SetTool { tool: Tool::Rect });
    c.handle(Input::PointerDown(at(100.0, 100.0, 1000.0)));
    c.handle(Input::PointerMove(at(200.0, 200.0, 1100.0)));
    c.handle(Input::PointerUp(at(200.0, 200.0, 1200.0)));
    c.take_events();
    c.editor().selection()[0].to_string()
}

/// Метка в мире `(x, y)`.
fn pin(c: &Core, id: &str, x: f64, y: f64) -> CommentPinDto {
    CommentPinDto {
        id: id.into(),
        page: page(c),
        node: None,
        x,
        y,
        wx: x,
        wy: y,
        initials: "АБ".into(),
        color: 0x0d74ce,
        replies: 2,
        unread: true,
        resolved: false,
        movable: true,
    }
}

fn available(c: &mut Core) {
    c.command(Command::CommentsAvailable { on: true });
}

#[test]
fn standalone_build_has_no_comment_mode() {
    let mut c = core();
    c.command(Command::Execute {
        command: "tool.comment".into(),
    });
    c.command(Command::SetTool {
        tool: Tool::Comment,
    });
    c.handle(Input::KeyDown(KeyEvent::new("KeyC", Modifiers::NONE)));
    c.handle(Input::KeyDown(KeyEvent::new("KeyC", Modifiers::SHIFT)));
    let ev = c.take_events();
    assert!(
        !ev.iter().any(|e| matches!(e, CoreEvent::Tool { .. })),
        "{ev:?}"
    );
    assert_eq!(c.editor().tool_kind(), ferrum_editor::ToolKind::Move);
    assert!(c.editor().comments_visible());
    // Состояние команд: обе серые.
    let states = |c: &Core| {
        let s = c
            .editor()
            .command_state(ferrum_editor::CommandId::ToolComment);
        let v = c
            .editor()
            .command_state(ferrum_editor::CommandId::ViewComments);
        (s.enabled, v.enabled)
    };
    assert_eq!(states(&c), (false, false));
    available(&mut c);
    assert_eq!(states(&c), (true, true));
    let ev = c.take_events();
    assert!(ev.contains(&CoreEvent::CommentsState {
        mode: false,
        visible: true,
        available: true
    }));
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::CommandStates { changed } if changed.iter().any(|s| s.key == "tool.comment" && s.enabled)
    )));
    c.handle(Input::KeyDown(KeyEvent::new("KeyC", Modifiers::NONE)));
    let ev = c.take_events();
    assert!(ev.contains(&CoreEvent::Tool {
        tool: Tool::Comment
    }));
    assert!(ev.contains(&CoreEvent::CommentsState {
        mode: true,
        visible: true,
        available: true
    }));
}

#[test]
fn host_input_is_validated_and_bad_pins_are_skipped_alone() {
    let mut c = core();
    available(&mut c);
    let good = pin(&c, "11111111-2222-3333-4444-555555555555", 10.0, 10.0);
    let mut bad = Vec::new();
    for f in [
        |p: &mut CommentPinDto| p.x = f64::NAN,
        |p: &mut CommentPinDto| p.wy = f64::INFINITY,
        |p: &mut CommentPinDto| p.y = 2e9,
        |p: &mut CommentPinDto| p.id = "x".repeat(65),
        |p: &mut CommentPinDto| p.id = "../a".into(),
        |p: &mut CommentPinDto| p.id = String::new(),
        |p: &mut CommentPinDto| p.page = "root".into(),
        |p: &mut CommentPinDto| p.page = "страница".into(),
        |p: &mut CommentPinDto| p.node = Some(format!("1@{}", u64::MAX)),
        |p: &mut CommentPinDto| p.node = Some("1@".into()),
        |p: &mut CommentPinDto| p.color = 0x1000000,
        |p: &mut CommentPinDto| p.initials = "e\u{301}".repeat(20),
        // Разбираются как `1@1`, но длиннее 64 байт.
        |p: &mut CommentPinDto| p.page = format!("{}1@1", "0".repeat(70)),
        |p: &mut CommentPinDto| p.node = Some(format!("{}1@1", "0".repeat(70))),
    ] {
        let mut p = pin(&c, "bad", 0.0, 0.0);
        f(&mut p);
        bad.push(p);
    }
    let mut long = pin(&c, "long", 0.0, 0.0);
    // Четыре графемы (вторая — с диакритикой): остаются три.
    long.initials = "Ae\u{301}ВГ".into();
    let mut pins = bad;
    pins.push(good);
    pins.push(long);
    c.command(Command::CommentPins {
        pins,
        replace: true,
    });
    let ed = c.editor();
    assert_eq!(ed.comment_pin_count(), 2);
    assert!(ed.comment_pin("bad").is_none());
    assert_eq!(&*ed.comment_pin("long").unwrap().initials, "Ae\u{301}В");
    // Ни одной ошибки: неверное — тихо пропущено, как записи присутствия.
    assert!(!c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
    // Сверх 10 000 меток лишние не принимаются.
    let many: Vec<_> = (0..COMMENT_PINS_MAX + 7)
        .map(|i| pin(&c, &format!("t{i}"), 0.0, 0.0))
        .collect();
    c.command(Command::CommentPins {
        pins: many,
        replace: true,
    });
    assert_eq!(c.editor().comment_pin_count(), COMMENT_PINS_MAX);
    // Неверные идентификаторы в остальных командах — ничего не делают.
    c.command(Command::CommentOpen {
        id: Some("t1".into()),
    });
    c.command(Command::CommentOpen {
        id: Some("a/b".into()),
    });
    assert_eq!(c.editor().comment_open(), Some("t1"));
    c.command(Command::CommentPinsRemove {
        ids: vec!["t1".into(), "y".repeat(100)],
    });
    assert_eq!(c.editor().comment_pin_count(), COMMENT_PINS_MAX - 1);
    c.command(Command::CommentDraft {
        draft: Some(CommentDraftDto {
            page: page(&c),
            node: None,
            x: f64::NAN,
            y: 0.0,
            wx: 0.0,
            wy: 0.0,
        }),
    });
    assert!(c
        .editor()
        .overlay()
        .comments
        .iter()
        .all(|m| !matches!(m.kind, ferrum_render::PinKind::Draft)));
}

#[test]
fn editor_events_become_protocol_events() {
    let mut c = core();
    available(&mut c);
    let r = rect(&mut c);
    c.command(Command::SetTool {
        tool: Tool::Comment,
    });
    c.take_events();
    // Щелчок по прямоугольнику (мир −300…−200) — место нового, привязанное к нему.
    click(&mut c, 150.0, 120.0, 5000.0);
    let place = c
        .take_events()
        .into_iter()
        .find(|e| matches!(e, CoreEvent::CommentPlace { .. }))
        .expect("CommentPlace");
    assert_eq!(
        place,
        CoreEvent::CommentPlace {
            page: page(&c),
            node: Some(r.clone()),
            x: 50.0,
            y: 20.0,
            wx: -250.0,
            wy: -180.0,
            sx: 150.0,
            sy: 120.0
        }
    );
    // Черновик — метка над точкой, пока пишут.
    c.command(Command::CommentDraft {
        draft: Some(CommentDraftDto {
            page: page(&c),
            node: Some(r.clone()),
            x: 50.0,
            y: 20.0,
            wx: -250.0,
            wy: -180.0,
        }),
    });
    assert!(c.take_events().contains(&CoreEvent::CommentAnchor {
        sx: 150.0,
        sy: 120.0,
        visible: true
    }));
    c.command(Command::CommentDraft { draft: None });
    // Своя метка в мире (0, 0) — экран (400, 300): щелчок открывает, протяжка двигает.
    let p = pin(&c, "mine", 0.0, 0.0);
    c.command(Command::CommentPins {
        pins: vec![p],
        replace: true,
    });
    c.take_events();
    click(&mut c, 410.0, 290.0, 6000.0);
    let ev = c.take_events();
    assert!(ev.contains(&CoreEvent::CommentOpened {
        id: "mine".into(),
        sx: 400.0,
        sy: 300.0
    }));
    c.handle(Input::PointerDown(at(410.0, 290.0, 7000.0)));
    c.handle(Input::PointerMove(at(450.0, 290.0, 7050.0)));
    c.handle(Input::PointerMove(at(510.0, 340.0, 7100.0)));
    c.handle(Input::PointerUp(at(510.0, 340.0, 7150.0)));
    let ev = c.take_events();
    assert!(
        ev.contains(&CoreEvent::CommentMoved {
            id: "mine".into(),
            page: page(&c),
            node: None,
            x: 100.0,
            y: 50.0,
            wx: 100.0,
            wy: 50.0
        }),
        "{ev:?}"
    );
    // Точка открытой метки за пачку — только последняя.
    let anchors = ev
        .iter()
        .filter(|e| matches!(e, CoreEvent::CommentAnchor { .. }))
        .count();
    assert!(anchors <= 1, "{ev:?}");
    assert!(ev.contains(&CoreEvent::CommentAnchor {
        sx: 500.0,
        sy: 350.0,
        visible: true
    }));
}

#[test]
fn comment_commands_do_not_take_the_presence_hold() {
    let mut c = core();
    available(&mut c);
    let r = rect(&mut c);
    let p = page(&c);
    // Участник тянет прямоугольник: слой чужого превью в сцене.
    c.command(Command::Presence {
        items: vec![
            PresenceItemDto::Join {
                participant: 1,
                user: "u-1".into(),
                name: "Анна".into(),
                color: 0x0d74ce,
            },
            PresenceItemDto::Part {
                participant: 1,
                rev: 1,
                index: 0,
                parts: 1,
                ids: vec![r.clone()],
            },
            PresenceItemDto::State {
                participant: 1,
                page: p.clone(),
                view: [0.0, 0.0, 800.0, 600.0],
                cursor: None,
                selection: Some(PresenceSelectionDto {
                    rev: 1,
                    count: 1,
                    parts: 1,
                    bounds: [0.0, 0.0, 1.0, 1.0],
                }),
                gesture: Some(PresenceGestureDto {
                    kind: GestureKindDto::Move,
                    transform: [1.0, 0.0, 0.0, 1.0, 40.0, 0.0],
                    frame: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    k: 1.0,
                }),
            },
        ],
    });
    assert!(c.editor().scene().has_remote_preview());
    let x = pin(&c, "a", 0.0, 0.0);
    for cmd in [
        Command::CommentPins {
            pins: vec![x],
            replace: true,
        },
        Command::CommentOpen {
            id: Some("a".into()),
        },
        Command::CommentDraft { draft: None },
        Command::CommentShowResolved { on: true },
        Command::CommentFocus { id: "a".into() },
        Command::CommentPinsRemove {
            ids: vec!["a".into()],
        },
        Command::CommentsAvailable { on: true },
    ] {
        let before = c.editor().scene().change_counter();
        c.command(cmd.clone());
        assert_eq!(
            c.editor().scene().change_counter(),
            before,
            "слой не снимался и не накладывался: {cmd:?}"
        );
    }
    // Для сравнения: команда с удержанием слой снимает и накладывает заново.
    let before = c.editor().scene().change_counter();
    c.command(Command::Execute {
        command: "view.zoom_in".into(),
    });
    assert_ne!(c.editor().scene().change_counter(), before);
}

#[test]
fn pins_stay_out_of_document_journal_and_file_thumbnail() {
    let mut c = core();
    c.command(Command::Demo { nodes: 300 });
    c.command(Command::FlushJournal);
    c.take_events();
    let thumb = |c: &mut Core| {
        c.command(Command::Thumbnail);
        c.take_events()
            .into_iter()
            .find_map(|e| match e {
                CoreEvent::Thumbnail { bytes, .. } => bytes,
                _ => None,
            })
            .expect("превью")
    };
    let plain = thumb(&mut c);
    let (w, h) = c.device_size();
    let mut b = CpuBackend::new(w, h);
    c.draw(&mut b);
    let without = b.render();
    available(&mut c);
    // Метки по всему видимому и по всей странице.
    let center = c.editor().camera().center();
    let mut pins = vec![pin(&c, "center", center.x, center.y)];
    pins.extend((0..50).map(|i| pin(&c, &format!("p{i}"), i as f64 * 300.0, i as f64 * 200.0)));
    c.command(Command::CommentPins {
        pins,
        replace: true,
    });
    c.command(Command::SetTool {
        tool: Tool::Comment,
    });
    click(&mut c, 30.0, 30.0, 9000.0);
    let ev = c.take_events();
    assert!(
        !ev.iter().any(|e| matches!(
            e,
            CoreEvent::Document { .. } | CoreEvent::Modified { modified: true }
        )),
        "{ev:?}"
    );
    // На экране метки есть, в превью файла — нет.
    let mut b = CpuBackend::new(w, h);
    c.draw(&mut b);
    let with = b.render();
    assert_ne!(with.data_as_u8_slice(), without.data_as_u8_slice());
    assert_eq!(thumb(&mut c), plain);
    // В журнал — ничего.
    c.command(Command::FlushJournal);
    let journal = c.take_events().into_iter().find_map(|e| match e {
        CoreEvent::Journal { bytes, .. } => Some(bytes),
        _ => None,
    });
    assert_eq!(journal, Some(None));
}

#[test]
fn comment_messages_have_typescript_field_names() {
    let pin = json!({
        "id": "t1", "page": "0@1", "x": 1.0, "y": 2.0, "wx": 3.0, "wy": 4.0,
        "initials": "АБ", "color": 255, "replies": 0, "unread": false, "resolved": false,
        "movable": true
    });
    let cmd: Command =
        serde_json::from_value(json!({"type": "commentPins", "pins": [pin], "replace": true}))
            .unwrap();
    assert!(
        matches!(cmd, Command::CommentPins { ref pins, replace: true } if pins[0].node.is_none())
    );
    for good in [
        json!({"type": "commentsAvailable", "on": true}),
        json!({"type": "commentPinsRemove", "ids": ["a"]}),
        json!({"type": "commentOpen"}),
        json!({"type": "commentOpen", "id": "a"}),
        json!({"type": "commentDraft"}),
        json!({"type": "commentDraft", "draft": {"page": "0@1", "node": "2@1", "x": 0, "y": 0, "wx": 0, "wy": 0}}),
        json!({"type": "commentFocus", "id": "a"}),
        json!({"type": "commentShowResolved", "on": false}),
        json!({"type": "setTool", "tool": "comment"}),
    ] {
        assert!(
            serde_json::from_value::<Command>(good.clone()).is_ok(),
            "{good}"
        );
    }
    // Лишние поля и неверные типы — ошибка разбора.
    for bad in [
        json!({"type": "commentsAvailable", "on": true, "x": 1}),
        json!({"type": "commentFocus"}),
        json!({"type": "commentPins", "pins": [{"id": "a"}], "replace": true}),
        json!({"type": "commentDraft", "draft": {"page": "0@1", "x": 0, "y": 0, "wx": 0, "wy": 0, "z": 1}}),
    ] {
        assert!(
            serde_json::from_value::<Command>(bad.clone()).is_err(),
            "{bad}"
        );
    }
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentPlace {
            page: "0@1".into(),
            node: None,
            x: 1.0,
            y: 2.0,
            wx: 3.0,
            wy: 4.0,
            sx: 5.0,
            sy: 6.0
        })
        .unwrap(),
        json!({"type": "commentPlace", "page": "0@1", "node": null, "x": 1.0, "y": 2.0, "wx": 3.0, "wy": 4.0, "sx": 5.0, "sy": 6.0})
    );
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentsState {
            mode: true,
            visible: false,
            available: true
        })
        .unwrap(),
        json!({"type": "commentsState", "mode": true, "visible": false, "available": true})
    );
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentAnchor {
            sx: 1.0,
            sy: 2.0,
            visible: true
        })
        .unwrap(),
        json!({"type": "commentAnchor", "sx": 1.0, "sy": 2.0, "visible": true})
    );
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentOpened {
            id: "a".into(),
            sx: 1.0,
            sy: 2.0
        })
        .unwrap()["type"],
        json!("commentOpened")
    );
    assert_eq!(
        serde_json::to_value(CoreEvent::CommentMoved {
            id: "a".into(),
            page: "0@1".into(),
            node: Some("2@1".into()),
            x: 1.0,
            y: 2.0,
            wx: 3.0,
            wy: 4.0
        })
        .unwrap()["node"],
        json!("2@1")
    );
}

/// Проверки ядра, которых нет у редактора: он принял бы и конечную точку за пределом мира, и
/// длинную страницу, и любой идентификатор перехода.
#[test]
fn core_rejects_what_the_editor_would_accept() {
    let mut c = core();
    available(&mut c);
    let has_draft = |c: &Core| {
        c.editor()
            .overlay()
            .comments
            .iter()
            .any(|m| matches!(m.kind, ferrum_render::PinKind::Draft))
    };
    let draft = |wx: f64, page: String| CommentDraftDto {
        page,
        node: None,
        x: wx,
        y: 0.0,
        wx,
        wy: 0.0,
    };
    // Конечная, но за пределом мира — у редактора метка была бы.
    let d = draft(2e9, page(&c));
    c.command(Command::CommentDraft { draft: Some(d) });
    assert!(!has_draft(&c));
    let d = draft(0.0, format!("{}{}", "0".repeat(70), page(&c)));
    c.command(Command::CommentDraft { draft: Some(d) });
    assert!(!has_draft(&c));
    let d = draft(0.0, page(&c));
    c.command(Command::CommentDraft { draft: Some(d) });
    assert!(has_draft(&c), "верный черновик виден");
    c.command(Command::CommentDraft { draft: None });
    // Открыть: идентификатор длиннее 64 байт из верных знаков — не меняет открытое.
    let p = pin(&c, "t1", 0.0, 0.0);
    c.command(Command::CommentPins {
        pins: vec![p],
        replace: true,
    });
    c.command(Command::CommentOpen {
        id: Some("t1".into()),
    });
    c.command(Command::CommentOpen {
        id: Some("x".repeat(65)),
    });
    assert_eq!(c.editor().comment_open(), Some("t1"));
    c.command(Command::CommentOpen { id: None });
    // Переход: неверный идентификатор не заменяет ждущий переход — метка `late` пришла позже
    // ссылки, переход к ней случается.
    c.command(Command::CommentFocus { id: "late".into() });
    c.command(Command::CommentFocus { id: "a/b".into() });
    c.command(Command::CommentFocus { id: "y".repeat(65) });
    let p = pin(&c, "late", 50.0, 60.0);
    c.command(Command::CommentPins {
        pins: vec![p],
        replace: false,
    });
    assert_eq!(c.editor().comment_open(), Some("late"));
}
