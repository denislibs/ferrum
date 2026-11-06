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
    // Наведение и нажатие — по тому, что видно (как в Figma): узел у 130. Нажатие — своё
    // действие: чужой слой снимается, и перетаскивание на 20 записывает только свой сдвиг от
    // документа.
    h.hover(130.0, 30.0);
    assert_eq!(h.ed.overlay().hover, Some(a), "наведение — по виду");
    h.down(130.0, 30.0, none());
    assert!(
        !h.ed.scene().has_remote_preview(),
        "свой жест — чужое превью снято"
    );
    assert_eq!(h.sel(), vec![a], "нажатие — тот же узел, что и наведение");
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

#[test]
fn panel_edit_under_peer_drag_writes_only_own_change() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    h.ed.select(&[a]);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a], 100.0);
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 110.0);
    let x5 = PanelEdit::Number {
        field: NumField::X,
        value: 5.0,
        relative: true,
    };
    // Перетаскивание подписи поля X: превью и запись — от документа, без чужого сдвига.
    h.ed.edit_selection(&x5, Phase::Preview);
    assert_eq!(
        h.ed.scene().node(a).unwrap().props.x,
        15.0,
        "превью поля — от документа"
    );
    h.ed.edit_selection(&x5, Phase::Commit);
    assert_eq!(h.props(a).x, 15.0, "в документе — только своя правка");
    assert_eq!(
        h.ed.scene().node(a).unwrap().props.x,
        115.0,
        "чужое превью вернулось поверх нового документа"
    );
    // Ввод в поле — одна запись без превью.
    h.ed.edit_selection(&x5, Phase::Commit);
    assert_eq!(h.props(a).x, 20.0);
    // Те же поля по заданным узлам.
    h.ed.edit_nodes(&x5, Phase::Commit, &[a]);
    assert_eq!(h.props(a).x, 25.0);
}

#[test]
fn copy_under_peer_drag_takes_document_position() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    h.ed.select(&[a]);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a], 100.0);
    let bytes = h.ed.copy().expect("есть что копировать");
    assert_eq!(
        h.ed.scene().node(a).unwrap().props.x,
        110.0,
        "после копии чужой слой — на месте"
    );
    h.ed.paste_with(&bytes, PasteMode::Over).unwrap();
    let pasted = h.sel();
    assert_eq!(pasted.len(), 1);
    assert_ne!(pasted[0], a);
    assert_eq!(h.props(pasted[0]).x, 10.0, "копия — с места в документе");
}

#[test]
fn text_style_edit_under_peer_drag_keeps_document_position() {
    let mut h = common::text::text_h();
    let t = common::text::add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.select(&[t]);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[t], 100.0);
    let size = |v: f64| TextStylePatch {
        size: Some(v),
        ..Default::default()
    };
    // Выбор в списке — одна запись; перетаскивание — превью, затем запись.
    h.ed.edit_text_style(&size(32.0), Phase::Commit, None)
        .unwrap();
    assert_eq!(h.props(t).text.unwrap().props.style.size, 32.0);
    h.ed.edit_text_style(&size(40.0), Phase::Preview, None)
        .unwrap();
    h.ed.edit_text_style(&size(40.0), Phase::Commit, None)
        .unwrap();
    assert_eq!(h.props(t).x, 100.0);
    assert_eq!(h.props(t).text.unwrap().props.style.size, 40.0);
    let center = TextPropsPatch {
        align_h: Some(ferrum_doc::AlignH::Center),
        ..Default::default()
    };
    h.ed.edit_text_props(&center, Phase::Commit, None).unwrap();
    assert_eq!(h.props(t).x, 100.0);
}

/// (поведение) Рисование там, где фрейм виден под чужим перетаскиванием: родитель — по
/// документу в точке мира, где начата фигура. Фрейм режет содержимое, и фигура в его осях
/// по документу была бы невидимой и во время рисования, и после отпускания участника.
#[test]
fn drawing_under_peer_drag_of_a_frame_is_placed_where_drawn() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 300.0);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[f], 500.0);
    assert_eq!(h.ed.scene().node(f).unwrap().props.x, 500.0);
    // Фрейм виден у 500…900, в документе — у 0…400: в точке 600 его по документу нет.
    h.ed.set_tool(ToolKind::Rect);
    h.drag((600.0, 100.0), (650.0, 150.0), none());
    let r = h.sel()[0];
    assert_eq!(h.parent(r), None, "родитель — по документу в точке мира");
    assert_eq!((h.props(r).x, h.props(r).y), (600.0, 100.0));
    assert_eq!((h.props(r).w, h.props(r).h), (50.0, 50.0));
    assert_eq!(h.props(f).x, 0.0);
    // Во фрейме по документу (у 100) — фигура в нём, в его осях.
    h.ed.set_tool(ToolKind::Rect);
    h.drag((100.0, 100.0), (150.0, 150.0), none());
    let r = h.sel()[0];
    assert_eq!(h.parent(r), Some(f));
    assert_eq!((h.props(r).x, h.props(r).y), (100.0, 100.0));
}

/// (поведение) То же у инструмента «Текст».
#[test]
fn text_tool_under_peer_drag_of_a_frame_is_placed_where_clicked() {
    let mut h = common::text::text_h();
    let f = h.frame(None, 0.0, 0.0, 400.0, 300.0);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[f], 500.0);
    h.ed.set_tool(ToolKind::Text);
    h.click(600.0, 100.0, none());
    h.ed.text_input(TextInputKind::Insert, "A").unwrap();
    let t = common::text::info(&h).id;
    assert_eq!(h.parent(t), None, "родитель — по документу в точке мира");
    assert_eq!(h.props(t).x, 600.0);
    assert_eq!(h.props(f).x, 0.0);
}

/// Финальное ревью, Important 2: ручка ресайза (и зона поворота), видная сдвинутой чужим перетаскиванием, —
/// точка нажатия сдвигается на чужой сдвиг, и размер меняется только на своё движение.
#[test]
fn resize_and_rotate_under_peer_drag_have_no_jump() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 100.0, 50.0);
    h.ed.select(&[a]);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a], 100.0);
    // Правый нижний угол виден у (200, 50).
    h.down(200.0, 50.0, none());
    for x in [205.0, 210.0] {
        h.mv(x, 60.0, none());
    }
    h.up(210.0, 60.0, none());
    let p = h.props(a);
    assert_eq!(
        (p.x, p.y, p.w, p.h),
        (0.0, 0.0, 110.0, 60.0),
        "размер — на своё движение"
    );
    // Поворот — то же: центр и точка нажатия — на рамке по документу в том же месте, и
    // четверть оборота указателя вокруг видного центра — четверть оборота узла.
    let a2 = h.rect(None, 0.0, 200.0, 100.0, 100.0);
    h.ed.select(&[a2]);
    h.ed.presence_left(1);
    join(&mut h, 2);
    drag_by(&mut h, 2, &[a2], 100.0);
    // Чуть снаружи правого верхнего угла (виден у 200, 200; центр — у 150, 250).
    h.down(206.0, 194.0, none());
    h.mv(206.0, 250.0, none());
    h.mv(206.0, 306.0, none());
    h.up(206.0, 306.0, none());
    let r = h.props(a2).rotation;
    assert!((r.rem_euclid(180.0) - 90.0).abs() < 1e-6, "поворот {r}");
}

#[test]
fn double_click_on_text_under_peer_drag_puts_caret_where_it_is_seen() {
    let mut h = common::text::text_h();
    let t = common::text::add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let (x1, y) = common::text::at_index(&h, t, 1);
    let (x2, _) = common::text::at_index(&h, t, 2);
    h.ed.select(&[]);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[t], 300.0);
    // У границы первой и второй буквы — там, где текст виден.
    h.dbl(x1 + (x2 - x1) * 0.25 + 300.0, y);
    assert_eq!(
        common::text::info(&h).focus,
        1,
        "курсор — у той же буквы, что под указателем"
    );
    assert!(
        !h.ed.scene().has_remote_preview(),
        "правка — без чужого слоя"
    );
    assert_eq!(h.props(t).x, 100.0);
}

#[test]
fn vector_edit_entered_under_peer_resize_takes_document_shape() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 40.0);
    join(&mut h, 1);
    h.ed.presence_part(1, 1, 0, 1, vec![a]);
    let mut s = select(&h.ed, &[a], 1);
    s.gesture = Some(Gesture {
        kind: GestureKind::Resize,
        transform: Affine::scale_non_uniform(2.0, 1.0),
        frame: Affine::IDENTITY,
        k: 1.0,
    });
    h.ed.presence_state(1, s);
    assert_eq!(h.ed.scene().node(a).unwrap().props.w, 100.0);
    assert!(h.ed.start_vector_edit(a));
    let right =
        h.ed.vector_work()
            .unwrap()
            .points
            .iter()
            .map(|p| p.x)
            .fold(f64::MIN, f64::max);
    assert_eq!(
        right, 50.0,
        "сеть — по ширине в документе, не в чужом превью"
    );
}

/// Своё, что ничего не правит: модификатор, его отпускание, отпускание без жеста, правая
/// кнопка, потеря фокуса — чужой слой не снимается и не накладывается заново. Запись (стрелка)
/// по-прежнему — без чужого сдвига.
#[test]
fn input_that_writes_nothing_leaves_peer_layer_alone() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    h.ed.select(&[a]);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a], 100.0);
    let changes = h.ed.scene().change_counter();
    h.ev(Input::KeyDown(KeyEvent::new("ShiftLeft", Modifiers::SHIFT)));
    h.ev(Input::KeyUp(KeyEvent::new("ShiftLeft", none())));
    h.up(500.0, 500.0, none());
    let mut right = h.pe(500.0, 500.0, none());
    right.button = PointerButton::Secondary;
    h.ev(Input::PointerDown(right));
    h.ev(Input::PointerUp(right));
    h.ev(Input::Blur);
    assert_eq!(
        h.ed.scene().change_counter(),
        changes,
        "слой не снимался и не накладывался"
    );
    h.key("ArrowRight", none());
    assert_eq!(h.props(a).x, 11.0, "запись — от документа");
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 111.0);
}

#[test]
fn own_selection_bounds_for_others_ignore_peer_preview() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    join(&mut h, 1);
    drag_by(&mut h, 1, &[a], 100.0);
    // Выделение сменилось под чужим слоем: рамка — по документу, слой остаётся.
    h.ed.select(&[a]);
    assert_eq!(
        h.ed.own_selection_bounds(),
        Some(Rect::new(10.0, 10.0, 60.0, 50.0))
    );
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 110.0);
    // Новый кадр участника — рамка та же.
    drag_by(&mut h, 1, &[a], 200.0);
    assert_eq!(
        h.ed.own_selection_bounds(),
        Some(Rect::new(10.0, 10.0, 60.0, 50.0))
    );
    // Без чужого превью — по сцене, со своим превью жеста.
    h.ed.presence_left(1);
    assert_eq!(
        h.ed.own_selection_bounds(),
        Some(Rect::new(10.0, 10.0, 60.0, 50.0))
    );
}

/// `n` прямоугольников 10×10 одной правкой (в `parent` или на странице), по 20 в ряд.
fn many(h: &mut H, parent: Option<NodeId>, n: usize) -> Vec<NodeId> {
    h.ed.transact(|tx| {
        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            let id = tx.create_node(parent, None, ferrum_doc::NodeKind::Rect)?;
            tx.set_props(
                id,
                &patch((i % 20) as f64 * 12.0, (i / 20) as f64 * 12.0, 10.0, 10.0),
            )?;
            ids.push(id);
        }
        Ok(ids)
    })
    .unwrap()
}

/// Сколько раз слой чужого превью менялся: счётчик сцены без своих изменений.
fn remote_changes(h: &H) -> u64 {
    h.ed.scene().change_counter() - h.ed.scene().own_change_counter()
}

/// Ревью безопасности, H2: пачка из многих записей с жестами — одна пересборка слоя и одно
/// `emit`, а не по записи.
#[test]
fn batch_of_peer_frames_rebuilds_preview_once() {
    let mut h = H::new();
    let ids = many(&mut h, None, 30);
    for p in 1..=3 {
        join(&mut h, p);
        let mine = &ids[(p as usize - 1) * 10..p as usize * 10];
        h.ed.presence_part(p, 1, 0, 1, mine.to_vec());
    }
    h.ed.take_events();
    let before = remote_changes(&h);
    h.ed.presence_batch(|ed| {
        for r in 0..64u32 {
            let p = r % 3 + 1;
            let mine = &ids[(p as usize - 1) * 10..p as usize * 10];
            let s = moving(ed, mine, f64::from(r + 1));
            ed.presence_state(p, s);
        }
    });
    assert!(
        remote_changes(&h) - before <= 2,
        "слой снят и наложен один раз: {}",
        remote_changes(&h) - before
    );
    let redraws =
        h.ed.take_events()
            .into_iter()
            .filter(|e| *e == EditorEvent::NeedsRedraw)
            .count();
    assert_eq!(redraws, 1, "события — один раз на пачку");
    // Слой — по последним записям каждого участника.
    for (id, dx) in [(ids[0], 64.0), (ids[10], 62.0), (ids[20], 63.0)] {
        let x = h.props(id).x;
        assert_eq!(h.ed.scene().node(id).unwrap().props.x, x + dx);
    }
}

/// (поведение) Пределы живого превью: выделение участника больше `PREVIEW_PEER_NODES` (с
/// поддеревом «Масштаба»), сумма больше `PREVIEW_NODES` и участники сверх `PREVIEW_PEERS` —
/// без превью; их курсор и рамка, сдвинутая жестом, остаются.
#[test]
fn peer_preview_is_capped_by_nodes_and_peers() {
    let mut h = H::new();
    // «Масштаб» фрейма с поддеревом больше предела участника: превью нет, обхода тоже.
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    many(&mut h, Some(f), PREVIEW_PEER_NODES);
    join(&mut h, 1);
    h.ed.presence_part(1, 1, 0, 1, vec![f]);
    let mut s = select(&h.ed, &[f], 1);
    s.cursor = Some(Point::new(5.0, 5.0));
    s.gesture = Some(Gesture {
        kind: GestureKind::Scale,
        transform: Affine::scale(2.0),
        frame: Affine::IDENTITY,
        k: 2.0,
    });
    h.ed.presence_state(1, s);
    assert!(
        !h.ed.scene().has_remote_preview(),
        "поддерево {} > {PREVIEW_PEER_NODES}",
        PREVIEW_PEER_NODES + 1
    );
    let m = h.ed.overlay().remote[0].clone();
    assert_eq!(m.cursor, Some(Point::new(5.0, 5.0)), "курсор остаётся");
    assert_eq!(
        m.frame,
        Some(Rect::new(0.0, 0.0, 2.0, 2.0)),
        "рамка из кадра, сдвинутая жестом"
    );
    assert!(m.boxes.is_empty());
    h.ed.presence_left(1);

    // Сумма: у первого участника превью есть, у второго оно не поместилось бы.
    let big = PREVIEW_NODES - PREVIEW_PEER_NODES + 1;
    let a = many(&mut h, None, PREVIEW_PEER_NODES);
    let b = many(&mut h, None, big);
    join(&mut h, 2);
    join(&mut h, 3);
    drag_by(&mut h, 2, &a, 1000.0);
    drag_by(&mut h, 3, &b, 1000.0);
    assert_eq!(h.ed.scene().node(a[0]).unwrap().props.x, 1000.0);
    assert_eq!(
        h.ed.scene().node(b[0]).unwrap().props.x,
        0.0,
        "{} + {big} > {PREVIEW_NODES}",
        PREVIEW_PEER_NODES
    );
    assert_eq!(h.ed.scene().remote_patches().len(), PREVIEW_PEER_NODES);
    // Первый закончил — второй помещается.
    let s = select(&h.ed, &a, 1);
    h.ed.presence_state(2, s);
    assert_eq!(h.ed.scene().node(b[0]).unwrap().props.x, 1000.0);
    h.ed.presence_reset();

    // Участников с превью — первые `PREVIEW_PEERS` по номеру.
    let one = many(&mut h, None, PREVIEW_PEERS + 1);
    for (i, id) in one.iter().enumerate() {
        let p = 10 + i as u32;
        join(&mut h, p);
        drag_by(&mut h, p, &[*id], 1000.0);
    }
    let moved = one
        .iter()
        .filter(|id| h.ed.scene().node(**id).unwrap().props.x >= 1000.0)
        .count();
    assert_eq!(moved, PREVIEW_PEERS);
    assert!(h.ed.scene().node(one[PREVIEW_PEERS]).unwrap().props.x < 1000.0);
}

fn fake_ids(from: usize, n: usize) -> Vec<NodeId> {
    (from..from + n)
        .map(|i| format!("{i}@99").parse().unwrap())
        .collect()
}

/// Ревью безопасности, M3: узлов в частях больше, чем сказано в заголовке, — часть не
/// хранится, выделение видно рамкой из кадра.
#[test]
fn peer_selection_parts_beyond_header_count_are_dropped() {
    let mut h = H::new();
    let a = h.rect(None, 10.0, 10.0, 50.0, 40.0);
    let b = h.rect(None, 200.0, 100.0, 30.0, 30.0);
    join(&mut h, 1);
    let mut s = select(&h.ed, &[a], 1);
    s.selection.as_mut().unwrap().bounds = Rect::new(1.0, 2.0, 3.0, 4.0);
    h.ed.presence_state(1, s);
    h.ed.presence_part(1, 1, 0, 1, vec![a, b]);
    let m = h.ed.overlay().remote[0].clone();
    assert!(m.boxes.is_empty(), "count = 1, пришло 2: {:?}", m.boxes);
    assert_eq!(m.frame, Some(Rect::new(1.0, 2.0, 3.0, 4.0)));
    // Правильная часть той же версии принимается.
    h.ed.presence_part(1, 1, 0, 1, vec![a]);
    assert_eq!(h.ed.overlay().remote[0].boxes.len(), 1);
    // Заголовок той же версии с другим числом узлов — собранные узлы не годятся.
    let mut s = select(&h.ed, &[a, b], 1);
    s.selection.as_mut().unwrap().bounds = Rect::new(1.0, 2.0, 3.0, 4.0);
    h.ed.presence_state(1, s);
    assert!(h.ed.overlay().remote[0].boxes.is_empty());
    let mut s = select(&h.ed, &[], 1);
    s.selection.as_mut().unwrap().count = 0;
    h.ed.presence_state(1, s);
    assert!(
        h.ed.overlay().remote[0].boxes.is_empty(),
        "меньше пришедших"
    );
}

/// Ревью безопасности, M3: узлы чужих выделений хранятся до `PEER_SELECTION_MAX` у участника и
/// до `REMOTE_IDS_MAX` у всех; сверх — рамка из кадра.
#[test]
fn stored_peer_selection_ids_are_capped_per_peer_and_in_total() {
    let mut h = H::new();
    let bounds = Rect::new(1.0, 2.0, 3.0, 4.0);
    let head = |ed: &Editor, n: usize| {
        let mut s = select(ed, &[], 1);
        let sel = s.selection.as_mut().unwrap();
        sel.count = n as u32;
        sel.bounds = bounds;
        s
    };
    // Больше предела участника.
    join(&mut h, 0);
    let s = head(&h.ed, PEER_SELECTION_MAX + 1);
    h.ed.presence_state(0, s);
    h.ed.presence_part(0, 1, 0, 1, fake_ids(0, PEER_SELECTION_MAX + 1));
    assert_eq!(h.ed.overlay().remote[0].frame, Some(bounds));
    // Ровно предел — хранится: узлов нет в сцене, рамки нет.
    let s = head(&h.ed, PEER_SELECTION_MAX);
    h.ed.presence_state(0, s);
    h.ed.presence_part(0, 1, 0, 1, fake_ids(0, PEER_SELECTION_MAX));
    assert_eq!(h.ed.overlay().remote[0].frame, None);
    h.ed.presence_reset();
    // Сумма: участники 1…N заняли весь предел, участнику 0 места нет.
    let n = REMOTE_IDS_MAX / PEER_SELECTION_MAX;
    for p in 0..=n {
        join(&mut h, p as u32);
        let s = head(&h.ed, PEER_SELECTION_MAX);
        h.ed.presence_state(p as u32, s);
    }
    for p in 1..=n {
        h.ed.presence_part(
            p as u32,
            1,
            0,
            1,
            fake_ids(p * PEER_SELECTION_MAX, PEER_SELECTION_MAX),
        );
    }
    h.ed.presence_part(0, 1, 0, 1, fake_ids(0, PEER_SELECTION_MAX));
    assert_eq!(h.ed.overlay().remote[0].frame, Some(bounds), "места нет");
    assert_eq!(h.ed.overlay().remote[1].frame, None, "хранится");
    // Один ушёл — место есть.
    h.ed.presence_left(1);
    h.ed.presence_part(0, 1, 0, 1, fake_ids(0, PEER_SELECTION_MAX));
    assert_eq!(h.ed.overlay().remote[0].frame, None);
}

/// T5-M2: курсоры и рамки рисуются у первых `PEERS_DRAWN` участников страницы.
#[test]
fn only_peers_drawn_are_drawn() {
    let mut h = H::new();
    for p in 0..=PEERS_DRAWN as u32 {
        join(&mut h, p);
        let s = state(&h.ed, Some((f64::from(p), 0.0)));
        h.ed.presence_state(p, s);
    }
    let remote = h.ed.overlay().remote;
    assert_eq!(remote.len(), PEERS_DRAWN);
    assert_eq!(
        remote[0].cursor,
        Some(Point::new(0.0, 0.0)),
        "первые по номеру"
    );
    assert_eq!(h.ed.peers().len(), PEERS_DRAWN + 1, "в списке — все");
}
