//! Другие участники (спека подсистемы 4, раздел 9): курсоры с плавным движением, рамки
//! выделения, живое превью жестов (только сцена, никогда документ), уход, следование.

mod common;

use common::*;
use ferrum_doc::NodeId;
use ferrum_editor::*;
use kurbo::{Affine, Point, Rect};

const BLUE: u32 = 0x0d74ce;

fn state(ed: &Editor, cursor: Option<(f64, f64)>) -> PeerState {
    PeerState {
        page: Some(ed.page()),
        view: Rect::new(0.0, 0.0, 1000.0, 800.0),
        cursor: cursor.map(Point::from),
        selection: None,
        gesture: None,
    }
}

fn select(ed: &Editor, ids: &[NodeId], rev: u32) -> PeerState {
    PeerState {
        selection: Some(PeerSelection {
            rev,
            count: ids.len() as u32,
            parts: 1,
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
        }),
        ..state(ed, None)
    }
}

fn moving(ed: &Editor, ids: &[NodeId], dx: f64) -> PeerState {
    PeerState {
        gesture: Some(Gesture {
            kind: GestureKind::Move,
            transform: Affine::translate((dx, 0.0)),
            frame: Affine::IDENTITY,
            k: 1.0,
        }),
        ..select(ed, ids, 1)
    }
}

fn join(h: &mut H, id: u32) {
    h.ed.presence_join(id, &format!("u-{id}"), "Анна", peer_color(BLUE));
}

/// Участник выделил `ids` (одной частью) и тянет их на `dx`.
fn drag_by(h: &mut H, id: u32, ids: &[NodeId], dx: f64) {
    h.ed.presence_part(id, 1, 0, 1, ids.to_vec());
    let s = moving(&h.ed, ids, dx);
    h.ed.presence_state(id, s);
}

fn cursor_of(h: &H, i: usize) -> Option<Point> {
    h.ed.overlay().remote.get(i).and_then(|m| m.cursor)
}

#[test]
fn peer_cursor_is_drawn_with_name_and_colour_only_on_its_page() {
    let mut h = H::new();
    join(&mut h, 3);
    // Без состояния участника на холсте нет.
    assert!(h.ed.overlay().remote.is_empty());
    let s = state(&h.ed, Some((120.0, 40.0)));
    h.ed.presence_state(3, s);
    let o = h.ed.overlay();
    assert_eq!(o.remote.len(), 1);
    assert_eq!(&*o.remote[0].name, "Анна");
    assert_eq!(o.remote[0].color, peer_color(BLUE));
    assert_eq!(o.remote[0].cursor, Some(Point::new(120.0, 40.0)));
    // Другая страница или неизвестная — курсора не видно, участник в списке есть.
    let other = h.ed.add_page("Вторая").unwrap();
    let mut s = state(&h.ed, Some((1.0, 1.0)));
    s.page = Some(other);
    h.ed.presence_state(3, s.clone());
    assert!(h.ed.overlay().remote.is_empty());
    s.page = None;
    h.ed.presence_state(3, s);
    assert!(h.ed.overlay().remote.is_empty());
    assert_eq!(h.ed.peers().len(), 1);
    // Без `join` состояние не принимается: имя и цвет знает только сервер.
    let s = state(&h.ed, Some((5.0, 5.0)));
    h.ed.presence_state(9, s);
    assert_eq!(h.ed.peers().len(), 1);
}

#[test]
fn cursor_moves_smoothly_between_peer_frames() {
    let mut h = H::new();
    h.ed.set_time(0.0);
    join(&mut h, 1);
    let s = state(&h.ed, Some((0.0, 0.0)));
    h.ed.presence_state(1, s);
    h.ed.set_time(50.0);
    h.ed.take_events();
    let s = state(&h.ed, Some((100.0, 0.0)));
    h.ed.presence_state(1, s);
    assert_eq!(
        cursor_of(&h, 0),
        Some(Point::new(0.0, 0.0)),
        "движение начинается с места"
    );
    h.ed.set_time(75.0);
    let mid = cursor_of(&h, 0).unwrap();
    assert!((mid.x - 50.0).abs() < 1e-9, "середина пути: {mid:?}");
    assert!(
        h.ed.take_events().contains(&EditorEvent::NeedsRedraw),
        "движение просит кадры"
    );
    h.ed.set_time(100.0);
    assert_eq!(cursor_of(&h, 0), Some(Point::new(100.0, 0.0)));
    h.ed.set_time(120.0);
    h.ed.take_events();
    h.ed.set_time(140.0);
    assert!(
        !h.ed.take_events().contains(&EditorEvent::NeedsRedraw),
        "стоящий курсор кадров не просит"
    );
    // Указатель ушёл с холста — курсора нет.
    let s = state(&h.ed, None);
    h.ed.presence_state(1, s);
    assert_eq!(h.ed.overlay().remote[0].cursor, None);
}

#[test]
fn silent_peer_vanishes_after_30_seconds_and_left_at_once() {
    let mut h = H::new();
    h.ed.set_time(1000.0);
    join(&mut h, 1);
    join(&mut h, 2);
    let s = state(&h.ed, Some((1.0, 1.0)));
    h.ed.presence_state(1, s.clone());
    h.ed.presence_state(2, s);
    h.ed.set_time(1000.0 + PEER_STALE_MS - 1.0);
    assert_eq!(h.ed.peers().len(), 2);
    let s = state(&h.ed, Some((2.0, 2.0)));
    h.ed.presence_state(2, s);
    h.ed.set_time(1000.0 + PEER_STALE_MS + 1.0);
    assert_eq!(
        h.ed.peers().iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![2]
    );
    h.ed.presence_left(2);
    assert!(h.ed.peers().is_empty());
    assert!(h.ed.overlay().remote.is_empty());
}

#[test]
fn reconnect_reset_drops_everyone() {
    let mut h = H::new();
    join(&mut h, 1);
    let s = state(&h.ed, Some((1.0, 1.0)));
    h.ed.presence_state(1, s);
    h.ed.presence_reset();
    assert!(h.ed.peers().is_empty());
}

#[test]
fn selection_frame_follows_nodes_and_waits_for_all_parts() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    let b = h.rect(None, 200.0, 100.0, 30.0, 30.0);
    join(&mut h, 1);
    let mut s = select(&h.ed, &[a, b], 4);
    s.selection.as_mut().unwrap().parts = 2;
    s.selection.as_mut().unwrap().bounds = Rect::new(1.0, 2.0, 3.0, 4.0);
    h.ed.presence_state(1, s);
    // Части не пришли: рамка из кадра.
    let m = h.ed.overlay().remote[0].clone();
    assert!(m.boxes.is_empty());
    assert_eq!(m.frame, Some(Rect::new(1.0, 2.0, 3.0, 4.0)));
    h.ed.presence_part(1, 4, 0, 2, vec![a]);
    assert!(h.ed.overlay().remote[0].boxes.is_empty());
    h.ed.presence_part(1, 4, 1, 2, vec![b]);
    let m = h.ed.overlay().remote[0].clone();
    assert_eq!(m.boxes.len(), 2);
    assert_eq!(m.frame, Some(Rect::new(10.0, 10.0, 230.0, 130.0)));
    // Узел сдвинулся — рамка за ним.
    h.set(b, patch(300.0, 100.0, 30.0, 30.0));
    assert_eq!(
        h.ed.overlay().remote[0].frame,
        Some(Rect::new(10.0, 10.0, 330.0, 130.0))
    );
    // Новая версия: прежние части не годятся.
    let s = select(&h.ed, &[a], 5);
    h.ed.presence_state(1, s);
    assert!(h.ed.overlay().remote[0].boxes.is_empty());
    h.ed.presence_part(1, 5, 0, 1, vec![a]);
    let m = h.ed.overlay().remote[0].clone();
    assert_eq!(m.boxes.len(), 1);
    assert_eq!(m.frame, None, "у одного узла рамка — его собственная");
}

#[test]
fn live_preview_moves_scene_never_document_and_ends_with_gesture_or_peer() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    let b = h.rect(None, 200.0, 100.0, 30.0, 30.0);
    let before = h.state();
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a, b], 100.0);
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 110.0);
    assert_eq!(h.ed.scene().node(b).unwrap().props.x, 300.0);
    assert_eq!(h.state(), before, "документ не тронут");
    // Рамка участника — по сдвинутым узлам.
    assert_eq!(h.ed.overlay().remote[0].frame.unwrap().x0, 110.0);
    // Жест кончился (или отменён): превью снято.
    let s = select(&h.ed, &[a, b], 1);
    h.ed.presence_state(1, s);
    assert!(!h.ed.scene().has_remote_preview());
    h.assert_scene_clean();
    // Ушёл посреди жеста — превью снято сразу.
    drag_by(&mut h, 1, &[a, b], 50.0);
    assert!(h.ed.scene().has_remote_preview());
    h.ed.presence_left(1);
    assert!(!h.ed.scene().has_remote_preview());
    h.assert_scene_clean();
    assert_eq!(h.state(), before);
}

#[test]
fn own_drag_of_a_node_under_peer_preview_writes_only_own_move() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a], 100.0);
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 110.0);
    // Нажатие там, где узел виден (под чужим превью), и перетаскивание на 20: во время своего
    // жеста чужого превью нет, записывается только свой сдвиг от документа.
    h.down(130.0, 30.0, none());
    assert!(
        !h.ed.scene().has_remote_preview(),
        "свой жест — чужое превью снято"
    );
    assert_eq!(h.sel(), vec![a]);
    for x in [135.0, 140.0, 150.0] {
        h.mv(x, 30.0, none());
    }
    assert_eq!(
        h.ed.scene().node(a).unwrap().props.x,
        30.0,
        "превью жеста — от документа"
    );
    h.up(150.0, 30.0, none());
    assert_eq!(h.props(a).x, 30.0, "в документе — только свой сдвиг");
    // После своего жеста чужое превью возвращается — сдвиг участника от нового документа.
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 130.0);
    // Стрелка клавиатуры и поле панели — тоже без чужого сдвига.
    h.ed.select(&[a]);
    h.key("ArrowRight", none());
    assert_eq!(h.props(a).x, 31.0);
    h.ed.transact(|tx| {
        tx.set_props(
            a,
            &ferrum_doc::PropsPatch {
                w: Some(70.0),
                ..Default::default()
            },
        )?;
        Ok(vec![a])
    })
    .unwrap();
    assert_eq!(h.props(a).x, 31.0);
    assert_eq!(h.props(a).w, 70.0);
}

#[test]
fn hold_removes_peer_preview_until_release() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a], 100.0);
    h.ed.presence_hold();
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 10.0);
    // Кадр участника во время своего действия слой не возвращает.
    drag_by(&mut h, 1, &[a], 120.0);
    assert!(!h.ed.scene().has_remote_preview());
    h.ed.presence_release();
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 130.0);
}

#[test]
fn rotate_and_resize_previews_use_own_gesture_maths() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 100.0, 50.0);
    join(&mut h, 1);
    h.ed.presence_part(1, 1, 0, 1, vec![a]);
    let mut s = select(&h.ed, &[a], 1);
    s.gesture = Some(Gesture {
        kind: GestureKind::Resize,
        transform: Affine::scale_non_uniform(2.0, 1.0),
        frame: Affine::IDENTITY,
        k: 1.0,
    });
    h.ed.presence_state(1, s.clone());
    assert_eq!(h.ed.scene().node(a).unwrap().props.w, 200.0);
    s.gesture = Some(Gesture {
        kind: GestureKind::Rotate,
        transform: Affine::rotate_about(-std::f64::consts::FRAC_PI_2, Point::new(50.0, 25.0)),
        frame: Affine::IDENTITY,
        k: 1.0,
    });
    h.ed.presence_state(1, s);
    assert!((h.ed.scene().node(a).unwrap().props.rotation - 90.0).abs() < 1e-6);
    assert_eq!(h.props(a).rotation, 0.0);
}

#[test]
fn follow_copies_page_and_view_and_ends_on_own_action_or_departure() {
    let mut h = H::new();
    let second = h.ed.add_page("Вторая").unwrap();
    let first = h.ed.page();
    join(&mut h, 1);
    let mut s = state(&h.ed, Some((0.0, 0.0)));
    s.view = Rect::new(1000.0, 1000.0, 1500.0, 1400.0);
    s.page = Some(second);
    h.ed.presence_state(1, s.clone());
    h.ed.take_events();
    h.ed.follow(Some(1));
    assert_eq!(h.ed.following(), Some(1));
    assert!(h
        .ed
        .take_events()
        .contains(&EditorEvent::Following(Some(1))));
    assert_eq!(h.ed.page(), second, "страница участника");
    let v = h.ed.camera().visible_world_rect();
    assert!(v.contains(Point::new(1001.0, 1001.0)) && v.contains(Point::new(1499.0, 1399.0)));
    assert!((v.center() - Point::new(1250.0, 1200.0)).hypot() < 1e-6);
    // Участник ушёл на другую страницу — за ним.
    s.page = Some(first);
    h.ed.presence_state(1, s.clone());
    assert_eq!(h.ed.page(), first);
    assert_eq!(
        h.ed.following(),
        Some(1),
        "смена страницы следованием — не своё действие"
    );
    // Движение указателя — не действие; нажатие — выход.
    h.hover(10.0, 10.0);
    assert_eq!(h.ed.following(), Some(1));
    h.down(10.0, 10.0, none());
    assert_eq!(h.ed.following(), None);
    assert!(h.ed.take_events().contains(&EditorEvent::Following(None)));
    h.up(10.0, 10.0, none());
    // Колесо и своя смена страницы — тоже выход; уход участника — конец.
    h.ed.follow(Some(1));
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(1.0, 1.0),
        delta: kurbo::Vec2::new(0.0, 10.0),
        modifiers: none(),
    }));
    assert_eq!(h.ed.following(), None);
    h.ed.follow(Some(1));
    h.ed.set_page(second).unwrap();
    assert_eq!(h.ed.following(), None);
    h.ed.follow(Some(1));
    h.ed.take_events();
    h.ed.presence_left(1);
    assert_eq!(h.ed.following(), None);
    assert!(h.ed.take_events().contains(&EditorEvent::Following(None)));
    // За неизвестным не следуют.
    h.ed.follow(Some(42));
    assert_eq!(h.ed.following(), None);
}

#[test]
fn own_presence_reports_cursor_selection_and_gesture() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    assert_eq!(h.ed.own_presence().cursor, None, "указателя ещё не было");
    h.hover(300.0, 200.0);
    let own = h.ed.own_presence();
    assert_eq!(own.cursor, Some(Point::new(300.0, 200.0)));
    assert_eq!(own.page, h.ed.page());
    assert_eq!(own.view, h.ed.camera().visible_world_rect());
    h.ev(Input::PointerLeave);
    assert_eq!(h.ed.own_presence().cursor, None);
    let v = h.ed.own_presence().selection_version;
    h.drag_hold((30.0, 30.0), (70.0, 30.0), none());
    let own = h.ed.own_presence();
    assert_ne!(own.selection_version, v);
    let g = own.gesture.expect("перенос виден другим");
    assert_eq!(g.kind, GestureKind::Move);
    assert_eq!(g.transform.translation().x, 40.0);
    h.up(70.0, 30.0, none());
    assert_eq!(h.ed.own_presence().gesture, None);
    assert_eq!(h.props(a).x, 50.0);
}
