//! Геометрия жестов, привязки и правила выделения на сцене — без редактора.

use std::collections::HashSet;

use ferrum_doc::{Document, NodeId, NodeKind, PageId, Props, PropsPatch};
use ferrum_editor::geometry::*;
use ferrum_editor::selection::{self, Pick};
use ferrum_editor::snap::{Snap, Snapper};
use ferrum_editor::{frame_presets, parse_presets};
use ferrum_render::Camera;
use ferrum_scene::{shape, Draft, Scene};
use kurbo::{Affine, Point, Rect, Size};

fn patch(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Страница по списку (родитель — индекс раньше в списке, тип, свойства).
fn page(nodes: &[(Option<usize>, NodeKind, PropsPatch)]) -> (Document, PageId, Vec<NodeId>) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |tx| {
            let mut ids: Vec<NodeId> = Vec::new();
            for (parent, kind, p) in nodes {
                let id = tx.create_node(parent.map(|i| ids[i]), None, *kind)?;
                tx.set_props(id, p)?;
                ids.push(id);
            }
            Ok(ids)
        })
        .unwrap();
    (doc, page, ids)
}

#[test]
fn decompose_recovers_pose_and_scale() {
    for (x, y, rotation, w, h) in [
        (0.0, 0.0, 0.0, 10.0, 20.0),
        (15.5, -7.0, 30.0, 100.0, 40.0),
        (-3.0, 4.0, 270.0, 5.0, 5.0),
    ] {
        let props = Props {
            x,
            y,
            rotation,
            w,
            h,
            ..Props::defaults(NodeKind::Rect)
        };
        let m = shape::local_transform(&props);
        let p = decompose(m, w, h);
        assert!(
            close(p.x, x) && close(p.y, y) && close(p.w, w) && close(p.h, h),
            "{p:?}"
        );
        assert!(close(p.rotation, rotation), "{p:?}");
        let s = decompose(m * Affine::scale_non_uniform(2.0, 3.0), w, h);
        assert!(close(s.w, 2.0 * w) && close(s.h, 3.0 * h) && close(s.rotation, rotation));
    }
    assert_eq!(normalize_deg(-90.0), 270.0);
    assert_eq!(normalize_deg(359.9999999999999), 0.0);
    assert_eq!(normalize_deg(720.0 + 15.0), 15.0);
    assert_eq!(snap_angle(37.0, 15.0), 30.0);
}

#[test]
fn resize_rect_anchors_aspect_and_center() {
    let r = Rect::new(0.0, 0.0, 100.0, 50.0);
    let br = resize_rect(
        r,
        Handle::BottomRight,
        Point::new(150.0, 80.0),
        false,
        false,
    );
    assert_eq!(br, Rect::new(0.0, 0.0, 150.0, 80.0));
    // За противоположную сторону: рамка меняет сторону, без отражения.
    let flip = resize_rect(r, Handle::Right, Point::new(-30.0, 999.0), false, false);
    assert_eq!(flip, Rect::new(-30.0, 0.0, 0.0, 50.0));
    // Пропорции: угол — по большему изменению; сторона — другая ось от центра.
    let k = resize_rect(r, Handle::BottomRight, Point::new(300.0, 60.0), true, false);
    assert_eq!(k, Rect::new(0.0, 0.0, 300.0, 150.0));
    let e = resize_rect(r, Handle::Bottom, Point::new(0.0, 100.0), true, false);
    assert_eq!(e, Rect::new(-50.0, 0.0, 150.0, 100.0));
    // От центра.
    let c = resize_rect(r, Handle::TopLeft, Point::new(-10.0, -5.0), false, true);
    assert_eq!(c, Rect::new(-10.0, -5.0, 110.0, 55.0));
    let m = rect_map(r, Rect::new(10.0, 10.0, 210.0, 60.0));
    assert_eq!(m * Point::new(100.0, 50.0), Point::new(210.0, 60.0));
    assert_eq!(m * Point::ZERO, Point::new(10.0, 10.0));
    // Нулевая сторона не масштабируется.
    let line = Rect::new(0.0, 0.0, 0.0, 10.0);
    let m = rect_map(line, Rect::new(5.0, 0.0, 25.0, 20.0));
    assert_eq!(m * Point::new(0.0, 10.0), Point::new(5.0, 20.0));
}

#[test]
fn group_scale_patches_children_and_rigid_moves_roots() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Group, PropsPatch::default()),
        (Some(0), NodeKind::Rect, patch(10.0, 10.0, 20.0, 20.0)),
        (Some(0), NodeKind::Line, patch(40.0, 10.0, 30.0, 0.0)),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let b = bases(&scene, &[ids[0]], true);
    assert_eq!(b.len(), 3, "группа и её дети");
    let patches = scale_patches(&b, Affine::scale(2.0), false);
    assert_eq!(patches.len(), 2, "у группы своей правки нет");
    let rect = &patches[0].1;
    assert_eq!(
        (rect.x, rect.y, rect.w, rect.h),
        (Some(20.0), Some(20.0), Some(40.0), Some(40.0))
    );
    let line = &patches[1].1;
    assert_eq!((line.w, line.h), (Some(60.0), None), "у линии высоты нет");
    // Поворот на 90° вокруг (20, 20): прямоугольник встаёт с левым верхним углом (30, 10).
    let t = Affine::translate((20.0, 20.0))
        * Affine::rotate(std::f64::consts::FRAC_PI_2)
        * Affine::translate((-20.0, -20.0));
    let rb = bases(&scene, &[ids[1]], false);
    let p = &rigid_patches(&rb, t, false)[0].1;
    assert!(
        close(p.x.unwrap(), 30.0) && close(p.y.unwrap(), 10.0),
        "{p:?}"
    );
    assert_eq!(p.rotation, Some(270.0));
    // Пиксельная привязка округляет положение неповёрнутых и размеры.
    let s = scale_patches(&rb, Affine::scale(1.33), true);
    assert_eq!((s[0].1.x, s[0].1.w), (Some(13.0), Some(27.0)));
}

#[test]
fn selection_box_and_top_level() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Group, PropsPatch::default()),
        (Some(0), NodeKind::Rect, patch(10.0, 10.0, 20.0, 20.0)),
        (None, NodeKind::Rect, patch(100.0, 0.0, 10.0, 10.0)),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert_eq!(top_level(&scene, &[ids[1], ids[0]]), vec![ids[0]]);
    let one = SelectionBox::of(&scene, &[ids[1]]).unwrap();
    assert_eq!(one.rect, Rect::new(0.0, 0.0, 20.0, 20.0));
    assert_eq!(one.point(1.0, 1.0), Point::new(30.0, 30.0));
    let both = SelectionBox::of(&scene, &[ids[0], ids[2]]).unwrap();
    assert_eq!(both.rect, Rect::new(10.0, 0.0, 110.0, 30.0));
    assert!(both.axis_aligned());
}

#[test]
fn snapper_finds_siblings_and_parent_frame_only() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Frame, patch(0.0, 0.0, 400.0, 400.0)),
        (Some(0), NodeKind::Rect, patch(100.0, 100.0, 50.0, 50.0)),
        (Some(0), NodeKind::Rect, patch(300.0, 300.0, 20.0, 20.0)),
        // Другой родитель и скрытый сосед — не кандидаты.
        (None, NodeKind::Rect, patch(203.0, 500.0, 10.0, 10.0)),
        (
            Some(0),
            NodeKind::Rect,
            PropsPatch {
                visible: Some(false),
                ..patch(251.0, 10.0, 10.0, 10.0)
            },
        ),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let moving = [ids[2]];
    let snapper = Snapper::new(&scene, &moving);
    let view = Rect::new(-1000.0, -1000.0, 1000.0, 1000.0);
    // Левый край на 152: ближе всего правый край соседа (150).
    let s = snapper.snap(
        &scene,
        &[152.0],
        &[],
        Rect::new(152.0, 300.0, 172.0, 320.0),
        4.0,
        view,
    );
    assert_eq!(s.dx, Some(-2.0));
    assert_eq!(s.dy, None);
    assert_eq!(s.lines.len(), 1);
    assert_eq!(s.lines[0].p0.x, 150.0);
    assert_eq!((s.lines[0].p0.y, s.lines[0].p1.y), (100.0, 320.0));
    // 205 — в 2 px от узла другого родителя (203), но он не сосед; центр фрейма — 200.
    let s = snapper.snap(
        &scene,
        &[205.0],
        &[],
        Rect::new(205.0, 300.0, 225.0, 320.0),
        4.0,
        view,
    );
    assert_eq!(s.dx, None, "{s:?}");
    let s = snapper.snap(
        &scene,
        &[202.0],
        &[],
        Rect::new(202.0, 300.0, 222.0, 320.0),
        4.0,
        view,
    );
    assert_eq!(s.dx, Some(-2.0), "центр родительского фрейма");
    // Скрытый сосед (251) не притягивает.
    let s = snapper.snap(
        &scene,
        &[253.0],
        &[],
        Rect::new(253.0, 300.0, 273.0, 320.0),
        4.0,
        view,
    );
    assert_eq!(s.dx, None);
    // Кандидаты вне видимой части не находятся.
    let small = Rect::new(500.0, 500.0, 600.0, 600.0);
    let s = snapper.snap(
        &scene,
        &[152.0],
        &[],
        Rect::new(152.0, 300.0, 172.0, 320.0),
        4.0,
        small,
    );
    assert_eq!(s.dx, None);
}

#[test]
fn selection_rules_on_scene() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Frame, patch(0.0, 0.0, 400.0, 400.0)),
        (Some(0), NodeKind::Group, PropsPatch::default()),
        (Some(1), NodeKind::Rect, patch(10.0, 10.0, 50.0, 50.0)),
        (Some(0), NodeKind::Frame, patch(200.0, 200.0, 100.0, 100.0)),
        (None, NodeKind::Rect, patch(500.0, 0.0, 50.0, 50.0)),
        (None, NodeKind::Frame, patch(600.0, 0.0, 50.0, 50.0)),
    ]);
    let [f, g, a, inner, r, empty] = [ids[0], ids[1], ids[2], ids[3], ids[4], ids[5]];
    let scene = Scene::from_page(doc.page(page).unwrap());
    let hit = selection::top_hit(&scene, Point::new(20.0, 20.0), 0.0).unwrap();
    assert_eq!(scene.at(hit).id, a);
    assert_eq!(selection::resolve(&scene, hit, &[], Pick::Normal), Some(g));
    assert_eq!(selection::resolve(&scene, hit, &[], Pick::Deepest), Some(a));
    assert_eq!(selection::deeper(&scene, hit, &[g]), Some(a));
    // Пустое место фрейма верхнего уровня с детьми — ничего; без детей — сам фрейм.
    let fh = selection::top_hit(&scene, Point::new(150.0, 350.0), 0.0).unwrap();
    assert_eq!(selection::resolve(&scene, fh, &[], Pick::Normal), None);
    let eh = selection::top_hit(&scene, Point::new(620.0, 20.0), 0.0).unwrap();
    assert_eq!(
        selection::resolve(&scene, eh, &[], Pick::Normal),
        Some(empty)
    );
    assert_eq!(
        selection::root_frame_at(&scene, Point::new(150.0, 350.0), 0.0),
        Some(f)
    );
    // Куда класть: самый глубокий фрейм, не из перетаскиваемых.
    let p = Point::new(250.0, 250.0);
    assert_eq!(
        selection::drop_frame_at(&scene, p, 0.0, &HashSet::new()),
        Some(inner)
    );
    assert_eq!(
        selection::drop_frame_at(&scene, p, 0.0, &HashSet::from([inner])),
        Some(f)
    );
    assert_eq!(
        selection::drop_frame_at(&scene, Point::new(520.0, 20.0), 0.0, &HashSet::new()),
        None
    );
    // Название фрейма — полоса над его верхним краем.
    let mut cam = Camera::new(Size::new(1000.0, 800.0), 1.0);
    cam.set_center(Point::new(500.0, 400.0));
    assert_eq!(
        selection::frame_label_at(&scene, &cam, Point::new(620.0, -10.0)),
        Some(empty)
    );
    assert_eq!(
        selection::frame_label_at(&scene, &cam, Point::new(620.0, -40.0)),
        None
    );
    // Рамка: в корне фрейм — только целиком; в контексте фрейма — его дети.
    let m = selection::marquee(&scene, Rect::new(-10.0, -10.0, 560.0, 80.0), None);
    assert_eq!(m, vec![r]);
    let m = selection::marquee(&scene, Rect::new(-10.0, -10.0, 700.0, 500.0), None);
    assert_eq!(m, vec![f, r, empty]);
    let m = selection::marquee(&scene, Rect::new(5.0, 5.0, 250.0, 250.0), Some(f));
    assert_eq!(m, vec![g, inner]);
}

#[test]
fn builtin_presets_match_spec() {
    let names: Vec<(&str, f64, f64)> = frame_presets()
        .iter()
        .map(|p| (p.name.as_str(), p.width, p.height))
        .collect();
    assert_eq!(
        names,
        vec![
            ("iPhone 16", 393.0, 852.0),
            ("iPhone 16 Pro Max", 440.0, 956.0),
            ("Android Compact", 412.0, 917.0),
            ("iPad mini", 744.0, 1133.0),
            ("iPad Pro 11\"", 834.0, 1194.0),
            ("Desktop", 1440.0, 1024.0),
            ("MacBook Air", 1280.0, 832.0),
            ("1920×1080", 1920.0, 1080.0),
            ("Instagram post", 1080.0, 1350.0),
            ("Story", 1080.0, 1920.0),
            ("A4", 595.0, 842.0),
        ]
    );
    assert!(parse_presets(
        r#"{"version":1,"presets":[{"group":"x","name":"y","width":1e9,"height":5}]}"#
    )
    .is_err());
}

/// Центр узла после правки: локальная матрица из новых свойств и центр в ней.
fn fix1_centre(props: &Props, patch: &PropsPatch) -> Point {
    let mut p = props.clone();
    patch.apply_to(&mut p);
    shape::local_transform(&p) * Point::new(p.w / 2.0, p.h / 2.0)
}

/// Свойства прямоугольника с поворотом (для расчёта ожидаемого центра).
fn fix1_rotated(x: f64, y: f64, rotation: f64, w: f64, h: f64) -> Props {
    Props {
        x,
        y,
        rotation,
        w,
        h,
        ..Props::defaults(NodeKind::Rect)
    }
}

#[test]
fn fix1_scaled_rotated_child_centre_maps_exactly() {
    // Квадрат 10×10 под 45° в группе, растянутой ×2 по x: центр должен лечь в (14.14, 0).
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Group, PropsPatch::default()),
        (
            Some(0),
            NodeKind::Rect,
            PropsPatch {
                rotation: Some(45.0),
                ..patch(0.0, 0.0, 10.0, 10.0)
            },
        ),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let child = fix1_rotated(0.0, 0.0, 45.0, 10.0, 10.0);
    let t = Affine::scale_non_uniform(2.0, 1.0);
    let want = t * shape::local_transform(&child) * Point::new(5.0, 5.0);
    assert!(
        (want.x - 14.142135).abs() < 1e-5 && want.y.abs() < 1e-9,
        "{want:?}"
    );
    let patches = scale_patches(&bases(&scene, &[ids[0]], true), t, false);
    assert_eq!(patches[0].0, ids[1]);
    let got = fix1_centre(&child, &patches[0].1);
    assert!(
        (got.x - want.x).abs() < 1e-6 && (got.y - want.y).abs() < 1e-6,
        "центр {got:?}, ожидался {want:?}"
    );
}

#[test]
fn fix1_rotated_child_in_multi_selection_resize_keeps_centre() {
    let (mut doc, page, ids) = page(&[
        (
            None,
            NodeKind::Rect,
            PropsPatch {
                rotation: Some(45.0),
                ..patch(0.0, 0.0, 10.0, 10.0)
            },
        ),
        (None, NodeKind::Rect, patch(50.0, 0.0, 10.0, 10.0)),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let a = fix1_rotated(0.0, 0.0, 45.0, 10.0, 10.0);
    let t = Affine::translate((3.0, 4.0)) * Affine::scale_non_uniform(2.0, 0.5);
    let want = t * shape::local_transform(&a) * Point::new(5.0, 5.0);
    let patches = scale_patches(&bases(&scene, &ids, false), t, false);
    assert_eq!(patches[0].0, ids[0]);
    let got = fix1_centre(&a, &patches[0].1);
    assert!(
        (got.x - want.x).abs() < 1e-6 && (got.y - want.y).abs() < 1e-6,
        "центр {got:?}, ожидался {want:?}"
    );
}

#[test]
fn fix1_snap_tie_break_is_stable_after_rebuild() {
    // Цели 150 (справа от соседа A) и 154 (слева от соседа B) при движущемся левом
    // крае 152: |d| равны, выигрывает меньшая цель 150, в любом порядке узлов.
    for b_first in [false, true] {
        let mut nodes = vec![(None, NodeKind::Frame, patch(0.0, 0.0, 400.0, 400.0))];
        let a = (Some(0), NodeKind::Rect, patch(100.0, 100.0, 50.0, 50.0));
        let b = (Some(0), NodeKind::Rect, patch(154.0, 300.0, 20.0, 20.0));
        let m = (Some(0), NodeKind::Rect, patch(300.0, 300.0, 20.0, 20.0));
        if b_first {
            nodes.extend([b, a, m]);
        } else {
            nodes.extend([a, b, m]);
        }
        let (mut doc, page, ids) = page(&nodes);
        let moving = *ids.last().unwrap();
        let view = Rect::new(-1000.0, -1000.0, 1000.0, 1000.0);
        let rect = Rect::new(152.0, 300.0, 172.0, 320.0);
        let scene = Scene::from_page(doc.page(page).unwrap());
        let s = Snapper::new(&scene, &[moving]).snap(&scene, &[152.0], &[], rect, 4.0, view);
        assert_eq!(s.dx, Some(-2.0), "порядок b_first={b_first}: {s:?}");
        // После пересборки сцены (другой порядок R-tree) ответ тот же.
        let scene2 = Scene::from_page(doc.page(page).unwrap());
        let s2 = Snapper::new(&scene2, &[moving]).snap(&scene2, &[152.0], &[], rect, 4.0, view);
        assert_eq!(
            s2.dx,
            Some(-2.0),
            "после пересборки, b_first={b_first}: {s2:?}"
        );
    }
}

#[test]
fn fix1_non_finite_snap_axes_are_ignored() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Frame, patch(0.0, 0.0, 400.0, 400.0)),
        (Some(0), NodeKind::Rect, patch(100.0, 100.0, 50.0, 50.0)),
        (Some(0), NodeKind::Rect, patch(300.0, 300.0, 20.0, 20.0)),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let snapper = Snapper::new(&scene, &[ids[2]]);
    let view = Rect::new(-1000.0, -1000.0, 1000.0, 1000.0);
    let rect = Rect::new(152.0, 300.0, 172.0, 320.0);
    // Нечисловые координаты: ни запроса, ни привязки.
    let s = snapper.snap(&scene, &[f64::NAN], &[], rect, 4.0, view);
    assert_eq!(s, Snap::default());
    // Нечисловая ось не отключает другую: x привязывается, y — нет.
    let s = snapper.snap(&scene, &[152.0], &[f64::INFINITY], rect, 4.0, view);
    assert_eq!(s.dx, Some(-2.0));
    assert_eq!(s.dy, None);
}

#[test]
fn fix1_non_finite_geometry_returns_base() {
    let (mut doc, page, ids) = page(&[(None, NodeKind::Rect, patch(10.0, 10.0, 20.0, 20.0))]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let b = bases(&scene, &ids, false);
    let nan = Affine::new([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let fmt = |v: Vec<(NodeId, PropsPatch)>| format!("{v:?}");
    assert_eq!(
        fmt(rigid_patches(&b, nan, false)),
        fmt(rigid_patches(&b, Affine::IDENTITY, false))
    );
    assert_eq!(
        fmt(scale_patches(&b, nan, false)),
        fmt(scale_patches(&b, Affine::IDENTITY, false))
    );
    let r = Rect::new(0.0, 0.0, 100.0, 50.0);
    assert_eq!(
        resize_rect(
            r,
            Handle::BottomRight,
            Point::new(f64::NAN, 3.0),
            false,
            false
        ),
        r
    );
    assert_eq!(
        rect_map(r, Rect::new(f64::NAN, 0.0, 1.0, 1.0)),
        Affine::IDENTITY
    );
    assert!(snap_angle(f64::NAN, 15.0).is_nan());
}

#[test]
fn fix1_collapsed_rotated_node_keeps_rotation() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Group, PropsPatch::default()),
        (
            Some(0),
            NodeKind::Rect,
            PropsPatch {
                rotation: Some(30.5),
                ..patch(10.0, 10.0, 20.0, 20.0)
            },
        ),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Масштаб 0 схлопывает ребёнка: угол не меняется, поэтому правки поворота нет.
    let p = scale_patches(&bases(&scene, &[ids[0]], true), Affine::scale(0.0), false);
    assert_eq!(p[0].1.rotation, None, "{:?}", p[0].1);
}

#[test]
fn fix1_translate_only_has_no_rotation_patch() {
    let (mut doc, page, ids) = page(&[(
        None,
        NodeKind::Rect,
        PropsPatch {
            rotation: Some(30.5),
            ..patch(10.0, 10.0, 20.0, 20.0)
        },
    )]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let r = rigid_patches(
        &bases(&scene, &ids, false),
        Affine::translate((5.0, 5.0)),
        false,
    );
    assert_eq!(r[0].1.rotation, None, "{:?}", r[0].1);
    assert_eq!(r[0].1.x, Some(15.0));
}

#[test]
fn fix1_marquee_full_enclosure_is_inclusive_and_normalised() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Frame, patch(0.0, 0.0, 400.0, 400.0)),
        (None, NodeKind::Rect, patch(500.0, 0.0, 50.0, 50.0)),
    ]);
    let [f] = [ids[0]];
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Правый и нижний края фрейма совпадают с краями рамки: это полное охватывание.
    let m = selection::marquee(&scene, Rect::new(-10.0, -10.0, 400.0, 400.0), None);
    assert_eq!(m, vec![f]);
    // Рамка с обратным порядком углов даёт тот же результат.
    let m2 = selection::marquee(&scene, Rect::new(400.0, 400.0, -10.0, -10.0), None);
    assert_eq!(m2, m);
}

#[test]
fn fix1_root_frame_at_skips_drafts() {
    let (mut doc, page, ids) = page(&[(None, NodeKind::Frame, patch(0.0, 0.0, 400.0, 400.0))]);
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    assert_eq!(
        selection::root_frame_at(&scene, Point::new(20.0, 20.0), 0.0),
        Some(ids[0])
    );
    // Черновик фрейма (id, которого нет в документе) не выбирается рамкой.
    let d: NodeId = "0@18446744073709551615".parse().unwrap();
    scene.set_drafts(&[Draft {
        id: d,
        parent: None,
        props: Props {
            x: 500.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            ..Props::defaults(NodeKind::Frame)
        },
    }]);
    assert_eq!(
        selection::root_frame_at(&scene, Point::new(550.0, 50.0), 0.0),
        None
    );
}

#[test]
fn fix1_pixel_rounding_is_per_axis() {
    // Сосед притянул x к 150.5: x не округляется, y — округляется.
    let (mut doc, page, ids) = page(&[(None, NodeKind::Rect, patch(0.0, 0.0, 20.0, 20.0))]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let b = bases(&scene, &ids, false);
    let t = Affine::translate((150.5, 20.3));
    let y_only = rigid_patches_snapped(&b, t, PixelSnap { x: false, y: true });
    assert_eq!((y_only[0].1.x, y_only[0].1.y), (Some(150.5), Some(20.0)));
    let both = rigid_patches_snapped(&b, t, PixelSnap::from(true));
    assert_eq!((both[0].1.x, both[0].1.y), (Some(151.0), Some(20.0)));
    // Привязка по x выключает округление именно этой оси.
    let s = Snap {
        dx: Some(-2.0),
        ..Default::default()
    };
    assert_eq!(
        PixelSnap::unless_snapped(&s),
        PixelSnap { x: false, y: true }
    );
    // Старая обёртка `pixel_snap: bool` ведёт себя как обе оси.
    assert_eq!(
        format!("{:?}", rigid_patches(&b, t, true)),
        format!("{:?}", rigid_patches_snapped(&b, t, PixelSnap::from(true)))
    );
}
