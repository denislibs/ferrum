//! Инструмент «Масштаб» (K, спека подсистемы 3, раздел 11): ручки и поле
//! «Масштаб» панели масштабируют выделение равномерно вместе с обводками,
//! радиусами, текстом, окончаниями и содержимым групп и фреймов — одним шагом.

mod common;

use std::sync::Arc;

use common::text::{add_text, text_h};
use common::*;
use ferrum_doc::TextSizing as Sizing;
use ferrum_doc::{
    Color, LetterSpacing, LineHeight, NodeKind, PropsPatch, Stroke, StrokeAlign, StrokeCap,
    TextAttrs, TextContent, TextRun, TextSizing, VectorNetwork, VectorPoint, VectorSegment,
};
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR};
use ferrum_editor::{CommandId, PanelEdit, Phase, ToolKind};

fn stroke(w: f64) -> Stroke {
    Stroke {
        color: Color::BLACK,
        width: w,
        align: StrokeAlign::Inside,
    }
}

/// Прямоугольник 100×100 в (200, 200) с обводкой 2 и радиусом 10, выделен; K.
fn styled_rect(h: &mut H) -> ferrum_doc::NodeId {
    let id = h.rect(None, 200.0, 200.0, 100.0, 100.0);
    h.set(
        id,
        PropsPatch {
            strokes: Some(vec![stroke(2.0)]),
            corner_radius: Some(10.0),
            ..Default::default()
        },
    );
    h.ed.select(&[id]);
    h.ed.execute(CommandId::ToolScale);
    id
}

#[test]
fn k_selects_the_scale_tool() {
    let mut h = H::new();
    h.key("KeyK", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Scale);
    h.key("Escape", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
}

#[test]
fn corner_drag_scales_size_stroke_and_radius_in_one_step() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag((300.0, 300.0), (400.0, 400.0), none());
    let p = h.props(id);
    assert!(close(p.w, 200.0) && close(p.h, 200.0), "{} × {}", p.w, p.h);
    assert!(close(p.x, 200.0) && close(p.y, 200.0));
    assert!(close(p.strokes[0].width, 4.0), "{}", p.strokes[0].width);
    assert!(close(p.corner_radius, 20.0));
    assert_eq!(h.ed.tool_kind(), ToolKind::Scale, "инструмент остаётся");
    h.assert_one_step(steps, &before);
}

#[test]
fn plain_resize_does_not_scale_properties() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    h.ed.execute(CommandId::ToolMove);
    h.drag((300.0, 300.0), (400.0, 400.0), none());
    let p = h.props(id);
    assert!(close(p.w, 200.0));
    assert!(close(p.strokes[0].width, 2.0));
    assert!(close(p.corner_radius, 10.0));
}

#[test]
fn side_handle_keeps_proportions() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    // Правая сторона на +50: и высота растёт в 1,5 раза (от середины левой стороны).
    h.drag((300.0, 250.0), (350.0, 250.0), none());
    let p = h.props(id);
    assert!(close(p.w, 150.0) && close(p.h, 150.0), "{} × {}", p.w, p.h);
    assert!(close(p.y, 175.0), "y {}", p.y);
    assert!(close(p.strokes[0].width, 3.0));
}

#[test]
fn alt_scales_from_center() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    h.drag((300.0, 300.0), (350.0, 350.0), alt());
    let p = h.props(id);
    assert!(close(p.w, 200.0), "{}", p.w);
    assert!(close(p.x, 150.0) && close(p.y, 150.0));
}

#[test]
fn escape_cancels_without_a_step() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    let steps = h.ed.step_count();
    h.drag_hold((300.0, 300.0), (400.0, 400.0), none());
    assert!(h.ed.scene().has_preview());
    h.key("Escape", none());
    assert_eq!(h.ed.step_count(), steps);
    assert!(close(h.props(id).w, 100.0));
    h.assert_scene_clean();
}

#[test]
fn group_and_frame_content_scales_with_them() {
    let mut h = H::new();
    let frame = h.frame(None, 0.0, 0.0, 100.0, 100.0);
    let child = h.rect(Some(frame), 10.0, 10.0, 20.0, 20.0);
    h.set(
        child,
        PropsPatch {
            strokes: Some(vec![stroke(1.0)]),
            ..Default::default()
        },
    );
    let a = h.rect(None, 300.0, 300.0, 50.0, 50.0);
    let b = h.rect(None, 400.0, 300.0, 50.0, 50.0);
    h.ed.select(&[a, b]);
    h.ed.execute(CommandId::Group);
    let group = h.sel()[0];
    // Фрейм: ×2 от левого верхнего угла — ребёнок вдвое дальше и вдвое больше.
    h.ed.select(&[frame]);
    h.ed.execute(CommandId::ToolScale);
    h.drag((100.0, 100.0), (200.0, 200.0), none());
    let f = h.props(frame);
    assert!(close(f.w, 200.0) && close(f.h, 200.0));
    let c = h.props(child);
    assert!(close(c.x, 20.0) && close(c.y, 20.0), "{} {}", c.x, c.y);
    assert!(close(c.w, 40.0) && close(c.strokes[0].width, 2.0));
    // Группа: дети масштабируются в её осях.
    h.ed.select(&[group]);
    h.drag((450.0, 350.0), (525.0, 375.0), none());
    let (pa, pb) = (h.props(a), h.props(b));
    assert!(
        close(pa.w, 75.0) && close(pb.w, 75.0),
        "{} {} {:?}",
        pa.w,
        pb.w,
        (pa.x, pb.x)
    );
    assert!(close(pb.x - pa.x, 150.0), "{}", pb.x - pa.x);
    h.assert_scene_clean();
}

#[test]
fn panel_factor_scales_around_the_anchor_in_one_step() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 0.5,
            anchor: Anchor::CENTER,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    assert!(close(p.w, 50.0) && close(p.x, 225.0) && close(p.y, 225.0));
    assert!(close(p.strokes[0].width, 1.0) && close(p.corner_radius, 5.0));
    h.assert_one_step(steps, &before);
    // Левый верхний угол стоит на месте.
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 3.0,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    assert!(close(p.x, 225.0) && close(p.w, 150.0));
}

#[test]
fn panel_preview_and_cancel_leave_no_trace() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    let steps = h.ed.step_count();
    let edit = PanelEdit::Scale {
        factor: 2.0,
        anchor: Anchor::CENTER,
    };
    h.ed.edit_selection(&edit, Phase::Preview);
    let n = h.ed.scene().node(id).unwrap();
    assert!(close(n.props.w, 200.0), "превью в сцене");
    h.ed.edit_selection(&edit, Phase::Cancel);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
    // Сто процентов и нечисла — ничего; сверх предела — зажимается.
    for f in [1.0, f64::NAN, f64::INFINITY] {
        h.ed.edit_selection(
            &PanelEdit::Scale {
                factor: f,
                anchor: Anchor::CENTER,
            },
            Phase::Commit,
        );
    }
    assert_eq!(h.ed.step_count(), steps);
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 1e9,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    assert!(close(h.props(id).w, 100.0 * MAX_SCALE_FACTOR));
}

#[test]
fn rotated_node_keeps_its_angle() {
    let mut h = H::new();
    let id = styled_rect(&mut h);
    h.set(
        id,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 2.0,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    assert!(close(p.rotation, 30.0), "{}", p.rotation);
    assert!(close(p.w, 200.0) && close(p.h, 200.0));
    // Якорь — угол повёрнутой рамки узла: он на месте.
    assert!(close(p.x, 200.0) && close(p.y, 200.0), "{} {}", p.x, p.y);
}

#[test]
fn text_sizes_spacing_and_runs_scale_and_auto_width_stays() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет мир", 12.0);
    let mut t = (*h.props(id).text.unwrap()).clone();
    t.props.style.line_height = LineHeight::Px(20.0);
    t.props.style.letter_spacing = LetterSpacing::Px(1.0);
    t.props.paragraph_spacing = 4.0;
    let content = TextContent {
        runs: vec![
            TextRun {
                text: "Привет ".into(),
                attrs: TextAttrs {
                    size: Some(16.0),
                    line_height: Some(LineHeight::Percent(120.0)),
                    ..Default::default()
                },
            },
            TextRun {
                text: "мир\n".into(),
                attrs: TextAttrs::default(),
            },
        ],
    };
    h.set(
        id,
        PropsPatch {
            text: Some(t.props.clone()),
            content: Some(content),
            ..Default::default()
        },
    );
    let before = h.props(id);
    let w0 = ferrum_editor::fit_size(&mut h.ed.fonts().borrow_mut(), &before)
        .unwrap()
        .unwrap()
        .0;
    h.ed.select(&[id]);
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 2.0,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    let t = p.text.as_ref().unwrap();
    assert_eq!(
        t.props.sizing,
        TextSizing::AutoWidth,
        "режим размера тот же"
    );
    assert!(close(t.props.style.size, 24.0));
    assert_eq!(t.props.style.line_height, LineHeight::Px(40.0));
    assert_eq!(t.props.style.letter_spacing, LetterSpacing::Px(2.0));
    assert!(close(t.props.paragraph_spacing, 8.0));
    assert_eq!(t.content.runs[0].attrs.size, Some(32.0));
    assert_eq!(
        t.content.runs[0].attrs.line_height,
        Some(LineHeight::Percent(120.0)),
        "проценты не масштабируются"
    );
    // Рамка «авто» — по раскладке: почти вдвое шире.
    assert!((p.w - 2.0 * w0).abs() <= 2.0, "{} против {}", p.w, 2.0 * w0);
}

#[test]
fn arrow_and_vector_point_radius_scale() {
    let mut h = H::new();
    let net = VectorNetwork {
        points: vec![
            VectorPoint {
                corner_radius: 3.0,
                ..VectorPoint::new(0.0, 0.0)
            },
            VectorPoint::new(100.0, 50.0),
        ],
        segments: vec![VectorSegment::line(0, 1)],
        regions: vec![],
    };
    let id = h.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            x: Some(100.0),
            y: Some(100.0),
            w: Some(100.0),
            h: Some(50.0),
            vector: Some(Arc::new(net)),
            stroke_cap_end: Some(StrokeCap::ArrowLines),
            ..Default::default()
        },
    );
    h.ed.select(&[id]);
    h.ed.edit_selection(
        &PanelEdit::Scale {
            factor: 2.0,
            anchor: Anchor::TOP_LEFT,
        },
        Phase::Commit,
    );
    let p = h.props(id);
    assert!(close(p.w, 200.0) && close(p.h, 100.0));
    assert!(
        close(p.strokes[0].width, 2.0),
        "окончание строится от толщины"
    );
    assert_eq!(p.stroke_style.cap_end, StrokeCap::ArrowLines);
    assert!(close(
        p.vector.as_ref().unwrap().points[0].corner_radius,
        6.0
    ));
}

#[test]
fn line_scales_by_its_length() {
    let mut h = H::new();
    let id = h.add(
        None,
        NodeKind::Line,
        PropsPatch {
            x: Some(100.0),
            y: Some(100.0),
            w: Some(100.0),
            ..Default::default()
        },
    );
    h.ed.select(&[id]);
    h.ed.execute(CommandId::ToolScale);
    // У «Масштаба» у линии — рамка: правый конец — угол рамки.
    h.drag((200.0, 100.0), (300.0, 100.0), none());
    let p = h.props(id);
    assert!(close(p.w, 200.0), "{}", p.w);
    assert!(close(p.strokes[0].width, 2.0));
}

fn panel_scale(h: &mut H, factor: f64, anchor: Anchor) {
    h.ed.edit_selection(&PanelEdit::Scale { factor, anchor }, Phase::Commit);
}

fn flip(h: &mut H, id: ferrum_doc::NodeId, x: bool, y: bool) {
    h.set(
        id,
        PropsPatch {
            flip_x: Some(x),
            flip_y: Some(y),
            ..Default::default()
        },
    );
}

#[test]
fn panel_anchor_is_visual_on_flipped_nodes() {
    // Отражённый по X: TOP_LEFT — видимый левый верхний угол, он на месте.
    let mut h = H::new();
    let id = h.rect(None, 200.0, 200.0, 100.0, 100.0);
    flip(&mut h, id, true, false);
    h.ed.select(&[id]);
    panel_scale(&mut h, 2.0, Anchor::TOP_LEFT);
    let p = h.props(id);
    assert!(
        close(p.x, 200.0) && close(p.w, 200.0),
        "x {} w {}",
        p.x,
        p.w
    );
    assert!(close(p.y, 200.0) && p.flip_x);

    // Отражённый по Y.
    let id = h.rect(None, 500.0, 200.0, 100.0, 100.0);
    flip(&mut h, id, false, true);
    h.ed.select(&[id]);
    panel_scale(&mut h, 2.0, Anchor::TOP_LEFT);
    let p = h.props(id);
    assert!(
        close(p.y, 200.0) && close(p.h, 200.0),
        "y {} h {}",
        p.y,
        p.h
    );
    assert!(close(p.x, 500.0) && p.flip_y);
}

#[test]
fn panel_anchor_is_visual_on_flipped_group_and_nested_child() {
    let mut h = H::new();
    let a = h.rect(None, 300.0, 300.0, 50.0, 50.0);
    let b = h.rect(None, 400.0, 300.0, 50.0, 50.0);
    flip(&mut h, a, true, false);
    h.ed.select(&[a, b]);
    h.ed.execute(CommandId::Group);
    let group = h.sel()[0];
    flip(&mut h, group, true, false);
    h.ed.select(&[group]);
    let b0 = h.ed.scene().node(group).unwrap().world_bounds.unwrap();
    panel_scale(&mut h, 2.0, Anchor::TOP_LEFT);
    let a0 = h.ed.scene().node(group).unwrap().world_bounds.unwrap();
    assert!(close(a0.x0, b0.x0) && close(a0.y0, b0.y0), "{a0:?} {b0:?}");
    assert!(close(a0.width(), 2.0 * b0.width()));
    // Вложенный отражённый ребёнок сохранил отражение и вырос вдвое.
    let (pa, pb) = (h.props(a), h.props(b));
    assert!(pa.flip_x && close(pa.w, 100.0) && close(pb.w, 100.0));
}

#[test]
fn nan_or_foreign_anchor_is_rejected() {
    assert!(Anchor::new(f64::NAN, 0.0).is_none());
    assert!(Anchor::new(0.0, f64::INFINITY).is_none());
    assert!(Anchor::new(0.3, 0.5).is_none());
    assert_eq!(
        Anchor::new(0.5, 1.0).map(|a| (a.fx(), a.fy())),
        Some((0.5, 1.0))
    );
}

#[test]
fn zero_height_child_of_a_frame_stays_zero() {
    let mut h = H::new();
    let frame = h.frame(None, 0.0, 0.0, 100.0, 100.0);
    let flat = h.rect(Some(frame), 10.0, 10.0, 40.0, 0.0);
    h.ed.select(&[frame]);
    h.ed.execute(CommandId::ToolScale);
    h.drag((100.0, 100.0), (200.0, 200.0), none());
    let p = h.props(flat);
    assert!(close(p.w, 80.0), "{}", p.w);
    // Не растёт вместе с масштабом (MIN_SIDE·k = 0,02): остаётся на MIN_SIDE позы.
    assert!(p.h <= 0.0101, "высота нулевого ребёнка: {}", p.h);
}

#[test]
fn tiny_factor_keeps_fixed_text_box_in_proportion() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 20.0);
    let mut t = (*h.props(id).text.unwrap()).clone();
    t.props.sizing = Sizing::Fixed;
    h.set(
        id,
        PropsPatch {
            text: Some(t.props),
            x: Some(100.0),
            y: Some(100.0),
            w: Some(200.0),
            h: Some(100.0),
            ..Default::default()
        },
    );
    h.ed.select(&[id]);
    panel_scale(&mut h, 0.01, Anchor::CENTER);
    let p = h.props(id);
    // Кегль 20 → 1 (зажим), а не 0,2: рамка 200×100 → 10×5 вокруг прежнего центра.
    assert!(close(p.text.as_ref().unwrap().props.style.size, 1.0));
    assert!(close(p.w, 10.0) && close(p.h, 5.0), "{} × {}", p.w, p.h);
    assert!(close(p.x + p.w / 2.0, 200.0) && close(p.y + p.h / 2.0, 150.0));
}

#[test]
fn text_in_auto_height_and_fixed_modes_scales_by_gesture() {
    for mode in [Sizing::AutoHeight, Sizing::Fixed] {
        let mut h = text_h();
        let id = add_text(&mut h, 100.0, 100.0, "Привет", 12.0);
        let mut t = (*h.props(id).text.unwrap()).clone();
        t.props.sizing = mode;
        h.set(
            id,
            PropsPatch {
                text: Some(t.props),
                x: Some(100.0),
                y: Some(100.0),
                w: Some(100.0),
                h: Some(60.0),
                ..Default::default()
            },
        );
        h.ed.select(&[id]);
        h.ed.execute(CommandId::ToolScale);
        let steps = h.ed.step_count();
        let before = h.state();
        // Правый нижний угол рамки 100..200 × 100..160 — на +100, +60.
        h.drag((200.0, 160.0), (300.0, 220.0), none());
        let p = h.props(id);
        let t = p.text.as_ref().unwrap();
        assert_eq!(t.props.sizing, mode);
        assert!(
            close(t.props.style.size, 24.0),
            "{mode:?}: {}",
            t.props.style.size
        );
        assert!(close(p.w, 200.0), "{mode:?}: w {}", p.w);
        h.assert_one_step(steps, &before);
    }
}

#[test]
fn hidden_and_locked_children_and_nested_boolean_scale_with_the_group() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(None, 130.0, 100.0, 50.0, 50.0);
    h.ed.select(&[a, b]);
    h.ed.execute(CommandId::BooleanUnion);
    let boolean = h.sel()[0];
    let hidden = h.rect(None, 300.0, 100.0, 40.0, 40.0);
    let locked = h.rect(None, 350.0, 100.0, 40.0, 40.0);
    h.set(
        hidden,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    h.set(
        locked,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    h.ed.select(&[boolean, hidden, locked]);
    h.ed.execute(CommandId::Group);
    let group = h.sel()[0];
    h.ed.select(&[group]);
    let steps = h.ed.step_count();
    panel_scale(&mut h, 2.0, Anchor::TOP_LEFT);
    assert_eq!(h.ed.step_count(), steps + 1);
    assert!(close(h.props(hidden).w, 80.0) && close(h.props(locked).w, 80.0));
    assert!(close(h.props(a).w, 100.0) && close(h.props(b).w, 100.0));
    assert!(close(h.props(b).x - h.props(a).x, 60.0));
    h.assert_scene_clean();
}
