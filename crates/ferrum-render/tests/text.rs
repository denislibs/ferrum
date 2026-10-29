//! Текст (подсистема 3, этап 1): глифы контурами `skrifa`, украшения, полосы
//! LOD мелкого текста, ленивая раскладка с пределом времени кадра, шрифт,
//! который ещё грузится, экспорт PNG и превью с текстом.

mod common;

use std::path::PathBuf;

use common::*;
use ferrum_doc::*;
use ferrum_render::{Fonts, NoImages, Renderer, SharedFonts};
use ferrum_text::{group, read_faces, SourceKind};

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
    let id = add_text(
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
    let _ = id;
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
    // Зум 20 %: строки по 3 px и меньше — полосы, глифов нет (кадр дешёвый).
    let mut cam = camera(200.0, 100.0);
    cam.set_zoom(0.2);
    cam.set_center(kurbo::Point::new(450.0, 200.0));
    let (w, h) = cam.device_size();
    let mut backend = ferrum_render::CpuBackend::new(w, h);
    let first = r.draw_scene(&mut backend, &scene, &cam, &NoImages);
    // Первый кадр раскладывает не всё: предел времени, остальное — полосы по оценке.
    assert!(
        first.text_layouts >= 1 && first.text_layouts < 600,
        "{first:?}"
    );
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
