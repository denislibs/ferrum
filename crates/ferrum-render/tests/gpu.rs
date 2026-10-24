//! Бэкенд `vello_gpu` собирается и принимает тот же кадр, что и CPU.
//! Запуск: `cargo test -p ferrum-render --features gpu --test gpu`.
#![cfg(feature = "gpu")]

mod common;

use common::*;
use ferrum_doc::{Fill, NodeKind, PropsPatch};
use ferrum_render::{Camera, GpuBackend, NoImages, Overlay, RenderBackend, Renderer};
use kurbo::{Point, Size};

#[test]
fn frame_encodes_into_vello_gpu_scene() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Экран".into()),
            opacity: Some(0.5),
            rotation: Some(10.0),
            ..rect(10.0, 10.0, 200.0, 150.0)
        },
    );
    let kid = p.add(Some(f), NodeKind::Ellipse, rect(150.0, 100.0, 100.0, 80.0));
    p.add(
        Some(f),
        NodeKind::Image,
        PropsPatch {
            fills: Some(vec![Fill::image("нет")]),
            ..rect(10.0, 10.0, 50.0, 50.0)
        },
    );
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(320.0, 240.0), 2.0);
    cam.set_center(Point::new(110.0, 90.0));
    let overlay = Overlay {
        selection: vec![kid],
        hover: Some(f),
        ..Default::default()
    };
    let (w, h) = cam.device_size();
    let mut backend = GpuBackend::new(w, h);
    assert!(!backend.supports_images());
    let stats =
        Renderer::new().render(&mut backend, &scene, &cam, &overlay, &NoImages, Some(WHITE));
    // Слой прозрачности и обрезка повёрнутого фрейма закодированы и закрыты.
    assert_eq!((stats.layers, stats.clips, stats.drawn), (1, 1, 3));
    assert_eq!(backend.size(), (640, 480));
    // Сильное отдаление: растрового кэша у GPU-бэкенда пока нет — узлы рисуются LOD.
    cam.set_zoom(0.02);
    backend.reset(w, h);
    let stats = Renderer::new().render(&mut backend, &scene, &cam, &overlay, &NoImages, None);
    assert_eq!(stats.rasters, 0);
}
