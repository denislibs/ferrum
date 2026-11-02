//! Умный дубликат и «Выделить подобные» (спека подсистемы 2, раздел 6): после
//! Ctrl/Cmd+D и сдвига копии следующий Ctrl/Cmd+D повторяет тот же сдвиг, пока
//! выделение не менялось; «Выделить подобные» — тот же тип, заливки и обводки.

mod common;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn dup(h: &mut H) -> Vec<NodeId> {
    h.key("KeyD", ctrl());
    h.sel()
}

fn xy(h: &mut H, id: NodeId) -> (f64, f64) {
    let p = h.props(id);
    (p.x, p.y)
}

#[test]
fn duplicate_repeats_the_move_of_the_copy() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    let c1 = dup(&mut h);
    assert_eq!(xy(&mut h, c1[0]), (100.0, 100.0), "первая копия — на месте");
    // Копию сдвинули протяжкой на (30, 20).
    h.drag((120.0, 120.0), (150.0, 140.0), Modifiers::NONE);
    assert_eq!(xy(&mut h, c1[0]), (130.0, 120.0));
    let c2 = dup(&mut h);
    assert_eq!(xy(&mut h, c2[0]), (160.0, 140.0), "тот же сдвиг");
    let c3 = dup(&mut h);
    assert_eq!(xy(&mut h, c3[0]), (190.0, 160.0), "и снова");
    assert_eq!(h.children(None).len(), 4);
}

#[test]
fn nudge_counts_as_a_move_and_other_selection_resets() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    let c1 = dup(&mut h);
    h.key("ArrowRight", Modifiers::SHIFT);
    let c2 = dup(&mut h);
    assert_eq!(xy(&mut h, c2[0]), (120.0, 100.0));
    // Другое выделение и обратно: шаг забыт — копия на месте.
    h.ed.select(&[r]);
    h.ed.select(&c2);
    let c3 = dup(&mut h);
    assert_eq!(xy(&mut h, c3[0]), (120.0, 100.0));
    // Отмена и повтор сдвига копии выделения не меняют: шаг — по итогу сдвига.
    let _ = c1;
    h.key("ArrowDown", Modifiers::SHIFT);
    h.ed.execute(CommandId::Undo);
    h.ed.execute(CommandId::Redo);
    let c4 = dup(&mut h);
    assert_eq!(xy(&mut h, c4[0]), (120.0, 120.0));
    // Отмена дубликата возвращает прежнее выделение: шаг забыт.
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.sel(), c3);
    h.ed.select(&[r]);
    h.ed.select(&c3);
    let c5 = dup(&mut h);
    assert_eq!(xy(&mut h, c5[0]), (120.0, 110.0));
}

#[test]
fn several_nodes_keep_their_layout() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    let b = h.rect(None, 200.0, 100.0, 40.0, 40.0);
    h.ed.select(&[b, a]);
    dup(&mut h);
    h.key("ArrowDown", Modifiers::SHIFT);
    let copies = dup(&mut h);
    let mut at: Vec<(f64, f64)> = copies.iter().map(|id| xy(&mut h, *id)).collect();
    at.sort_by(|p, q| p.0.total_cmp(&q.0));
    assert_eq!(at, [(100.0, 120.0), (200.0, 120.0)]);
}

fn fill(r: u8) -> Vec<Fill> {
    vec![Fill::solid(Color::rgba(r, 0, 0, 255))]
}

#[test]
fn select_similar_matches_type_fills_and_strokes_on_the_page() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 500.0, 500.0);
    let mk = |h: &mut H, parent, kind, r: u8| {
        let id = h.add(parent, kind, patch(10.0, 10.0, 20.0, 20.0));
        h.set(
            id,
            PropsPatch {
                fills: Some(fill(r)),
                ..Default::default()
            },
        );
        id
    };
    let a = mk(&mut h, None, NodeKind::Rect, 1);
    let b = mk(&mut h, Some(f), NodeKind::Rect, 1);
    let other_fill = mk(&mut h, None, NodeKind::Rect, 2);
    let ellipse = mk(&mut h, None, NodeKind::Ellipse, 1);
    let hidden = mk(&mut h, None, NodeKind::Rect, 1);
    h.set(
        hidden,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    let stroked = mk(&mut h, None, NodeKind::Rect, 1);
    h.set(
        stroked,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 1.0,
                align: StrokeAlign::Inside,
            }]),
            ..Default::default()
        },
    );
    assert!(!h.ed.command_state(CommandId::SelectSimilar).enabled);
    h.ed.select(&[a]);
    let steps = h.ed.step_count();
    h.ed.execute(CommandId::SelectSimilar);
    // Порядок обхода страницы: фрейм (его ребёнок), затем корни по порядку.
    assert_eq!(h.sel(), [b, a]);
    assert_eq!(h.ed.step_count(), steps, "выбор — не шаг истории");
    // Образцы — все выделенные: эллипс добавляет свой вид.
    h.ed.select(&[a, ellipse]);
    h.ed.execute(CommandId::SelectSimilar);
    assert_eq!(h.sel().len(), 3);
    assert!(!h.sel().contains(&other_fill) && !h.sel().contains(&stroked));
    // Совпавший контейнер выделяется без потомков.
    let g = h.add(None, NodeKind::Group, PropsPatch::default());
    let g2 = h.add(None, NodeKind::Group, PropsPatch::default());
    h.ed.select(&[g]);
    h.ed.execute(CommandId::SelectSimilar);
    assert_eq!(h.sel(), [g, g2]);
}
