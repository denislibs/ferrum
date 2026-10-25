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

#[test]
fn cache_recomputes_only_on_change() {
    use ferrum_wasm::summary::SummaryCache;
    let mut p = Page::new();
    let a = p.add(None, NodeKind::Rect, at(1.0, 2.0, 3.0, 4.0));
    let scene = p.scene();
    let mut c = SummaryCache::new();
    assert_eq!(c.get(&scene, &[a], 1).x, same(1.0));
    // Тот же ключ: выделение подменено, но версия не менялась — берётся кэш.
    assert_eq!(c.get(&scene, &[], 1).count, 1, "кэш по ключу");
    assert_eq!(c.get(&scene, &[], 2).count, 0, "версия выделения сменилась");
}
