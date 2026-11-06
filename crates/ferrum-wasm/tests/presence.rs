//! Присутствие в ядре worker'а (спека подсистемы 4, раздел 9): записи сервера — редактору с
//! проверкой, своё состояние — не чаще 20 раз в секунду и только при изменении, чужое превью
//! не попадает в документ ни через команду, ни через ввод.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerEvent};
use ferrum_wasm::core::presence::{encode_selection, PRESENCE_PART_BYTES};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

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

fn world(c: &Core, x: f64, y: f64) -> kurbo::Point {
    c.editor().camera().screen_to_world(kurbo::Point::new(x, y))
}

fn page(c: &Core) -> String {
    c.editor().page().to_string()
}

/// Прямоугольник инструментом; возвращает его идентификатор.
fn rect(c: &mut Core) -> String {
    let at = |x: f64, y: f64, t: f64| PointerEvent::new((x, y), Modifiers::NONE, t);
    c.command(Command::SetTool { tool: Tool::Rect });
    c.handle(Input::PointerDown(at(100.0, 100.0, 1000.0)));
    c.handle(Input::PointerMove(at(200.0, 200.0, 1100.0)));
    c.handle(Input::PointerUp(at(200.0, 200.0, 1200.0)));
    c.take_events();
    c.editor().selection()[0].to_string()
}

fn join(id: u32) -> PresenceItemDto {
    PresenceItemDto::Join {
        participant: id,
        user: format!("u-{id}"),
        name: "Анна".into(),
        color: 0x0d74ce,
    }
}

fn st(c: &Core, id: u32, cursor: [f64; 2]) -> PresenceItemDto {
    PresenceItemDto::State {
        participant: id,
        page: page(c),
        view: [0.0, 0.0, 800.0, 600.0],
        cursor: Some(cursor),
        selection: None,
        gesture: None,
    }
}

fn drag(c: &Core, id: u32, node: &str, dx: f64) -> Vec<PresenceItemDto> {
    vec![
        PresenceItemDto::Part {
            participant: id,
            rev: 1,
            index: 0,
            parts: 1,
            ids: vec![node.into()],
        },
        PresenceItemDto::State {
            participant: id,
            page: page(c),
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
                transform: [1.0, 0.0, 0.0, 1.0, dx, 0.0],
                frame: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                k: 1.0,
            }),
        },
    ]
}

fn x_of(c: &mut Core, id: &str) -> (f64, f64) {
    let node = id.parse().unwrap();
    let shown = c.editor().scene().node(node).unwrap().props.x;
    (shown, x_in_doc(c, id))
}

fn x_in_doc(c: &mut Core, id: &str) -> f64 {
    let node = id.parse().unwrap();
    let page = c.editor().page();
    c.editor()
        .document()
        .loaded_page(page)
        .unwrap()
        .props(node)
        .unwrap()
        .x
}

#[test]
fn server_items_reach_the_editor_and_bad_ones_are_skipped_alone() {
    let mut c = core();
    let p = page(&c);
    let mut bad_view = st(&c, 1, [1.0, 1.0]);
    if let PresenceItemDto::State { view, .. } = &mut bad_view {
        *view = [0.0, 0.0, f64::NAN, 1.0];
    }
    c.command(Command::Presence {
        items: vec![
            join(1),
            bad_view,
            // Без `join` участника нет.
            st(&c, 2, [3.0, 3.0]),
            PresenceItemDto::State {
                participant: 1,
                page: p.clone(),
                view: [0.0, 0.0, 800.0, 600.0],
                cursor: Some([1e12, 0.0]),
                selection: None,
                gesture: None,
            },
            PresenceItemDto::Part {
                participant: 1,
                rev: 1,
                index: 1,
                parts: 1,
                ids: vec!["1@2".into()],
            },
            PresenceItemDto::Part {
                participant: 1,
                rev: 1,
                index: 0,
                parts: 17,
                ids: vec!["1@2".into()],
            },
            PresenceItemDto::Part {
                participant: 1,
                rev: 1,
                index: 0,
                parts: 1,
                ids: vec!["мусор".into()],
            },
        ],
    });
    assert_eq!(c.editor().peers().len(), 1);
    assert!(
        c.editor().overlay().remote.is_empty(),
        "ни одно неверное состояние не принято"
    );
    c.command(Command::Presence {
        items: vec![st(&c, 1, [40.0, 50.0])],
    });
    let o = c.editor().overlay();
    assert_eq!(o.remote.len(), 1);
    assert_eq!(o.remote[0].cursor, Some(kurbo::Point::new(40.0, 50.0)));
    c.command(Command::Presence {
        items: vec![PresenceItemDto::Left { participant: 1 }],
    });
    assert!(c.editor().peers().is_empty());
}

#[test]
fn participants_are_capped() {
    let mut c = core();
    let items = (0..PRESENCE_PEERS_MAX as u32 + 10).map(join).collect();
    c.command(Command::Presence { items });
    assert_eq!(c.editor().peers().len(), PRESENCE_PEERS_MAX);
}

/// Своё состояние: только после начала синхронизации, не чаще 50 мс, только при изменении.
#[test]
fn own_presence_is_sent_on_change_at_most_20_times_a_second() {
    let mut c = core();
    let presence = |c: &mut Core| {
        c.take_events()
            .into_iter()
            .filter(|e| matches!(e, CoreEvent::Presence { .. }))
            .collect::<Vec<_>>()
    };
    c.set_time(0.0);
    assert!(presence(&mut c).is_empty(), "без синхронизации — молчит");
    c.command(Command::SyncStart);
    c.set_time(10.0);
    assert_eq!(presence(&mut c).len(), 1);
    c.set_time(100.0);
    assert!(presence(&mut c).is_empty(), "без изменений — молчит");
    let at = |x: f64| PointerEvent::new((x, 10.0), Modifiers::NONE, 0.0);
    c.handle(Input::PointerMove(at(10.0)));
    c.set_time(120.0);
    let sent = presence(&mut c);
    let x10 = world(&c, 10.0, 10.0).x;
    assert!(matches!(&sent[..], [CoreEvent::Presence { cursor: Some([x, _]), .. }] if *x == x10));
    c.handle(Input::PointerMove(at(20.0)));
    c.set_time(140.0);
    assert!(presence(&mut c).is_empty(), "раньше 50 мс — ждёт");
    c.set_time(175.0);
    let x20 = world(&c, 20.0, 10.0).x;
    assert!(
        matches!(&presence(&mut c)[..], [CoreEvent::Presence { cursor: Some([x, _]), .. }] if *x == x20)
    );
    c.handle(Input::PointerLeave);
    c.set_time(240.0);
    assert!(matches!(
        &presence(&mut c)[..],
        [CoreEvent::Presence { cursor: None, .. }]
    ));
}

#[test]
fn own_selection_goes_with_parts_once_per_version() {
    let mut c = core();
    c.command(Command::SyncStart);
    let id = rect(&mut c);
    c.set_time(1000.0);
    let events = c.take_events();
    let (sel, parts) = events
        .iter()
        .find_map(|e| match e {
            CoreEvent::Presence {
                selection, parts, ..
            } => Some((*selection, parts.clone())),
            _ => None,
        })
        .unwrap();
    let sel = sel.expect("выделение");
    assert_eq!((sel.count, sel.parts), (1, 1));
    let (a, b) = (world(&c, 100.0, 100.0), world(&c, 200.0, 200.0));
    assert_eq!(sel.bounds, [a.x, a.y, b.x, b.y]);
    let parts = parts.expect("части с новой версией");
    assert_eq!(
        parts[0].to_vec(),
        encode_selection(&[id.parse().unwrap()])[0]
    );
    // Рамка сдвинулась, версия та же: частей нет.
    let at = |x: f64, y: f64, t: f64| PointerEvent::new((x, y), Modifiers::NONE, t);
    c.handle(Input::PointerDown(at(150.0, 150.0, 2000.0)));
    c.handle(Input::PointerMove(at(170.0, 150.0, 2100.0)));
    c.set_time(2000.0);
    let again = c.take_events();
    let found = again.iter().find_map(|e| match e {
        CoreEvent::Presence {
            selection,
            parts,
            gesture,
            ..
        } => Some((*selection, parts.is_some(), *gesture)),
        _ => None,
    });
    let (sel2, has_parts, gesture) = found.unwrap();
    assert_eq!(sel2.unwrap().rev, sel.rev);
    assert!(!has_parts);
    assert_eq!(gesture.unwrap().kind, GestureKindDto::Move);
}

/// Совпадает с `packages/protocol/test/presence.test.ts` («узлы выделения: запись одна»).
#[test]
fn selection_encoding_matches_protocol() {
    let ids: Vec<_> = [
        "5@2",
        "1@2",
        "1@18446744073709551615",
        "2147483647@3",
        "7@2",
    ]
    .iter()
    .map(|s| s.parse().unwrap())
    .collect();
    let parts = encode_selection(&ids);
    assert_eq!(parts.len(), 1);
    let mut want = Vec::new();
    want.extend_from_slice(&2u64.to_le_bytes());
    want.extend_from_slice(&[3, 1, 4, 2]);
    want.extend_from_slice(&3u64.to_le_bytes());
    want.extend_from_slice(&[1, 0xff, 0xff, 0xff, 0xff, 0x07]);
    want.extend_from_slice(&u64::MAX.to_le_bytes());
    want.extend_from_slice(&[1, 1]);
    assert_eq!(parts[0], want);
}

#[test]
fn big_selection_is_split_into_bounded_parts() {
    let ids: Vec<_> = (0..40_000u64)
        .map(|i| {
            format!("{}@{}", (i * 7919) % 2_000_000_000, i % 20)
                .parse()
                .unwrap()
        })
        .collect();
    let parts = encode_selection(&ids);
    assert_eq!(parts.len(), 16);
    assert!(parts
        .iter()
        .all(|p| p.len() <= PRESENCE_PART_BYTES && !p.is_empty()));
}

#[test]
fn peer_preview_never_reaches_document_through_commands_or_input() {
    let mut c = core();
    let id = rect(&mut c);
    c.command(Command::Presence {
        items: [vec![join(1)], drag(&c, 1, &id, 300.0)].concat(),
    });
    let x0 = world(&c, 100.0, 100.0).x;
    assert_eq!(x_of(&mut c, &id), (x0 + 300.0, x0));
    // Поле панели «X»: +10 к документу, а не к превью.
    c.command(Command::EditSelection {
        edit: Edit::Number {
            field: Field::X,
            value: 10.0,
            relative: true,
        },
        phase: EditPhase::Commit,
        ids: None,
        selection_version: None,
    });
    assert_eq!(x_in_doc(&mut c, &id), x0 + 10.0);
    assert_eq!(
        x_of(&mut c, &id).0,
        x0 + 310.0,
        "превью вернулось поверх нового документа"
    );
    // Клавиши: стрелка сдвигает документ на 1.
    c.handle(Input::KeyDown(ferrum_editor::KeyEvent::new(
        "ArrowRight",
        Modifiers::NONE,
    )));
    c.handle(Input::KeyUp(ferrum_editor::KeyEvent::new(
        "ArrowRight",
        Modifiers::NONE,
    )));
    assert_eq!(x_in_doc(&mut c, &id), x0 + 11.0);
    // Отмена и повтор — свои шаги, без чужого сдвига.
    c.command(Command::Execute {
        command: "edit.undo".into(),
    });
    assert_eq!(x_in_doc(&mut c, &id), x0 + 10.0);
    // Участник ушёл — сцена совпадает с документом.
    c.command(Command::Presence {
        items: vec![PresenceItemDto::Left { participant: 1 }],
    });
    assert_eq!(x_of(&mut c, &id), (x0 + 10.0, x0 + 10.0));
}

#[test]
fn following_is_reported_to_the_interface() {
    let mut c = core();
    c.command(Command::Presence {
        items: vec![join(4), st(&c, 4, [1.0, 1.0])],
    });
    c.take_events();
    c.command(Command::Follow {
        participant: Some(4),
    });
    let ev = c.take_events();
    assert!(ev.contains(&CoreEvent::Following {
        participant: Some(4)
    }));
    c.handle(Input::PointerDown(PointerEvent::new(
        (5.0, 5.0),
        Modifiers::NONE,
        10.0,
    )));
    assert!(c
        .take_events()
        .contains(&CoreEvent::Following { participant: None }));
}

/// Рисует инструментом `tool` от `from` до `to` (экран); возвращает выделенный узел.
fn draw(c: &mut Core, tool: Tool, from: (f64, f64), to: (f64, f64)) -> String {
    let at = |p: (f64, f64), t: f64| PointerEvent::new(p, Modifiers::NONE, t);
    c.command(Command::SetTool { tool });
    c.handle(Input::PointerDown(at(from, 5000.0)));
    c.handle(Input::PointerMove(at(to, 5100.0)));
    c.handle(Input::PointerUp(at(to, 5200.0)));
    c.take_events();
    c.editor().selection()[0].to_string()
}

fn parent_in_doc(c: &Core, id: &str) -> Option<String> {
    let page = c.editor().page();
    c.editor()
        .document()
        .loaded_page(page)
        .unwrap()
        .parent(id.parse().unwrap())
        .map(|p| p.to_string())
}

/// Ввод — своё действие: нажатие инструмента под чужим перетаскиванием фрейма берёт фрейм и
/// его положение из документа, а не из превью.
#[test]
fn drawing_under_peer_frame_drag_is_placed_by_document() {
    let mut c = core();
    let frame = draw(&mut c, Tool::Frame, (100.0, 100.0), (300.0, 300.0));
    c.command(Command::Presence {
        items: [vec![join(1)], drag(&c, 1, &frame, 400.0)].concat(),
    });
    // Фрейм виден у экранных 500…700; прямоугольник рисуется там.
    let r = draw(&mut c, Tool::Rect, (550.0, 150.0), (600.0, 200.0));
    assert_eq!(parent_in_doc(&c, &r), None, "в документе фрейма там нет");
    assert_eq!(x_in_doc(&mut c, &r), world(&c, 550.0, 150.0).x);
    let x0 = world(&c, 100.0, 100.0).x;
    assert_eq!(x_in_doc(&mut c, &frame), x0);
}

fn presence_events(c: &mut Core) -> Vec<CoreEvent> {
    c.take_events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::Presence { .. }))
        .collect()
}

fn sent_selection(e: &CoreEvent) -> (Option<PresenceSelectionDto>, Option<Vec<Vec<u8>>>) {
    match e {
        CoreEvent::Presence {
            selection, parts, ..
        } => (
            *selection,
            parts
                .as_ref()
                .map(|p| p.iter().map(|b| b.to_vec()).collect()),
        ),
        _ => unreachable!(),
    }
}

/// Части выделения уходят, когда о нём впервые можно сказать: у выделенной группы без видимых
/// детей рамки нет — части ждут её, а не теряются.
#[test]
fn selection_parts_wait_for_bounds_instead_of_being_lost() {
    let mut c = core();
    c.command(Command::SyncStart);
    let a = rect(&mut c);
    c.command(Command::Execute {
        command: "object.group".into(),
    });
    let group = c.editor().selection()[0].to_string();
    c.command(Command::SetLayerFlags {
        ids: vec![a.clone()],
        visible: Some(false),
        locked: None,
    });
    c.command(Command::Select {
        ids: vec![],
        mode: SelectMode::Replace,
    });
    c.set_time(1000.0);
    c.take_events();
    // Новая версия выделения — группа без рамки: о выделении не сказать.
    c.command(Command::Select {
        ids: vec![group.clone()],
        mode: SelectMode::Replace,
    });
    c.set_time(2000.0);
    let sent = presence_events(&mut c);
    assert!(sent.iter().all(|e| sent_selection(e).0.is_none()));
    // Ребёнок снова виден — рамка есть, части уходят с ней.
    c.command(Command::SetLayerFlags {
        ids: vec![a],
        visible: Some(true),
        locked: None,
    });
    c.set_time(3000.0);
    let sent = presence_events(&mut c);
    let (sel, parts) = sent_selection(sent.last().expect("выделение объявлено"));
    assert_eq!(sel.expect("выделение").parts, 1);
    assert_eq!(
        parts.expect("части той же версии"),
        encode_selection(&[group.parse().unwrap()])
    );
}

/// Части не теряются, если кадр не ушёл: вид вне мира (холст уведён за предел) — кадра нет,
/// части — со следующим.
#[test]
fn selection_parts_are_kept_until_a_frame_actually_goes() {
    let mut c = core();
    c.command(Command::SyncStart);
    let id = rect(&mut c);
    let far = PointerEvent::new((10.0, 10.0), Modifiers::NONE, 0.0);
    let wheel = |dy: f64| {
        Input::Wheel(ferrum_editor::WheelEvent {
            point: far.point,
            delta: kurbo::Vec2::new(0.0, dy),
            modifiers: Modifiers::NONE,
        })
    };
    c.handle(wheel(2e9));
    c.set_time(1000.0);
    assert!(
        presence_events(&mut c).is_empty(),
        "вид вне мира — кадра нет"
    );
    c.handle(wheel(-2e9));
    c.set_time(2000.0);
    let sent = presence_events(&mut c);
    let (sel, parts) = sent_selection(sent.last().expect("кадр ушёл"));
    assert!(sel.is_some());
    assert_eq!(
        parts.expect("части с первым ушедшим кадром"),
        encode_selection(&[id.parse().unwrap()])
    );
}

/// Новый файл — своё присутствие с нуля: версия выделения и части прошлого файла не мешают
/// объявить выделение нового.
#[test]
fn opening_a_file_resets_own_presence() {
    let mut c = core();
    c.command(Command::SyncStart);
    rect(&mut c);
    c.set_time(1000.0);
    assert!(sent_selection(&presence_events(&mut c)[0]).1.is_some());
    c.command(Command::Demo { nodes: 0 });
    c.command(Command::SyncStart);
    let id = rect(&mut c);
    c.set_time(2000.0);
    let sent = presence_events(&mut c);
    let (sel, parts) = sent_selection(sent.last().expect("кадр нового файла"));
    assert!(sel.is_some());
    assert_eq!(
        parts.expect("части нового выделения"),
        encode_selection(&[id.parse().unwrap()])
    );
}

/// Своя рамка выделения — по документу: участник, тянущий мои узлы, её не сдвигает.
#[test]
fn own_selection_frame_ignores_peer_preview() {
    let mut c = core();
    c.command(Command::SyncStart);
    let id = rect(&mut c);
    c.set_time(1000.0);
    let (sel, _) = sent_selection(&presence_events(&mut c)[0]);
    let bounds = sel.unwrap().bounds;
    c.command(Command::Presence {
        items: [vec![join(1)], drag(&c, 1, &id, 300.0)].concat(),
    });
    assert!(c.editor().scene().has_remote_preview());
    c.handle(Input::PointerMove(PointerEvent::new(
        (20.0, 20.0),
        Modifiers::NONE,
        0.0,
    )));
    c.set_time(2000.0);
    let sent = presence_events(&mut c);
    let (sel, _) = sent_selection(sent.last().expect("курсор сдвинулся — кадр"));
    assert_eq!(sel.unwrap().bounds, bounds);
}

/// Коэффициент «Масштаба» из сети: не число или не больше нуля — кадр пропускается.
#[test]
fn gesture_with_bad_scale_factor_is_skipped() {
    let mut c = core();
    let id = rect(&mut c);
    for k in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut items = [vec![join(1)], drag(&c, 1, &id, 300.0)].concat();
        if let PresenceItemDto::State {
            gesture: Some(g), ..
        } = &mut items[2]
        {
            g.kind = GestureKindDto::Scale;
            g.k = k;
        }
        c.command(Command::Presence { items });
        assert!(
            !c.editor().scene().has_remote_preview(),
            "k = {k}: превью нет"
        );
        assert!(c.editor().overlay().remote.is_empty(), "k = {k}");
        c.command(Command::Presence {
            items: vec![PresenceItemDto::Left { participant: 1 }],
        });
    }
}
