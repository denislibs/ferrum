//! Раскладка текста на вшитых шрифтах (файлы — `apps/web/public/fonts`):
//! вариативные начертания, перенос строк, абзацы и списки, украшения,
//! отсутствующий шрифт и загрузка, обрезка с многоточием, вытеснение.

use std::path::PathBuf;

use ferrum_doc::*;
use ferrum_text::*;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Реестр со списком вшитых Inter и Roboto; Inter загружен, Roboto — нет.
fn fonts() -> Fonts {
    let mut fonts = Fonts::new();
    let mut faces = Vec::new();
    for rel in [
        "inter/Inter-Variable.ttf",
        "inter/Inter-Italic-Variable.ttf",
        "roboto/Roboto-Variable.ttf",
        "roboto/Roboto-Italic-Variable.ttf",
    ] {
        faces.extend(read_faces(&font_file(rel), rel));
    }
    fonts.add_catalog(SourceKind::Bundled, group(faces));
    for rel in [
        "inter/Inter-Variable.ttf",
        "inter/Inter-Italic-Variable.ttf",
    ] {
        fonts.load("Inter", rel, font_file(rel)).unwrap();
    }
    fonts.take_requests();
    fonts
}

fn text(s: &str, f: impl FnOnce(&mut TextProps)) -> TextData {
    let mut props = TextProps::default();
    props.style.size = 24.0;
    f(&mut props);
    TextData {
        props,
        content: TextContent::plain(s),
    }
}

fn black() -> Vec<Fill> {
    vec![Fill::solid(Color::BLACK)]
}

fn lay(fonts: &mut Fonts, t: &TextData, w: f64, h: f64) -> TextLayout {
    fonts.layout(&LayoutRequest::for_node(t, &black(), w, h))
}

#[test]
fn variable_weight_comes_from_the_style_name() {
    let mut fonts = fonts();
    let regular = lay(&mut fonts, &text("Ferrum", |_| {}), 0.0, 0.0);
    let bold = lay(
        &mut fonts,
        &text("Ferrum", |p| {
            p.style.font = FontName::new("Inter", "Semi Bold")
        }),
        0.0,
        0.0,
    );
    assert!(!regular.loading && !bold.loading);
    assert!(regular.missing.is_empty() && bold.missing.is_empty());
    assert!(
        bold.width > regular.width,
        "{} {}",
        bold.width,
        regular.width
    );
    // Вес вариативного шрифта — координатой оси wght, а не синтетическим жирным.
    assert!(
        regular.runs[0].coords.iter().all(|c| *c == 0)
            || regular.runs[0].coords != bold.runs[0].coords
    );
    assert_ne!(bold.runs[0].coords, regular.runs[0].coords);
    let g = bold.runs[0].glyphs[0];
    let path = fonts.outline(&bold.runs[0].font, g.id, &bold.runs[0].coords);
    assert!(!path.elements().is_empty());
}

#[test]
fn auto_width_keeps_one_line_per_paragraph() {
    let mut fonts = fonts();
    let one = lay(&mut fonts, &text("Привет, мир", |_| {}), 0.0, 0.0);
    assert_eq!(one.lines.len(), 1);
    assert!(one.width > 100.0 && one.width < 200.0, "{}", one.width);
    // «Авто» — по метрикам Inter: около 1.21 кегля.
    assert!((one.height - 24.0 * 1.21).abs() < 1.0, "{}", one.height);
    let two = lay(
        &mut fonts,
        &text("Привет\nмир", |p| p.paragraph_spacing = 10.0),
        0.0,
        0.0,
    );
    assert_eq!(two.lines.len(), 2);
    assert!(
        (two.height - (2.0 * one.height + 10.0)).abs() < 0.5,
        "{}",
        two.height
    );
    assert!(two.width < one.width);
    // Пустой абзац — строка своей высоты.
    let blank = lay(&mut fonts, &text("а\n\nб", |_| {}), 0.0, 0.0);
    assert_eq!(blank.lines.len(), 3);
}

#[test]
fn auto_height_wraps_at_the_box_width() {
    let mut fonts = fonts();
    let t = text(
        "Съешь же ещё этих мягких французских булок",
        |p| p.sizing = TextSizing::AutoHeight,
    );
    let l = lay(&mut fonts, &t, 150.0, 0.0);
    assert!(l.lines.len() >= 3, "{}", l.lines.len());
    for line in &l.lines {
        assert!(line.rect.x1 <= 150.0 + 0.5, "{:?}", line.rect);
    }
    // По центру: строки посередине рамки.
    let c = text("ab", |p| {
        p.sizing = TextSizing::AutoHeight;
        p.align_h = AlignH::Center;
    });
    let l = lay(&mut fonts, &c, 200.0, 0.0);
    let r = l.lines[0].rect;
    assert!(((r.x0 + r.x1) / 2.0 - 100.0).abs() < 1.0, "{r:?}");
}

#[test]
fn styles_colors_and_decorations_by_range() {
    let mut fonts = fonts();
    let mut t = text("Красный и подчёркнутый", |_| {});
    let red = Fill::solid(Color::rgba(255, 0, 0, 255));
    t.content = TextContent {
        runs: vec![
            TextRun {
                text: "Красный".into(),
                attrs: TextAttrs {
                    fill: Some(red),
                    size: Some(40.0),
                    ..Default::default()
                },
            },
            TextRun {
                text: " и ".into(),
                attrs: TextAttrs::default(),
            },
            TextRun {
                text: "подчёркнутый\n".into(),
                attrs: TextAttrs {
                    decoration: Some(Decoration::Underline),
                    ..Default::default()
                },
            },
        ],
    };
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert_eq!(l.runs[0].color, Color::rgba(255, 0, 0, 255));
    assert_eq!(l.runs[0].size, 40.0);
    assert_eq!(l.runs.last().unwrap().color, Color::BLACK);
    assert_eq!(l.decorations.len(), 1);
    let d = l.decorations[0].rect;
    // Под базовой линией, по ширине подчёркнутого участка.
    assert!(
        d.y0 > l.lines[0].baseline && d.y0 < l.lines[0].baseline + 6.0,
        "{d:?}"
    );
    assert!(d.width() > 100.0);
}

#[test]
fn lists_get_markers_and_numbers() {
    let mut fonts = fonts();
    let mut t = text("", |_| {});
    let item = |s: &str, list: ListKind| {
        [
            TextRun {
                text: s.into(),
                attrs: TextAttrs::default(),
            },
            TextRun {
                text: "\n".into(),
                attrs: TextAttrs {
                    list: Some(list),
                    ..Default::default()
                },
            },
        ]
    };
    t.content = TextContent {
        runs: [
            item("один", ListKind::Ordered),
            item("два", ListKind::Ordered),
            item("пункт", ListKind::Bullet),
        ]
        .concat(),
    };
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert_eq!(l.lines.len(), 3);
    // Текст сдвинут на ступень списка, маркеры — левее текста на тех же строках.
    for line in &l.lines {
        assert!(
            (line.rect.x0 - LIST_INDENT_EM * 24.0).abs() < 0.5,
            "{:?}",
            line.rect
        );
    }
    let left_of_text = l
        .runs
        .iter()
        .flat_map(|r| r.glyphs.iter())
        .filter(|g| (g.x as f64) < LIST_INDENT_EM * 24.0 - 0.5)
        .count();
    // «1.», «2.» — по два глифа, «•» — один.
    assert_eq!(left_of_text, 5);
}

#[test]
fn missing_font_falls_back_to_inter_of_the_same_style() {
    let mut fonts = fonts();
    let t = text("Ferrum", |p| {
        p.style.font = FontName::new("Comic Sans", "Bold")
    });
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(!l.loading);
    assert_eq!(l.missing, vec![FontName::new("Comic Sans", "Bold")]);
    let inter_bold = lay(
        &mut fonts,
        &text("Ferrum", |p| p.style.font = FontName::new("Inter", "Bold")),
        0.0,
        0.0,
    );
    assert_eq!(l.width, inter_bold.width);
    assert_eq!(
        fonts.status(&FontName::new("Comic Sans", "Bold")),
        FontStatus::Missing
    );
    // Начертания нет и у Inter — Inter Regular.
    let odd = lay(
        &mut fonts,
        &text("Ferrum", |p| {
            p.style.font = FontName::new("Roboto", "Wide Hairline")
        }),
        0.0,
        0.0,
    );
    assert_eq!(odd.missing.len(), 1);
    assert_eq!(
        odd.width,
        lay(&mut fonts, &text("Ferrum", |_| {}), 0.0, 0.0).width
    );
}

#[test]
fn unloaded_font_is_requested_once_and_relaid_after_load() {
    let mut fonts = fonts();
    let t = text("Ferrum", |p| p.style.font = FontName::new("Roboto", "Bold"));
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(l.loading && l.missing.is_empty());
    let reqs = fonts.take_requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].family, "Roboto");
    assert_eq!(reqs[0].source, SourceKind::Bundled);
    assert_eq!(reqs[0].files.len(), 2);
    lay(&mut fonts, &t, 0.0, 0.0);
    assert!(fonts.take_requests().is_empty(), "повторного запроса нет");
    let gen = fonts.generation();
    for f in &reqs[0].files {
        fonts.load("Roboto", f, font_file(f)).unwrap();
    }
    assert!(fonts.generation() > gen);
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(!l.loading && l.missing.is_empty());
    assert_eq!(
        fonts.status(&FontName::new("Roboto", "Bold")),
        FontStatus::Ready
    );
    // Неразборчивый файл — ошибка и «отсутствует».
    let mut fresh = self::fonts();
    lay(&mut fresh, &t, 0.0, 0.0);
    let err = fresh.load("Roboto", "roboto/Roboto-Variable.ttf", b"garbage".to_vec());
    assert!(err.unwrap_err().contains("не читается"));
    assert_eq!(
        fresh.status(&FontName::new("Roboto", "Bold")),
        FontStatus::Missing
    );
}

#[test]
fn before_the_bundled_list_nothing_is_missing() {
    let mut fonts = Fonts::new();
    let t = text("Ferrum", |_| {});
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(l.loading);
    assert!(l.missing.is_empty());
    assert_eq!(
        fonts.status(&FontName::inter_regular()),
        FontStatus::Pending
    );
    // Вшитый Onest готов без загрузки.
    assert_eq!(
        fonts.status(&FontName::new("Onest", "Regular")),
        FontStatus::Ready
    );
}

#[test]
fn fixed_box_truncates_with_an_ellipsis() {
    let mut fonts = fonts();
    let t = text(
        "Съешь же ещё этих мягких французских булок да выпей чаю",
        |p| {
            p.sizing = TextSizing::Fixed;
            p.truncate = true;
        },
    );
    let full = lay(
        &mut fonts,
        &TextData {
            props: TextProps {
                truncate: false,
                ..t.props.clone()
            },
            ..t.clone()
        },
        160.0,
        70.0,
    );
    assert!(full.lines.len() > 2 && !full.truncated);
    let l = lay(&mut fonts, &t, 160.0, 70.0);
    assert!(l.truncated);
    assert_eq!(l.lines.len(), 2);
    assert!(l.height <= 70.0);
    let last = l.lines[1];
    assert!(last.rect.x1 <= 160.0 + 0.5, "{:?}", last.rect);
    // Последний глиф строки — многоточие.
    let run = l.runs.last().unwrap();
    assert_eq!(run.glyphs.len(), 1);
    assert!((run.glyphs[0].y as f64 - last.baseline).abs() < 0.01);
    // Не больше `max_lines` строк.
    let one = TextData {
        props: TextProps {
            sizing: TextSizing::AutoHeight,
            max_lines: Some(1),
            ..t.props.clone()
        },
        ..t.clone()
    };
    assert_eq!(lay(&mut fonts, &one, 160.0, 0.0).lines.len(), 1);
}

#[test]
fn vertical_alignment_inside_a_fixed_box() {
    let mut fonts = fonts();
    let t = text("Ferrum", |p| {
        p.sizing = TextSizing::Fixed;
        p.align_v = AlignV::Bottom;
    });
    let l = lay(&mut fonts, &t, 200.0, 100.0);
    assert!(
        (l.lines[0].rect.y1 - 100.0).abs() < 0.5,
        "{:?}",
        l.lines[0].rect
    );
}

#[test]
fn eviction_unloads_fonts_not_on_the_page() {
    let mut fonts = fonts();
    fonts.set_memory_limit_for_tests(2_000_000);
    fonts.set_in_use(["Inter".to_string()]);
    let t = text("Ferrum", |p| {
        p.style.font = FontName::new("Roboto", "Regular")
    });
    lay(&mut fonts, &t, 0.0, 0.0);
    for f in fonts.take_requests()[0].files.clone() {
        fonts.load("Roboto", &f, font_file(&f)).unwrap();
    }
    // Inter (1.8 МБ) на странице, Roboto (1 МБ) — нет: вытеснен Roboto.
    assert!(fonts.memory() <= 2_000_000, "{}", fonts.memory());
    assert_eq!(fonts.status(&FontName::inter_regular()), FontStatus::Ready);
    assert_eq!(
        fonts.status(&FontName::new("Roboto", "Regular")),
        FontStatus::Loading
    );
    let l = lay(&mut fonts, &t, 0.0, 0.0);
    assert!(l.loading);
    assert_eq!(
        fonts.take_requests().len(),
        1,
        "вытесненный запрашивается снова"
    );
}

#[test]
fn auto_width_aligns_each_paragraph_inside_the_widest_line() {
    let mut fonts = fonts();
    for (align, k) in [(AlignH::Center, 0.5), (AlignH::Right, 1.0)] {
        let t = text("Длинная первая строка\nмир", |p| {
            p.align_h = align
        });
        let l = lay(&mut fonts, &t, 0.0, 0.0);
        assert_eq!(l.lines.len(), 2);
        let (a, b) = (l.lines[0].rect, l.lines[1].rect);
        let want = (a.width() - b.width()) * k;
        assert!(a.x0.abs() < 0.5, "{a:?}");
        assert!((b.x0 - want).abs() < 0.5, "{align:?}: {b:?} want x0 {want}");
        // Глифы сдвинуты вместе со строкой.
        let first_x = l.runs.last().unwrap().glyphs[0].x as f64;
        assert!((first_x - b.x0).abs() < 2.0, "{first_x} {b:?}");
    }
}

#[test]
fn replacing_a_system_family_by_a_bundled_one_frees_its_fonts() {
    let mut fonts = Fonts::new();
    let base = fonts.memory();
    let rels = [
        "roboto/Roboto-Variable.ttf",
        "roboto/Roboto-Italic-Variable.ttf",
    ];
    let faces: Vec<_> = rels
        .iter()
        .flat_map(|r| read_faces(&font_file(r), r))
        .collect();
    fonts.add_catalog(SourceKind::System, group(faces.clone()));
    for r in rels {
        fonts.load("Roboto", r, font_file(r)).unwrap();
    }
    assert!(fonts.memory() > base);
    assert_eq!(
        fonts.status(&FontName::new("Roboto", "Regular")),
        FontStatus::Ready
    );
    fonts.add_catalog(SourceKind::Bundled, group(faces));
    assert_eq!(fonts.memory(), base);
    assert_eq!(
        fonts.status(&FontName::new("Roboto", "Regular")),
        FontStatus::Loading
    );
}

#[test]
fn oversized_file_over_a_loaded_slot_frees_it() {
    let mut fonts = fonts();
    let base = fonts.memory();
    let big = vec![0u8; MAX_FONT_FILE + 1];
    let err = fonts
        .load("Inter", "inter/Inter-Variable.ttf", big)
        .unwrap_err();
    assert!(err.contains("64"), "{err}");
    assert!(fonts.memory() < base);
    assert_eq!(
        fonts.status(&FontName::inter_regular()),
        FontStatus::Missing
    );
}

#[test]
fn ellipsis_follows_the_real_advance_of_the_last_glyph() {
    let mut fonts = fonts();
    let t = text("ШШШШШШШШШШШШШШШШШШШШ", |p| {
        p.sizing = TextSizing::Fixed;
        p.truncate = true;
    });
    let l = lay(&mut fonts, &t, 160.0, 30.0);
    assert!(l.truncated);
    let ell = l.runs.last().unwrap().glyphs[0];
    let line_glyphs: Vec<_> = l.runs[..l.runs.len() - 1]
        .iter()
        .flat_map(|r| r.glyphs.iter())
        .filter(|g| (g.y - ell.y).abs() < 0.5)
        .collect();
    let adv = line_glyphs[1].x - line_glyphs[0].x;
    let last = line_glyphs
        .iter()
        .max_by(|a, b| a.x.total_cmp(&b.x))
        .unwrap();
    assert!(
        (ell.x - (last.x + adv)).abs() < 0.01,
        "{} {} {}",
        ell.x,
        last.x,
        adv
    );
}

#[test]
fn ellipsis_does_not_follow_a_trailing_space() {
    let mut fonts = fonts();
    let t = text(
        "Съешь же ещё этих мягких французских булок да выпей чаю",
        |p| {
            p.sizing = TextSizing::Fixed;
            p.truncate = true;
        },
    );
    let l = lay(&mut fonts, &t, 160.0, 70.0);
    assert!(l.truncated);
    let ell = l.runs.last().unwrap().glyphs[0];
    let run_font = &l.runs[0];
    let mut best: Option<(f32, u32)> = None;
    for r in &l.runs[..l.runs.len() - 1] {
        for g in r.glyphs.iter().filter(|g| (g.y - ell.y).abs() < 0.5) {
            if best.is_none_or(|(x, _)| g.x > x) {
                best = Some((g.x, g.id));
            }
        }
    }
    let (_, id) = best.unwrap();
    assert!(
        !fonts
            .outline(&run_font.font, id, &run_font.coords)
            .elements()
            .is_empty(),
        "последний глиф перед многоточием — пробел"
    );
}
