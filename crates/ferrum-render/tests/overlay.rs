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
    // Измерения с Alt — `danger` дизайн-системы.
    assert_eq!(hex(dark.measure), [0xff, 0x7a, 0x7a, 255]);
    assert_eq!(hex(light.measure), [0xc2, 0x2c, 0x2c, 255]);
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

#[test]
fn measurement_badge_sits_on_visible_part_of_long_line() {
    // Большой зум: линия до дальнего края фрейма уходит за экран на тысячи пикселей.
    // Плашка — посередине видимой части (и целиком на экране), а не посередине всей
    // линии далеко за краем; сама линия видна от края до края.
    use ferrum_render::{Measure, MeasureKind};
    let scene = Page::new().scene();
    let cam = camera(240.0, 160.0);
    let bg = ferrum_render::Rgba::from_rgba8(0x1e, 0x1e, 0x1e, 255);
    let measure = [0xff, 0x7a, 0x7a];
    let draw = |a: (f64, f64), b: (f64, f64)| {
        let overlay = Overlay {
            measurements: vec![Measure {
                line: kurbo::Line::new(a, b),
                value: 14000.0,
                kind: MeasureKind::Distance,
            }],
            frame_labels: false,
            pixel_grid: false,
            ..Default::default()
        };
        render_overlay(&scene, &cam, &overlay, bg)
    };
    let is_measure = |img: &ferrum_render::Pixmap, x: u16, y: u16| {
        let p = px(img, x, y);
        (0..3).all(|i| p[i].abs_diff(measure[i]) <= 12)
    };
    // Видимая часть горизонтали — весь экран 0..240: плашка по центру, x ≈ 120.
    let img = draw((-9000.0, 40.0), (5000.0, 40.0));
    assert!(
        is_measure(&img, 2, 40) && is_measure(&img, 237, 40),
        "линия от края до края"
    );
    let plate: Vec<u16> = (0..240u16)
        .filter(|&x| (33..48u16).filter(|&y| is_measure(&img, x, y)).count() >= 8)
        .collect();
    assert!(plate.len() >= 14, "столбцов плашки: {}", plate.len());
    let c = (f64::from(plate[0]) + f64::from(*plate.last().unwrap())) / 2.0;
    assert!((c - 120.0).abs() <= 2.0, "центр плашки {c}");
    // Видимая часть — 0..30 у левого края: плашка не вылезает за экран.
    let img = draw((-9000.0, 80.0), (30.0, 80.0));
    let cols: Vec<u16> = (0..240u16)
        .filter(|&x| (73..88u16).filter(|&y| is_measure(&img, x, y)).count() >= 8)
        .collect();
    assert!(cols.len() >= 14, "столбцов плашки: {}", cols.len());
    assert_eq!(cols[0], 0, "плашка прижата к левому краю: {cols:?}");
    // Линия целиком за экраном — ни линии, ни плашки.
    let img = draw((-9000.0, 400.0), (5000.0, 400.0));
    assert!(img.data().iter().all(|c| c.r == 0x1e));
}

mod remote {
    use super::common::*;
    use ferrum_render::{
        Camera, CpuBackend, ImagePaint, NoImages, Overlay, RemoteMark, RenderBackend, Renderer,
        Rgba, REMOTE_BOXES_MAX,
    };
    use ferrum_scene::Scene;
    use kurbo::{Affine, BezPath, Point, Rect, Stroke};
    use std::sync::Arc;
    use vello_cpu::peniko::Fill;

    const BLUE: [u8; 3] = [0x0d, 0x74, 0xce];

    fn mark(cursor: Option<Point>, name: &str) -> RemoteMark {
        RemoteMark {
            color: Rgba::from_rgba8(BLUE[0], BLUE[1], BLUE[2], 255),
            name: name.into(),
            cursor,
            boxes: Vec::new(),
            frame: None,
        }
    }

    fn overlay(marks: Vec<RemoteMark>) -> Overlay {
        Overlay {
            frame_labels: false,
            remote: Arc::from(marks),
            ..Default::default()
        }
    }

    fn frame(r: &mut Renderer, b: &mut impl RenderBackend, cam: &Camera, o: &Overlay) {
        r.render(b, &Scene::new(), cam, o, &NoImages, Some(WHITE));
    }

    /// Бэкенд без картинок: курсор рисуется глифами и обводкой напрямую.
    struct NoImagesBackend(CpuBackend);

    impl RenderBackend for NoImagesBackend {
        fn begin_frame(&mut self, w: u16, h: u16) {
            self.0.begin_frame(w, h)
        }
        fn size(&self) -> (u16, u16) {
            self.0.size()
        }
        fn set_transform(&mut self, t: Affine) {
            self.0.set_transform(t)
        }
        fn set_color(&mut self, c: Rgba) {
            self.0.set_color(c)
        }
        fn set_image(&mut self, i: &ImagePaint) {
            self.0.set_image(i)
        }
        fn fill_path(&mut self, p: &BezPath, f: Fill) {
            self.0.fill_path(p, f)
        }
        fn fill_rect(&mut self, r: &Rect) {
            self.0.fill_rect(r)
        }
        fn stroke_path(&mut self, p: &BezPath, s: &Stroke) {
            self.0.stroke_path(p, s)
        }
        fn push_clip(&mut self, p: &BezPath) {
            self.0.push_clip(p)
        }
        fn pop_clip(&mut self) {
            self.0.pop_clip()
        }
        fn push_opacity_layer(&mut self, a: f32) {
            self.0.push_opacity_layer(a)
        }
        fn pop_layer(&mut self) {
            self.0.pop_layer()
        }
        fn supports_images(&self) -> bool {
            false
        }
    }

    #[test]
    fn cursor_and_label_are_drawn_at_world_point_in_peer_colour() {
        let mut cam = camera(200.0, 100.0);
        // Мир сдвинут: курсор в мире (1000, 500) — на экране (40, 30).
        cam.set_center(Point::new(1060.0, 520.0));
        let img = render_overlay(
            &Scene::new(),
            &cam,
            &overlay(vec![mark(Some(Point::new(1000.0, 500.0)), "Анна")]),
            WHITE,
        );
        // Стрелка у острия — цвет участника; левее острия — фон.
        assert_px(&img, 42, 38, BLUE, 40);
        assert_px(&img, 35, 38, [255, 255, 255], 0);
        // Плашка имени справа снизу от острия: цвет участника вокруг букв.
        assert_px(&img, 52, 51, BLUE, 40);
    }

    #[test]
    fn hidden_and_off_screen_cursors_are_not_drawn() {
        let cam = camera(100.0, 100.0);
        for m in [
            mark(None, "Анна"),
            mark(Some(Point::new(5000.0, 50.0)), "Анна"),
            mark(Some(Point::new(50.0, 5000.0)), "Анна"),
            mark(Some(Point::new(-5000.0, 50.0)), "Анна"),
        ] {
            let mut r = Renderer::new();
            let mut cpu = CpuBackend::new(100, 100);
            frame(&mut r, &mut cpu, &cam, &overlay(vec![m]));
            let img = cpu.render();
            for (x, y) in [(5, 5), (50, 50), (95, 95)] {
                assert_px(&img, x, y, [255, 255, 255], 0);
            }
            // Курсор за краем экрана и картинки не строит.
            assert_eq!(r.cursor_sprites(), 0);
        }
    }

    #[test]
    fn sprite_looks_like_direct_drawing_and_is_built_once() {
        let cam = camera(160.0, 80.0);
        let o = overlay(vec![mark(Some(Point::new(20.0, 10.0)), "Вероника")]);
        let mut r = Renderer::new();
        let mut cpu = CpuBackend::new(160, 80);
        frame(&mut r, &mut cpu, &cam, &o);
        let sprite = cpu.render();
        assert_eq!(r.cursor_sprites(), 1);
        frame(&mut r, &mut cpu, &cam, &o);
        assert_eq!(r.cursor_sprites(), 1, "тот же курсор — та же картинка");
        let mut direct = NoImagesBackend(CpuBackend::new(160, 80));
        let mut r2 = Renderer::new();
        frame(&mut r2, &mut direct, &cam, &o);
        assert_eq!(r2.cursor_sprites(), 0);
        let direct = direct.0.render();
        let mut worst = 0u8;
        for y in 0..80 {
            for x in 0..160 {
                let (a, b) = (px(&sprite, x, y), px(&direct, x, y));
                worst = worst.max((0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap());
            }
        }
        assert!(
            worst <= 8,
            "картинка курсора отличается от прямого рисования на {worst}"
        );
    }

    #[test]
    fn sprites_are_built_a_few_per_frame() {
        let cam = camera(400.0, 300.0);
        let marks: Vec<_> = (0..30)
            .map(|i| {
                mark(
                    Some(Point::new(10.0 + i as f64 * 12.0, 20.0)),
                    &format!("Участник {i}"),
                )
            })
            .collect();
        let o = overlay(marks);
        let mut r = Renderer::new();
        let mut cpu = CpuBackend::new(400, 300);
        frame(&mut r, &mut cpu, &cam, &o);
        assert_eq!(r.cursor_sprites(), 8, "не больше восьми картинок за кадр");
        for _ in 0..3 {
            frame(&mut r, &mut cpu, &cam, &o);
        }
        assert_eq!(r.cursor_sprites(), 30);
    }

    #[test]
    fn selection_boxes_and_frame_in_peer_colour() {
        let cam = camera(200.0, 120.0);
        let mut m = mark(None, "Анна");
        m.boxes = vec![
            (
                Affine::translate((20.0, 20.0)),
                Rect::new(0.0, 0.0, 40.0, 30.0),
            ),
            (
                Affine::translate((150.0, 60.0)) * Affine::rotate(0.5),
                Rect::new(0.0, 0.0, 30.0, 20.0),
            ),
        ];
        m.frame = Some(Rect::new(10.0, 10.0, 190.0, 110.0));
        let img = render_overlay(&Scene::new(), &cam, &overlay(vec![m]), WHITE);
        // Рамка узла: левая сторона — x = 20, внутри — фон.
        assert_px(&img, 20, 35, BLUE, 30);
        assert_px(&img, 35, 35, [255, 255, 255], 0);
        // Общая рамка.
        assert_px(&img, 100, 10, BLUE, 30);
        assert_px(&img, 190, 60, BLUE, 30);
    }

    #[test]
    fn boxes_beyond_the_limit_are_not_drawn() {
        let cam = camera(400.0, 40.0);
        let mut m = mark(None, "Анна");
        m.boxes = (0..REMOTE_BOXES_MAX + 1)
            .map(|i| {
                (
                    Affine::translate((i as f64 * 5.0, 10.0)),
                    Rect::new(0.0, 0.0, 3.0, 10.0),
                )
            })
            .collect();
        let img = render_overlay(&Scene::new(), &cam, &overlay(vec![m]), WHITE);
        assert_px(&img, ((REMOTE_BOXES_MAX - 1) * 5) as u16, 15, BLUE, 30);
        assert_px(&img, (REMOTE_BOXES_MAX * 5) as u16, 15, [255, 255, 255], 0);
    }
}
