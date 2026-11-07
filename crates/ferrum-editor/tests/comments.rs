//! Метки комментариев (спека подсистемы 4, разделы 10.2 и 10.3): режим «Комментарий» (C),
//! Shift+C, привязка к верхнему узлу, раскладка за сценой (со своим и чужим превью), группы при
//! мелком зуме, попадание раньше выделения, перетаскивание своей метки, переход к обсуждению.
//! Метки — не документ: ни правок, ни шагов истории, ни `own_change_counter`.

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::{NodeId, PageId, PropsPatch};
use ferrum_editor::*;
use ferrum_render::{PinKind, Rgba};
use kurbo::{Affine, Point, Rect, Vec2};

fn anchor(page: PageId, node: Option<NodeId>, local: (f64, f64), world: (f64, f64)) -> PinAnchor {
    PinAnchor {
        page,
        node,
        local: local.into(),
        world: world.into(),
    }
}

fn pin(id: &str, anchor: PinAnchor, movable: bool) -> CommentPin {
    CommentPin {
        id: Arc::from(id),
        anchor,
        initials: Arc::from("АБ"),
        color: Rgba::from_rgba8(0x0d, 0x74, 0xce, 255),
        replies: 0,
        unread: false,
        resolved: false,
        movable,
    }
}

/// Стенд с комментариями: файл на сервере, роль пришла.
fn available() -> H {
    let mut h = H::new();
    h.ed.set_comments_available(true);
    h.ed.take_events();
    h
}

/// Метка в мире без узла.
fn world_pin(h: &H, id: &str, x: f64, y: f64, movable: bool) -> CommentPin {
    pin(id, anchor(h.ed.page(), None, (x, y), (x, y)), movable)
}

/// Экранные точки меток раскладки (не групп).
fn pins_on_screen(h: &H) -> Vec<Point> {
    h.ed.comment_layout_for_tests()
        .into_iter()
        .filter(|(_, k)| matches!(k, PinKind::Pin { .. }))
        .map(|(p, _)| p)
        .collect()
}

fn clusters(h: &H) -> Vec<u32> {
    h.ed.comment_layout_for_tests()
        .into_iter()
        .filter_map(|(_, k)| match k {
            PinKind::Cluster { count, .. } => Some(count),
            _ => None,
        })
        .collect()
}

/// Щелчок по капле метки с остриём в экранной точке `tip`.
fn click_pin(h: &mut H, tip: Point) {
    h.click(tip.x + 10.0, tip.y - 10.0, none());
}

fn close_pt(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-6
}

#[test]
fn without_comments_tool_and_shift_c_are_disabled_and_keys_do_nothing() {
    let mut h = H::new();
    assert!(!h.ed.command_state(CommandId::ToolComment).enabled);
    assert!(!h.ed.command_state(CommandId::ViewComments).enabled);
    h.key("KeyC", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    h.key("KeyC", shift());
    assert!(h.ed.comments_visible(), "Shift+C молчит");
    h.ed.execute(CommandId::ToolComment);
    h.ed.set_tool(ToolKind::Comment);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move, "и тулбаром не включить");
    // Меток без комментариев не видно и нажатие их не ловит.
    let p = world_pin(&h, "t1", 100.0, 100.0, true);
    h.ed.set_comment_pins(vec![p], true);
    assert!(h.ed.overlay().comments.is_empty());

    h.ed.set_comments_available(true);
    let ev = h.ed.take_events();
    assert!(ev.contains(&EditorEvent::CommentsState {
        mode: false,
        visible: true,
        available: true
    }));
    assert!(ev
        .iter()
        .any(|e| matches!(e, EditorEvent::CommandStates(_))));
    assert!(h.ed.command_state(CommandId::ToolComment).enabled);
    assert_eq!(
        h.ed.command_state(CommandId::ViewComments).checked,
        Some(true)
    );
    assert_eq!(h.ed.overlay().comments.len(), 1);
    h.key("KeyC", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Comment);
    assert!(h
        .ed
        .take_events()
        .contains(&EditorEvent::ToolChanged(ToolKind::Comment)));
    // Комментариев не стало (роль отняли): режим кончается, меток нет.
    h.ed.set_comments_available(false);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert!(h.ed.overlay().comments.is_empty());
}

#[test]
fn comment_mode_disables_other_tools_but_not_the_camera() {
    let mut h = available();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyC", none());
    h.ed.take_events();
    // Щелчок по узлу — не выделение, а место нового комментария, привязанное к узлу.
    h.click(150.0, 120.0, none());
    assert!(h.sel().is_empty());
    let ev = h.ed.take_events();
    let place = ev.iter().find_map(|e| match e {
        EditorEvent::CommentPlace { anchor, screen } => Some((*anchor, *screen)),
        _ => None,
    });
    let (a, s) = place.expect("место нового комментария");
    assert_eq!(a.node, Some(r));
    assert!(close_pt(a.local, Point::new(50.0, 20.0)), "{:?}", a.local);
    assert!(close_pt(a.world, Point::new(150.0, 120.0)));
    assert!(close_pt(s, Point::new(150.0, 120.0)));
    // Протяжка по холсту — ни рамки выделения, ни нового комментария.
    h.drag((400.0, 400.0), (600.0, 600.0), none());
    assert!(h.sel().is_empty());
    assert!(h.ed.overlay().marquee.is_none());
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentPlace { .. })));
    // Камера: колесо, колесо с Ctrl, щипок, средняя кнопка, Space.
    let c0 = h.ed.camera().center();
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(500.0, 400.0),
        delta: Vec2::new(0.0, 40.0),
        modifiers: none(),
    }));
    assert!(!close_pt(h.ed.camera().center(), c0), "колесо двигает вид");
    let z0 = h.ed.camera().zoom();
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(500.0, 400.0),
        delta: Vec2::new(0.0, -50.0),
        modifiers: ctrl(),
    }));
    assert!(h.ed.camera().zoom() > z0, "Ctrl+колесо — зум");
    let z1 = h.ed.camera().zoom();
    h.ev(Input::Pinch {
        point: Point::new(500.0, 400.0),
        scale: 0.5,
    });
    assert!(h.ed.camera().zoom() < z1, "щипок — зум");
    let c1 = h.ed.camera().center();
    let mut mid = h.pe(300.0, 300.0, none());
    mid.button = PointerButton::Middle;
    h.ev(Input::PointerDown(mid));
    mid.point = Point::new(350.0, 300.0);
    h.ev(Input::PointerMove(mid));
    h.ev(Input::PointerUp(mid));
    assert!(!close_pt(h.ed.camera().center(), c1), "средняя кнопка");
    let c2 = h.ed.camera().center();
    h.ev(Input::KeyDown(KeyEvent::new("Space", none())));
    h.drag((300.0, 300.0), (380.0, 300.0), none());
    h.ev(Input::KeyUp(KeyEvent::new("Space", none())));
    assert!(!close_pt(h.ed.camera().center(), c2), "Space");
    assert!(
        !h.ed
            .take_events()
            .iter()
            .any(|e| matches!(e, EditorEvent::CommentPlace { .. })),
        "камера — не новый комментарий"
    );
    assert_eq!(h.ed.tool_kind(), ToolKind::Comment);
    // Ничего не записано.
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    // Клавиша инструмента выводит из режима; Esc — к «Перемещению».
    h.key("KeyR", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
    h.key("KeyC", none());
    h.key("Escape", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
}

#[test]
fn pins_never_touch_document_history_or_scene_counter() {
    let mut h = available();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    h.ed.mark_journal();
    let before = h.state();
    let steps = h.ed.step_count();
    let can_undo = h.ed.document().can_undo();
    let own = h.ed.scene().own_change_counter();
    let page = h.ed.page();
    h.ed.set_comment_pins(
        vec![
            pin(
                "a",
                anchor(page, Some(r), (10.0, 10.0), (110.0, 110.0)),
                true,
            ),
            world_pin(&h, "b", 500.0, 500.0, true),
        ],
        true,
    );
    h.key("KeyC", none());
    h.click(160.0, 160.0, none());
    // Своя метка: перетаскивание, открытие, черновик, переход, скрытие, удаление.
    h.drag((520.0, 490.0), (620.0, 540.0), none());
    click_pin(&mut h, Point::new(110.0, 110.0));
    h.ed.set_comment_draft(Some(anchor(page, None, (5.0, 5.0), (5.0, 5.0))));
    h.ed.comment_focus("a");
    h.key("KeyC", shift());
    h.key("KeyC", shift());
    h.ed.remove_comment_pins(&["a", "b"]);
    assert_eq!(h.state(), before, "документ тот же");
    assert_eq!(h.ed.step_count(), steps, "шагов истории нет");
    assert_eq!(h.ed.document().can_undo(), can_undo);
    assert_eq!(h.ed.scene().own_change_counter(), own);
    assert_eq!(h.ed.take_journal().unwrap(), None, "в журнал ничего");
    h.assert_scene_clean();
}

#[test]
fn own_movable_pin_drags_with_preview_and_reports_on_release() {
    let mut h = available();
    let r = h.rect(None, 600.0, 500.0, 200.0, 200.0);
    let p = world_pin(&h, "mine", 300.0, 300.0, true);
    h.ed.set_comment_pins(vec![p], true);
    h.ed.take_events();
    // Тянем каплю «Перемещением»: метка едет, событие — только при отпускании.
    h.drag_hold((310.0, 290.0), (410.0, 340.0), none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(400.0, 350.0)]);
    assert!(h.sel().is_empty(), "не выделение");
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentMoved { .. })));
    // Esc возвращает метку, события нет.
    h.key("Escape", none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(300.0, 300.0)]);
    h.up(410.0, 340.0, none());
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentMoved { .. })));
    // Протянутая на узел — привязана к нему.
    h.drag((310.0, 290.0), (710.0, 590.0), none());
    let moved = h.ed.take_events().into_iter().find_map(|e| match e {
        EditorEvent::CommentMoved { id, anchor } => Some((id, anchor)),
        _ => None,
    });
    let (id, a) = moved.expect("CommentMoved");
    assert_eq!(&*id, "mine");
    assert_eq!(a.node, Some(r));
    assert!(close_pt(a.local, Point::new(100.0, 100.0)), "{:?}", a.local);
    assert!(close_pt(a.world, Point::new(700.0, 600.0)));
    assert_eq!(pins_on_screen(&h), vec![Point::new(700.0, 600.0)]);
}

#[test]
fn foreign_pin_does_not_move() {
    let mut h = available();
    let p = world_pin(&h, "theirs", 300.0, 300.0, false);
    h.ed.set_comment_pins(vec![p], true);
    h.ed.take_events();
    h.drag_hold((310.0, 290.0), (410.0, 340.0), none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(300.0, 300.0)]);
    h.up(410.0, 340.0, none());
    let ev = h.ed.take_events();
    assert!(!ev
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentMoved { .. })));
    assert_eq!(
        h.ed.comment_pin("theirs").unwrap().anchor.world,
        Point::new(300.0, 300.0)
    );
}

#[test]
fn close_pins_cluster_at_small_zoom_and_cluster_click_zooms_in() {
    let mut h = available();
    let page = h.ed.page();
    let pins: Vec<_> = (0..10)
        .map(|i| {
            let x = 100.0 + i as f64 * 60.0;
            pin(
                &format!("p{i}"),
                anchor(page, None, (x, 300.0), (x, 300.0)),
                false,
            )
        })
        .collect();
    h.ed.set_comment_pins(pins, true);
    assert_eq!(pins_on_screen(&h).len(), 10, "зум 1 — каждая отдельно");
    assert!(clusters(&h).is_empty());
    for _ in 0..3 {
        h.ed.execute(CommandId::ZoomOut);
    }
    // Ряд в 67 px на экране — две группы, их круги не наползают друг на друга.
    assert_eq!(clusters(&h).iter().sum::<u32>(), 10);
    assert!(pins_on_screen(&h).is_empty());
    h.ed.execute(CommandId::ZoomOut);
    let zoom = h.ed.camera().zoom();
    assert!(zoom <= 0.0625);
    assert_eq!(clusters(&h), vec![10], "мелкий зум — одна группа из 10");
    assert!(pins_on_screen(&h).is_empty());
    assert_eq!(h.ed.overlay().comments.len(), 1);
    let (at, _) = h.ed.comment_layout_for_tests()[0].clone();
    h.click(at.x, at.y, none());
    assert!(
        h.ed.camera().zoom() >= zoom * 2.0,
        "щелчок по группе приближает"
    );
    assert!(
        clusters(&h).is_empty() && pins_on_screen(&h).len() == 10,
        "метки разошлись: {:?}",
        h.ed.comment_layout_for_tests()
    );
}

#[test]
fn shift_c_hides_and_shows_pins_and_hidden_pins_do_not_catch_clicks() {
    let mut h = available();
    let r = h.rect(None, 200.0, 200.0, 200.0, 200.0);
    let p = world_pin(&h, "a", 250.0, 300.0, false);
    h.ed.set_comment_pins(vec![p], true);
    h.ed.take_events();
    h.key("KeyC", shift());
    assert!(h.ed.overlay().comments.is_empty());
    assert_eq!(
        h.ed.command_state(CommandId::ViewComments).checked,
        Some(false)
    );
    assert!(h.ed.take_events().contains(&EditorEvent::CommentsState {
        mode: false,
        visible: false,
        available: true
    }));
    // Скрытая метка не ловит щелчок: выделяется узел под ней.
    click_pin(&mut h, Point::new(250.0, 300.0));
    assert_eq!(h.sel(), vec![r]);
    h.key("KeyC", shift());
    assert_eq!(h.ed.overlay().comments.len(), 1);
    // Вход в режим комментариев показывает скрытые метки.
    h.key("KeyC", shift());
    h.key("KeyC", none());
    assert!(h.ed.comments_visible());
}

#[test]
fn clicking_a_pin_opens_it_before_canvas_selection_and_anchor_follows_camera() {
    let mut h = available();
    let r = h.rect(None, 200.0, 200.0, 200.0, 200.0);
    let p = world_pin(&h, "a", 250.0, 300.0, false);
    h.ed.set_comment_pins(vec![p], true);
    h.ed.take_events();
    click_pin(&mut h, Point::new(250.0, 300.0));
    assert!(h.sel().is_empty(), "метка — раньше выделения");
    let ev = h.ed.take_events();
    assert!(ev.contains(&EditorEvent::CommentOpened {
        id: Arc::from("a"),
        screen: Point::new(250.0, 300.0)
    }));
    assert!(ev.contains(&EditorEvent::CommentAnchor {
        screen: Point::new(250.0, 300.0),
        visible: true
    }));
    assert_eq!(h.ed.comment_open(), Some("a"));
    // Камера сдвинулась — окно едет за меткой; за краем холста — «не видно».
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(500.0, 400.0),
        delta: Vec2::new(-30.0, 0.0),
        modifiers: none(),
    }));
    assert!(h.ed.take_events().contains(&EditorEvent::CommentAnchor {
        screen: Point::new(280.0, 300.0),
        visible: true
    }));
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(500.0, 400.0),
        delta: Vec2::new(2000.0, 0.0),
        modifiers: none(),
    }));
    assert!(h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentAnchor { visible: false, .. })));
    // Мимо метки — обычное выделение.
    h.ed.set_comment_open(None);
    h.ed.execute(CommandId::Zoom100);
    let mut cam = h.ed.camera().clone();
    cam.set_center(Point::new(500.0, 400.0));
    h.ed.set_camera(cam);
    h.click(350.0, 350.0, none());
    assert_eq!(h.sel(), vec![r]);
}

#[test]
fn focus_goes_to_the_pins_page_centres_camera_and_waits_for_unknown_pins() {
    let mut h = available();
    let first = h.ed.page();
    let second = h.ed.add_page("Вторая").unwrap();
    h.ed.set_comment_pins(
        vec![pin(
            "far",
            anchor(second, None, (5000.0, 4000.0), (5000.0, 4000.0)),
            false,
        )],
        true,
    );
    // Метка другой страницы здесь не рисуется.
    assert!(h.ed.overlay().comments.is_empty());
    h.ed.take_events();
    h.ed.comment_focus("far");
    assert_eq!(h.ed.page(), second);
    assert!(close_pt(h.ed.camera().center(), Point::new(5000.0, 4000.0)));
    assert_eq!(h.ed.comment_open(), Some("far"));
    let ev = h.ed.take_events();
    assert!(ev.contains(&EditorEvent::PageChanged(second)));
    assert!(ev.contains(&EditorEvent::CommentOpened {
        id: Arc::from("far"),
        screen: Point::new(500.0, 400.0)
    }));
    assert_eq!(h.ed.overlay().comments.len(), 1);
    // Обсуждения ещё нет (ссылка раньше списка): переход, когда метка придёт.
    h.ed.comment_focus("late");
    assert_eq!(h.ed.page(), second);
    h.ed.set_comment_pins(
        vec![pin(
            "late",
            anchor(first, None, (-300.0, 200.0), (-300.0, 200.0)),
            false,
        )],
        false,
    );
    assert_eq!(h.ed.page(), first);
    assert!(close_pt(h.ed.camera().center(), Point::new(-300.0, 200.0)));
    assert_eq!(h.ed.comment_open(), Some("late"));
}

#[test]
fn pin_of_hidden_or_deleted_node_stays_at_last_world_point() {
    let mut h = available();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    let page = h.ed.page();
    h.ed.set_comment_pins(
        vec![pin(
            "a",
            anchor(page, Some(r), (10.0, 20.0), (900.0, 700.0)),
            false,
        )],
        true,
    );
    assert_eq!(pins_on_screen(&h), vec![Point::new(110.0, 120.0)]);
    h.set(
        r,
        PropsPatch {
            x: Some(150.0),
            ..Default::default()
        },
    );
    assert_eq!(
        pins_on_screen(&h),
        vec![Point::new(160.0, 120.0)],
        "едет с узлом"
    );
    h.set(
        r,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    assert_eq!(pins_on_screen(&h), vec![Point::new(900.0, 700.0)], "скрыт");
    h.set(
        r,
        PropsPatch {
            visible: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(pins_on_screen(&h), vec![Point::new(160.0, 120.0)]);
    h.ed.select(&[r]);
    h.ed.execute(CommandId::Delete);
    assert_eq!(pins_on_screen(&h), vec![Point::new(900.0, 700.0)], "удалён");
}

#[test]
fn click_over_nested_frame_anchors_to_top_level_frame() {
    let mut h = available();
    let outer = h.frame(None, 50.0, 50.0, 500.0, 500.0);
    let inner = h.frame(Some(outer), 100.0, 100.0, 200.0, 200.0);
    let _leaf = h.rect(Some(inner), 10.0, 10.0, 50.0, 50.0);
    h.key("KeyC", none());
    h.ed.take_events();
    // Мир (170, 170) — над листом во вложенном фрейме.
    h.click(170.0, 170.0, none());
    let a =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::CommentPlace { anchor, .. } => Some(anchor),
                _ => None,
            })
            .unwrap();
    assert_eq!(a.node, Some(outer));
    assert!(close_pt(a.local, Point::new(120.0, 120.0)), "{:?}", a.local);
    // Пустое место — мир.
    h.click(800.0, 700.0, none());
    let a =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::CommentPlace { anchor, .. } => Some(anchor),
                _ => None,
            })
            .unwrap();
    assert_eq!(a.node, None);
    assert!(close_pt(a.local, Point::new(800.0, 700.0)));
}

#[test]
fn pin_follows_its_node_during_own_drag_and_remote_live_preview() {
    let mut h = available();
    let r = h.rect(None, 100.0, 100.0, 200.0, 100.0);
    let page = h.ed.page();
    h.ed.set_comment_pins(
        vec![pin(
            "a",
            anchor(page, Some(r), (10.0, 20.0), (110.0, 120.0)),
            false,
        )],
        true,
    );
    // Своё перетаскивание: превью жеста в сцене — метка с ним.
    h.drag_hold((250.0, 180.0), (350.0, 180.0), none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(210.0, 120.0)]);
    h.key("Escape", none());
    h.up(350.0, 180.0, none());
    assert_eq!(pins_on_screen(&h), vec![Point::new(110.0, 120.0)]);
    // Чужое живое превью: слой сцены — метка с ним, документ тот же.
    let own = h.ed.scene().own_change_counter();
    h.ed.presence_join(1, "u-1", "Анна", peer_color(0x0d74ce));
    h.ed.presence_part(1, 1, 0, 1, vec![r]);
    let state = |dx: f64| PeerState {
        page: Some(page),
        view: Rect::new(0.0, 0.0, 1000.0, 800.0),
        cursor: None,
        selection: Some(PeerSelection {
            rev: 1,
            count: 1,
            parts: 1,
            bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
        }),
        gesture: Some(Gesture {
            kind: GestureKind::Move,
            transform: Affine::translate((dx, 0.0)),
            frame: Affine::IDENTITY,
            k: 1.0,
        }),
    };
    h.ed.presence_state(1, state(50.0));
    assert_eq!(pins_on_screen(&h), vec![Point::new(160.0, 120.0)]);
    h.ed.presence_state(1, state(80.0));
    assert_eq!(pins_on_screen(&h), vec![Point::new(190.0, 120.0)]);
    assert_eq!(h.ed.scene().own_change_counter(), own);
    h.ed.presence_left(1);
    assert_eq!(pins_on_screen(&h), vec![Point::new(110.0, 120.0)]);
}

#[test]
fn resolved_pins_are_hidden_unless_shown_or_open() {
    let mut h = available();
    let mut p = world_pin(&h, "done", 300.0, 300.0, false);
    p.resolved = true;
    h.ed.set_comment_pins(vec![p], true);
    assert!(pins_on_screen(&h).is_empty());
    h.ed.set_comments_show_resolved(true);
    assert_eq!(pins_on_screen(&h).len(), 1);
    h.ed.set_comments_show_resolved(false);
    h.ed.set_comment_open(Some("done"));
    assert_eq!(pins_on_screen(&h).len(), 1, "открытое видно и закрытым");
}

#[test]
fn pin_store_is_bounded_and_removal_closes_the_open_thread() {
    let mut h = available();
    let page = h.ed.page();
    let pins: Vec<_> = (0..COMMENT_PINS_MAX + 5)
        .map(|i| {
            pin(
                &format!("p{i}"),
                anchor(page, None, (0.0, 0.0), (0.0, 0.0)),
                false,
            )
        })
        .collect();
    h.ed.set_comment_pins(pins, true);
    assert_eq!(h.ed.comment_pin_count(), COMMENT_PINS_MAX);
    // Замена известной — и сверх предела.
    let mut known = pin("p0", anchor(page, None, (1.0, 1.0), (1.0, 1.0)), false);
    known.replies = 3;
    h.ed.set_comment_pins(vec![known], false);
    assert_eq!(h.ed.comment_pin("p0").unwrap().replies, 3);
    h.ed.set_comment_open(Some("p1"));
    h.ed.take_events();
    h.ed.remove_comment_pins(&["p1"]);
    assert_eq!(h.ed.comment_open(), None);
    assert!(h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::CommentAnchor { visible: false, .. })));
}
