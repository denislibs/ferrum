mod common;

use common::*;
use ferrum_doc::NodeKind;
use ferrum_render::{format_size, Camera, Overlay, OverlayTheme, TextEngine, HANDLE_SIZE};
use kurbo::{Point, Size};

#[test]
fn labels_truncate_with_ellipsis() {
    let mut t = TextEngine::new();
    let full = t.label("Главный экран", 11.0, 400, 1000.0).unwrap();
    assert!(full.width > 40.0 && full.width < 120.0, "{}", full.width);
    assert!(full.ascent > 5.0 && full.descent > 1.0);
    let short = t.label("Главный экран", 11.0, 400, 40.0).unwrap();
    assert!(short.width <= 40.0 && short.width > 20.0, "{}", short.width);
    assert!(t.label("Главный экран", 11.0, 400, 2.0).is_none());
    assert!(t.label("", 11.0, 400, 100.0).is_none());
}

#[test]
fn size_badge_format() {
    assert_eq!(format_size(393.0), "393");
    assert_eq!(format_size(12.345), "12.35");
    assert_eq!(format_size(0.5), "0.5");
}

#[test]
fn overlay_with_dpr_scales_handles() {
    let mut p = Page::new();
    let id = p.add(None, NodeKind::Rect, rect(10.0, 10.0, 40.0, 20.0));
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(60.0, 40.0), 2.0);
    cam.set_center(Point::new(30.0, 20.0));
    let overlay = Overlay {
        selection: vec![id],
        frame_labels: false,
        ..Default::default()
    };
    let img = render_overlay(&scene, &cam, &overlay, WHITE);
    assert_eq!((img.width(), img.height()), (120, 80));
    // Ручка 8 CSS px = 16 пикселей устройства: белая середина в 5 px от угла.
    assert_px(&img, 25, 25, [255, 255, 255], 0);
}

#[test]
fn themes_follow_design_system() {
    let hex = |c: ferrum_render::Rgba| c.to_rgba8().to_u8_array();
    let dark = OverlayTheme::dark();
    let light = OverlayTheme::light();
    assert_eq!(hex(dark.select), [0x4a, 0xa3, 0xff, 255]);
    assert_eq!(hex(light.select), [0x17, 0x69, 0xe0, 255]);
    assert_eq!(hex(dark.on_select), [0x1a, 0x19, 0x18, 255]);
    assert_eq!(hex(light.on_select), [255, 255, 255, 255]);
    assert_eq!(hex(dark.component), [0xb4, 0x9c, 0xff, 255]);
    assert_eq!(hex(light.component), [0x6d, 0x44, 0xd9, 255]);
    assert_eq!(hex(dark.frame_label), [0xa9, 0xa3, 0x9c, 255]);
    assert_eq!(hex(light.frame_label), [0x65, 0x5f, 0x58, 255]);
    assert_eq!(hex(dark.handle_fill), [255, 255, 255, 255]);
    assert_eq!(HANDLE_SIZE, 8.0);
}
