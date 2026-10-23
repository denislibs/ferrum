use ferrum_doc::container::{decode, encode};
use ferrum_doc::{
    Color, Document, Fill, NodeId, NodeKind, PageId, PropsPatch, SceneRow, Stroke, StrokeAlign,
};
use ferrum_scene::{testing, testing::snapshot, Scene};
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

/// Страница: фрейм (100, 100, 400×300) с прямоугольником и группой из двух эллипсов,
/// линия в корне. Возвращает [фрейм, прямоугольник, группа, эллипс 1, эллипс 2, линия].
fn sample() -> (Document, PageId, [NodeId; 6]) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = Vec::new();
    doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        p.set_props(f, &patch(100.0, 100.0, 400.0, 300.0))?;
        let r = p.create_node(Some(f), None, NodeKind::Rect)?;
        p.set_props(r, &patch(10.0, 10.0, 50.0, 40.0))?;
        let g = p.create_node(Some(f), None, NodeKind::Group)?;
        p.set_props(g, &patch(200.0, 100.0, 0.0, 0.0))?;
        let e1 = p.create_node(Some(g), None, NodeKind::Ellipse)?;
        p.set_props(e1, &patch(0.0, 0.0, 20.0, 20.0))?;
        let e2 = p.create_node(Some(g), None, NodeKind::Ellipse)?;
        p.set_props(e2, &patch(30.0, 40.0, 20.0, 20.0))?;
        let l = p.create_node(None, None, NodeKind::Line)?;
        p.set_props(l, &patch(0.0, 600.0, 300.0, 0.0))?;
        ids = vec![f, r, g, e1, e2, l];
        Ok(vec![])
    })
    .unwrap();
    doc.take_changes(page);
    (doc, page, ids.try_into().unwrap())
}

/// Сцена, обновлённая по изменениям, совпадает со сценой, построенной заново.
fn assert_matches_rebuild(scene: &Scene, doc: &mut Document, page: PageId) {
    let fresh = Scene::from_page(doc.page(page).unwrap());
    assert_eq!(snapshot(scene), snapshot(&fresh));
}

fn apply(scene: &mut Scene, doc: &mut Document, page: PageId) {
    let changes = doc.take_changes(page);
    assert!(
        !changes.is_empty(),
        "операция должна сообщить об изменениях"
    );
    scene.apply(&changes, doc.page(page).unwrap());
}

fn close(a: Rect, b: Rect) -> bool {
    (a.x0 - b.x0).abs() < 1e-9
        && (a.y0 - b.y0).abs() < 1e-9
        && (a.x1 - b.x1).abs() < 1e-9
        && (a.y1 - b.y1).abs() < 1e-9
}

#[test]
fn world_transforms_and_bounds_nest() {
    let (mut doc, page, [f, r, g, e1, e2, l]) = sample();
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert_eq!(scene.len(), 6);
    let world = |id| scene.node(id).unwrap().world_bounds.unwrap();
    assert!(close(world(f), Rect::new(100.0, 100.0, 500.0, 400.0)));
    assert!(close(world(r), Rect::new(110.0, 110.0, 160.0, 150.0)));
    assert!(close(world(e1), Rect::new(300.0, 200.0, 320.0, 220.0)));
    // Группа: объединение детей в её координатах.
    let group = scene.node(g).unwrap();
    assert!(close(
        group.local_bounds.unwrap(),
        Rect::new(0.0, 0.0, 50.0, 60.0)
    ));
    assert!(close(world(g), Rect::new(300.0, 200.0, 350.0, 260.0)));
    assert!(close(world(e2), Rect::new(330.0, 240.0, 350.0, 260.0)));
    // Обводка линии (1 px по центру) расширяет границы на полпикселя.
    assert!(close(world(l), Rect::new(-0.5, 599.5, 300.5, 600.5)));
    assert_eq!(scene.node(g).unwrap().depth, 1);
    assert_eq!(scene.index(scene.slot(e2).unwrap()), 1);
    let all = scene.bounds().unwrap();
    assert!(close(all, Rect::new(-0.5, 100.0, 500.0, 600.5)));
}

#[test]
fn rotated_frame_rotates_children() {
    let (mut doc, page, [f, r, ..]) = sample();
    doc.transact(page, vec![], |p| {
        p.set_props(
            f,
            &PropsPatch {
                rotation: Some(90.0),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Ребёнок (10, 10, 50×40) во фрейме, повёрнутом на 90° против часовой вокруг (100, 100).
    let b = scene.node(r).unwrap().world_bounds.unwrap();
    assert!(close(b, Rect::new(110.0, 40.0, 150.0, 90.0)), "{b:?}");
}

#[test]
fn rows_from_file_cache_build_the_same_scene() {
    let (doc, page, _) = sample();
    let bytes = encode(&doc).unwrap();
    let mut opened = decode(&bytes).unwrap().document;
    let rows = opened.cached_scene(page).unwrap().to_vec();
    let from_cache = Scene::from_rows(&rows);
    let from_page = Scene::from_page(opened.page(page).unwrap());
    assert_eq!(snapshot(&from_cache), snapshot(&from_page));
}

#[test]
fn untrusted_rows_are_sanitized_and_duplicates_dropped() {
    let (mut doc, page, [f, r, g, ..]) = sample();
    let mut rows: Vec<SceneRow> = doc.page(page).unwrap().scene_rows().unwrap();
    rows[1].props.x = f64::NAN;
    rows[1].props.w = -10.0;
    // Повтор фрейма в конце таблицы и строка-ребёнок повтора — отбрасываются обе.
    let mut dup = rows[0].clone();
    dup.parent = None;
    rows.push(dup);
    let mut orphan = rows[1].clone();
    orphan.parent = Some(rows.len() as u32 - 1);
    rows.push(orphan);
    let scene = Scene::from_rows(&rows);
    assert_eq!(scene.len(), 6);
    let rect = scene.node(r).unwrap();
    assert_eq!((rect.props.x, rect.props.w), (0.0, 0.0));
    assert_eq!(scene.roots().len(), 2);
    assert_eq!(scene.node(g).unwrap().parent, scene.slot(f));
}

#[test]
fn apply_props_updates_node_descendants_and_ancestors() {
    let (mut doc, page, [f, _, g, e1, e2, _]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    // Сдвиг ребёнка группы меняет границы группы и поддерева фрейма.
    doc.transact(page, vec![], |p| {
        p.set_props(e2, &patch(130.0, 140.0, 20.0, 20.0))
            .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    let group = scene.node(g).unwrap();
    assert!(close(
        group.local_bounds.unwrap(),
        Rect::new(0.0, 0.0, 150.0, 160.0)
    ));
    assert_matches_rebuild(&scene, &mut doc, page);
    // Сдвиг фрейма двигает всех потомков.
    doc.transact(page, vec![], |p| {
        p.set_props(
            f,
            &PropsPatch {
                x: Some(0.0),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    let b = scene.node(e1).unwrap().world_bounds.unwrap();
    assert!(close(b, Rect::new(200.0, 200.0, 220.0, 220.0)));
    assert_matches_rebuild(&scene, &mut doc, page);
}

type Step = dyn Fn(&ferrum_doc::Tx) -> ferrum_doc::DocResult<()>;

#[test]
fn apply_handles_every_structural_operation_and_undo() {
    let (mut doc, page, [f, r, g, e1, _, l]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let steps: Vec<Box<Step>> = vec![
        Box::new(move |p| p.create_node(Some(g), Some(1), NodeKind::Rect).map(|_| ())),
        Box::new(move |p| p.move_node(l, Some(f), 0)),
        Box::new(move |p| p.move_nodes(&[e1, r], None, 0)),
        Box::new(move |p| p.delete_nodes(&[g])),
        Box::new(move |p| {
            // Вынести узел из фрейма и удалить фрейм в одной транзакции.
            p.move_node(l, None, 0)?;
            p.delete_nodes(&[f])
        }),
    ];
    for step in &steps {
        doc.transact(page, vec![], |p| step(p).map(|_| vec![]))
            .unwrap();
        apply(&mut scene, &mut doc, page);
        assert_matches_rebuild(&scene, &mut doc, page);
    }
    for _ in 0..steps.len() {
        doc.undo().unwrap();
        apply(&mut scene, &mut doc, page);
        assert_matches_rebuild(&scene, &mut doc, page);
    }
    for _ in 0..steps.len() {
        doc.redo().unwrap();
        apply(&mut scene, &mut doc, page);
        assert_matches_rebuild(&scene, &mut doc, page);
    }
}

#[test]
fn hit_test_finds_topmost_visible_unclipped_node() {
    let (mut doc, page, [f, r, _, e1, _, l]) = sample();
    let top = doc
        .transact(page, vec![], |p| {
            let top = p.create_node(Some(f), None, NodeKind::Rect)?;
            p.set_props(top, &patch(20.0, 20.0, 30.0, 30.0))?;
            Ok(vec![top])
        })
        .unwrap()[0];
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    // Перекрытие: верхний первым, затем прямоугольник под ним, затем фрейм.
    assert_eq!(
        scene.hit_test_all(Point::new(125.0, 125.0), 0.0),
        vec![top, r, f]
    );
    // Тонкая линия ловится в пределах допуска.
    assert_eq!(scene.hit_test(Point::new(150.0, 603.0), 4.0), Some(l));
    assert_eq!(scene.hit_test(Point::new(150.0, 606.0), 4.0), None);
    // Эллипс: угол его рамки — мимо, в фрейм.
    assert_eq!(scene.hit_test(Point::new(301.0, 201.0), 0.0), Some(f));
    assert_eq!(scene.hit_test(Point::new(310.0, 210.0), 0.0), Some(e1));

    // Скрытый узел и обрезанная фреймом часть не ловятся; заблокированный ловится.
    doc.transact(page, vec![], |p| {
        p.set_props(
            top,
            &PropsPatch {
                visible: Some(false),
                ..Default::default()
            },
        )?;
        p.set_props(
            r,
            &PropsPatch {
                x: Some(-30.0),
                locked: Some(true),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    assert_eq!(scene.hit_test(Point::new(115.0, 125.0), 0.0), Some(r));
    assert_eq!(
        scene.hit_test(Point::new(90.0, 125.0), 0.0),
        None,
        "за краем фрейма"
    );
    assert_eq!(
        scene.query_rect(Rect::new(80.0, 120.0, 85.0, 125.0)),
        vec![r]
    );
}

#[test]
fn hit_test_on_rotated_node() {
    let (mut doc, page, _) = sample();
    let id = doc
        .transact(page, vec![], |p| {
            let id = p.create_node(None, None, NodeKind::Rect)?;
            p.set_props(
                id,
                &PropsPatch {
                    x: Some(1000.0),
                    y: Some(1000.0),
                    w: Some(100.0),
                    h: Some(10.0),
                    rotation: Some(90.0),
                    ..Default::default()
                },
            )?;
            Ok(vec![id])
        })
        .unwrap()[0];
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Повёрнут вокруг (1000, 1000): теперь тянется вверх.
    assert_eq!(scene.hit_test(Point::new(1005.0, 950.0), 0.0), Some(id));
    assert_eq!(scene.hit_test(Point::new(1050.0, 1005.0), 0.0), None);
}

#[test]
fn preview_overrides_scene_without_touching_document() {
    let (mut doc, page, [f, r, ..]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let before = snapshot(&scene);
    let moved = PropsPatch {
        x: Some(300.0),
        ..Default::default()
    };
    scene.set_preview(&[(r, moved.clone())]);
    assert!(scene.has_preview());
    assert_eq!(scene.node(r).unwrap().props.x, 300.0);
    assert_eq!(doc.page(page).unwrap().props(r).unwrap().x, 10.0);
    assert!(close(
        scene.node(r).unwrap().world_bounds.unwrap(),
        Rect::new(400.0, 110.0, 450.0, 150.0)
    ));
    assert_eq!(scene.hit_test(Point::new(420.0, 120.0), 0.0), Some(r));

    // Правка документа под превью: свойства документа обновляются, превью остаётся.
    doc.transact(page, vec![], |p| {
        p.set_props(
            r,
            &PropsPatch {
                y: Some(50.0),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    let n = scene.node(r).unwrap();
    assert_eq!((n.props.x, n.props.y), (300.0, 50.0));

    // Новый список превью заменяет прежний; снятие возвращает документ.
    scene.set_preview(&[(f, moved)]);
    assert_eq!(scene.node(r).unwrap().props.x, 10.0);
    scene.clear_preview();
    assert!(!scene.has_preview());
    assert_matches_rebuild(&scene, &mut doc, page);
    assert_ne!(snapshot(&scene), before, "правка y дошла до сцены");
}

#[test]
fn deleting_previewed_node_drops_its_preview() {
    let (mut doc, page, [_, r, ..]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    scene.set_preview(&[(
        r,
        PropsPatch {
            x: Some(1.0),
            ..Default::default()
        },
    )]);
    doc.transact(page, vec![], |p| p.delete_nodes(&[r]).map(|_| vec![]))
        .unwrap();
    apply(&mut scene, &mut doc, page);
    assert!(!scene.has_preview());
    assert!(scene.node(r).is_none());
    assert_matches_rebuild(&scene, &mut doc, page);
}

#[test]
fn strokes_and_clip_shape_subtree_bounds() {
    let (mut doc, page, [f, r, ..]) = sample();
    doc.transact(page, vec![], |p| {
        p.set_props(
            r,
            &PropsPatch {
                x: Some(-50.0),
                strokes: Some(vec![Stroke {
                    color: Color::BLACK,
                    width: 4.0,
                    align: StrokeAlign::Outside,
                }]),
                fills: Some(vec![Fill::solid(Color::WHITE)]),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    let rect = scene.node(r).unwrap().world_bounds.unwrap();
    assert!(close(rect, Rect::new(46.0, 106.0, 104.0, 154.0)));
    // Фрейм обрезает содержимое: выступающий ребёнок не расширяет поддерево.
    let frame = scene.node(f).unwrap();
    assert_eq!(frame.subtree_bounds, frame.world_bounds);
    doc.transact(page, vec![], |p| {
        p.set_props(
            f,
            &PropsPatch {
                clip_content: Some(false),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    let sub = scene.node(f).unwrap().subtree_bounds.unwrap();
    assert!(close(sub, Rect::new(46.0, 100.0, 500.0, 400.0)));
}

#[test]
fn versions_track_appearance_and_subtree() {
    let (mut doc, page, [f, r, ..]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let before = |s: &Scene| {
        let n = s.node(r).unwrap();
        (n.version, s.node(f).unwrap().subtree_version)
    };
    let (v0, sub0) = before(&scene);
    // Сдвиг: вид в локальных координатах тот же, поддерево фрейма изменилось.
    doc.transact(page, vec![], |p| {
        p.set_props(r, &patch(15.0, 10.0, 50.0, 40.0))
            .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    let (v1, sub1) = before(&scene);
    assert_eq!(v1, v0);
    assert!(sub1 > sub0);
    // Размер меняет вид.
    doc.transact(page, vec![], |p| {
        p.set_props(r, &patch(15.0, 10.0, 51.0, 40.0))
            .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    assert!(scene.node(r).unwrap().version > v1);
}

#[test]
fn large_batches_match_rebuild() {
    // Больше порога пакетного обновления R-tree: удаление проходом по дереву.
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |p| ferrum_doc::fixture::fill(p, 3000))
        .unwrap();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    doc.take_changes(page);
    let moved: Vec<NodeId> = ids.iter().copied().step_by(3).collect();
    doc.transact(page, vec![], |p| {
        for (i, id) in moved.iter().enumerate() {
            p.set_props(
                *id,
                &PropsPatch {
                    x: Some(i as f64),
                    ..Default::default()
                },
            )?;
        }
        let frame = p.create_node(None, None, NodeKind::Frame)?;
        p.move_nodes(&moved[..500], Some(frame), 0)?;
        p.delete_nodes(&moved[500..800])?;
        Ok(vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    assert_matches_rebuild(&scene, &mut doc, page);
    doc.undo().unwrap();
    apply(&mut scene, &mut doc, page);
    assert_matches_rebuild(&scene, &mut doc, page);
}

/// Номера версий всех узлов сцены и их поддеревьев.
fn versions(scene: &Scene) -> Vec<u64> {
    let mut out = Vec::new();
    let mut stack = scene.roots().to_vec();
    while let Some(s) = stack.pop() {
        let n = scene.at(s);
        out.push(n.version);
        out.push(n.subtree_version);
        stack.extend(n.children.iter().copied());
    }
    out
}

#[test]
fn rebuilt_scene_versions_are_fresh() {
    let (mut doc, page, _) = sample();
    let rows = doc.page(page).unwrap().scene_rows().unwrap();
    let a = Scene::from_rows(&rows);
    let max_a = versions(&a).into_iter().max().unwrap();
    // Вторая сцена из тех же строк не должна выдать номер, который выдала первая.
    let b = Scene::from_rows(&rows);
    let min_b = versions(&b).into_iter().min().unwrap();
    assert!(
        min_b > max_a,
        "версии сцены B ({min_b}) не новее сцены A ({max_a})"
    );
}

#[test]
fn fallback_rebuild_keeps_preview_and_fresh_versions() {
    let (mut doc, page, [_, r, g, _, _, l]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    scene.set_preview(&[(
        r,
        PropsPatch {
            x: Some(300.0),
            ..Default::default()
        },
    )]);
    let max_before = versions(&scene).into_iter().max().unwrap();
    // Группа пропала из таблицы идентификаторов: перенос в неё не сходится со сценой,
    // и `apply` перестраивает сцену целиком.
    testing::forget(&mut scene, g);
    doc.transact(page, vec![], |p| p.move_node(l, Some(g), 0).map(|_| vec![]))
        .unwrap();
    apply(&mut scene, &mut doc, page);
    assert!(scene.has_preview(), "превью пережило перестройку");
    assert_eq!(scene.node(r).unwrap().props.x, 300.0);
    let mut fresh = Scene::from_page(doc.page(page).unwrap());
    fresh.set_preview(&[(
        r,
        PropsPatch {
            x: Some(300.0),
            ..Default::default()
        },
    )]);
    assert_eq!(snapshot(&scene), snapshot(&fresh));
    assert!(
        versions(&scene).iter().all(|&v| v > max_before),
        "после перестройки есть повторный номер версии"
    );
}

#[test]
fn hit_test_inside_rotated_nested_frames() {
    let (mut doc, page, [f, ..]) = sample();
    doc.transact(page, vec![], |p| {
        p.set_props(
            f,
            &PropsPatch {
                rotation: Some(30.0),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    let ids = doc
        .transact(page, vec![], |p| {
            let inner = p.create_node(Some(f), None, NodeKind::Frame)?;
            p.set_props(
                inner,
                &PropsPatch {
                    x: Some(10.0),
                    y: Some(10.0),
                    w: Some(100.0),
                    h: Some(100.0),
                    rotation: Some(45.0),
                    ..Default::default()
                },
            )?;
            let child = p.create_node(Some(inner), None, NodeKind::Rect)?;
            p.set_props(child, &patch(10.0, 10.0, 20.0, 20.0))?;
            Ok(vec![inner, child])
        })
        .unwrap();
    let (inner, child) = (ids[0], ids[1]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Центр ребёнка (его геометрия (0, 0, 20, 20) в собственных координатах), переведённый
    // в мир через оба поворота.
    let center = scene.node(child).unwrap().world * Point::new(10.0, 10.0);
    assert_eq!(scene.hit_test(center, 0.0), Some(child));
    // Точка слева от ребёнка в его собственных координатах: мимо него, но в повёрнутом фрейме.
    let outside_child = scene.node(child).unwrap().world * Point::new(-1.0, 10.0);
    assert_ne!(scene.hit_test(outside_child, 0.0), Some(child));
    // Внутренний фрейм ловится в собственной точке, а не в точке внешнего.
    let inner_point = scene.node(inner).unwrap().world * Point::new(50.0, 50.0);
    assert_eq!(scene.hit_test(inner_point, 0.0), Some(inner));
}

#[test]
fn hit_tolerance_on_rotated_thin_line() {
    let (mut doc, page, _) = sample();
    let line = doc
        .transact(page, vec![], |p| {
            let id = p.create_node(None, None, NodeKind::Line)?;
            p.set_props(
                id,
                &PropsPatch {
                    x: Some(1000.0),
                    y: Some(1000.0),
                    w: Some(100.0),
                    h: Some(0.0),
                    rotation: Some(90.0),
                    ..Default::default()
                },
            )?;
            Ok(vec![id])
        })
        .unwrap()[0];
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Повёрнута на 90° против часовой: от (1000, 1000) вверх до (1000, 900).
    assert_eq!(scene.hit_test(Point::new(1003.0, 950.0), 4.0), Some(line));
    assert_eq!(scene.hit_test(Point::new(1006.0, 950.0), 4.0), None);
    // Конец отрезка: допуск считается до отрезка, а не до бесконечной прямой.
    assert_eq!(scene.hit_test(Point::new(1000.0, 897.0), 4.0), Some(line));
    assert_eq!(scene.hit_test(Point::new(1000.0, 895.0), 4.0), None);
}

/// Прозрачность и обрезка не меняют вид узла в его координатах (версия узла и
/// его список отрисовки остаются), но меняют картинку поддерева: растровый кэш
/// рендера следит за `subtree_version` узла и всех его предков.
#[test]
fn opacity_and_clip_bump_subtree_version_of_node_and_ancestors_only() {
    let (mut doc, page, [f, r, ..]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let edits = [
        PropsPatch {
            opacity: Some(0.5),
            ..Default::default()
        },
        PropsPatch {
            clip_content: Some(false),
            ..Default::default()
        },
    ];
    for edit in edits {
        let (fv, fs) = (
            scene.node(f).unwrap().version,
            scene.node(f).unwrap().subtree_version,
        );
        let (rv, rs) = (
            scene.node(r).unwrap().version,
            scene.node(r).unwrap().subtree_version,
        );
        doc.transact(page, vec![], |p| p.set_props(r, &edit).map(|_| vec![]))
            .unwrap();
        apply(&mut scene, &mut doc, page);
        assert_eq!(
            scene.node(r).unwrap().version,
            rv,
            "версия узла не меняется"
        );
        assert_ne!(scene.node(r).unwrap().subtree_version, rs);
        assert_eq!(scene.node(f).unwrap().version, fv);
        assert_ne!(scene.node(f).unwrap().subtree_version, fs, "предок тоже");
    }
    // То же для превью жеста.
    let fs = scene.node(f).unwrap().subtree_version;
    scene.set_preview(&[(
        r,
        PropsPatch {
            opacity: Some(0.1),
            ..Default::default()
        },
    )]);
    assert_ne!(scene.node(f).unwrap().subtree_version, fs);
}
