//! Экспорт SVG (спека подсистемы 3, раздел 11): каждый тест сначала проверяет
//! смысл файла разбором XML (элементы, атрибуты, обрезка, маски, картинки), затем
//! сравнивает его с эталоном `tests/golden/<имя>.svg` побайтно.
//! Пересоздать эталоны: `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test svg`.

mod common;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_render::{ExportBudget, ExportError, NoImageBytes, Renderer, SvgOptions};
use roxmltree::{Document as Xml, Node};

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

/// Сравнивает файл с эталоном `tests/golden/<name>.svg` (с `FERRUM_UPDATE_GOLDEN=1`
/// — перезаписывает).
#[track_caller]
fn golden_svg(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.svg"));
    if std::env::var("FERRUM_UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::write(&path, actual).expect("запись эталона");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "нет эталона {}: создайте его командой FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test svg",
            path.display()
        )
    });
    assert_eq!(expected, actual, "эталон {name}");
}

fn export(p: &mut Page, ids: &[NodeId], opts: SvgOptions) -> String {
    let scene = p.scene();
    Renderer::new()
        .export_svg(&scene, ids, &p.doc.images, opts, &ExportBudget::unlimited())
        .unwrap()
}

fn by_tag<'a>(doc: &'a Xml<'a>, tag: &str) -> Vec<Node<'a, 'a>> {
    doc.descendants()
        .filter(|n| n.tag_name().name() == tag)
        .collect()
}

#[test]
fn frame_shapes_strokes_and_opacity() {
    let mut p = Page::new();
    let frame = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Карточка".into()),
            fills: solid(255, 255, 255, 255),
            strokes: stroke(1.0, StrokeAlign::Inside),
            ..rect(10.0, 20.0, 200.0, 120.0)
        },
    );
    // Скруглённый прямоугольник с внутренней обводкой; выступает за фрейм — обрезка.
    p.add(
        Some(frame),
        NodeKind::Rect,
        PropsPatch {
            corner_radius: Some(8.0),
            fills: solid(255, 0, 0, 255),
            strokes: stroke(2.0, StrokeAlign::Inside),
            ..rect(150.0, 10.0, 80.0, 40.0)
        },
    );
    // Эллипс с внешней полупрозрачной обводкой и прозрачностью узла.
    p.add(
        Some(frame),
        NodeKind::Ellipse,
        PropsPatch {
            opacity: Some(0.5),
            fills: solid(0, 0, 255, 128),
            strokes: Some(vec![Stroke {
                color: Color::rgba(0, 128, 0, 255),
                width: 3.0,
                align: StrokeAlign::Outside,
            }]),
            ..rect(20.0, 20.0, 60.0, 40.0)
        },
    );
    // Повёрнутая линия: атрибут transform.
    p.add(
        Some(frame),
        NodeKind::Line,
        PropsPatch {
            rotation: Some(-30.0),
            strokes: stroke(2.0, StrokeAlign::Center),
            ..rect(20.0, 90.0, 100.0, 0.0)
        },
    );
    let svg = export(&mut p, &[frame], SvgOptions::default());
    let doc = Xml::parse(&svg).expect("SVG разбирается как XML");
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "svg");
    assert_eq!(root.attribute("width"), Some("200"));
    assert_eq!(root.attribute("height"), Some("120"));
    assert_eq!(root.attribute("viewBox"), Some("0 0 200 120"));
    // Фрейм — заливка, обрезанные дети, обводка над ними.
    let clips = by_tag(&doc, "clipPath");
    assert_eq!(clips.len(), 3, "обрезка фрейма и две внутренние обводки");
    let g = doc
        .descendants()
        .find(|n| n.attribute("clip-path").is_some() && n.tag_name().name() == "g")
        .expect("<g clip-path> фрейма");
    let kids: Vec<_> = g.children().filter(|n| n.is_element()).collect();
    assert!(kids.len() >= 4, "дети внутри обрезки");
    let rects = by_tag(&doc, "rect");
    let red = rects
        .iter()
        .find(|r| r.attribute("fill") == Some("#FF0000"))
        .expect("красный прямоугольник");
    assert_eq!(
        [red.attribute("x"), red.attribute("y"), red.attribute("rx")],
        [Some("150"), Some("10"), Some("8")],
        "сдвиг запечён в координаты"
    );
    let inside = rects
        .iter()
        .find(|r| r.attribute("stroke-width") == Some("4"))
        .expect("внутренняя обводка — двойная толщина");
    assert!(inside.attribute("clip-path").is_some());
    // Внешняя обводка — маска «всё, кроме формы».
    let masks = by_tag(&doc, "mask");
    assert_eq!(masks.len(), 1);
    let mask_kids: Vec<_> = masks[0].children().filter(|n| n.is_element()).collect();
    assert_eq!(mask_kids[0].attribute("fill"), Some("white"));
    assert_eq!(mask_kids[1].attribute("fill"), Some("black"));
    let ellipse: Vec<_> = by_tag(&doc, "ellipse")
        .into_iter()
        .filter(|n| !n.ancestors().any(|a| a.tag_name().name() == "defs"))
        .collect();
    assert_eq!(ellipse.len(), 2, "заливка и обводка (третий — в маске)");
    assert_eq!(ellipse[0].attribute("fill-opacity"), Some("0.502"));
    assert_eq!(
        ellipse[0].parent().unwrap().attribute("opacity"),
        Some("0.5")
    );
    let line = &by_tag(&doc, "line")[0];
    assert!(line.attribute("transform").unwrap().starts_with("matrix("));
    // Обводка фрейма — последней, вне обрезки детей.
    let last = root
        .descendants()
        .rfind(|n| n.is_element() && n.attribute("stroke").is_some())
        .unwrap();
    assert_eq!(last.attribute("stroke-width"), Some("2"));
    assert!(!last
        .ancestors()
        .skip(1)
        .any(|a| a.attribute("clip-path").is_some()));
    golden_svg("svg_frame_shapes", &svg);
}

#[test]
fn vectors_arrows_and_booleans_are_paths() {
    let mut p = Page::new();
    let net = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(100.0, 60.0),
        ],
        segments: vec![VectorSegment::line(0, 1), VectorSegment::line(1, 2)],
        regions: vec![],
    };
    let arrow = p.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(net)),
            strokes: stroke(2.0, StrokeAlign::Center),
            stroke_cap_end: Some(StrokeCap::ArrowLines),
            stroke_cap_start: Some(StrokeCap::Circle),
            stroke_join: Some(StrokeJoin::Round),
            ..rect(10.0, 10.0, 100.0, 60.0)
        },
    );
    let b = p.add(
        None,
        NodeKind::Boolean,
        PropsPatch {
            x: Some(150.0),
            y: Some(10.0),
            boolean_op: Some(BooleanOp::Subtract),
            fills: solid(0, 160, 0, 255),
            ..Default::default()
        },
    );
    p.add(
        Some(b),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0, 255),
            ..rect(0.0, 0.0, 60.0, 60.0)
        },
    );
    p.add(
        Some(b),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 255, 255),
            ..rect(20.0, 20.0, 20.0, 20.0)
        },
    );
    let svg = export(&mut p, &[arrow, b], SvgOptions::default());
    let doc = Xml::parse(&svg).unwrap();
    let paths = by_tag(&doc, "path");
    let line = paths
        .iter()
        .find(|n| n.attribute("stroke-linejoin") == Some("round"))
        .expect("путь вектора — обводка со скруглённым стыком");
    assert!(line.attribute("d").unwrap().starts_with('M'));
    // Окончания — залитые фигуры (круг) и стрелка линиями.
    assert!(paths
        .iter()
        .any(|n| n.attribute("fill") == Some("#000000") && n.attribute("stroke").is_none()));
    // Булева группа — один путь цветом группы, дети не выводятся; дыра — evenodd или
    // второй контур.
    let green: Vec<_> = paths
        .iter()
        .filter(|n| n.attribute("fill") == Some("#00A000"))
        .collect();
    assert_eq!(green.len(), 1);
    assert_eq!(
        green[0].attribute("d").unwrap().matches('M').count(),
        2,
        "контур и дыра"
    );
    assert!(!svg.contains("#FF0000") && !svg.contains("#0000FF"));
    golden_svg("svg_vector_boolean", &svg);
}

/// PNG 2×1: красный и синий пиксели.
fn tiny_png() -> Vec<u8> {
    let mut px = ferrum_render::Pixmap::new(2, 1);
    px.data_as_u8_slice_mut()
        .copy_from_slice(&[255, 0, 0, 255, 0, 0, 255, 255]);
    px.into_png().unwrap()
}

#[test]
fn image_fills_are_data_png_once_per_file() {
    let mut p = Page::new();
    let hash = p.doc.images.add(tiny_png());
    let mut ids = vec![];
    for (i, mode) in [ScaleMode::Fill, ScaleMode::Fit, ScaleMode::Tile]
        .into_iter()
        .enumerate()
    {
        ids.push(p.add(
            None,
            NodeKind::Rect,
            PropsPatch {
                fills: Some(vec![Fill {
                    scale_mode: mode,
                    ..Fill::image(hash.clone())
                }]),
                ..rect(i as f64 * 50.0, 0.0, 40.0, 40.0)
            },
        ));
    }
    let svg = export(&mut p, &ids, SvgOptions::default());
    let doc = Xml::parse(&svg).unwrap();
    let images = by_tag(&doc, "image");
    assert_eq!(images.len(), 1, "байты картинки — один раз");
    let href = images[0]
        .attribute(("http://www.w3.org/1999/xlink", "href"))
        .unwrap();
    assert!(href.starts_with("data:image/png;base64,iVBORw0KGgo"));
    assert_eq!(by_tag(&doc, "pattern").len(), 3);
    assert_eq!(by_tag(&doc, "use").len(), 3);
    // Плитка — узор размером с картинку (2×1).
    let tile = by_tag(&doc, "pattern")[2];
    assert_eq!(
        [tile.attribute("width"), tile.attribute("height")],
        [Some("2"), Some("1")]
    );
    // Без байтов картинки заливка пропускается, а не ломает файл.
    let scene = p.scene();
    let no = Renderer::new()
        .export_svg(
            &scene,
            &ids,
            &NoImageBytes,
            SvgOptions::default(),
            &ExportBudget::unlimited(),
        )
        .unwrap();
    assert!(Xml::parse(&no).is_ok() && !no.contains("<image"));
    golden_svg("svg_image_fills", &svg);
}

#[test]
fn ids_come_from_layer_names_and_are_unique() {
    let mut p = Page::new();
    let g = p.add(
        None,
        NodeKind::Group,
        PropsPatch {
            name: Some("Иконка 1".into()),
            ..Default::default()
        },
    );
    for _ in 0..2 {
        p.add(
            Some(g),
            NodeKind::Rect,
            PropsPatch {
                name: Some("Фон & <тень>".into()),
                fills: solid(1, 2, 3, 255),
                ..rect(0.0, 0.0, 10.0, 10.0)
            },
        );
    }
    let svg = export(
        &mut p,
        &[g],
        SvgOptions {
            ids: true,
            ..SvgOptions::default()
        },
    );
    let doc = Xml::parse(&svg).unwrap();
    let ids: Vec<_> = doc
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .collect();
    assert_eq!(ids, ["Иконка_1", "Фон____тень_", "Фон____тень__2"]);
    // Без флажка группа без прозрачности — без своего <g>.
    let plain = export(&mut p, &[g], SvgOptions::default());
    assert!(!plain.contains("<g") && !plain.contains(" id="));
}

#[test]
fn errors_and_limits() {
    let mut p = Page::new();
    let id = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 0, 255),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let hidden = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            visible: Some(false),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let scene = p.scene();
    let mut r = Renderer::new();
    let run = |r: &mut Renderer, ids: &[NodeId], budget: &ExportBudget| {
        r.export_svg(&scene, ids, &HashMap::new(), SvgOptions::default(), budget)
    };
    assert_eq!(
        run(&mut r, &[hidden], &ExportBudget::unlimited()),
        Err(ExportError::Empty)
    );
    assert_eq!(
        run(&mut r, &[], &ExportBudget::unlimited()),
        Err(ExportError::NotFound)
    );
    let tight = ExportBudget {
        max_bytes: 64,
        ..ExportBudget::unlimited()
    };
    assert_eq!(run(&mut r, &[id], &tight), Err(ExportError::TooBig));
}

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Реестр с загруженным вшитым Inter.
fn inter() -> ferrum_render::SharedFonts {
    let shared = ferrum_render::Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let files = [
            "inter/Inter-Variable.ttf",
            "inter/Inter-Italic-Variable.ttf",
        ];
        let faces = files
            .iter()
            .flat_map(|r| ferrum_text::read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(ferrum_text::SourceKind::Bundled, ferrum_text::group(faces));
        for r in files {
            f.load("Inter", r, font_file(r)).unwrap();
        }
    }
    shared
}

/// Текст «Привет <мир> & co» Inter Bold 24 с трекингом 1 px; « & co» — красным.
fn styled_text(p: &mut Page, family: &str) -> NodeId {
    let mut props = TextProps::default();
    props.style.size = 24.0;
    props.style.font = FontName::new(family, "Bold");
    props.style.letter_spacing = LetterSpacing::Px(1.0);
    let content = TextContent {
        runs: vec![
            TextRun {
                text: "Привет <мир>".into(),
                attrs: TextAttrs::default(),
            },
            TextRun {
                text: " & co\n".into(),
                attrs: TextAttrs {
                    fill: Some(Fill::solid(Color::rgba(255, 0, 0, 255))),
                    ..Default::default()
                },
            },
        ],
    };
    p.doc
        .transact(p.page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_props(
                id,
                &PropsPatch {
                    text: Some(props.clone()),
                    strokes: stroke(1.0, StrokeAlign::Outside),
                    ..rect(10.0, 10.0, 300.0, 40.0)
                },
            )?;
            tx.set_text_content(id, &content)?;
            Ok(vec![id])
        })
        .unwrap()[0]
}

#[test]
fn text_as_elements_or_outlines() {
    let fonts = inter();
    let mut p = Page::new();
    let id = styled_text(&mut p, "Inter");
    let scene = p.scene();
    let mut r = Renderer::new();
    r.set_fonts(fonts);
    let mut run = |outline_text: bool| {
        r.export_svg(
            &scene,
            &[id],
            &NoImageBytes,
            SvgOptions {
                outline_text,
                ids: false,
            },
            &ExportBudget::unlimited(),
        )
        .unwrap()
    };
    // Текстом: <text> со шрифтом, текст с экранированием читается обратно.
    let svg = run(false);
    let doc = Xml::parse(&svg).unwrap();
    let texts: Vec<_> = by_tag(&doc, "text")
        .into_iter()
        .filter(|n| !n.ancestors().any(|a| a.tag_name().name() == "defs"))
        .collect();
    let filled: Vec<_> = texts
        .iter()
        .filter(|t| t.attribute("fill").is_some())
        .collect();
    // Прогон — элемент: участок другого цвета и смена письменности делят строку.
    assert_eq!(filled.len(), 3, "по элементу на прогон");
    let t = filled[0];
    assert_eq!(t.attribute("font-family"), Some("'Inter', sans-serif"));
    assert_eq!(t.attribute("font-weight"), Some("700"));
    assert_eq!(t.attribute("font-size"), Some("24"));
    assert_eq!(t.attribute("letter-spacing"), Some("1"));
    let spans: String = filled
        .iter()
        .flat_map(|t| t.children())
        .map(|s| s.text().unwrap_or_default())
        .collect();
    assert_eq!(spans, "Привет <мир> & co");
    assert_eq!(filled[1].attribute("fill"), Some("#FF0000"));
    // Внешняя обводка текста — одной маской со всеми <text> (как у рендера: обводка
    // одного прогона не ложится на заливку соседнего), обводки — после заливок.
    let masks = by_tag(&doc, "mask");
    assert_eq!(masks.len(), 1);
    assert_eq!(
        masks[0]
            .descendants()
            .filter(|n| n.tag_name().name() == "text")
            .count(),
        3
    );
    let stroked = texts
        .iter()
        .filter(|t| t.attribute("stroke").is_some())
        .collect::<Vec<_>>();
    assert_eq!(stroked.len(), 3);
    for t in &stroked {
        let g = t.parent_element().unwrap();
        assert_eq!(g.attribute("mask"), Some("url(#mask0)"));
        assert!(filled.iter().all(|f| f.range().end <= g.range().start));
    }
    golden_svg("svg_text_elements", &svg);
    // Кривыми: ни одного <text>, глифы — пути.
    let svg = run(true);
    let doc = Xml::parse(&svg).unwrap();
    assert!(by_tag(&doc, "text").is_empty());
    assert!(by_tag(&doc, "path")
        .iter()
        .any(|n| n.attribute("fill") == Some("#FF0000")));
}

/// Отсутствующий шрифт: текст разложен запасным Inter — так он и пишется.
#[test]
fn missing_font_is_written_with_the_fallback() {
    let fonts = inter();
    let mut p = Page::new();
    let id = styled_text(&mut p, "Нет Такого");
    let scene = p.scene();
    let mut r = Renderer::new();
    r.set_fonts(fonts);
    let svg = r
        .export_svg(
            &scene,
            &[id],
            &NoImageBytes,
            SvgOptions {
                outline_text: false,
                ids: false,
            },
            &ExportBudget::unlimited(),
        )
        .unwrap();
    assert!(svg.contains("font-family=\"'Inter', sans-serif\"") && !svg.contains("Нет Такого"));
}

/// Узел текста `content` шириной `w` (Fixed, высота `h`) в (0, 0); `f` правит свойства.
fn text_node(
    p: &mut Page,
    content: &str,
    w: f64,
    h: f64,
    strokes: Option<Vec<Stroke>>,
    f: impl FnOnce(&mut TextProps),
) -> NodeId {
    let mut props = TextProps::default();
    props.style.size = 20.0;
    props.sizing = TextSizing::Fixed;
    f(&mut props);
    let content = TextContent::plain(content);
    p.doc
        .transact(p.page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_props(
                id,
                &PropsPatch {
                    text: Some(props.clone()),
                    fills: solid(0, 0, 0, 255),
                    strokes: strokes.clone(),
                    ..rect(0.0, 0.0, w, h)
                },
            )?;
            tx.set_text_content(id, &content)?;
            Ok(vec![id])
        })
        .unwrap()[0]
}

/// SVG узлов текстом (`<text>`) с вшитым Inter.
fn text_svg(scene: &ferrum_scene::Scene, ids: &[NodeId]) -> String {
    let mut r = Renderer::new();
    r.set_fonts(inter());
    r.export_svg(
        scene,
        ids,
        &NoImageBytes,
        SvgOptions {
            outline_text: false,
            ids: false,
        },
        &ExportBudget::unlimited(),
    )
    .unwrap()
}

/// Числа списка атрибута (`x`, `y`).
fn numbers(n: &Node, attr: &str) -> Vec<f64> {
    n.attribute(attr)
        .unwrap_or_default()
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect()
}

/// `<text>` с заливкой (не в `<defs>`, не обводки).
fn filled_texts<'a>(doc: &'a Xml<'a>) -> Vec<Node<'a, 'a>> {
    by_tag(doc, "text")
        .into_iter()
        .filter(|n| n.attribute("fill").is_some())
        .filter(|n| !n.ancestors().any(|a| a.tag_name().name() == "defs"))
        .collect()
}

/// Позиции глифов раскладки (левые края глифов с шагом) — как их рисует холст.
fn glyph_xs(
    fonts: &ferrum_render::SharedFonts,
    scene: &ferrum_scene::Scene,
    id: NodeId,
) -> Vec<f64> {
    let p = &scene.at(scene.slot(id).unwrap()).props;
    let mut f = fonts.borrow_mut();
    let layout = f.layout(&ferrum_text::LayoutRequest::for_node(
        p.text.as_ref().unwrap(),
        &p.fills,
        p.w,
        p.h,
    ));
    layout
        .runs
        .iter()
        .flat_map(|r| r.glyphs.iter())
        .filter(|g| g.advance > 0.0)
        .map(|g| g.x as f64)
        .collect()
}

/// Позиции `<text>` — по глифам раскладки: выключка по ширине и кернинг не
/// теряются (у каждого символа — x его кластера, по единице UTF-16).
#[test]
fn text_positions_follow_the_layout_glyphs() {
    let fonts = inter();
    let words = "AVAV Ferrum keeps every glyph where the canvas put it, even justified lines";
    let mut p = Page::new();
    let justify = text_node(&mut p, words, 260.0, 200.0, None, |t| {
        t.align_h = AlignH::Justify
    });
    let left = text_node(&mut p, words, 260.0, 200.0, None, |_| {});
    let scene = p.scene();
    let mut r = Renderer::new();
    r.set_fonts(fonts.clone());
    let svg_of = |r: &mut Renderer, id| {
        r.export_svg(
            &scene,
            &[id],
            &NoImageBytes,
            SvgOptions {
                outline_text: false,
                ids: false,
            },
            &ExportBudget::unlimited(),
        )
        .unwrap()
    };
    let (svg_j, svg_l) = (svg_of(&mut r, justify), svg_of(&mut r, left));
    assert_ne!(svg_j, svg_l, "выключка по ширине видна в файле");
    for (svg, id) in [(&svg_j, justify), (&svg_l, left)] {
        let doc = Xml::parse(svg).unwrap();
        let texts = filled_texts(&doc);
        assert!(texts.len() > 1, "несколько строк");
        let mut got = Vec::new();
        for t in &texts {
            let text = t.text().unwrap();
            let xs = numbers(t, "x");
            assert_eq!(xs.len(), text.encode_utf16().count(), "{text}");
            assert_eq!(numbers(t, "y").len(), 1, "y строки — одним числом");
            got.extend(xs);
        }
        got.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        let want = glyph_xs(&fonts, &scene, id);
        // Узел в (0, 0): координаты файла — координаты раскладки. Пробелы в
        // конце строк в <text> не попадают.
        for x in &got {
            assert!(
                want.iter().any(|w| (w - x).abs() < 0.01),
                "x {x} нет среди глифов"
            );
        }
        assert!(got.len() + texts.len() >= want.len());
    }
}

/// Прогон справа налево: `direction="rtl"`, позиция символа — правый край его
/// кластера, пробел в конце не сдвигает буквы.
#[test]
fn rtl_text_gets_direction_and_right_edges() {
    let fonts = inter();
    let mut p = Page::new();
    let id = text_node(&mut p, "שלום עולם ", 300.0, 40.0, None, |_| {});
    let scene = p.scene();
    let doc_svg = text_svg(&scene, &[id]);
    let doc = Xml::parse(&doc_svg).unwrap();
    let texts = filled_texts(&doc);
    assert_eq!(texts.len(), 1);
    let t = texts[0];
    assert_eq!(t.attribute("direction"), Some("rtl"));
    assert_eq!(t.attribute("unicode-bidi"), Some("embed"));
    let text = t.text().unwrap();
    assert!(text.starts_with("שלום"), "{text}");
    let xs = numbers(&t, "x");
    assert_eq!(xs.len(), text.encode_utf16().count());
    assert!(xs.windows(2).all(|w| w[0] >= w[1]), "справа налево: {xs:?}");
    // Первая буква — у правого края самого правого глифа.
    let p = &scene.at(scene.slot(id).unwrap()).props;
    let mut f = fonts.borrow_mut();
    let layout = f.layout(&ferrum_text::LayoutRequest::for_node(
        p.text.as_ref().unwrap(),
        &p.fills,
        p.w,
        p.h,
    ));
    let right = layout
        .runs
        .iter()
        .flat_map(|r| r.glyphs.iter())
        .map(|g| (g.x + g.advance) as f64)
        .fold(f64::MIN, f64::max);
    assert!((xs[0] - right).abs() < 0.01, "{} {right}", xs[0]);
}

/// Лигатура (у Inter — стрелка «->»): символы одного кластера — с одной позицией.
#[test]
fn ligature_characters_share_their_cluster_position() {
    let mut p = Page::new();
    let id = text_node(&mut p, "office ffi fl ->", 300.0, 40.0, None, |_| {});
    let scene = p.scene();
    let svg = text_svg(&scene, &[id]);
    let doc = Xml::parse(&svg).unwrap();
    let t = filled_texts(&doc)[0];
    let text = t.text().unwrap();
    let xs = numbers(&t, "x");
    assert_eq!(text, "office ffi fl ->", "символы лигатуры не теряются");
    assert_eq!(xs.len(), text.len());
    assert!(xs.windows(2).all(|w| w[0] <= w[1]));
    // У Inter «->» — стрелка одним глифом: оба символа — в позиции её кластера.
    assert_eq!(xs[14], xs[15]);
}

/// Недопустимые в XML символы (U+FFFE, U+FFFF, управляющие) выбрасываются вместе
/// с позицией: файл разбирается, позиций столько же, сколько символов.
#[test]
fn non_xml_characters_are_dropped_from_text() {
    let mut p = Page::new();
    let id = text_node(
        &mut p,
        "a\u{FFFE}b\u{FFFF}c\u{1}d",
        300.0,
        40.0,
        None,
        |_| {},
    );
    let scene = p.scene();
    let svg = text_svg(&scene, &[id]);
    let doc = Xml::parse(&svg).expect("файл разбирается");
    let t = filled_texts(&doc)[0];
    assert_eq!(t.text(), Some("abcd"));
    assert_eq!(numbers(&t, "x").len(), 4);
}

/// Пустой абзац (в раскладке — U+200B) не даёт ни `<text>`, ни обводки, ни маски.
#[test]
fn empty_paragraphs_write_nothing() {
    let mut p = Page::new();
    let id = text_node(
        &mut p,
        "Один\n\nДва",
        300.0,
        100.0,
        stroke(1.0, StrokeAlign::Outside),
        |_| {},
    );
    let scene = p.scene();
    let svg = text_svg(&scene, &[id]);
    let doc = Xml::parse(&svg).unwrap();
    let all: Vec<_> = by_tag(&doc, "text");
    assert!(all
        .iter()
        .all(|t| !t.text().unwrap_or_default().contains('\u{200B}')));
    assert_eq!(filled_texts(&doc).len(), 2);
    let masks = by_tag(&doc, "mask");
    assert_eq!(masks.len(), 1);
    assert_eq!(
        masks[0]
            .descendants()
            .filter(|n| n.tag_name().name() == "text")
            .count(),
        2
    );
}

/// Строки за рамкой Fixed не теряют внешнюю обводку: рамка маски — узел вместе с
/// глифами.
#[test]
fn outside_stroke_mask_covers_overflowing_lines() {
    let mut p = Page::new();
    let id = text_node(
        &mut p,
        "один\nдва\nтри\nчетыре",
        200.0,
        20.0,
        stroke(2.0, StrokeAlign::Outside),
        |_| {},
    );
    let scene = p.scene();
    let svg = text_svg(&scene, &[id]);
    let doc = Xml::parse(&svg).unwrap();
    let masks = by_tag(&doc, "mask");
    assert_eq!(masks.len(), 1);
    let m = masks[0];
    let num = |a: &str| m.attribute(a).unwrap().parse::<f64>().unwrap();
    let (y0, y1) = (num("y"), num("y") + num("height"));
    let lines = filled_texts(&doc);
    assert_eq!(lines.len(), 4);
    for t in &lines {
        let y = numbers(t, "y")[0];
        // Глиф строки — от верха кегля над базовой линией до нижних выносных.
        assert!(
            y - 20.0 >= y0 && y + 6.0 <= y1,
            "строка {y} вне маски {y0}..{y1}"
        );
    }
}

/// Полупрозрачная обводка нескольких прогонов — одним слоем `<g opacity>` с
/// непрозрачным цветом: перекрытия не темнеют.
#[test]
fn translucent_text_stroke_is_one_layer() {
    let fonts = inter();
    let mut p = Page::new();
    let id = styled_text(&mut p, "Inter");
    p.edit(
        id,
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::rgba(0, 0, 0, 128),
                width: 1.0,
                align: StrokeAlign::Center,
            }]),
            ..Default::default()
        },
    );
    let scene = p.scene();
    let mut r = Renderer::new();
    r.set_fonts(fonts);
    let svg = r
        .export_svg(
            &scene,
            &[id],
            &NoImageBytes,
            SvgOptions {
                outline_text: false,
                ids: false,
            },
            &ExportBudget::unlimited(),
        )
        .unwrap();
    let doc = Xml::parse(&svg).unwrap();
    let stroked: Vec<_> = by_tag(&doc, "text")
        .into_iter()
        .filter(|t| t.attribute("stroke").is_some())
        .collect();
    assert_eq!(stroked.len(), 3);
    for t in &stroked {
        assert_eq!(t.attribute("stroke-opacity"), None);
        assert_eq!(
            t.parent_element().unwrap().attribute("opacity"),
            Some("0.502")
        );
    }
}

/// Все `id` уникальны, а каждая ссылка `url(#…)` ведёт на `clipPath`, `mask` или
/// `pattern`.
#[track_caller]
fn assert_ids_and_refs(doc: &Xml) {
    let mut seen = std::collections::HashSet::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(id) = n.attribute("id") {
            assert!(seen.insert(id.to_string()), "повторный id {id}");
        }
    }
    for n in doc.descendants().filter(|n| n.is_element()) {
        for a in n.attributes() {
            let Some(r) = a.value().strip_prefix("url(#") else {
                continue;
            };
            let id = r.trim_end_matches(')');
            let target = doc
                .descendants()
                .find(|t| t.attribute("id") == Some(id))
                .unwrap_or_else(|| panic!("нет элемента {id}"));
            assert!(
                matches!(target.tag_name().name(), "clipPath" | "mask" | "pattern"),
                "url(#{id}) ведёт на <{}>",
                target.tag_name().name()
            );
        }
    }
}

#[test]
fn generated_ids_never_collide_with_layer_ids() {
    let mut p = Page::new();
    let hash = p.doc.images.add(tiny_png());
    // Слои названы как служебные id (clip0…, mask0…, pattern0…, image0…): часть
    // создаётся до служебных, часть после.
    let mut names = vec![];
    for i in 0..8 {
        for prefix in ["clip", "mask", "pattern", "image"] {
            names.push(format!("{prefix}{i}"));
        }
    }
    let mut tops = vec![];
    for (i, name) in names.into_iter().enumerate() {
        let f = p.add(
            None,
            NodeKind::Frame,
            PropsPatch {
                name: Some(name),
                fills: Some(vec![Fill::image(hash.clone())]),
                strokes: Some(vec![
                    Stroke {
                        color: Color::BLACK,
                        width: 2.0,
                        align: StrokeAlign::Outside,
                    },
                    Stroke {
                        color: Color::BLACK,
                        width: 2.0,
                        align: StrokeAlign::Inside,
                    },
                ]),
                clip_content: Some(true),
                ..rect(i as f64 * 60.0, 0.0, 50.0, 50.0)
            },
        );
        p.add(
            Some(f),
            NodeKind::Rect,
            PropsPatch {
                name: Some(format!("clip{}", i / 2)),
                fills: solid(1, 2, 3, 255),
                ..rect(5.0, 5.0, 60.0, 20.0)
            },
        );
        tops.push(f);
    }
    for ids in [true, false] {
        let svg = export(
            &mut p,
            &tops,
            SvgOptions {
                ids,
                ..SvgOptions::default()
            },
        );
        let doc = Xml::parse(&svg).unwrap();
        assert_ids_and_refs(&doc);
        assert!(!by_tag(&doc, "clipPath").is_empty() && !by_tag(&doc, "mask").is_empty());
    }
}

#[test]
fn layer_ids_are_valid_xml_names() {
    let mut p = Page::new();
    let mut tops = vec![];
    for (i, name) in ["ª²", "1abc", "", "a:b.c"].into_iter().enumerate() {
        tops.push(p.add(
            None,
            NodeKind::Rect,
            PropsPatch {
                name: Some(name.into()),
                fills: solid(1, 2, 3, 255),
                ..rect(i as f64 * 20.0, 0.0, 10.0, 10.0)
            },
        ));
    }
    let svg = export(
        &mut p,
        &tops,
        SvgOptions {
            ids: true,
            ..SvgOptions::default()
        },
    );
    let doc = Xml::parse(&svg).unwrap();
    let ids: Vec<_> = doc
        .descendants()
        .filter_map(|n| n.attribute("id"))
        .collect();
    assert_eq!(ids, ["__", "_1abc", "_", "a_b_c"]);
}

/// Квадрат с диагональю: четыре точки степени 3 — замкнутый контур с кругами в
/// стыках.
fn junction_square(p: &mut Page, align: StrokeAlign) -> NodeId {
    let pts = [(0.0, 0.0), (60.0, 0.0), (60.0, 60.0), (0.0, 60.0)];
    let net = VectorNetwork {
        points: pts.iter().map(|&(x, y)| VectorPoint::new(x, y)).collect(),
        segments: vec![
            VectorSegment::line(0, 1),
            VectorSegment::line(1, 2),
            VectorSegment::line(2, 3),
            VectorSegment::line(3, 0),
            VectorSegment::line(0, 2),
        ],
        regions: vec![],
    };
    p.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(net)),
            strokes: stroke(4.0, align),
            rotation: Some(10.0),
            ..rect(10.0, 10.0, 60.0, 60.0)
        },
    )
}

#[test]
fn junction_dots_share_the_inside_and_outside_clip() {
    for (align, tag) in [
        (StrokeAlign::Inside, "clip-path"),
        (StrokeAlign::Outside, "mask"),
    ] {
        let mut p = Page::new();
        let v = junction_square(&mut p, align);
        let svg = export(&mut p, &[v], SvgOptions::default());
        let doc = Xml::parse(&svg).unwrap();
        assert_ids_and_refs(&doc);
        // Круги — заливка без обводки; каждая такая заливка вне <defs> лежит
        // внутри группы с той же обрезкой или маской, что и обводка кольца.
        let dots: Vec<_> = doc
            .descendants()
            .filter(|n| {
                n.tag_name().name() == "path"
                    && n.attribute("stroke").is_none()
                    && n.attribute("fill").is_some()
                    && !n.ancestors().any(|a| a.tag_name().name() == "defs")
            })
            .collect();
        assert_eq!(dots.len(), 1, "{align:?}: круги стыков");
        let group = dots[0].parent().unwrap();
        assert_eq!(group.tag_name().name(), "g");
        let r = group
            .attribute(tag)
            .unwrap_or_else(|| panic!("{align:?}: нет {tag}"));
        assert!(
            group.attribute("transform").is_some(),
            "матрица узла — на группе"
        );
        let ring = group
            .children()
            .find(|n| n.attribute("stroke-width") == Some("8"))
            .expect("обводка кольца в той же группе");
        assert!(ring.attribute("transform").is_none() && ring.attribute(tag).is_none());
        assert!(r.starts_with("url(#"));
    }
}

#[test]
fn budget_stops_before_encoding_a_large_image() {
    let mut p = Page::new();
    // Заголовок PNG 10×10 и 3 МБ мусора: размер читается, байты не декодируются.
    let mut raw = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    raw.extend_from_slice(&[0, 0, 0, 10, 0, 0, 0, 10]);
    raw.resize(3_000_000, 7);
    let hash = p.doc.images.add(raw);
    let ids: Vec<_> = (0..3)
        .map(|i| {
            p.add(
                None,
                NodeKind::Rect,
                PropsPatch {
                    fills: Some(vec![Fill::image(hash.clone())]),
                    ..rect(i as f64 * 20.0, 0.0, 10.0, 10.0)
                },
            )
        })
        .collect();
    let scene = p.scene();
    let tight = ExportBudget {
        max_bytes: 100_000,
        ..ExportBudget::unlimited()
    };
    let run = |budget: &ExportBudget| {
        Renderer::new().export_svg(&scene, &ids, &p.doc.images, SvgOptions::default(), budget)
    };
    assert_eq!(run(&tight), Err(ExportError::TooBig));
    // Достаточный предел пропускает ту же картинку.
    let ok = ExportBudget {
        max_bytes: 5_000_000,
        ..ExportBudget::unlimited()
    };
    assert!(run(&ok).is_ok());
}

#[test]
fn zero_size_area_is_empty() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(0, 0, 0, 255),
            ..rect(0.0, 0.0, 50.0, 0.0)
        },
    );
    let scene = p.scene();
    assert_eq!(
        Renderer::new().export_svg(
            &scene,
            &[f],
            &NoImageBytes,
            SvgOptions::default(),
            &ExportBudget::unlimited()
        ),
        Err(ExportError::Empty)
    );
}
