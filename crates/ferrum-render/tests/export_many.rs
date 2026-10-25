//! Экспорт нескольких узлов одной картинкой («Скопировать как PNG» для выделения).

mod common;

use common::*;
use ferrum_doc::{Color, Fill, NodeId, NodeKind, PropsPatch};
use ferrum_render::{ExportError, NoImages, Renderer};

fn solid(r: u8, g: u8, b: u8, a: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, a))])
}

#[test]
fn union_of_areas_in_paint_order() {
    let mut p = Page::new();
    let red = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0, 255),
            ..rect(0.0, 0.0, 20.0, 20.0)
        },
    );
    let blue = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 255, 255),
            ..rect(10.0, 10.0, 30.0, 10.0)
        },
    );
    let scene = p.scene();
    // Порядок выделения обратный порядку отрисовки: синий всё равно сверху.
    let img = Renderer::new()
        .export_pixmap_nodes(&scene, &[blue, red], 2.0, &NoImages)
        .unwrap();
    assert_eq!((img.width(), img.height()), (80, 40));
    assert_eq!(px(&img, 5, 5), [255, 0, 0, 255]);
    assert_eq!(px(&img, 30, 30), [0, 0, 255, 255], "синий над красным");
    assert_eq!(px(&img, 70, 30), [0, 0, 255, 255]);
    // Вне обоих узлов — прозрачно.
    assert_eq!(px(&img, 70, 5), [0, 0, 0, 0]);
    let png = Renderer::new()
        .export_png_nodes(&scene, &[red, blue], 1.0, &NoImages)
        .unwrap();
    assert_eq!(&png[1..4], b"PNG");
}

#[test]
fn child_of_exported_parent_is_drawn_once() {
    let mut p = Page::new();
    let g = p.add(None, NodeKind::Group, PropsPatch::default());
    let leaf = p.add(
        Some(g),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0, 128),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let scene = p.scene();
    let once = Renderer::new()
        .export_pixmap_nodes(&scene, &[g], 1.0, &NoImages)
        .unwrap();
    let both = Renderer::new()
        .export_pixmap_nodes(&scene, &[leaf, g, leaf], 1.0, &NoImages)
        .unwrap();
    assert_eq!(px(&once, 5, 5)[3], 128);
    assert_eq!(px(&both, 5, 5), px(&once, 5, 5), "ребёнок нарисован дважды");
}

#[test]
fn unknown_and_hidden_nodes_are_skipped() {
    let mut p = Page::new();
    let shown = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 255, 0, 255),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let hidden = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            visible: Some(false),
            fills: solid(0, 0, 0, 255),
            ..rect(100.0, 100.0, 10.0, 10.0)
        },
    );
    let scene = p.scene();
    let ghost: NodeId = format!("1@{}", u64::MAX - 1).parse().unwrap();
    let img = Renderer::new()
        .export_pixmap_nodes(&scene, &[ghost, hidden, shown], 1.0, &NoImages)
        .unwrap();
    // Скрытый узел не расширил область.
    assert_eq!((img.width(), img.height()), (10, 10));
    let mut r = Renderer::new();
    assert_eq!(
        r.export_png_nodes(&scene, &[ghost], 1.0, &NoImages),
        Err(ExportError::NotFound)
    );
    assert_eq!(
        r.export_png_nodes(&scene, &[hidden], 1.0, &NoImages),
        Err(ExportError::Empty)
    );
    assert_eq!(
        r.export_png_nodes(&scene, &[], 1.0, &NoImages),
        Err(ExportError::NotFound)
    );
}

#[test]
fn clipped_child_does_not_widen_area() {
    let mut p = Page::new();
    let frame = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            clip_content: Some(true),
            fills: solid(255, 0, 0, 255),
            ..rect(0.0, 0.0, 20.0, 20.0)
        },
    );
    let child = p.add(
        Some(frame),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 255, 255),
            ..rect(10.0, 10.0, 60.0, 60.0)
        },
    );
    let scene = p.scene();
    let only = Renderer::new()
        .export_pixmap_nodes(&scene, &[frame], 1.0, &NoImages)
        .unwrap();
    let both = Renderer::new()
        .export_pixmap_nodes(&scene, &[child, frame], 1.0, &NoImages)
        .unwrap();
    assert_eq!((only.width(), only.height()), (20, 20), "область фрейма");
    assert_eq!((both.width(), both.height()), (20, 20));
    assert_eq!(px(&both, 15, 15), px(&only, 15, 15));
    assert_eq!(px(&both, 5, 5), [255, 0, 0, 255]);
}

#[test]
fn too_large_union_is_refused() {
    let mut p = Page::new();
    let a = p.add(None, NodeKind::Rect, rect(0.0, 0.0, 10.0, 10.0));
    let b = p.add(None, NodeKind::Rect, rect(40000.0, 0.0, 10.0, 10.0));
    let scene = p.scene();
    assert!(matches!(
        Renderer::new().export_png_nodes(&scene, &[a, b], 1.0, &NoImages),
        Err(ExportError::TooLarge(_, _))
    ));
}

#[test]
fn duplicate_id_is_drawn_once() {
    let mut p = Page::new();
    let a = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0, 128),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let scene = p.scene();
    let img = Renderer::new()
        .export_pixmap_nodes(&scene, &[a, a], 1.0, &NoImages)
        .unwrap();
    assert_eq!(px(&img, 5, 5)[3], 128);
}

#[test]
fn rotated_and_flipped_nodes() {
    let mut p = Page::new();
    let rot = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            rotation: Some(90.0),
            fills: solid(255, 0, 0, 255),
            ..rect(0.0, 0.0, 20.0, 10.0)
        },
    );
    let scene = p.scene();
    let img = Renderer::new()
        .export_pixmap_nodes(&scene, &[rot], 1.0, &NoImages)
        .unwrap();
    // Область — габарит повёрнутого узла: стороны поменялись местами.
    assert_eq!((img.width(), img.height()), (10, 20));
    assert_eq!(px(&img, 5, 10), [255, 0, 0, 255]);

    let mut p = Page::new();
    let flip = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            flip_x: Some(true),
            fills: solid(0, 255, 0, 255),
            ..rect(0.0, 0.0, 20.0, 10.0)
        },
    );
    let scene = p.scene();
    let img = Renderer::new()
        .export_pixmap_nodes(&scene, &[flip], 1.0, &NoImages)
        .unwrap();
    assert_eq!((img.width(), img.height()), (20, 10));
    assert_eq!(px(&img, 10, 5), [0, 255, 0, 255]);
}

#[test]
fn nodes_under_different_parents_and_frame_in_selection() {
    let mut p = Page::new();
    let f1 = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            clip_content: Some(true),
            fills: solid(255, 0, 0, 255),
            ..rect(0.0, 0.0, 20.0, 20.0)
        },
    );
    let f2 = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(255, 255, 255, 255),
            ..rect(100.0, 0.0, 20.0, 20.0)
        },
    );
    let c1 = p.add(
        Some(f1),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 255, 255),
            ..rect(2.0, 2.0, 4.0, 4.0)
        },
    );
    let c2 = p.add(
        Some(f2),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 255, 0, 255),
            ..rect(2.0, 2.0, 4.0, 4.0)
        },
    );
    let scene = p.scene();
    let img = Renderer::new()
        .export_pixmap_nodes(&scene, &[c2, c1], 1.0, &NoImages)
        .unwrap();
    // Узлы под разными родителями: область — объединение их границ.
    assert_eq!((img.width(), img.height()), (104, 4));
    assert_eq!(px(&img, 1, 1), [0, 0, 255, 255]);
    assert_eq!(px(&img, 102, 1), [0, 255, 0, 255]);
    // Фрейм в выделении: область — его форма.
    let img = Renderer::new()
        .export_pixmap_nodes(&scene, &[f1, f2], 1.0, &NoImages)
        .unwrap();
    assert_eq!((img.width(), img.height()), (120, 20));
    assert_eq!(px(&img, 3, 3), [0, 0, 255, 255]);
    assert_eq!(px(&img, 50, 10), [0, 0, 0, 0]);
}
