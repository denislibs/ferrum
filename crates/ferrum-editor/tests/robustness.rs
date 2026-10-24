//! Замечания ревью: абсолютный угол Shift-поворота, нечисловой ввод, черновики при
//! удалении родителя посреди жеста, сцена после правок на нескольких страницах.

mod common;

use common::*;
use ferrum_doc::{NodeKind, PropsPatch};
use ferrum_editor::*;
use kurbo::{Point, Vec2};

#[test]
fn shift_rotation_snaps_absolute_angle_not_delta() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    h.set(
        r,
        PropsPatch {
            rotation: Some(10.0),
            ..Default::default()
        },
    );
    h.click(150.0, 125.0, none());
    let n = h.ed.scene().node(r).unwrap();
    let center = n.world * Point::new(50.0, 25.0);
    let corner = n.world * Point::new(100.0, 50.0);
    let out = (corner - center).normalize();
    let start = corner + out * 8.0;
    h.hover(start.x, start.y);
    assert!(matches!(h.ed.cursor(), Cursor::Rotate { .. }));
    // Против часовой на 7°: относительный шаг дал бы 10° (7 → 0), абсолютный — 15°.
    let a0 = (start - center).atan2();
    let a1 = a0 - 7f64.to_radians();
    let end = center + Vec2::new(a1.cos(), a1.sin()) * (start - center).hypot();
    h.drag((start.x, start.y), (end.x, end.y), shift());
    assert_eq!(h.props(r).rotation, 15.0);
}

#[test]
fn non_finite_input_never_reaches_nodes() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    h.click(150.0, 125.0, none());
    let before = h.state();
    let steps = h.ed.step_count();
    let cam = h.ed.camera().clone();
    let nan = f64::NAN;
    // Поворот, перетаскивание и ресайз с NaN посреди жеста.
    for start in [(208.0, 158.0), (150.0, 125.0), (200.0, 150.0)] {
        h.down(start.0, start.1, shift());
        h.mv(start.0 + 20.0, start.1 + 20.0, shift());
        h.mv(nan, 10.0, shift());
        h.mv(10.0, f64::INFINITY, shift());
        h.up(nan, nan, shift());
        h.ed.cancel_gesture();
        let p = h.props(r);
        assert!(
            [p.x, p.y, p.w, p.h, p.rotation]
                .iter()
                .all(|v| v.is_finite()),
            "{p:?}"
        );
    }
    h.down(nan, 5.0, none());
    assert!(!h.ed.is_busy());
    h.ev(Input::Wheel(WheelEvent {
        point: Point::new(nan, 1.0),
        delta: v(0.0, nan),
        modifiers: ctrl(),
    }));
    h.ev(Input::Pinch {
        point: Point::new(1.0, 1.0),
        scale: nan,
    });
    assert_eq!(*h.ed.camera(), cam);
    h.assert_scene_clean();
    assert_eq!(h.ed.step_count(), steps);
    let _ = before;
}

#[test]
fn deleting_parent_mid_gesture_cancels_without_leaks() {
    // Копия-черновик внутри фрейма, который удаляют посреди жеста.
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 600.0, 600.0);
    let a = h.rect(Some(f), 100.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    h.drag_hold((120.0, 120.0), (220.0, 120.0), alt());
    assert!(h.ed.scene().has_preview());
    h.ed.transact(|tx| {
        tx.delete_nodes(&[f])?;
        Ok(vec![])
    })
    .unwrap();
    assert!(!h.ed.is_busy());
    assert!(h.sel().is_empty());
    h.assert_scene_clean();
    h.up(220.0, 120.0, alt());
    h.mv(300.0, 300.0, none());
    assert!(h.children(None).is_empty());
    assert!(h.ed.scene().node(a).is_none());
    h.assert_scene_clean();

    // Рисование внутри фрейма, который удаляют.
    let f = h.frame(None, 0.0, 0.0, 600.0, 600.0);
    h.key("KeyR", none());
    h.drag_hold((100.0, 100.0), (200.0, 150.0), none());
    assert!(h.ed.scene().has_preview());
    h.ed.transact(|tx| {
        tx.delete_nodes(&[f])?;
        Ok(vec![])
    })
    .unwrap();
    h.assert_scene_clean();
    h.up(200.0, 150.0, none());
    assert!(h.children(None).is_empty(), "фигура не создана");
    // Наведение на удалённый узел не остаётся в оверлее.
    h.ed.set_tool(ToolKind::Move);
    let g = h.rect(None, 700.0, 100.0, 50.0, 50.0);
    h.hover(720.0, 120.0);
    assert_eq!(h.ed.overlay().hover, Some(g));
    h.ed.transact(|tx| {
        tx.delete_nodes(&[g])?;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(h.ed.overlay().hover, None);
}

#[test]
fn undo_and_redo_across_pages_keep_scenes_consistent() {
    let mut h = H::new();
    let p1 = h.ed.page();
    let a = h.rect(None, 10.0, 10.0, 20.0, 20.0);
    let p2 = h.ed.add_page("Вторая").unwrap();
    h.ed.set_page(p2).unwrap();
    let b = h.rect(None, 50.0, 50.0, 20.0, 20.0);
    h.assert_scene_clean();
    // Шаг первой страницы отменяется со страницы 2: редактор переходит на 1.
    h.ed.set_page(p1).unwrap();
    h.set(
        a,
        PropsPatch {
            x: Some(99.0),
            ..Default::default()
        },
    );
    h.ed.set_page(p2).unwrap();
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.page(), p1);
    assert_eq!(h.props(a).x, 10.0);
    h.assert_scene_clean();
    h.ed.execute(CommandId::Redo);
    assert_eq!(h.props(a).x, 99.0);
    h.assert_scene_clean();
    // Шаг второй страницы: отмена из первой.
    h.ed.execute(CommandId::Undo);
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.page(), p2);
    assert!(h.ed.scene().node(b).is_none());
    h.assert_scene_clean();
    h.ed.execute(CommandId::Redo);
    assert!(h.ed.scene().node(b).is_some());
    h.assert_scene_clean();
    h.ed.set_page(p1).unwrap();
    h.assert_scene_clean();
    let _ = NodeKind::Rect;
}
