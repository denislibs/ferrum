//! Способы вставки (спека подсистемы 2, раздел 6): повтор вставки со сдвигом
//! +10, +10, «Вставить поверх» на исходные координаты, «Вставить с заменой» на
//! место каждого выделенного узла одним шагом.

mod common;

use common::*;
use ferrum_doc::{NodeKind, PropsPatch};
use ferrum_editor::*;
use kurbo::Point;

fn copy_of(h: &mut H, ids: &[ferrum_doc::NodeId]) -> Vec<u8> {
    h.ed.select(ids);
    h.ed.take_events();
    h.ed.execute(CommandId::Copy);
    h.ed.take_events()
        .into_iter()
        .find_map(|e| match e {
            EditorEvent::Clipboard(b) => Some(b),
            _ => None,
        })
        .expect("копия в событиях")
}

fn xy(h: &mut H, id: ferrum_doc::NodeId) -> (f64, f64) {
    let p = h.props(id);
    (p.x, p.y)
}

fn away(h: &mut H) {
    let mut cam = ferrum_render::Camera::new(VIEW, 1.0);
    cam.set_center(Point::new(5000.0, 5000.0));
    h.ed.set_camera(cam);
}

#[test]
fn repeated_paste_of_the_same_content_steps_by_ten() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let bytes = copy_of(&mut h, &[r]);
    let mut at = Vec::new();
    for _ in 0..3 {
        h.ed.paste(&bytes).unwrap();
        let id = h.sel()[0];
        at.push(xy(&mut h, id));
    }
    assert_eq!(at, [(100.0, 100.0), (110.0, 110.0), (120.0, 120.0)]);
    // Другое содержимое — снова на место.
    let other = h.rect(None, 300.0, 100.0, 40.0, 40.0);
    let b2 = copy_of(&mut h, &[other]);
    h.ed.paste(&b2).unwrap();
    let id = h.sel()[0];
    assert_eq!(xy(&mut h, id), (300.0, 100.0));
    // Новая копия того же — счёт заново.
    let b1 = copy_of(&mut h, &[r]);
    assert_eq!(b1, bytes);
    h.ed.paste(&b1).unwrap();
    let id = h.sel()[0];
    assert_eq!(xy(&mut h, id), (100.0, 100.0));
    // Исходное не видно — в центр вида, как раньше.
    away(&mut h);
    h.ed.paste(&b1).unwrap();
    let id = h.sel()[0];
    assert_eq!(xy(&mut h, id), (4980.0, 4980.0));
}

#[test]
fn paste_over_keeps_original_coordinates() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let bytes = copy_of(&mut h, &[r]);
    away(&mut h);
    for _ in 0..2 {
        h.ed.paste_with(&bytes, PasteMode::Over).unwrap();
        let id = h.sel()[0];
        assert_eq!(xy(&mut h, id), (100.0, 100.0), "без сдвига к центру и шага");
    }
}

#[test]
fn paste_replace_takes_place_parent_and_order_in_one_step() {
    let mut h = H::new();
    let src = h.add(None, NodeKind::Ellipse, patch(700.0, 700.0, 30.0, 20.0));
    let bytes = copy_of(&mut h, &[src]);
    let a = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let above = h.rect(None, 500.0, 500.0, 10.0, 10.0);
    let f = h.frame(None, 200.0, 200.0, 300.0, 300.0);
    let low = h.rect(Some(f), 0.0, 0.0, 10.0, 10.0);
    let b = h.rect(Some(f), 50.0, 60.0, 40.0, 40.0);
    let high = h.rect(Some(f), 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[a, b]);
    let steps = h.ed.step_count();
    let before = h.state();
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    let new = h.sel();
    assert_eq!(new.len(), 2);
    // Узлы заменены: их больше нет, на их местах — эллипсы.
    let roots = h.children(None);
    assert!(!roots.contains(&a) && !h.children(Some(f)).contains(&b));
    let at_a = roots[1];
    assert_eq!(roots, [src, at_a, above, f], "место в слоях корня");
    assert_eq!(
        h.children(Some(f)),
        [low, new_in(&new, &h.children(Some(f))), high]
    );
    let p = h.props(at_a);
    assert_eq!(
        (p.kind, p.x, p.y, p.w, p.h),
        (NodeKind::Ellipse, 100.0, 100.0, 30.0, 20.0)
    );
    let in_f = new_in(&new, &h.children(Some(f)));
    let p = h.props(in_f);
    assert_eq!((p.x, p.y), (50.0, 60.0), "координаты родителя-фрейма");
    h.assert_one_step(steps, &before);
}

/// Вставленный узел, который лежит среди `children`.
fn new_in(new: &[ferrum_doc::NodeId], children: &[ferrum_doc::NodeId]) -> ferrum_doc::NodeId {
    *new.iter()
        .find(|n| children.contains(n))
        .expect("вставка в этом родителе")
}

#[test]
fn paste_replace_skips_locked_and_without_selection_is_a_paste() {
    let mut h = H::new();
    let src = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let bytes = copy_of(&mut h, &[src]);
    let locked = h.rect(None, 300.0, 300.0, 40.0, 40.0);
    h.set(
        locked,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    let free = h.rect(None, 500.0, 300.0, 40.0, 40.0);
    h.ed.select(&[locked, free]);
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    let roots = h.children(None);
    assert!(roots.contains(&locked) && !roots.contains(&free));
    assert_eq!(h.sel().len(), 1);
    // Выделение сняли, пока читался буфер: обычная вставка.
    h.ed.select(&[]);
    let n = h.children(None).len();
    h.ed.paste_with(&bytes, PasteMode::Replace).unwrap();
    assert_eq!(h.children(None).len(), n + 1);
}

#[test]
fn paste_commands_ask_the_ui_with_their_mode_and_follow_state() {
    let mut h = H::new();
    assert!(h.ed.command_state(CommandId::PasteOver).enabled);
    assert!(
        !h.ed.command_state(CommandId::PasteReplace).enabled,
        "заменять нечего"
    );
    h.ed.execute(CommandId::PasteReplace);
    assert!(!h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::PasteRequested(_))));
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    h.ed.take_events();
    h.ed.execute(CommandId::PasteOver);
    h.ed.execute(CommandId::PasteReplace);
    let asked: Vec<PasteMode> =
        h.ed.take_events()
            .into_iter()
            .filter_map(|e| match e {
                EditorEvent::PasteRequested(m) => Some(m),
                _ => None,
            })
            .collect();
    assert_eq!(asked, [PasteMode::Over, PasteMode::Replace]);
    h.key("KeyV", ctrl().with_shift());
    assert!(h
        .ed
        .take_events()
        .contains(&EditorEvent::PasteRequested(PasteMode::Over)));
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    assert!(!h.ed.command_state(CommandId::PasteOver).enabled);
    assert!(!h.ed.command_state(CommandId::PasteReplace).enabled);
}
