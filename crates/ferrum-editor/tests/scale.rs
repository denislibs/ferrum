//! Инструмент «Масштаб» (K, спека подсистемы 3, раздел 11): ручки и поле
//! «Масштаб» панели масштабируют выделение равномерно вместе с обводками,
//! радиусами, текстом, окончаниями и содержимым групп и фреймов — одним шагом.

mod common;

use std::sync::Arc;

use common::text::{add_text, text_h};
use common::*;
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
