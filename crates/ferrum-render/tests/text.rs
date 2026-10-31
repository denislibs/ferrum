//! Текст (подсистема 3, этап 1): глифы контурами `skrifa`, украшения, полосы
//! LOD мелкого текста, ленивая раскладка с пределом времени кадра, шрифт,
//! который ещё грузится, экспорт PNG и превью с текстом.

mod common;

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_render::{Clock, Fonts, NoImages, Renderer, SharedFonts};
use ferrum_text::{group, read_faces, SourceKind};

/// Часы, сдвигающиеся на 5 мс при каждом чтении: любая раскладка «длится» 5 мс,
/// то есть дольше предела кадра в 4 мс, независимо от скорости машины.
struct SteppingClock(Cell<f64>);

impl Clock for SteppingClock {
    fn now_ms(&self) -> f64 {
        let t = self.0.get();
        self.0.set(t + 5.0);
        t
    }
}

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const INTER: [&str; 2] = [
    "inter/Inter-Variable.ttf",
    "inter/Inter-Italic-Variable.ttf",
];

/// Реестр со вшитым Inter; `load` — загрузить ли его байты.
fn fonts(load: bool) -> SharedFonts {
    let shared = Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let faces = INTER
            .iter()
            .flat_map(|r| read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(SourceKind::Bundled, group(faces));
        if load {
            for r in INTER {
                f.load("Inter", r, font_file(r)).unwrap();
            }
        }
    }
    shared
}

fn renderer(fonts: &SharedFonts) -> Renderer {
    let mut r = Renderer::new();
    r.set_fonts(fonts.clone());
    r
}

/// Текстовый узел с содержимым и свойствами.
fn add_text(
    p: &mut Page,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    content: TextContent,
    props: TextProps,
) -> NodeId {
    p.doc
        .transact(p.page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_props(
                id,
                &PropsPatch {
                    text: Some(props),
                    ..rect(x, y, w, h)
                },
            )?;
            tx.set_text_content(id, &content)?;
            Ok(vec![id])
        })
        .unwrap()[0]
}

fn props(size: f64, sizing: TextSizing) -> TextProps {
    TextProps {
        sizing,
        style: TextStyle {
            size,
            ..TextStyle::default()
        },
        ..TextProps::default()
    }
}

fn dark_pixels(img: &ferrum_render::Pixmap, x0: u16, y0: u16, x1: u16, y1: u16) -> usize {
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if px(img, x, y)[0] < 128 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn text_golden() {
    let fonts = fonts(true);
    let mut p = Page::new();
    let red = Fill::solid(Color::rgba(220, 30, 30, 255));
    let content = TextContent {
        runs: vec![
            TextRun {
                text: "Ferrum ".into(),
                attrs: TextAttrs {
                    style: Some("Bold".into()),
                    ..Default::default()
                },
            },
            TextRun {
                text: "текст".into(),
                attrs: TextAttrs {
                    fill: Some(red),
                    decoration: Some(Decoration::Underline),
                    ..Default::default()
                },
            },
            TextRun {
                text: "\nвторая строка\n".into(),
                attrs: TextAttrs::default(),
            },
        ],
    };
    add_text(
        &mut p,
        10.0,
        10.0,
        200.0,
        70.0,
        content,
        props(24.0, TextSizing::AutoHeight),
    );
    let img = ferrum_render_with(&fonts, &p.scene(), 220.0, 90.0);
    // Чёрный «F» в начале, красное «текст», белое поле справа от строк.
    assert!(dark_pixels(&img, 10, 14, 40, 38) > 40);
    let reds = (100..200)
        .flat_map(|x| (14..44).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let c = px(&img, x, y);
            c[0] > 180 && c[1] < 90
        })
        .count();
    assert!(reds > 40, "{reds}");
    assert_px(&img, 215, 20, [255, 255, 255], 0);
    golden("text", &img);
}

fn ferrum_render_with(
    fonts: &SharedFonts,
    scene: &ferrum_scene::Scene,
    w: f64,
    h: f64,
) -> ferrum_render::Pixmap {
    render_with(&mut renderer(fonts), scene, &camera(w, h), &NoImages)
}

#[test]
fn small_text_is_drawn_as_bars() {
    let fonts = fonts(true);
    let mut p = Page::new();
    add_text(
        &mut p,
        0.0,
        0.0,
        400.0,
        200.0,
        TextContent::plain("Строка один\nСтрока два"),
        props(40.0, TextSizing::AutoWidth),
    );
    let scene = p.scene();
    let mut r = renderer(&fonts);
    let mut cam = camera(200.0, 100.0);
    // Зум 5 %: строка 48 px → 2.4 px на экране — полосой.
    cam.set_zoom(0.05);
    cam.set_center(kurbo::Point::new(100.0, 50.0));
    let (w, h) = cam.device_size();
    let mut backend = ferrum_render::CpuBackend::new(w, h);
    let stats = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
    assert_eq!((stats.text_greeked, stats.text_glyphs), (1, 0), "{stats:?}");
}

#[test]
fn layout_is_lazy_cached_and_time_limited() {
    let fonts = fonts(true);
    let mut p = Page::new();
    for i in 0..600 {
        add_text(
            &mut p,
            (i % 30) as f64 * 30.0,
            (i / 30) as f64 * 20.0,
            28.0,
            16.0,
            TextContent::plain(&format!("Текст {i}")),
            props(12.0, TextSizing::Fixed),
        );
    }
    let scene = p.scene();
    let mut r = renderer(&fonts);
    r.set_clock(Some(Box::new(SteppingClock(Cell::new(0.0)))));
    // Зум 20 %: строки по 3 px и меньше — полосы, глифов нет (кадр дешёвый).
    let mut cam = camera(200.0, 100.0);
    cam.set_zoom(0.2);
    cam.set_center(kurbo::Point::new(450.0, 200.0));
    let (w, h) = cam.device_size();
    let mut backend = ferrum_render::CpuBackend::new(w, h);
    let first = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
    // Первая раскладка — всегда, дальше предел времени (часы шагают на 5 мс >
    // 4 мс): остальное — полосы по оценке.
    assert_eq!(first.text_layouts, 1, "{first:?}");
    assert_eq!(first.text_greeked, 600, "{first:?}");
    assert!(first.needs_another_frame);
    let mut frames = 1;
    loop {
        let s = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
        frames += 1;
        if !s.needs_another_frame {
            assert_eq!(s.text_greeked, 600, "{s:?}");
            break;
        }
        assert!(frames < 1000, "раскладка не догнала");
    }
    // Разложенное не раскладывается заново.
    let again = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
    assert_eq!(again.text_layouts, 0);
}

#[test]
fn loading_font_draws_nothing_until_it_arrives() {
    let fonts = fonts(false);
    let mut p = Page::new();
    add_text(
        &mut p,
        10.0,
        10.0,
        0.0,
        0.0,
        TextContent::plain("Ferrum"),
        props(24.0, TextSizing::AutoWidth),
    );
    // Размер рамки ставит редактор; здесь — вручную, как в файле.
    let mut scene = p.scene();
    let ids = p.doc.page(p.page).unwrap().walk();
    p.edit(ids[0], rect(10.0, 10.0, 90.0, 30.0));
    p.update(&mut scene);
    let mut r = renderer(&fonts);
    let blank = render_with(&mut r, &scene, &camera(120.0, 50.0), &NoImages);
    assert_eq!(dark_pixels(&blank, 0, 0, 120, 50), 0);
    let reqs = fonts.borrow_mut().take_requests();
    assert_eq!(reqs.len(), 1);
    for f in &reqs[0].files {
        fonts.borrow_mut().load("Inter", f, font_file(f)).unwrap();
    }
    let drawn = render_with(&mut r, &scene, &camera(120.0, 50.0), &NoImages);
    assert!(dark_pixels(&drawn, 0, 0, 120, 50) > 50);
}

#[test]
fn png_export_and_thumbnail_include_text() {
    let fonts = fonts(true);
    let mut p = Page::new();
    let id = add_text(
        &mut p,
        0.0,
        0.0,
        120.0,
        30.0,
        TextContent::plain("Экспорт"),
        props(24.0, TextSizing::Fixed),
    );
    let scene = p.scene();
    let mut r = renderer(&fonts);
    let png = r.export_pixmap(&scene, id, 2.0, &NoImages).unwrap();
    assert_eq!((png.width(), png.height()), (240, 60));
    let inked = png.data().iter().filter(|c| c.a > 128).count();
    assert!(inked > 300, "{inked}");
    let thumb = r
        .thumbnail_png(&scene, 480, 320, Some(WHITE), &NoImages)
        .unwrap()
        .unwrap();
    let thumb = ferrum_render::Pixmap::from_png(std::io::Cursor::new(thumb)).unwrap();
    assert!(dark_pixels(&thumb, 0, 0, 480, 320) > 300);
}

/// Фрейм с текстом, мелкий настолько, что рисуется из растра.
fn frame_with_text(count: usize) -> Page {
    let mut p = Page::new();
    let frame = p.add(None, NodeKind::Frame, rect(0.0, 0.0, 400.0, 100.0));
    for i in 0..count {
        p.doc
            .transact(p.page, vec![], |tx| {
                let id = tx.create_node(Some(frame), None, NodeKind::Text)?;
                tx.set_props(
                    id,
                    &PropsPatch {
                        text: Some(props(48.0, TextSizing::AutoWidth)),
                        ..rect(10.0 + i as f64 * 150.0, 10.0, 140.0, 60.0)
                    },
                )?;
                tx.set_text_content(id, &TextContent::plain("Ferrum"))?;
                Ok(vec![id])
            })
            .unwrap();
    }
    p
}

#[test]
fn raster_does_not_keep_text_of_a_loading_font() {
    let fonts = fonts(false);
    let mut p = frame_with_text(1);
    let scene = p.scene();
    let mut r = renderer(&fonts);
    let mut cam = camera(120.0, 40.0);
    cam.set_zoom(0.25);
    cam.set_center(kurbo::Point::new(200.0, 50.0));
    let blank = render_with(&mut r, &scene, &cam, &NoImages);
    let inked = |img: &ferrum_render::Pixmap| dark_pixels(img, 0, 0, 120, 40);
    assert_eq!(inked(&blank), 0);
    let reqs = fonts.borrow_mut().take_requests();
    for f in &reqs[0].files {
        fonts.borrow_mut().load("Inter", f, font_file(f)).unwrap();
    }
    // Растр фрейма построен без текста; байты пришли — кадр показывает глифы.
    let drawn = render_with(&mut r, &scene, &cam, &NoImages);
    assert!(inked(&drawn) > 20, "{}", inked(&drawn));
}

#[test]
fn raster_catches_up_deferred_text_layout() {
    let fonts = fonts(true);
    let mut p = frame_with_text(2);
    let scene = p.scene();
    let mut r = renderer(&fonts);
    // Каждая раскладка «длится» дольше предела кадра: вторая откладывается.
    r.set_clock(Some(Box::new(SteppingClock(Cell::new(0.0)))));
    let mut cam = camera(120.0, 40.0);
    cam.set_zoom(0.25);
    cam.set_center(kurbo::Point::new(200.0, 50.0));
    let (w, h) = cam.device_size();
    let mut backend = ferrum_render::CpuBackend::new(w, h);
    // Растр с одним настоящим текстом и полосами по оценке вместо второго.
    let first = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
    assert_eq!(first.text_layouts, 1, "{first:?}");
    assert!(first.needs_another_frame, "{first:?}");
    // Следующий кадр перестраивает растр и раскладывает оставшийся текст.
    let second = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
    assert_eq!(second.text_layouts, 1, "{second:?}");
    assert_eq!(second.text_glyphs, 2, "{second:?}");
    assert!(!second.needs_another_frame, "{second:?}");
}

/// Картинка 16×8 из полос: красная слева, синяя справа.
fn two_tone() -> Arc<ferrum_render::Pixmap> {
    let mut img = ferrum_render::Pixmap::new(16, 8);
    for y in 0..8 {
        for x in 0..16u16 {
            let (r, b) = if x < 8 { (255, 0) } else { (0, 255) };
            img.set_pixel(x, y, vello_cpu::color::PremulRgba8 { r, g: 0, b, a: 255 });
        }
    }
    Arc::new(img)
}

#[test]
fn image_fill_and_strokes_paint_glyphs() {
    let fonts = fonts(true);
    let mut p = Page::new();
    let text = TextContent::plain("IIII");
    let bold = props(48.0, TextSizing::Fixed);
    let mut bold = bold;
    bold.style.font = FontName::new("Inter", "Black");
    // Заливка изображением по рамке узла: слева красное, справа синее.
    let a = add_text(&mut p, 10.0, 10.0, 100.0, 60.0, text.clone(), bold.clone());
    p.edit(
        a,
        PropsPatch {
            fills: Some(vec![Fill::image("two-tone")]),
            ..Default::default()
        },
    );
    // Без заливки, с обводкой по центру зелёного цвета в 3 px.
    let b = add_text(&mut p, 130.0, 10.0, 100.0, 60.0, text, bold);
    p.edit(
        b,
        PropsPatch {
            fills: Some(vec![]),
            strokes: Some(vec![Stroke {
                color: Color::rgba(0, 160, 0, 255),
                width: 3.0,
                align: StrokeAlign::Center,
            }]),
            ..Default::default()
        },
    );
    let images: HashMap<String, Arc<ferrum_render::Pixmap>> =
        [("two-tone".to_string(), two_tone())].into();
    // Без часов: оба текста раскладываются в этом же кадре при любой нагрузке.
    let mut r = renderer(&fonts);
    r.set_clock(None);
    let img = render_with(&mut r, &p.scene(), &camera(240.0, 80.0), &images);
    let count = |x0: u16, x1: u16, f: &dyn Fn([u8; 4]) -> bool| {
        (x0..x1)
            .flat_map(|x| (10..70).map(move |y| (x, y)))
            .filter(|&(x, y)| f(px(&img, x, y)))
            .count()
    };
    let red = |c: [u8; 4]| c[0] > 200 && c[2] < 60;
    let blue = |c: [u8; 4]| c[2] > 200 && c[0] < 60;
    let green = |c: [u8; 4]| c[1] > 120 && c[0] < 60 && c[2] < 60;
    // Глифы левой половины рамки — красные, правой — синие; между глифами — фон.
    assert!(
        count(10, 60, &red) > 100,
        "красных: {}",
        count(10, 60, &red)
    );
    assert!(
        count(60, 110, &blue) > 20,
        "синих: {}",
        count(60, 110, &blue)
    );
    assert_eq!(count(10, 60, &blue), 0);
    // Обводка без заливки: зелёный контур, середина буквы — фон.
    assert!(
        count(130, 230, &green) > 100,
        "зелёных: {}",
        count(130, 230, &green)
    );
    golden("text_paints", &img);
}

// ---- Заливки и обводки текста: попиксельные проверки --------------------

const RED: Color = Color::rgba(255, 0, 0, 255);
const GREEN: Color = Color::rgba(0, 200, 0, 255);
const BLUE: Color = Color::rgba(0, 0, 255, 255);

fn is_red(c: [u8; 4]) -> bool {
    c[0] > 200 && c[1] < 60 && c[2] < 60
}
fn is_blue(c: [u8; 4]) -> bool {
    c[2] > 200 && c[0] < 60 && c[1] < 60
}
fn is_green(c: [u8; 4]) -> bool {
    c[1] > 150 && c[0] < 60 && c[2] < 60
}
fn is_white(c: [u8; 4]) -> bool {
    c[0] > 250 && c[1] > 250 && c[2] > 250
}

/// Один кадр узла с текстом Inter Black 48 в рамке (10, 10, 100×60).
/// Возвращает картинку и статистику кадра.
fn paint_frame(
    content: TextContent,
    fills: Option<Vec<Fill>>,
    strokes: Vec<Stroke>,
    images: &HashMap<String, Arc<ferrum_render::Pixmap>>,
) -> (ferrum_render::Pixmap, ferrum_render::FrameStats) {
    let fonts = fonts(true);
    let mut p = Page::new();
    let mut tp = props(48.0, TextSizing::Fixed);
    tp.style.font = FontName::new("Inter", "Black");
    let id = add_text(&mut p, 10.0, 10.0, 100.0, 60.0, content, tp);
    p.edit(
        id,
        PropsPatch {
            fills,
            strokes: Some(strokes),
            ..Default::default()
        },
    );
    let mut r = renderer(&fonts);
    r.set_clock(None);
    let cam = camera(120.0, 80.0);
    let (w, h) = cam.device_size();
    let mut backend = ferrum_render::CpuBackend::new(w, h);
    use ferrum_render::RenderBackend;
    backend.set_transform(kurbo::Affine::IDENTITY);
    backend.set_color(WHITE);
    backend.fill_rect(&kurbo::Rect::new(0.0, 0.0, w as f64, h as f64));
    let stats = r.draw_scene(&mut backend, &p.scene(), &cam, images);
    (backend.render(), stats)
}

fn no_images() -> HashMap<String, Arc<ferrum_render::Pixmap>> {
    HashMap::new()
}

fn stroke(color: Color, width: f64, align: StrokeAlign) -> Stroke {
    Stroke {
        color,
        width,
        align,
    }
}

/// Пиксели сплошной внутренности глифов (эталон: чёрная заливка).
fn interior() -> Vec<(u16, u16)> {
    let (img, _) = paint_frame(TextContent::plain("I"), None, vec![], &no_images());
    let mut v = vec![];
    for y in 0..80 {
        for x in 0..120 {
            if px(&img, x, y)[0] < 10 {
                v.push((x, y));
            }
        }
    }
    assert!(v.len() > 100, "эталон пуст");
    v
}

fn count(img: &ferrum_render::Pixmap, f: impl Fn([u8; 4]) -> bool) -> usize {
    (0..80u16)
        .flat_map(|y| (0..120u16).map(move |x| (x, y)))
        .filter(|&(x, y)| f(px(img, x, y)))
        .count()
}

#[test]
fn outside_stroke_is_a_ring_outside_only() {
    let inside = interior();
    // Без заливки: внутренность глифа остаётся фоном, кольцо — снаружи.
    let (img, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![]),
        vec![stroke(RED, 4.0, StrokeAlign::Outside)],
        &no_images(),
    );
    assert!(inside.iter().all(|&(x, y)| is_white(px(&img, x, y))));
    assert!(count(&img, is_red) > 100);
    // Полупрозрачная заливка: обводка не просвечивает сквозь неё.
    let mut half = Fill::solid(Color::rgba(0, 0, 0, 255));
    half.opacity = 0.5;
    let (img, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![half]),
        vec![stroke(RED, 4.0, StrokeAlign::Outside)],
        &no_images(),
    );
    assert!(inside.iter().all(|&(x, y)| {
        let c = px(&img, x, y);
        c[0].abs_diff(128) < 4 && c[1].abs_diff(128) < 4
    }));
}

#[test]
fn inside_stroke_stays_within_glyphs() {
    let inside = interior();
    let (img, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![]),
        vec![stroke(RED, 2.0, StrokeAlign::Inside)],
        &no_images(),
    );
    let red_in = inside
        .iter()
        .filter(|&&(x, y)| is_red(px(&img, x, y)))
        .count();
    let white_in = inside
        .iter()
        .filter(|&&(x, y)| is_white(px(&img, x, y)))
        .count();
    assert!(red_in > 50 && white_in > 0, "{red_in} {white_in}");
    // Вне глифов (где эталон полностью белый) красного нет.
    let (reference, _) = paint_frame(TextContent::plain("I"), None, vec![], &no_images());
    let leak = (0..80u16)
        .flat_map(|y| (0..120u16).map(move |x| (x, y)))
        .filter(|&(x, y)| is_white(px(&reference, x, y)) && is_red(px(&img, x, y)))
        .count();
    assert_eq!(leak, 0);
}

#[test]
fn strokes_are_drawn_in_list_order_with_miter_joins() {
    let inside = interior();
    let (x0, y0) = (
        inside.iter().map(|p| p.0).min().unwrap(),
        inside.iter().map(|p| p.1).min().unwrap(),
    );
    let y_mid = inside[inside.len() / 2].1;
    let xl = inside
        .iter()
        .filter(|p| p.1 == y_mid)
        .map(|p| p.0)
        .min()
        .unwrap();
    // Пиксель левее края (xl — первый полностью тёмный; xl-1 — краевой): попадает и в кольцо снаружи, и в обводку по центру.
    let at_edge = |img: &ferrum_render::Pixmap| px(img, xl - 2, y_mid);
    let more_blue = |c: [u8; 4]| c[2] as i16 > c[0] as i16 + 80;
    let more_red = |c: [u8; 4]| c[0] as i16 > c[2] as i16 + 80;
    let outside_red = stroke(RED, 4.0, StrokeAlign::Outside);
    let center_blue = stroke(BLUE, 4.0, StrokeAlign::Center);
    let (a, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![]),
        vec![outside_red.clone(), center_blue.clone()],
        &no_images(),
    );
    assert!(more_blue(at_edge(&a)), "{:?}", at_edge(&a));
    let (b, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![]),
        vec![center_blue, outside_red],
        &no_images(),
    );
    assert!(more_red(at_edge(&b)), "{:?}", at_edge(&b));
    // Острый стык: угол обводки по центру в 16 px не скруглён.
    let (c, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![]),
        vec![stroke(BLUE, 16.0, StrokeAlign::Center)],
        &no_images(),
    );
    assert!(
        is_blue(px(&c, x0 - 7, y0 - 7)),
        "{:?}",
        px(&c, x0 - 7, y0 - 7)
    );
}

#[test]
fn node_fills_paint_in_order_and_hidden_fills_do_not() {
    // Верхняя (последняя) заливка побеждает.
    let (img, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![Fill::solid(RED), Fill::solid(BLUE)]),
        vec![],
        &no_images(),
    );
    assert!(count(&img, is_blue) > 100 && count(&img, is_red) == 0);
    // Скрытая верхняя заливка не рисуется.
    let mut hidden = Fill::solid(BLUE);
    hidden.visible = false;
    let (img, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![Fill::solid(RED), hidden.clone()]),
        vec![],
        &no_images(),
    );
    assert!(count(&img, is_red) > 100 && count(&img, is_blue) == 0);
    // Единственная скрытая заливка — текст не виден совсем.
    let (img, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![hidden]),
        vec![],
        &no_images(),
    );
    assert_eq!(count(&img, |c| !is_white(c)), 0);
}

fn two_runs(first: Fill) -> TextContent {
    TextContent {
        runs: vec![
            TextRun {
                text: "I".into(),
                attrs: TextAttrs {
                    fill: Some(first),
                    ..Default::default()
                },
            },
            TextRun {
                text: "I\n".into(),
                attrs: TextAttrs::default(),
            },
        ],
    }
}

#[test]
fn range_fill_overrides_node_fills_and_hidden_range_paints_nothing() {
    let green = Fill::solid(GREEN);
    let (img, _) = paint_frame(
        two_runs(green),
        Some(vec![Fill::solid(RED)]),
        vec![],
        &no_images(),
    );
    // Участок — зелёный, остальное — красная заливка узла.
    assert!(count(&img, is_green) > 100 && count(&img, is_red) > 100);
    // Скрытая заливка участка: первая буква не рисуется ни цветом узла, ни чёрным.
    let mut hidden = Fill::solid(GREEN);
    hidden.visible = false;
    let (img, _) = paint_frame(
        two_runs(hidden),
        Some(vec![Fill::solid(RED)]),
        vec![],
        &no_images(),
    );
    let red_n = count(&img, is_red);
    let (single, _) = paint_frame(
        TextContent::plain("I"),
        Some(vec![Fill::solid(RED)]),
        vec![],
        &no_images(),
    );
    // Красная одна буква (не две), ни зелёного, ни чёрного.
    assert_eq!(count(&img, is_green), 0);
    assert!(
        red_n > 100 && red_n < count(&single, is_red) * 3 / 2,
        "{red_n} {}",
        count(&single, is_red)
    );
    assert_eq!(count(&img, |c| c[0] < 60 && c[1] < 60 && c[2] < 60), 0);
    // Нулевая непрозрачность — то же самое.
    let mut clear = Fill::solid(GREEN);
    clear.opacity = 0.0;
    let (img, _) = paint_frame(
        two_runs(clear),
        Some(vec![Fill::solid(RED)]),
        vec![],
        &no_images(),
    );
    assert_eq!(count(&img, is_green), 0);
    assert_eq!(count(&img, |c| c[0] < 60 && c[1] < 60 && c[2] < 60), 0);
}

#[test]
fn missing_image_fill_draws_placeholder_and_requests_another_frame() {
    let fill = vec![Fill::image("later")];
    let (img, stats) = paint_frame(
        TextContent::plain("I"),
        Some(fill.clone()),
        vec![],
        &no_images(),
    );
    let gray = |c: [u8; 4]| c[0].abs_diff(217) < 4 && c[1].abs_diff(217) < 4;
    assert!(count(&img, gray) > 100);
    assert!(stats.needs_another_frame, "{stats:?}");
    // Картинка появилась — кадр полный, заглушки нет.
    let images: HashMap<_, _> = [("later".to_string(), two_tone())].into();
    let (img, stats) = paint_frame(TextContent::plain("I"), Some(fill), vec![], &images);
    assert!(!stats.needs_another_frame, "{stats:?}");
    assert_eq!(count(&img, gray), 0);
}

#[test]
fn decorations_follow_the_visible_paint_of_their_range() {
    let underlined = |fill: Fill| TextContent {
        runs: vec![TextRun {
            text: "I\n".into(),
            attrs: TextAttrs {
                fill: Some(fill),
                decoration: Some(Decoration::Underline),
                ..Default::default()
            },
        }],
    };
    // Видимая заливка участка: подчёркивание есть и того же цвета.
    let (img, _) = paint_frame(
        underlined(Fill::solid(GREEN)),
        Some(vec![Fill::solid(RED)]),
        vec![],
        &no_images(),
    );
    let (plain, _) = paint_frame(TextContent::plain("I"), Some(vec![]), vec![], &no_images());
    assert_eq!(count(&plain, |c| !is_white(c)), 0);
    assert!(count(&img, is_green) > 300, "{}", count(&img, is_green));
    // Скрытая и прозрачная — ни глифов, ни подчёркивания (никакого чёрного).
    let mut hidden = Fill::solid(GREEN);
    hidden.visible = false;
    let mut clear = Fill::solid(GREEN);
    clear.opacity = 0.0;
    for f in [hidden, clear] {
        let (img, _) = paint_frame(
            underlined(f),
            Some(vec![Fill::solid(RED)]),
            vec![],
            &no_images(),
        );
        assert_eq!(count(&img, |c| !is_white(c)), 0);
    }
}
