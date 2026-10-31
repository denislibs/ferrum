//! Сводка выделения для панели свойств: одинаковые значения, «Смешанные»,
//! поля только там, где они есть у узлов, рамка группы, списки длиннее предела.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_doc::{Color, Document, Fill, NodeId, NodeKind, PropsPatch, Stroke, StrokeAlign};
use ferrum_scene::Scene;
use ferrum_wasm::layers::NodeType;
use ferrum_wasm::summary::{summarize, Num, MAX_PAINTS};

struct Page {
    doc: Document,
}

impl Page {
    fn new() -> Page {
        Page {
            doc: Document::new(),
        }
    }

    fn add(&mut self, parent: Option<NodeId>, kind: NodeKind, patch: PropsPatch) -> NodeId {
        let p = self.doc.pages()[0].0;
        self.doc
            .transact(p, vec![], |tx| {
                let id = tx.create_node(parent, None, kind)?;
                tx.set_props(id, &patch)?;
                Ok(vec![id])
            })
            .unwrap()[0]
    }

    fn scene(&mut self) -> Scene {
        let p = self.doc.pages()[0].0;
        Scene::from_page(self.doc.page(p).unwrap())
    }
}

fn at(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

fn same(v: f64) -> Num {
    Num {
        value: v,
        mixed: false,
    }
}

#[test]
fn empty_selection_and_unknown_ids() {
    let mut p = Page::new();
    let s = summarize(&p.scene(), &[]);
    assert_eq!(s.count, 0);
    let ghost: NodeId = "7@7".parse().unwrap();
    assert_eq!(
        summarize(&p.scene(), &[ghost]).count,
        0,
        "неизвестные пропущены"
    );
}

#[test]
fn single_rect_gives_exact_values() {
    let mut p = Page::new();
    let r = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            name: Some("Кнопка".into()),
            corner_radius: Some(4.0),
            rotation: Some(270.0),
            ..at(10.0, 20.0, 30.0, 40.0)
        },
    );
    let s = summarize(&p.scene(), &[r]);
    assert_eq!(
        (s.count, s.kind, s.name.as_deref()),
        (1, Some(NodeType::Rect), Some("Кнопка"))
    );
    assert_eq!([s.x, s.y, s.w], [same(10.0), same(20.0), same(30.0)]);
    assert_eq!(s.h, Some(same(40.0)));
    assert_eq!(s.rotation, same(-90.0), "поворот как в Figma");
    assert_eq!(s.corner_radius, Some(same(4.0)));
    assert_eq!(s.clip_content, None, "обрезка — только у фреймов");
    let fills = s.fills.unwrap();
    assert!(!fills.mixed);
    assert_eq!(fills.list[0].color, "#d9d9d9ff");
}

#[test]
fn differing_values_are_mixed() {
    let mut p = Page::new();
    let a = p.add(None, NodeKind::Rect, at(10.0, 5.0, 30.0, 40.0));
    let b = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![Fill::solid(Color::rgba(255, 0, 0, 255))]),
            ..at(50.0, 5.0, 30.0, 40.0)
        },
    );
    let s = summarize(&p.scene(), &[a, b]);
    assert_eq!(
        s.x,
        Num {
            value: 10.0,
            mixed: true
        },
        "значение первого"
    );
    assert_eq!(s.y, same(5.0));
    let fills = s.fills.unwrap();
    assert!(fills.mixed && fills.list.is_empty());
    assert_eq!(s.name, None);
    assert_eq!(s.kind, Some(NodeType::Rect));
}

#[test]
fn fields_only_where_they_apply() {
    let mut p = Page::new();
    let r = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            corner_radius: Some(8.0),
            ..at(0.0, 0.0, 10.0, 10.0)
        },
    );
    let e = p.add(None, NodeKind::Ellipse, at(20.0, 0.0, 10.0, 10.0));
    let s = summarize(&p.scene(), &[r, e]);
    assert_eq!(s.kind, None, "разные типы");
    assert_eq!(
        s.corner_radius,
        Some(same(8.0)),
        "у эллипса радиуса нет — не «Смешанные»"
    );
    let l = p.add(None, NodeKind::Line, at(0.0, 50.0, 100.0, 0.0));
    let s = summarize(&p.scene(), &[l]);
    assert_eq!(
        (s.h, s.fills.is_none()),
        (None, true),
        "у линии нет высоты и заливок"
    );
    assert_eq!(s.strokes.unwrap().list[0].width, 1.0);
    let f = p.add(None, NodeKind::Frame, at(0.0, 100.0, 50.0, 50.0));
    let s = summarize(&p.scene(), &[f, r]);
    assert_eq!(
        s.clip_content.map(|c| (c.value, c.mixed)),
        Some((true, false))
    );
}

#[test]
fn group_shows_bounds_of_children() {
    let mut p = Page::new();
    let g = p.add(
        None,
        NodeKind::Group,
        PropsPatch {
            x: Some(100.0),
            y: Some(50.0),
            ..Default::default()
        },
    );
    p.add(Some(g), NodeKind::Rect, at(10.0, 20.0, 30.0, 30.0));
    p.add(Some(g), NodeKind::Rect, at(60.0, 20.0, 20.0, 60.0));
    let s = summarize(&p.scene(), &[g]);
    assert_eq!([s.x, s.y, s.w], [same(110.0), same(70.0), same(70.0)]);
    assert_eq!(s.h, Some(same(60.0)));
    assert!(
        s.fills.is_none() && s.strokes.is_none(),
        "у группы своих заливок нет"
    );
}

#[test]
fn paint_lists_are_capped() {
    let mut p = Page::new();
    let fills: Vec<Fill> = (0..100)
        .map(|i| Fill::solid(Color::rgba(i, 0, 0, 255)))
        .collect();
    let strokes: Vec<Stroke> = (0..=MAX_PAINTS)
        .map(|_| Stroke {
            color: Color::BLACK,
            width: 1.0,
            align: StrokeAlign::Center,
        })
        .collect();
    let r = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(fills),
            strokes: Some(strokes),
            ..at(0.0, 0.0, 10.0, 10.0)
        },
    );
    // Длиннее предела — как «Смешанные»: усечённый список панель не получает и
    // не может отправить обратно вместо полного.
    let s = summarize(&p.scene(), &[r]);
    let (f, st) = (s.fills.unwrap(), s.strokes.unwrap());
    assert!(f.mixed && f.list.is_empty() && st.mixed && st.list.is_empty());
}

// ---- Fix round 1 ----

fn two(a: PropsPatch, b: PropsPatch) -> ferrum_wasm::summary::Summary {
    let mut p = Page::new();
    let x = p.add(None, NodeKind::Rect, a);
    let y = p.add(None, NodeKind::Rect, b);
    summarize(&p.scene(), &[x, y])
}

#[test]
fn float_noise_is_not_mixed() {
    let s = two(
        PropsPatch {
            opacity: Some(0.1 + 0.2),
            corner_radius: Some(0.1 + 0.2),
            rotation: Some(360.0),
            ..at(0.1 + 0.2, 1e9, 30.0, 40.0)
        },
        PropsPatch {
            opacity: Some(0.3),
            corner_radius: Some(0.3),
            rotation: Some(0.0),
            ..at(0.3, 1e9 + 1e-4, 30.0, 40.0)
        },
    );
    assert!(!s.x.mixed && !s.y.mixed, "{:?} {:?}", s.x, s.y);
    assert!(!s.rotation.mixed, "360 и 0 — одно и то же");
    assert!(!s.opacity.mixed && !s.corner_radius.unwrap().mixed);
    let s = two(
        PropsPatch {
            rotation: Some(-1e-8),
            ..at(0.0, 0.0, 1.0, 1.0)
        },
        PropsPatch {
            rotation: Some(0.0),
            ..at(0.0, 0.0, 1.0, 1.0)
        },
    );
    assert!(!s.rotation.mixed, "{:?}", s.rotation);
    let s = two(
        PropsPatch {
            rotation: Some(180.0 + 1e-9),
            ..at(0.0, 0.0, 1.0, 1.0)
        },
        PropsPatch {
            rotation: Some(180.0 - 1e-9),
            ..at(0.0, 0.0, 1.0, 1.0)
        },
    );
    assert!(!s.rotation.mixed, "около 180 по обе стороны шва");
}

#[test]
fn real_differences_are_mixed() {
    let s = two(
        PropsPatch {
            opacity: Some(0.5),
            rotation: Some(10.0),
            ..at(1.0, 0.0, 30.0, 40.0)
        },
        PropsPatch {
            opacity: Some(0.5001),
            rotation: Some(10.01),
            ..at(1.001, 0.0, 30.0, 40.0)
        },
    );
    assert!(s.x.mixed && s.opacity.mixed && s.rotation.mixed);
    assert!(!s.y.mixed);
}

#[test]
fn paint_numbers_compare_with_tolerance_colors_exact() {
    let fill = |o: f64, c: Color| Fill {
        opacity: o,
        ..Fill::solid(c)
    };
    let stroke = |w: f64, c: Color| Stroke {
        color: c,
        width: w,
        align: StrokeAlign::Center,
    };
    let red = Color::rgba(255, 0, 0, 255);
    let near = Color::rgba(254, 0, 0, 255);
    let noise = |a: Option<Fill>, b: Option<Fill>, sa: Stroke, sb: Stroke| {
        two(
            PropsPatch {
                fills: a.map(|f| vec![f]),
                strokes: Some(vec![sa]),
                ..at(0.0, 0.0, 1.0, 1.0)
            },
            PropsPatch {
                fills: b.map(|f| vec![f]),
                strokes: Some(vec![sb]),
                ..at(0.0, 0.0, 1.0, 1.0)
            },
        )
    };
    let s = noise(
        Some(fill(0.1 + 0.2, red)),
        Some(fill(0.3, red)),
        stroke(0.1 + 0.2, red),
        stroke(0.3, red),
    );
    assert!(!s.fills.unwrap().mixed && !s.strokes.unwrap().mixed);
    let s = noise(
        Some(fill(0.3, red)),
        Some(fill(0.3, near)),
        stroke(0.3, red),
        stroke(0.3, near),
    );
    assert!(
        s.fills.unwrap().mixed && s.strokes.unwrap().mixed,
        "цвета точно"
    );
    let s = noise(
        Some(fill(0.3, red)),
        Some(fill(0.31, red)),
        stroke(0.3, red),
        stroke(0.31, red),
    );
    assert!(s.fills.unwrap().mixed && s.strokes.unwrap().mixed);
}

#[test]
fn rotated_flipped_and_nested_nodes() {
    let mut p = Page::new();
    let f = p.add(None, NodeKind::Frame, at(100.0, 100.0, 500.0, 500.0));
    let r = p.add(
        Some(f),
        NodeKind::Rect,
        PropsPatch {
            rotation: Some(90.0),
            flip_x: Some(true),
            ..at(10.0, 20.0, 30.0, 40.0)
        },
    );
    let s = summarize(&p.scene(), &[r]);
    assert_eq!(
        [s.x, s.y, s.w],
        [same(10.0), same(20.0), same(30.0)],
        "локальные"
    );
    assert_eq!((s.h, s.rotation), (Some(same(40.0)), same(90.0)));
    let fl = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            flip_y: Some(true),
            ..at(10.0, 20.0, 30.0, 40.0)
        },
    );
    let s = summarize(&p.scene(), &[r, fl]);
    assert!(s.rotation.mixed && !s.x.mixed && !s.w.mixed);
}

#[test]
fn image_fill_empty_list_and_paint_limit_boundary() {
    let mut p = Page::new();
    let img = Fill {
        kind: ferrum_doc::FillKind::Image,
        image_hash: Some("ab".repeat(32)),
        scale_mode: ferrum_doc::ScaleMode::Tile,
        ..Fill::solid(Color::WHITE)
    };
    let a = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![img]),
            ..at(0.0, 0.0, 1.0, 1.0)
        },
    );
    let f = summarize(&p.scene(), &[a]).fills.unwrap();
    assert!(!f.mixed);
    assert_eq!(f.list[0].kind, ferrum_wasm::summary::PaintKind::Image);
    assert_eq!(
        f.list[0].image_hash.as_deref(),
        Some("ab".repeat(32).as_str())
    );
    assert_eq!(
        f.list[0].scale_mode,
        Some(ferrum_wasm::summary::ScaleModeDto::Tile)
    );
    let e = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![]),
            strokes: Some(vec![]),
            ..at(0.0, 0.0, 1.0, 1.0)
        },
    );
    let s = summarize(&p.scene(), &[e]);
    let (f, st) = (s.fills.unwrap(), s.strokes.unwrap());
    assert!(!f.mixed && f.list.is_empty() && !st.mixed && st.list.is_empty());
    let n = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some((0..MAX_PAINTS).map(|_| Fill::solid(Color::BLACK)).collect()),
            ..at(0.0, 0.0, 1.0, 1.0)
        },
    );
    let f = summarize(&p.scene(), &[n]).fills.unwrap();
    assert!(
        !f.mixed && f.list.len() == MAX_PAINTS,
        "ровно предел — целиком"
    );
}

// ---- Сводка и круговая правка через редактор ----

mod live {
    use super::*;
    use ferrum_editor::{Editor, NumField, PanelEdit, Phase, Platform};
    use kurbo::Size;

    fn editor() -> (Editor, NodeId) {
        let mut ed = Editor::new(
            Document::new(),
            None,
            Size::new(1000.0, 800.0),
            1.0,
            Platform::Windows,
        )
        .unwrap();
        let id = ed
            .transact(|tx| {
                let id = tx.create_node(None, None, NodeKind::Rect)?;
                tx.set_props(id, &at(10.0, 20.0, 30.0, 40.0))?;
                Ok(vec![id])
            })
            .unwrap()[0];
        ed.select(&[id]);
        (ed, id)
    }

    fn x(v: f64) -> PanelEdit {
        PanelEdit::Number {
            field: NumField::X,
            value: v,
            relative: false,
        }
    }

    #[test]
    fn preview_and_commit_change_the_summary() {
        let (mut ed, id) = editor();
        let sel = ed.selection().to_vec();
        assert_eq!(summarize(ed.scene(), &sel).x, same(10.0));
        ed.edit_selection(&x(55.0), Phase::Preview);
        assert_eq!(summarize(ed.scene(), &sel).x, same(55.0), "превью видно");
        ed.edit_selection(&x(0.0), Phase::Cancel);
        assert_eq!(summarize(ed.scene(), &sel).x, same(10.0), "отмена жеста");
        ed.edit_selection(&x(70.0), Phase::Commit);
        assert_eq!(summarize(ed.scene(), &sel).x, same(70.0), "запись");
        let _ = id;
    }

    #[test]
    fn shown_values_applied_back_make_no_step() {
        let (mut ed, id) = editor();
        let g = ed
            .transact(|tx| {
                let g = tx.create_node(None, None, NodeKind::Group)?;
                let c = tx.create_node(Some(g), None, NodeKind::Rect)?;
                tx.set_props(c, &at(3.3, 4.4, 5.5, 6.6))?;
                tx.set_props(
                    g,
                    &PropsPatch {
                        rotation: Some(270.0),
                        ..Default::default()
                    },
                )?;
                Ok(vec![g])
            })
            .unwrap()[0];
        ed.transact(|tx| {
            tx.set_props(
                id,
                &PropsPatch {
                    rotation: Some(270.0),
                    x: Some(10.3),
                    w: Some(30.7),
                    ..Default::default()
                },
            )?;
            Ok(vec![])
        })
        .unwrap();
        for target in [id, g] {
            ed.select(&[target]);
            let s = summarize(ed.scene(), &[target]);
            let steps = ed.step_count();
            for (field, value) in [
                (NumField::X, s.x.value),
                (NumField::Y, s.y.value),
                (NumField::W, s.w.value),
                (NumField::Rotation, s.rotation.value),
            ] {
                ed.edit_selection(
                    &PanelEdit::Number {
                        field,
                        value,
                        relative: false,
                    },
                    Phase::Commit,
                );
            }
            assert_eq!(ed.step_count(), steps, "показанное значение — не правка");
        }
    }
}

// --- стиль обводки: правила строк и «Смешанные» ---

use ferrum_doc::{StrokeCap, StrokeJoin, VectorNetwork, VectorPoint, VectorSegment};
use ferrum_wasm::summary::{StrokeCapDto, StrokeJoinDto};
use std::sync::Arc;

fn net(points: usize, segs: &[(u32, u32)]) -> Arc<VectorNetwork> {
    Arc::new(VectorNetwork {
        points: (0..points)
            .map(|i| VectorPoint::new(i as f64 * 10.0, (i % 2) as f64 * 10.0))
            .collect(),
        segments: segs
            .iter()
            .map(|&(a, b)| VectorSegment::line(a, b))
            .collect(),
        regions: vec![],
    })
}

fn open_vector(p: &mut Page, patch: PropsPatch) -> NodeId {
    p.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(net(3, &[(0, 1), (1, 2)])),
            ..patch
        },
    )
}

fn closed_vector(p: &mut Page) -> NodeId {
    p.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(net(3, &[(0, 1), (1, 2), (2, 0)])),
            ..Default::default()
        },
    )
}

#[test]
fn stroke_style_rows_follow_the_node_kinds() {
    let mut p = Page::new();
    let line = p.add(None, NodeKind::Line, at(0.0, 0.0, 50.0, 0.0));
    let rect = p.add(None, NodeKind::Rect, at(0.0, 0.0, 50.0, 50.0));
    let round = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            corner_radius: Some(8.0),
            ..at(0.0, 0.0, 50.0, 50.0)
        },
    );
    let ellipse = p.add(None, NodeKind::Ellipse, at(0.0, 0.0, 50.0, 50.0));
    let group = p.add(None, NodeKind::Group, PropsPatch::default());
    let text = p.add(None, NodeKind::Text, at(0.0, 0.0, 50.0, 20.0));
    let open = open_vector(&mut p, PropsPatch::default());
    let closed = closed_vector(&mut p);
    let sc = p.scene();
    let st = |ids: &[NodeId]| summarize(&sc, ids).stroke_style;
    let rows = |ids: &[NodeId]| st(ids).map(|s| (s.ends, s.corners));
    assert_eq!(rows(&[line]), Some((true, false)));
    assert_eq!(rows(&[rect]), Some((false, true)));
    assert_eq!(rows(&[line, rect]), Some((true, true)));
    assert_eq!(rows(&[open]), Some((true, true)));
    assert_eq!(rows(&[closed]), Some((false, true)));
    // Скруглённый прямоугольник и эллипс — без строк; группа и текст — без стиля.
    assert_eq!(rows(&[round]), None);
    assert_eq!(rows(&[ellipse]), None);
    assert_eq!(rows(&[group]), None);
    assert_eq!(rows(&[text]), None);
    // Группа и текст в выделении не мешают.
    assert_eq!(rows(&[group, text, closed]), Some((false, true)));
    // У вектора-«Т» (ветвление) свободные концы есть.
    let t = p.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(net(4, &[(0, 1), (1, 2), (1, 3)])),
            ..Default::default()
        },
    );
    let sc = p.scene();
    assert_eq!(
        summarize(&sc, &[t]).stroke_style.map(|s| s.ends),
        Some(true)
    );
}

#[test]
fn stroke_style_is_mixed_across_a_selection() {
    let mut p = Page::new();
    let a = open_vector(
        &mut p,
        PropsPatch {
            stroke_cap_start: Some(StrokeCap::Round),
            stroke_cap_end: Some(StrokeCap::Square),
            stroke_join: Some(StrokeJoin::Bevel),
            stroke_miter_limit: Some(8.0),
            ..Default::default()
        },
    );
    let b = open_vector(
        &mut p,
        PropsPatch {
            stroke_cap_start: Some(StrokeCap::Round),
            ..Default::default()
        },
    );
    let sc = p.scene();
    let st = summarize(&sc, &[a, b]).stroke_style.unwrap();
    assert_eq!(st.cap_start, Some(StrokeCapDto::Round));
    assert_eq!(st.cap_end, None);
    assert_eq!(st.join, None);
    assert!(st.miter_limit.mixed);
    let one = summarize(&sc, &[a]).stroke_style.unwrap();
    assert_eq!(one.join, Some(StrokeJoinDto::Bevel));
    assert_eq!(one.miter_limit, same(8.0));
}

#[test]
fn hidden_rows_do_not_cause_false_mixed() {
    let mut p = Page::new();
    // У линии соединение и предел скрыты — их отличия не «смешивают» вектор.
    let line = p.add(
        None,
        NodeKind::Line,
        PropsPatch {
            stroke_join: Some(StrokeJoin::Round),
            stroke_miter_limit: Some(9.0),
            ..at(0.0, 0.0, 50.0, 0.0)
        },
    );
    let v = closed_vector(&mut p);
    let sc = p.scene();
    let st = summarize(&sc, &[line, v]).stroke_style.unwrap();
    assert_eq!(st.join, Some(StrokeJoinDto::Miter));
    assert_eq!(st.miter_limit, same(4.0));
    // Закрытый вектор не подмешивает окончания к линии.
    assert_eq!(st.cap_start, Some(StrokeCapDto::None));
}
