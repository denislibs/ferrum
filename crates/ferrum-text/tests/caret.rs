//! Курсор по раскладке на вшитом Inter (подсистема 3, этап 2): позиции между
//! кластерами, попадание по точке, перенос строк и абзацы, выделение, регистр,
//! выравнивание и защита от NaN из файла.

use std::path::PathBuf;

use ferrum_doc::*;
use ferrum_text::*;
use kurbo::Point;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Реестр с загруженным вшитым Inter.
fn fonts() -> Fonts {
    let mut fonts = Fonts::new();
    let rel = "inter/Inter-Variable.ttf";
    fonts.add_catalog(SourceKind::Bundled, group(read_faces(&font_file(rel), rel)));
    fonts.load("Inter", rel, font_file(rel)).unwrap();
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

fn lay(fonts: &mut Fonts, t: &TextData, w: f64) -> TextLayout {
    let fills = vec![Fill::solid(Color::BLACK)];
    let l = fonts.layout(&LayoutRequest::for_node(t, &fills, w, 0.0));
    assert!(!l.loading);
    l
}

#[test]
fn one_line_has_a_stop_per_cluster_from_zero_to_width() {
    let mut fonts = fonts();
    let l = lay(&mut fonts, &text("Ferrum", |_| {}), 0.0);
    assert_eq!(l.carets.len(), 1);
    let c = &l.carets[0];
    assert_eq!((c.start, c.end, c.hard), (0, 6, true));
    let idx: Vec<usize> = c.stops.iter().map(|s| s.0).collect();
    assert_eq!(idx, vec![0, 1, 2, 3, 4, 5, 6]);
    assert!(c.stops.windows(2).all(|w| w[1].1 > w[0].1), "{:?}", c.stops);
    assert!(c.stops[0].1.abs() < 1e-6);
    assert!(
        (c.stops[6].1 - l.width).abs() < 0.5,
        "{:?} {}",
        c.stops,
        l.width
    );
    assert!(c.top < c.baseline && c.baseline < c.bottom);
}

#[test]
fn hit_picks_the_nearest_boundary_and_clamps_outside() {
    let mut fonts = fonts();
    let l = lay(&mut fonts, &text("Ferrum", |_| {}), 0.0);
    let c = &l.carets[0];
    let mid = |i: usize| (c.stops[i].1 + c.stops[i + 1].1) / 2.0;
    let y = c.baseline;
    assert_eq!(l.hit(Point::new(mid(2) - 0.5, y)).index, 2);
    assert_eq!(l.hit(Point::new(mid(2) + 0.5, y)).index, 3);
    assert_eq!(l.hit(Point::new(-50.0, -50.0)).index, 0);
    assert_eq!(l.hit(Point::new(1e4, 1e4)).index, 6);
    let (x, top, bottom) = l.caret_rect(Caret::at(3)).unwrap();
    assert_eq!((x, top, bottom), (c.stops[3].1, c.top, c.bottom));
}

#[test]
fn paragraphs_are_lines_and_stops_cross_the_newline() {
    let mut fonts = fonts();
    let l = lay(&mut fonts, &text("ab\ncd", |_| {}), 0.0);
    assert_eq!(l.carets.len(), 2);
    assert_eq!((l.carets[0].start, l.carets[0].end), (0, 2));
    assert_eq!((l.carets[1].start, l.carets[1].end), (3, 5));
    assert!(l.carets[1].top >= l.carets[0].bottom - 1e-6);
    // Ход по кластерам: из конца первого абзаца — в начало второго.
    assert_eq!(l.next_stop(2), 3);
    assert_eq!(l.prev_stop(3), 2);
    assert_eq!(l.next_stop(5), 5);
    assert_eq!(l.prev_stop(0), 0);
    // Выделение через перевод строки — полоска за концом первой строки.
    let r = l.selection_rects(1..4);
    assert_eq!(r.len(), 2);
    assert!(r[0].x1 > l.carets[0].stops.last().unwrap().1);
    assert_eq!(r[1].x0, l.carets[1].stops[0].1);
    // Вверх с первой строки и вниз с последней — нет строки.
    assert!(l.vertical(Caret::at(1), 10.0, -1).is_none());
    assert_eq!(l.vertical(Caret::at(1), 0.0, 1).unwrap().index, 3);
}

#[test]
fn soft_wrap_position_belongs_to_the_next_line_unless_upstream() {
    let mut fonts = fonts();
    let t = text("один два три", |p| {
        p.sizing = TextSizing::AutoHeight
    });
    // Рамка уже двух слов: каждое слово — своя строка.
    let l = lay(&mut fonts, &t, 70.0);
    assert!(l.carets.len() >= 2, "{:?}", l.carets);
    let (first, second) = (&l.carets[0], &l.carets[1]);
    assert_eq!(first.end, second.start);
    assert!(!first.hard);
    let wrap = first.end;
    assert_eq!(l.caret_line(Caret::at(wrap)), Some(1));
    assert_eq!(
        l.caret_line(Caret {
            index: wrap,
            upstream: true
        }),
        Some(0)
    );
    let (home, end) = l.line_bounds(Caret::at(1)).unwrap();
    assert_eq!(home.index, 0);
    assert_eq!((end.index, end.upstream), (wrap, true));
    // Щелчок за концом первой строки — её конец, курсор остаётся на ней.
    let hit = l.hit(Point::new(1e3, first.baseline));
    assert_eq!((hit.index, hit.upstream), (wrap, true));
}

#[test]
fn empty_paragraph_has_one_stop() {
    let mut fonts = fonts();
    let l = lay(&mut fonts, &text("а\n\nб", |_| {}), 0.0);
    assert_eq!(l.carets.len(), 3);
    let e = &l.carets[1];
    assert_eq!((e.start, e.end, e.stops.len()), (2, 2, 1));
    assert!(e.bottom - e.top > 10.0);
    // Пустой текст — одна строка с одной позицией.
    let empty = lay(&mut fonts, &text("", |_| {}), 0.0);
    assert_eq!(empty.carets.len(), 1);
    assert_eq!(empty.carets[0].stops, vec![(0, 0.0)]);
}

#[test]
fn case_transform_keeps_positions_of_document_chars() {
    let mut fonts = fonts();
    // «ß» в верхнем регистре — «SS»: два глифа, одна позиция содержимого.
    let l = lay(
        &mut fonts,
        &text("aßb", |p| p.style.case = TextCase::Upper),
        0.0,
    );
    let idx: Vec<usize> = l.carets[0].stops.iter().map(|s| s.0).collect();
    assert_eq!(idx, vec![0, 1, 2, 3]);
}

#[test]
fn centered_auto_width_paragraphs_shift_carets_with_lines() {
    let mut fonts = fonts();
    let l = lay(
        &mut fonts,
        &text("a\nширокая строка", |p| {
            p.align_h = AlignH::Center
        }),
        0.0,
    );
    let short = &l.carets[0];
    assert!((short.stops[0].1 - l.lines[0].rect.x0).abs() < 0.5);
    assert!(short.stops[0].1 > 10.0);
}

#[test]
fn nan_from_a_file_does_not_reach_the_layout() {
    let mut fonts = fonts();
    let t = text("Ferrum", |p| {
        p.style.size = f64::NAN;
        p.paragraph_indent = f64::NAN;
        p.style.letter_spacing = LetterSpacing::Px(f64::INFINITY);
    });
    let l = lay(&mut fonts, &t, 0.0);
    assert!(l.width.is_finite() && l.width > 0.0 && l.height.is_finite());
    assert!(l
        .carets
        .iter()
        .flat_map(|c| c.stops.iter())
        .all(|s| s.1.is_finite()));
}

#[test]
fn non_finite_spacing_and_line_height_keep_lines_apart() {
    let mut fonts = fonts();
    let t = text("Fer\nrum", |p| {
        p.paragraph_spacing = f64::INFINITY;
        p.style.line_height = LineHeight::Px(f64::NAN);
    });
    let l = lay(&mut fonts, &t, 0.0);
    assert!(l.height.is_finite() && l.height > 0.0);
    assert_eq!(l.lines.len(), 2);
    // Интерлиньяж «Авто», а не нулевой: строки не наезжают друг на друга.
    assert!(l.lines[1].rect.y0 >= l.lines[0].rect.y1 - 0.01);
    assert!(l.lines[0].rect.height() > 1.0);
}
