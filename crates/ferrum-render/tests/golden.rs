//! Эталонные картинки рендера через `vello_cpu`. Каждый тест сначала проверяет
//! смысл картинки по отдельным пикселям (тест полезен и без эталона), затем
//! сравнивает её с `tests/golden/<имя>.png` с допуском по каналам.
//! Пересоздать эталоны: `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test golden`.

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch, ScaleMode, Stroke, StrokeAlign};
use ferrum_render::{Camera, Overlay, OverlayTheme, Pixmap, Renderer, Rgba};
use kurbo::{Line, Point, Rect, Size};
use vello_cpu::color::PremulRgba8;

fn solid(r: u8, g: u8, b: u8, a: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, a))])
}

fn stroke(width: f64, align: StrokeAlign) -> Option<Vec<Stroke>> {
    Some(vec![Stroke {
        color: Color::BLACK,
        width,
        align,
    }])
}

#[test]
fn shapes() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            corner_radius: Some(12.0),
            fills: solid(255, 0, 0, 255),
            ..rect(10.0, 10.0, 60.0, 40.0)
        },
    );
    p.add(
        None,
        NodeKind::Ellipse,
        PropsPatch {
            fills: solid(0, 0, 255, 255),
            ..rect(90.0, 10.0, 60.0, 40.0)
        },
    );
    p.add(
        None,
        NodeKind::Line,
        PropsPatch {
            strokes: stroke(3.0, StrokeAlign::Center),
            ..rect(10.0, 90.0, 140.0, 0.0)
        },
    );
    let img = render(&p.scene(), &camera(160.0, 120.0));
    assert_px(&img, 40, 30, [255, 0, 0], 0);
    assert_px(&img, 11, 11, [255, 255, 255], 0);
    assert_px(&img, 120, 30, [0, 0, 255], 0);
    assert_px(&img, 92, 12, [255, 255, 255], 0);
    assert_px(&img, 80, 90, [0, 0, 0], 0);
    assert_px(&img, 80, 94, [255, 255, 255], 0);
    golden("shapes", &img);
}

#[test]
fn stroke_alignment() {
    let mut p = Page::new();
    let aligns = [
        (15.0, StrokeAlign::Inside, 255),
        (65.0, StrokeAlign::Center, 255),
        (115.0, StrokeAlign::Outside, 255),
        // Полупрозрачная заливка: внутренняя обводка кольцом, а не слиянием с заливкой.
        (165.0, StrokeAlign::Inside, 128),
    ];
    for (x, align, alpha) in aligns {
        p.add(
            None,
            NodeKind::Rect,
            PropsPatch {
                fills: solid(200, 200, 200, alpha),
                strokes: stroke(6.0, align),
                ..rect(x, 15.0, 30.0, 30.0)
            },
        );
    }
    let img = render(&p.scene(), &camera(210.0, 60.0));
    // Внутренняя: у края внутри чёрное, снаружи фон, глубже заливка.
    assert_px(&img, 16, 30, [0, 0, 0], 0);
    assert_px(&img, 13, 30, [255, 255, 255], 0);
    assert_px(&img, 24, 30, [200, 200, 200], 0);
    // По центру: по 3 пикселя в обе стороны от края.
    assert_px(&img, 63, 30, [0, 0, 0], 0);
    assert_px(&img, 67, 30, [0, 0, 0], 0);
    assert_px(&img, 60, 30, [255, 255, 255], 0);
    assert_px(&img, 72, 30, [200, 200, 200], 0);
    // Снаружи: 6 пикселей за краем.
    assert_px(&img, 111, 30, [0, 0, 0], 0);
    assert_px(&img, 107, 30, [255, 255, 255], 0);
    assert_px(&img, 117, 30, [200, 200, 200], 0);
    // Кольцо поверх полупрозрачной заливки.
    assert_px(&img, 166, 30, [0, 0, 0], 0);
    assert_px(&img, 180, 30, [227, 227, 227], 2);
    golden("stroke_alignment", &img);
}

#[test]
fn frame_clip() {
    let mut p = Page::new();
    for (x, clip) in [(10.0, true), (90.0, false)] {
        let f = p.add(
            None,
            NodeKind::Frame,
            PropsPatch {
                clip_content: Some(clip),
                fills: solid(230, 230, 230, 255),
                ..rect(x, 10.0, 50.0, 50.0)
            },
        );
        p.add(
            Some(f),
            NodeKind::Rect,
            PropsPatch {
                fills: solid(0, 160, 0, 255),
                ..rect(20.0, 20.0, 40.0, 40.0)
            },
        );
    }
    let img = render(&p.scene(), &camera(160.0, 80.0));
    assert_px(&img, 40, 40, [0, 160, 0], 0);
    assert_px(&img, 65, 40, [255, 255, 255], 0);
    assert_px(&img, 145, 40, [0, 160, 0], 0);
    golden("frame_clip", &img);
}

#[test]
fn container_and_leaf_opacity() {
    let mut p = Page::new();
    let g = p.add(
        None,
        NodeKind::Group,
        PropsPatch {
            x: Some(10.0),
            y: Some(10.0),
            opacity: Some(0.5),
            ..Default::default()
        },
    );
    for (x, y) in [(0.0, 0.0), (20.0, 20.0)] {
        p.add(
            Some(g),
            NodeKind::Rect,
            PropsPatch {
                fills: solid(255, 0, 0, 255),
                ..rect(x, y, 40.0, 40.0)
            },
        );
    }
    for (x, y) in [(90.0, 10.0), (110.0, 30.0)] {
        p.add(
            None,
            NodeKind::Rect,
            PropsPatch {
                opacity: Some(0.5),
                fills: solid(255, 0, 0, 255),
                ..rect(x, y, 40.0, 40.0)
            },
        );
    }
    let img = render(&p.scene(), &camera(160.0, 80.0));
    // Группа — одним слоем: перекрытие не темнее остального.
    assert_px(&img, 15, 15, [255, 128, 128], 2);
    assert_px(&img, 40, 40, [255, 128, 128], 2);
    // Листья — каждый со своей прозрачностью: перекрытие темнее.
    assert_px(&img, 95, 15, [255, 128, 128], 2);
    assert_px(&img, 120, 40, [255, 64, 64], 2);
    golden("container_and_leaf_opacity", &img);
}

/// Картинка 4×2: левые два столбца красные, правые синие.
fn two_tone() -> Arc<Pixmap> {
    let mut img = Pixmap::new(4, 2);
    for y in 0..2 {
        for x in 0..4 {
            let (r, b) = if x < 2 { (255, 0) } else { (0, 255) };
            img.set_pixel(x, y, PremulRgba8 { r, g: 0, b, a: 255 });
        }
    }
    Arc::new(img)
}

#[test]
fn image_fill_modes() {
    let mut p = Page::new();
    for (x, mode) in [
        (10.0, ScaleMode::Fill),
        (80.0, ScaleMode::Fit),
        (150.0, ScaleMode::Tile),
    ] {
        p.add(
            None,
            NodeKind::Image,
            PropsPatch {
                fills: Some(vec![Fill {
                    scale_mode: mode,
                    ..Fill::image("two-tone")
                }]),
                ..rect(x, 10.0, 60.0, 40.0)
            },
        );
    }
    let images: HashMap<String, Arc<Pixmap>> = [("two-tone".to_string(), two_tone())].into();
    let mut r = Renderer::new();
    let img = render_with(&mut r, &p.scene(), &camera(220.0, 60.0), &images);
    // «Заполнить»: картинка 80×40 обрезана по бокам — половина красная, половина синяя.
    assert_px(&img, 15, 30, [255, 0, 0], 8);
    assert_px(&img, 65, 30, [0, 0, 255], 8);
    // «Вписать»: 60×30 по центру, сверху и снизу по 5 пикселей фона.
    assert_px(&img, 110, 12, [255, 255, 255], 0);
    assert_px(&img, 85, 30, [255, 0, 0], 8);
    assert_px(&img, 135, 30, [0, 0, 255], 8);
    // «Плитка»: картинка в натуральном размере повторяется каждые 4 пикселя.
    assert_px(&img, 150, 20, [255, 0, 0], 8);
    assert_px(&img, 153, 20, [0, 0, 255], 8);
    assert_px(&img, 154, 20, [255, 0, 0], 8);
    golden("image_fill_modes", &img);
}

#[test]
fn overlays() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Экран".into()),
            ..rect(20.0, 30.0, 140.0, 100.0)
        },
    );
    let sel = p.add(Some(f), NodeKind::Rect, rect(20.0, 20.0, 60.0, 40.0));
    let hover = p.add(Some(f), NodeKind::Ellipse, rect(100.0, 30.0, 30.0, 30.0));
    let scene = p.scene();
    let overlay = Overlay {
        theme: OverlayTheme::light(),
        selection: vec![sel],
        hover: Some(hover),
        marquee: Some(Rect::new(170.0, 100.0, 230.0, 150.0)),
        snap_lines: vec![Line::new((0.0, 50.0), (240.0, 50.0))],
        ..Default::default()
    };
    let img = render_overlay(&scene, &camera(240.0, 160.0), &overlay, WHITE);
    // Ручка в углу выделения: белая середина.
    assert_px(&img, 40, 50, [255, 255, 255], 30);
    // Линия привязки красная.
    let snap = px(&img, 5, 50);
    assert!(snap[0] > 200 && snap[1] < 140, "{snap:?}");
    // Рамка выделения мышью подкрашена цветом выделения.
    let m = px(&img, 200, 125);
    assert!(m[2] > m[0] && m[0] < 250, "{m:?}");
    // Бейдж «60 × 40» под выделением — синяя плашка.
    let blue = (50..90)
        .flat_map(|x| (96..114).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let c = px(&img, x, y);
            c[2] > 180 && c[0] < 60
        })
        .count();
    assert!(blue > 100, "пикселей бейджа: {blue}");
    // Обводка наведения вокруг эллипса: левая точка (120, 75) окрашена цветом выделения.
    let h = px(&img, 120, 75);
    assert!(h[2] > h[0] + 40, "обводка наведения: {h:?}");
    golden("overlays", &img);
}

#[test]
fn pixel_grid() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0, 255),
            ..rect(2.0, 2.0, 3.0, 2.0)
        },
    );
    let mut cam = Camera::new(Size::new(120.0, 80.0), 1.0);
    cam.set_center(Point::new(6.0, 4.0));
    cam.set_zoom(10.0);
    let overlay = Overlay {
        theme: OverlayTheme::light(),
        frame_labels: false,
        ..Default::default()
    };
    let img = render_overlay(&p.scene(), &cam, &overlay, WHITE);
    assert_px(&img, 10, 35, [235, 235, 235], 1);
    assert_px(&img, 15, 35, [255, 255, 255], 0);
    assert_px(&img, 35, 25, [255, 0, 0], 0);
    // Ниже 800% сетки нет.
    cam.set_zoom(4.0);
    let plain = render_overlay(&p.scene(), &cam, &overlay, WHITE);
    assert!(plain.data().iter().all(|c| c.r == 255 || c.g == 0));
    golden("pixel_grid", &img);
}

#[test]
fn frame_labels() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Главный экран".into()),
            ..rect(20.0, 40.0, 120.0, 60.0)
        },
    );
    p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Очень длинное название фрейма".into()),
            ..rect(160.0, 40.0, 60.0, 60.0)
        },
    );
    let bg = Rgba::from_rgba8(0x1e, 0x1e, 0x1e, 255);
    let overlay = Overlay {
        pixel_grid: false,
        ..Default::default()
    };
    let img = render_overlay(&p.scene(), &camera(260.0, 120.0), &overlay, bg);
    let lit = |x0: u16, x1: u16| {
        (x0..x1)
            .flat_map(|x| (24..38).map(move |y| (x, y)))
            .filter(|&(x, y)| px(&img, x, y)[0] > 0x40)
            .count()
    };
    assert!(lit(20, 140) > 40, "подпись первого фрейма");
    assert!(lit(160, 221) > 20, "усечённая подпись второго фрейма");
    assert_eq!(lit(221, 260), 0, "подпись не выходит за ширину фрейма");
    golden("frame_labels", &img);
}

#[test]
fn rotation() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            rotation: Some(30.0),
            fills: solid(220, 220, 220, 255),
            ..rect(60.0, 60.0, 80.0, 60.0)
        },
    );
    p.add(
        Some(f),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 140, 0, 255),
            ..rect(10.0, 10.0, 90.0, 20.0)
        },
    );
    p.add(
        None,
        NodeKind::Ellipse,
        PropsPatch {
            rotation: Some(-45.0),
            fills: solid(0, 170, 80, 255),
            ..rect(20.0, 130.0, 60.0, 24.0)
        },
    );
    let img = render(&p.scene(), &camera(180.0, 180.0));
    // Точка внутри повёрнутого прямоугольника: (30, 20) в координатах фрейма.
    let t = kurbo::Affine::translate((60.0, 60.0)) * kurbo::Affine::rotate(-30f64.to_radians());
    let inside = t * Point::new(30.0, 20.0);
    assert_px(&img, inside.x as u16, inside.y as u16, [255, 140, 0], 0);
    golden("rotation", &img);
}

#[test]
fn far_coordinates_render_like_origin() {
    let draw = |origin: f64| {
        let mut p = Page::new();
        p.add(
            None,
            NodeKind::Rect,
            PropsPatch {
                corner_radius: Some(6.0),
                fills: solid(255, 0, 0, 255),
                strokes: stroke(1.0, StrokeAlign::Center),
                ..rect(origin + 10.0, origin + 10.0, 40.0, 30.0)
            },
        );
        let mut cam = Camera::new(Size::new(240.0, 160.0), 1.0);
        cam.set_center(Point::new(origin + 30.0, origin + 25.0));
        cam.set_zoom(4.0);
        render(&p.scene(), &cam)
    };
    let near = draw(0.0);
    let far = draw(1.0e7);
    assert!(
        near.data() == far.data(),
        "на 1e7 рисуется так же, как у начала координат"
    );
}

#[test]
fn frame_stroke_above_child() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            clip_content: Some(false),
            fills: solid(230, 230, 230, 255),
            strokes: stroke(6.0, StrokeAlign::Inside),
            ..rect(20.0, 20.0, 60.0, 60.0)
        },
    );
    // Дочерний прямоугольник пересекает левый край фрейма (мир: x 10..50).
    p.add(
        Some(f),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 160, 0, 255),
            ..rect(-10.0, 20.0, 40.0, 20.0)
        },
    );
    let img = render(&p.scene(), &camera(100.0, 100.0));
    // Вне фрейма ребёнок виден (не обрезан), внутри полосы обводки — чёрный (обводка поверх ребёнка),
    // за полосой — зелёный ребёнок, ниже ребёнка — полоса обводки на сером фоне.
    assert_px(&img, 15, 50, [0, 160, 0], 0);
    assert_px(&img, 22, 50, [0, 0, 0], 0);
    assert_px(&img, 35, 50, [0, 160, 0], 0);
    assert_px(&img, 22, 70, [0, 0, 0], 0);
    assert_px(&img, 40, 70, [230, 230, 230], 0);
    golden("frame_stroke_above_child", &img);
}

#[test]
fn leaf_opacity_fill_and_inside_stroke() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            opacity: Some(0.5),
            fills: solid(255, 0, 0, 255),
            strokes: stroke(6.0, StrokeAlign::Inside),
            ..rect(10.0, 10.0, 40.0, 40.0)
        },
    );
    let img = render(&p.scene(), &camera(60.0, 60.0));
    // Внутри — заливка с прозрачностью 0.5; кольцо — чёрный с 0.5 целиком (заливка под ним
    // не просвечивает и не затемняет его): серый 128, а не тёмный 64.
    assert_px(&img, 30, 30, [255, 128, 128], 2);
    assert_px(&img, 12, 30, [128, 128, 128], 2);
    assert_px(&img, 5, 30, [255, 255, 255], 0);
    golden("leaf_opacity_fill_and_inside_stroke", &img);
}

#[test]
fn light_theme_labels() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Главный экран".into()),
            ..rect(20.0, 40.0, 120.0, 60.0)
        },
    );
    let overlay = Overlay {
        theme: OverlayTheme::light(),
        pixel_grid: false,
        ..Default::default()
    };
    let img = render_overlay(&p.scene(), &camera(160.0, 120.0), &overlay, WHITE);
    // Подпись тёмная на светлом фоне.
    let dark = (20..140u16)
        .flat_map(|x| (24..38u16).map(move |y| (x, y)))
        .filter(|&(x, y)| px(&img, x, y)[0] < 0xa0)
        .count();
    assert!(dark > 40, "тёмных пикселей подписи: {dark}");
    golden("light_theme_labels", &img);
}
