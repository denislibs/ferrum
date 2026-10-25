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
    // Тот же ключ: подменённый id той же длины не читается — берётся кэш.
    let ghost: NodeId = "7@7".parse().unwrap();
    assert_eq!(c.get(&scene, &[ghost], 1).count, 1, "кэш по ключу");
    assert_eq!(c.get(&scene, &[], 2).count, 0, "версия выделения сменилась");
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

// ---- Кэш и круговая правка через редактор ----

mod live {
    use super::*;
    use ferrum_editor::{Editor, NumField, PanelEdit, Phase, Platform};
    use ferrum_wasm::summary::SummaryCache;
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
    fn preview_and_commit_change_the_cached_summary() {
        let (mut ed, id) = editor();
        let mut c = SummaryCache::new();
        let sel = ed.selection().to_vec();
        let v = ed.selection_version();
        assert_eq!(c.get(ed.scene(), &sel, v).x, same(10.0));
        ed.edit_selection(&x(55.0), Phase::Preview);
        assert_eq!(c.get(ed.scene(), &sel, v).x, same(55.0), "превью видно");
        ed.edit_selection(&x(0.0), Phase::Cancel);
        assert_eq!(c.get(ed.scene(), &sel, v).x, same(10.0), "отмена жеста");
        ed.edit_selection(&x(70.0), Phase::Commit);
        assert_eq!(c.get(ed.scene(), &sel, v).x, same(70.0), "запись");
        let _ = id;
    }

    #[test]
    fn new_scene_instance_invalidates_the_cache() {
        let mut p1 = Page::new();
        let a = p1.add(None, NodeKind::Rect, at(1.0, 0.0, 1.0, 1.0));
        let s1 = p1.scene();
        let mut c = SummaryCache::new();
        assert_eq!(c.get(&s1, &[a], 1).x, same(1.0));
        // Тот же документ, новая сцена: счётчики могли совпасть, экземпляр — нет.
        let s2 = p1.scene();
        assert_eq!(
            (s1.change_counter(), s1.structure_stamp()),
            (s2.change_counter(), s2.structure_stamp())
        );
        p1.doc
            .transact(p1.doc.pages()[0].0, vec![], |tx| {
                tx.set_props(a, &at(9.0, 0.0, 1.0, 1.0))?;
                Ok(vec![])
            })
            .unwrap();
        let s3 = p1.scene();
        assert_eq!(c.get(&s3, &[a], 1).x, same(9.0));
    }

    #[test]
    fn ids_length_guards_the_cache() {
        let mut p = Page::new();
        let a = p.add(None, NodeKind::Rect, at(1.0, 0.0, 1.0, 1.0));
        let b = p.add(None, NodeKind::Rect, at(2.0, 0.0, 1.0, 1.0));
        let s = p.scene();
        let mut c = SummaryCache::new();
        assert_eq!(c.get(&s, &[a], 1).count, 1);
        assert_eq!(c.get(&s, &[a, b], 1).count, 2);
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
