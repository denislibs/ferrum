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
        // Полупрозрачная заливка под непрозрачной внутренней обводкой.
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
    // Границы полос закреплены попиксельно: изменение толщины обводки на ±1 px ломает
    // проверку (например, 7 px вместо 6 закрасит пиксели 21, 61, 68, 108 и 115).
    // Внутренняя: прямоугольник 15..45, обводка занимает пиксели 15..=20.
    assert_px(&img, 14, 30, [255, 255, 255], 0);
    assert_px(&img, 15, 30, [0, 0, 0], 0);
    assert_px(&img, 20, 30, [0, 0, 0], 0);
    assert_px(&img, 21, 30, [200, 200, 200], 0);
    // По центру: край 65, обводка 62..=67 (по 3 пикселя в обе стороны). Обводка «внутри»
    // вместо «по центру» закрасила бы 65..=70 и сдвинула бы обе границы.
    assert_px(&img, 61, 30, [255, 255, 255], 0);
    assert_px(&img, 62, 30, [0, 0, 0], 0);
    assert_px(&img, 67, 30, [0, 0, 0], 0);
    assert_px(&img, 68, 30, [200, 200, 200], 0);
    // Снаружи: край 115, обводка 109..=114; по вертикали (край 15) — строки 9..=14.
    assert_px(&img, 108, 30, [255, 255, 255], 0);
    assert_px(&img, 109, 30, [0, 0, 0], 0);
    assert_px(&img, 114, 30, [0, 0, 0], 0);
    assert_px(&img, 115, 30, [200, 200, 200], 0);
    assert_px(&img, 130, 8, [255, 255, 255], 0);
    assert_px(&img, 130, 9, [0, 0, 0], 0);
    assert_px(&img, 130, 14, [0, 0, 0], 0);
    assert_px(&img, 130, 15, [200, 200, 200], 0);
    // Полупрозрачная заливка с непрозрачной внутренней обводкой: кольцо чёрное, а
    // середина — заливка с прозрачностью, не затемнённая обводкой.
    assert_px(&img, 164, 30, [255, 255, 255], 0);
    assert_px(&img, 165, 30, [0, 0, 0], 0);
    assert_px(&img, 170, 30, [0, 0, 0], 0);
    assert_px(&img, 171, 30, [227, 227, 227], 2);
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

/// Картинка 16×8 из четырёх цветных блоков шириной по 4 пикселя: красный, зелёный,
/// синий, жёлтый. Пропорции 2:1 отличаются от пропорций рамки 3:2. Широкие блоки нужны,
/// чтобы билинейное сглаживание задевало только узкие полосы у границ блоков.
fn four_tone() -> Arc<Pixmap> {
    let cols = [(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0)];
    let mut img = Pixmap::new(16, 8);
    for y in 0..8 {
        for x in 0..16u16 {
            let (r, g, b) = cols[usize::from(x / 4)];
            img.set_pixel(x, y, PremulRgba8 { r, g, b, a: 255 });
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
                    ..Fill::image("four-tone")
                }]),
                ..rect(x, 10.0, 60.0, 40.0)
            },
        );
    }
    let images: HashMap<String, Arc<Pixmap>> = [("four-tone".to_string(), four_tone())].into();
    let mut r = Renderer::new();
    let img = render_with(&mut r, &p.scene(), &camera(220.0, 60.0), &images);
    let (red, green, blue, yellow, white) = (
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [255, 255, 0],
        [255, 255, 255],
    );
    // «Заполнить» (рамка 10..70 × 10..50): масштаб 5, картинка 80×40 со сдвигом −10,
    // блоки по 20 пикселей: красный виден на 10..20 (половина), зелёный 20..40, синий 40..60,
    // жёлтый 60..70 (половина). Рамка закрашена целиком, включая верхнюю и нижнюю строки:
    // «вписать» оставило бы там фон.
    assert_px(&img, 50, 10, blue, 8);
    assert_px(&img, 50, 49, blue, 8);
    assert_px(&img, 12, 11, red, 8);
    assert_px(&img, 67, 48, yellow, 8);
    // Обрезка по бокам: зелёный начинается с 20, а не с 25, жёлтый с 60, а не с 55. У «вписать»
    // и растяжения (блоки по 15) в точке 23 был бы красный, в точке 56 — жёлтый.
    assert_px(&img, 23, 30, green, 8);
    assert_px(&img, 56, 30, blue, 8);
    // Вне рамки фон.
    assert_px(&img, 8, 30, white, 0);
    assert_px(&img, 72, 30, white, 0);
    // «Вписать» (рамка 80..140): масштаб 3.75, картинка 60×30, полосы фона по 5 пикселей
    // сверху (10..15) и снизу (45..50); блоки по 15 пикселей. Растяжение или «заполнить»
    // закрасили бы полосы.
    assert_px(&img, 87, 12, white, 0);
    assert_px(&img, 117, 12, white, 0);
    assert_px(&img, 87, 47, white, 0);
    assert_px(&img, 87, 17, red, 8);
    assert_px(&img, 102, 30, green, 8);
    assert_px(&img, 117, 30, blue, 8);
    assert_px(&img, 132, 30, yellow, 8);
    // «Плитка» (рамка с 150): картинка в натуральном размере, блоки по 4 пикселя, период
    // 16 по горизонтали и 8 по вертикали; в центрах блоков цвет совпадает через период.
    // Масштабирование вместо плитки сломало бы совпадение через период.
    for (dx, c) in [(1, red), (5, green), (9, blue), (13, yellow)] {
        let x = 150 + dx;
        assert_px(&img, x, 14, c, 8);
        assert_px(&img, x + 16, 14, c, 8);
        assert_px(&img, x + 32, 14, c, 8);
        assert_px(&img, x, 22, c, 8);
        assert_px(&img, x, 38, c, 8);
    }
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
    // Рамка выделения: прямоугольник 40..100 × 50..90 (зум 1), линия в 1 px по пикселям
    // 40, 100 и 90. Без рамки или со сдвигом на пиксель эти пиксели останутся фоном.
    let select = [23, 105, 224];
    assert_px(&img, 40, 60, select, 2);
    assert_px(&img, 100, 60, select, 2);
    assert_px(&img, 55, 90, select, 2);
    assert_px(&img, 41, 60, [217, 217, 217], 1);
    // Ручки в точных местах: у левого верхнего угла (центр 40.5, 50.5) граница квадрата 8×8
    // проходит по пикселям 36 и 44, середина белая. Остальные: правая середина, правый
    // нижний угол и нижняя середина; по белой середине видно, что ручка поверх рамки.
    assert_px(&img, 36, 52, select, 2);
    assert_px(&img, 44, 52, select, 2);
    assert_px(&img, 35, 52, [255, 255, 255], 0);
    assert_px(&img, 45, 52, [217, 217, 217], 1);
    assert_px(&img, 100, 72, [255, 255, 255], 2);
    assert_px(&img, 96, 72, select, 2);
    assert_px(&img, 100, 91, [255, 255, 255], 2);
    assert_px(&img, 96, 92, select, 2);
    assert_px(&img, 70, 91, [255, 255, 255], 2);
    assert_px(&img, 66, 92, select, 2);
    // Бейдж: плашка выровнена по центру выделения (x = 70), а внутри неё есть пиксели
    // текста цвета `on_select` (белые). Поиск — ниже ручек (их белая середина и синяя
    // обводка кончаются на y = 94), иначе ручки сошли бы и за плашку, и за текст.
    let blue = |x: u16, y: u16| {
        let c = px(&img, x, y);
        c[2] > 180 && c[0] < 60
    };
    let plate: Vec<(u16, u16)> = (30..110u16)
        .flat_map(|x| (96..120u16).map(move |y| (x, y)))
        .filter(|&(x, y)| blue(x, y))
        .collect();
    let (x0, x1) = (
        plate.iter().map(|q| q.0).min().unwrap(),
        plate.iter().map(|q| q.0).max().unwrap(),
    );
    let (y0, y1) = (
        plate.iter().map(|q| q.1).min().unwrap(),
        plate.iter().map(|q| q.1).max().unwrap(),
    );
    let center = (f64::from(x0) + f64::from(x1) + 1.0) / 2.0;
    assert!((center - 70.5).abs() <= 1.5, "центр плашки {center}");
    assert!(y0 >= 97 && y1 - y0 >= 10, "плашка {y0}..{y1}");
    // Плашка замкнута: верхний и нижний края сплошные (без скруглённых углов).
    for x in x0 + 3..x1 - 2 {
        assert!(blue(x, y0) && blue(x, y1), "край плашки разорван в x = {x}");
    }
    // Текст — белые пиксели строго внутри плашки, отступив от краёв.
    let text_px = (x0 + 2..x1 - 1)
        .flat_map(|x| (y0 + 2..y1 - 1).map(move |y| (x, y)))
        .filter(|&(x, y)| px(&img, x, y)[0] > 200)
        .count();
    assert!(text_px >= 20, "пикселей текста в плашке: {text_px}");
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
    // Порог 800%: серые пиксели сетки (r != 255) есть при зуме 8.0 и ни одного при 7.99.
    // Порог «> 8» оставил бы 8.0 без сетки, а «≥ 7» показал бы её при 7.99.
    let gray = |img: &Pixmap| img.data().iter().filter(|c| c.r != 255).count();
    cam.set_zoom(8.0);
    assert!(gray(&render_overlay(&p.scene(), &cam, &overlay, WHITE)) > 100);
    cam.set_zoom(7.99);
    assert_eq!(gray(&render_overlay(&p.scene(), &cam, &overlay, WHITE)), 0);
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
    // Оранжевая полоса (локально x 10..100, y 10..30) шире фрейма (80): внутри кадра она
    // видна, за правым краем фрейма (локально x = 85) обрезана и виден фон. Без обрезки
    // по повёрнутому фрейму там было бы оранжевое.
    let at = |x: f64, y: f64| {
        let q = t * Point::new(x, y);
        (q.x as u16, q.y as u16)
    };
    let (qx, qy) = at(75.0, 20.0);
    assert_px(&img, qx, qy, [255, 140, 0], 0);
    let (qx, qy) = at(85.0, 20.0);
    assert_px(&img, qx, qy, [255, 255, 255], 0);
    let (qx, qy) = at(5.0, 20.0);
    assert_px(&img, qx, qy, [220, 220, 220], 0);
    // Эллипс (поворот −45°, т. е. по часовой стрелке, вокруг левого верхнего угла (20, 130));
    // центр в мире (32.7, 159.7), большая ось идёт вниз-вправо. Точка (50, 177) на большой
    // оси внутри, зеркальная ей (50, 142) — снаружи; при повороте против часовой стрелки
    // или вокруг центра эллипса обе проверки поменялись бы местами. Центр (33, 160) тоже
    // зелёный, а при вращении вокруг центра он оказался бы вне эллипса.
    assert_px(&img, 50, 177, [0, 170, 80], 0);
    assert_px(&img, 50, 142, [255, 255, 255], 0);
    assert_px(&img, 33, 160, [0, 170, 80], 0);
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

/// Картинка 16×8 из четырёх четвертей: красная слева сверху, зелёная справа сверху,
/// синяя слева снизу, жёлтая справа снизу — по ней видно отражение по обеим осям.
fn quadrants() -> Arc<Pixmap> {
    let mut img = Pixmap::new(16, 8);
    for y in 0..8u16 {
        for x in 0..16u16 {
            let (r, g, b) = match (x < 8, y < 4) {
                (true, true) => (255, 0, 0),
                (false, true) => (0, 255, 0),
                (true, false) => (0, 0, 255),
                (false, false) => (255, 255, 0),
            };
            img.set_pixel(x, y, PremulRgba8 { r, g, b, a: 255 });
        }
    }
    Arc::new(img)
}

#[test]
fn mirroring() {
    let mut p = Page::new();
    let image = |flip_x: bool, flip_y: bool, x: f64| PropsPatch {
        fills: Some(vec![Fill::image("quadrants")]),
        flip_x: Some(flip_x),
        flip_y: Some(flip_y),
        // Рамка 64×32 в пропорциях картинки: масштаб 4, четверть — 32×16.
        ..rect(x, 10.0, 64.0, 32.0)
    };
    p.add(None, NodeKind::Image, image(false, false, 10.0));
    let mirrored = p.add(None, NodeKind::Image, image(true, false, 84.0));
    p.add(None, NodeKind::Image, image(true, true, 158.0));
    // Отражённый по x фрейм: ребёнок у левого края рисуется у правого.
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(220, 220, 220, 255),
            flip_x: Some(true),
            ..rect(10.0, 60.0, 100.0, 60.0)
        },
    );
    p.add(
        Some(f),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 140, 0, 255),
            ..rect(5.0, 5.0, 30.0, 20.0)
        },
    );
    let scene = p.scene();
    let images: HashMap<String, Arc<Pixmap>> = [("quadrants".to_string(), quadrants())].into();
    let cam = camera(240.0, 130.0);
    let (w, h) = cam.device_size();
    let mut backend = ferrum_render::CpuBackend::new(w, h);
    let overlay = Overlay {
        theme: OverlayTheme::light(),
        selection: vec![mirrored],
        frame_labels: false,
        ..Default::default()
    };
    Renderer::new().render(&mut backend, &scene, &cam, &overlay, &images, Some(WHITE));
    let img = backend.render();
    let (red, green, blue, yellow) = ([255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]);
    // Без отражения: четверти на своих местах (центры четвертей).
    assert_px(&img, 26, 18, red, 8);
    assert_px(&img, 58, 18, green, 8);
    assert_px(&img, 26, 34, blue, 8);
    assert_px(&img, 58, 34, yellow, 8);
    // Отражение по x, как в Figma: левая и правая половины поменялись.
    assert_px(&img, 100, 18, green, 8);
    assert_px(&img, 132, 18, red, 8);
    assert_px(&img, 100, 34, yellow, 8);
    assert_px(&img, 132, 34, blue, 8);
    // По обеим осям: картинка повёрнута на 180°.
    assert_px(&img, 174, 18, yellow, 8);
    assert_px(&img, 206, 18, blue, 8);
    assert_px(&img, 174, 34, green, 8);
    assert_px(&img, 206, 34, red, 8);
    // Ребёнок отражённого фрейма: 10 + 100 − 5 − 30 = 75..105 по x, 65..85 по y.
    assert_px(&img, 90, 75, [255, 140, 0], 0);
    assert_px(&img, 25, 75, [220, 220, 220], 0);
    // Рамка выделения отражённой картинки лежит на её видимых краях, ручки — в
    // видимых углах: белая середина ручки в углах (84, 10) и (148, 42), рамка между
    // ручками на верхнем крае.
    assert_px(&img, 84, 10, [255, 255, 255], 30);
    assert_px(&img, 148, 42, [255, 255, 255], 30);
    let edge = px(&img, 100, 10);
    assert!(edge[2] > 180 && edge[0] < 80, "рамка выделения: {edge:?}");
    golden("mirroring", &img);
}
