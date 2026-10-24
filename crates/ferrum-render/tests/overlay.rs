mod common;

use common::*;
use ferrum_doc::{NodeKind, PropsPatch};
use ferrum_render::{
    format_size, Camera, Overlay, OverlayTheme, Renderer, TextEngine, HANDLE_SIZE,
};
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
    // Левый верхний угол узла — (20, 20) пикселя устройства. Ручка 8 CSS px = 16
    // пикселей устройства занимает 12..28: внутри белая заливка, по левому краю
    // (x = 12) проходит обводка цвета выделения, левее — фон.
    assert_px(&img, 25, 25, [255, 255, 255], 0);
    let select = OverlayTheme::dark().select.to_rgba8().to_u8_array();
    assert_px(&img, 12, 20, [select[0], select[1], select[2]], 2);
    assert_px(&img, 9, 20, [255, 255, 255], 0);
}

#[test]
fn overlay_uses_effective_dpr_when_surface_is_capped() {
    let mut p = Page::new();
    let id = p.add(None, NodeKind::Rect, rect(0.0, -10.0, 100.0, 20.0));
    let scene = p.scene();
    // 20000 CSS px при dpr 2 не помещаются в 16384: реальный dpr меньше запрошенного.
    let mut cam = Camera::new(Size::new(20000.0, 100.0), 2.0);
    cam.set_center(Point::new(0.0, 0.0));
    let eff = cam.effective_dpr();
    assert!(eff < cam.dpr());
    let overlay = Overlay {
        selection: vec![id],
        frame_labels: false,
        ..Default::default()
    };
    let img = render_overlay(&scene, &cam, &overlay, BLACK);
    // Левый верхний угол узла: x = 8192, y ≈ 32.8. Ручка шириной HANDLE_SIZE·eff
    // (≈ 6,55) покрывает 8189..8195; при запрошенном dpr она была бы шириной 16
    // (8184..8200) и закрасила бы пиксель левее узла.
    assert!((HANDLE_SIZE * eff - HANDLE_SIZE * cam.dpr()).abs() > 1.0);
    assert_px(&img, 8191, 33, [255, 255, 255], 0);
    assert_px(&img, 8185, 33, [0, 0, 0], 0);
}

#[test]
fn pixel_grid_terminates_at_huge_coordinates() {
    let scene = Page::new().scene();
    let mut cam = Camera::new(Size::new(100.0, 100.0), 1.0);
    // При |x| ≥ 2^53 прибавление 1.0 ничего не меняет: цикл с шагом 1 не кончался.
    cam.set_center(Point::new(1e16, 1e16));
    cam.set_zoom(8.0);
    let img = render_overlay(&scene, &cam, &Overlay::default(), WHITE);
    assert_eq!((img.width(), img.height()), (100, 100));
}

#[test]
fn repeated_frames_do_no_text_layouts() {
    let mut p = Page::new();
    let w = 100.0;
    let f = |name: &str, x: f64| PropsPatch {
        name: Some(name.into()),
        ..rect(x, 0.0, w, 300.0)
    };
    p.add(None, NodeKind::Frame, f("Главный экран", 0.0));
    p.add(None, NodeKind::Frame, f("Профиль", 1000.0));
    let scene = p.scene();
    // Зум 0,3: фрейм 30 px (подпись усекается); зум 0,02 (минимальный): 2 px —
    // подпись не помещается даже с многоточием, и этот отказ тоже кэшируется.
    for zoom in [0.3, 0.02] {
        let mut cam = camera(200.0, 100.0);
        cam.set_zoom(zoom);
        let mut r = Renderer::new();
        let mut b = ferrum_render::CpuBackend::new(200, 100);
        let o = Overlay::default();
        r.render(&mut b, &scene, &cam, &o, &ferrum_render::NoImages, None);
        let first = r.text_layouts();
        r.render(&mut b, &scene, &cam, &o, &ferrum_render::NoImages, None);
        assert_eq!(
            r.text_layouts(),
            first,
            "зум {zoom}: повторный кадр раскладывает текст"
        );
    }
}

#[test]
fn zoomed_out_frames_skip_labels_without_layout() {
    let mut p = Page::new();
    for i in 0..50 {
        p.add(
            None,
            NodeKind::Frame,
            PropsPatch {
                name: Some(format!("Экран {i}")),
                ..rect(i as f64 * 500.0, 0.0, 100.0, 300.0)
            },
        );
    }
    let scene = p.scene();
    let mut cam = camera(400.0, 100.0);
    cam.set_zoom(0.02);
    let mut r = Renderer::new();
    let mut b = ferrum_render::CpuBackend::new(400, 100);
    r.render(
        &mut b,
        &scene,
        &cam,
        &Overlay::default(),
        &ferrum_render::NoImages,
        None,
    );
    // Фреймы по 2 px: уже «…» не помещается, достаточно одного замера «…».
    assert!(r.text_layouts() <= 1, "{}", r.text_layouts());
}

#[test]
fn label_width_floor_never_exceeds_request() {
    for base in 20..80 {
        let mut t = TextEngine::new();
        let hi = base as f64 + 0.95;
        let lo = base as f64 + 0.05;
        t.label("Главный экран", 11.0, 400, hi);
        if let Some(l) = t.label("Главный экран", 11.0, 400, lo) {
            assert!(l.width <= lo, "{} > {lo}", l.width);
        }
    }
}

#[test]
fn hostile_names_are_sanitised() {
    let mut t = TextEngine::new();
    let start = std::time::Instant::now();
    let huge = "ы".repeat(5 * 1024 * 1024);
    let l = t.label(&huge, 11.0, 400, 300.0).unwrap();
    assert!(l.width <= 300.0 && l.width > 100.0, "{}", l.width);
    assert!(start.elapsed().as_secs() < 2, "{:?}", start.elapsed());
    // Перевод строки — обычный пробел, а не вторая строка.
    let nl = t
        .label("Экран\nвход\r\n\t\u{2028}x", 11.0, 400, 1000.0)
        .unwrap();
    let sp = t.label("Экран вход x", 11.0, 400, 1000.0).unwrap();
    assert!(
        (nl.width - sp.width).abs() < 1e-6,
        "{} {}",
        nl.width,
        sp.width
    );
    assert_eq!((nl.ascent, nl.descent), (sp.ascent, sp.descent));
    // Пустое и из одних пробелов — без подписи.
    assert!(t.label("", 11.0, 400, 100.0).is_none());
    assert!(t.label(" \n\t ", 11.0, 400, 100.0).is_none());
    // Письмо справа налево, эмодзи и «ломаные» последовательности не роняют.
    for s in [
        "שלום עולם",
        "مرحبا",
        "😀👨‍👩‍👧‍👦🏳️‍🌈",
        "a\u{0301}\u{0301}\u{0301}",
        "\u{202e}abc\u{0}",
    ] {
        let _ = t.label(s, 11.0, 400, 200.0);
    }
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

#[test]
fn hover_outline_is_not_drawn_for_hidden_nodes() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            visible: Some(false),
            ..rect(10.0, 10.0, 60.0, 40.0)
        },
    );
    // Видимый сам, но внутри скрытого фрейма — тоже скрыт.
    let kid = p.add(Some(f), NodeKind::Rect, rect(5.0, 5.0, 20.0, 20.0));
    let lone = p.add(
        None,
        NodeKind::Ellipse,
        PropsPatch {
            visible: Some(false),
            ..rect(80.0, 10.0, 20.0, 20.0)
        },
    );
    let scene = p.scene();
    let cam = camera(120.0, 60.0);
    let plain = render_overlay(&scene, &cam, &Overlay::default(), WHITE);
    for id in [f, kid, lone] {
        let hovered = Overlay {
            hover: Some(id),
            ..Default::default()
        };
        let img = render_overlay(&scene, &cam, &hovered, WHITE);
        assert!(
            img.data() == plain.data(),
            "обводка наведения у скрытого узла"
        );
    }
}
