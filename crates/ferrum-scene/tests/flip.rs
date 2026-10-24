//! Отражение узла в сцене: локальная матрица `T(x, y) · R(rotation) · F`, где `F`
//! отражает рамку узла относительно её центра. Рамка стоит на месте, а содержимое —
//! дети фрейма или группы — отражено; мировые границы, R-tree, hit-test и превью
//! видят то же самое.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::{Document, NodeId, NodeKind, PageId, Props, PropsPatch};
use ferrum_scene::{shape, testing::snapshot, Scene};
use kurbo::{Point, Rect};

fn patch(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-9
}

fn close(a: Rect, b: Rect) -> bool {
    near(a.origin(), b.origin()) && near(Point::new(a.x1, a.y1), Point::new(b.x1, b.y1))
}

#[test]
fn flip_mirrors_about_box_centre_before_rotation() {
    let mut p = Props::defaults(NodeKind::Rect);
    (p.x, p.y, p.w, p.h) = (10.0, 20.0, 40.0, 10.0);
    p.flip_x = true;
    let t = shape::local_transform(&p);
    // Левый верхний угол содержимого уходит вправо, рамка остаётся на месте.
    assert!(near(t * Point::ZERO, Point::new(50.0, 20.0)));
    assert!(near(t * Point::new(40.0, 0.0), Point::new(10.0, 20.0)));
    assert!(near(t * Point::new(20.0, 5.0), Point::new(30.0, 25.0)));
    assert!(t.determinant() < 0.0);

    p.flip_x = false;
    p.flip_y = true;
    let t = shape::local_transform(&p);
    assert!(near(t * Point::ZERO, Point::new(10.0, 30.0)));
    assert!(near(t * Point::new(40.0, 10.0), Point::new(50.0, 20.0)));

    // Оба отражения — то же, что поворот на 180° вокруг центра; поворот модели
    // применяется после отражения, вокруг левого верхнего угла рамки.
    p.flip_x = true;
    p.rotation = 90.0;
    let t = shape::local_transform(&p);
    let unflipped = {
        let mut q = p.clone();
        (q.flip_x, q.flip_y) = (false, false);
        shape::local_transform(&q)
    };
    for corner in [(0.0, 0.0), (40.0, 0.0), (40.0, 10.0), (0.0, 10.0)] {
        let c = Point::new(corner.0, corner.1);
        let opposite = Point::new(40.0 - c.x, 10.0 - c.y);
        assert!(near(t * c, unflipped * opposite), "{corner:?}");
    }
}

/// Фрейм 100×50 в (100, 100) с прямоугольником 20×10 у левого края (5, 5).
fn sample(flip: PropsPatch) -> (Document, PageId, NodeId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = Vec::new();
    doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        p.set_props(f, &patch(100.0, 100.0, 100.0, 50.0))?;
        p.set_props(f, &flip)?;
        let c = p.create_node(Some(f), None, NodeKind::Rect)?;
        p.set_props(c, &patch(5.0, 5.0, 20.0, 10.0))?;
        ids = vec![f, c];
        Ok(vec![])
    })
    .unwrap();
    doc.take_changes(page);
    (doc, page, ids[0], ids[1])
}

fn flip_x() -> PropsPatch {
    PropsPatch {
        flip_x: Some(true),
        ..Default::default()
    }
}

#[test]
fn child_of_mirrored_frame_is_mirrored_in_bounds_tree_and_hit_test() {
    let (mut doc, page, f, c) = sample(flip_x());
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Рамка фрейма на месте, ребёнок — у правого края: 100 + 100 − 5 − 20 = 175.
    let frame = scene.node(f).unwrap();
    assert!(close(
        frame.world_bounds.unwrap(),
        Rect::new(100.0, 100.0, 200.0, 150.0)
    ));
    let child = scene.node(c).unwrap();
    assert!(close(
        child.world_bounds.unwrap(),
        Rect::new(175.0, 105.0, 195.0, 115.0)
    ));
    // Hit-test через обратную матрицу: прямоугольник под точкой справа, а на прежнем
    // месте слева попадается только фрейм.
    assert_eq!(scene.hit_test(Point::new(190.0, 110.0), 0.0), Some(c));
    assert_eq!(scene.hit_test(Point::new(110.0, 110.0), 0.0), Some(f));
    // Асимметрия внутри ребёнка: его левый верхний угол (в его координатах) тоже
    // отражён и лежит справа.
    assert!(near(child.world * Point::ZERO, Point::new(195.0, 105.0)));
    // R-tree знает отражённое место.
    let right = scene.query_rect(Rect::new(176.0, 106.0, 180.0, 108.0));
    assert!(right.contains(&c) && right.contains(&f), "{right:?}");
    let left = scene.query_rect(Rect::new(106.0, 106.0, 110.0, 108.0));
    assert!(!left.contains(&c), "{left:?}");
}

#[test]
fn flip_by_patch_and_preview_updates_world_and_matches_rebuild() {
    let (mut doc, page, f, c) = sample(PropsPatch::default());
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    assert_eq!(scene.hit_test(Point::new(110.0, 110.0), 0.0), Some(c));

    // Превью: документ не меняется, сцена показывает отражённый фрейм.
    scene.set_preview(&[(
        f,
        PropsPatch {
            flip_y: Some(true),
            ..Default::default()
        },
    )]);
    // По y: 100 + 50 − 5 − 10 = 135.
    assert!(close(
        scene.node(c).unwrap().world_bounds.unwrap(),
        Rect::new(105.0, 135.0, 125.0, 145.0)
    ));
    assert_eq!(scene.hit_test(Point::new(110.0, 140.0), 0.0), Some(c));
    assert!(!doc.page(page).unwrap().props(f).unwrap().flip_y);
    scene.clear_preview();
    assert_eq!(scene.hit_test(Point::new(110.0, 110.0), 0.0), Some(c));

    // Правка документа: сцена по изменениям совпадает с построенной заново.
    doc.transact(page, vec![], |p| {
        p.set_props(f, &flip_x())?;
        Ok(vec![])
    })
    .unwrap();
    let changes = doc.take_changes(page);
    scene.apply(&changes, doc.page(page).unwrap());
    let fresh = Scene::from_page(doc.page(page).unwrap());
    assert_eq!(snapshot(&scene), snapshot(&fresh));
    assert_eq!(scene.hit_test(Point::new(190.0, 110.0), 0.0), Some(c));
}

#[test]
fn mirrored_group_mirrors_children_about_its_origin() {
    // У группы нет своей рамки (w = h = 0): её `F` отражает относительно начала
    // координат группы. Редактор отражает группы через детей, но значения из файла
    // должны давать ту же картину и в границах, и в попадании.
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = Vec::new();
    doc.transact(page, vec![], |p| {
        let g = p.create_node(None, None, NodeKind::Group)?;
        p.set_props(
            g,
            &PropsPatch {
                x: Some(300.0),
                y: Some(300.0),
                ..flip_x()
            },
        )?;
        let r = p.create_node(Some(g), None, NodeKind::Rect)?;
        p.set_props(r, &patch(10.0, 0.0, 30.0, 20.0))?;
        ids = vec![g, r];
        Ok(vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    let (g, r) = (ids[0], ids[1]);
    assert!(close(
        scene.node(r).unwrap().world_bounds.unwrap(),
        Rect::new(260.0, 300.0, 290.0, 320.0)
    ));
    assert!(close(
        scene.node(g).unwrap().subtree_bounds.unwrap(),
        Rect::new(260.0, 300.0, 290.0, 320.0)
    ));
    assert_eq!(scene.hit_test(Point::new(270.0, 310.0), 0.0), Some(r));
    assert_eq!(scene.hit_test(Point::new(320.0, 310.0), 0.0), None);
}

#[test]
fn scene_from_file_cache_keeps_flips() {
    let (doc, page, f, c) = sample(PropsPatch {
        flip_x: Some(true),
        flip_y: Some(true),
        ..Default::default()
    });
    let mut opened = decode(&encode(&doc).unwrap()).unwrap();
    let rows = opened
        .document
        .cached_scene(page)
        .expect("кэш сцены")
        .to_vec();
    let from_rows = Scene::from_rows(&rows);
    let from_loro = Scene::from_page(opened.document.page(page).unwrap());
    assert_eq!(snapshot(&from_rows), snapshot(&from_loro));
    assert!(from_rows.node(f).unwrap().props.flip_x);
    // Оба отражения: ребёнок в правом нижнем углу.
    assert!(close(
        from_rows.node(c).unwrap().world_bounds.unwrap(),
        Rect::new(175.0, 135.0, 195.0, 145.0)
    ));
}
