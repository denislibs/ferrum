//! Текст в редакторе (подсистема 3, этап 1): размер по раскладке в той же
//! транзакции, ожидание шрифтов, ресайз переводит в `Fixed`, отсутствующие шрифты
//! и замена одним шагом, копирование и дублирование с содержимым.

mod common;

use std::path::PathBuf;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_text::{group, read_faces, Fonts, SharedFonts, SourceKind};
use kurbo::Point;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const FILES: [&str; 4] = [
    "inter/Inter-Variable.ttf",
    "inter/Inter-Italic-Variable.ttf",
    "roboto/Roboto-Variable.ttf",
    "roboto/Roboto-Italic-Variable.ttf",
];

/// Реестр со списком Inter и Roboto; загружены — те, что в `loaded`.
fn fonts(loaded: &[&str]) -> SharedFonts {
    let shared = Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let faces = FILES
            .iter()
            .flat_map(|r| read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(SourceKind::Bundled, group(faces));
        for rel in loaded {
            let family = if rel.starts_with("inter") {
                "Inter"
            } else {
                "Roboto"
            };
            f.load(family, rel, font_file(rel)).unwrap();
        }
        f.take_requests();
    }
    shared
}

fn h_with(fonts: &SharedFonts) -> H {
    let mut h = H::new();
    h.ed.set_fonts(fonts.clone());
    h
}

fn props(size: f64) -> TextProps {
    let mut p = TextProps::default();
    p.style.size = size;
    p
}

fn text_of(h: &mut H, id: NodeId) -> Props {
    h.ed.page_doc().props(id).unwrap()
}

#[test]
fn created_text_gets_its_size_in_the_same_step() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
    let id =
        h.ed.create_text(
            Point::new(10.0, 20.0),
            &TextContent::plain("Привет"),
            &props(24.0),
            0.0,
            0.0,
        )
        .unwrap();
    let p = text_of(&mut h, id);
    assert_eq!((p.x, p.y), (10.0, 20.0));
    assert!(p.w > 60.0 && p.w < 120.0, "{}", p.w);
    assert_eq!(p.h, (24.0f64 * 1.2109375).ceil());
    assert_eq!(h.ed.selection(), &[id]);
    assert_eq!(h.ed.step_count(), 1);
    h.ed.execute(CommandId::Undo);
    assert!(h.ed.page_doc().walk().is_empty());
    // Перенос по ширине рамки: высота по строкам.
    let mut tall = props(24.0);
    tall.sizing = TextSizing::AutoHeight;
    let id =
        h.ed.create_text(
            Point::ZERO,
            &TextContent::plain("Съешь же ещё этих мягких булок"),
            &tall,
            120.0,
            0.0,
        )
        .unwrap();
    let p = text_of(&mut h, id);
    assert_eq!(p.w, 120.0);
    assert!(p.h > 3.0 * 29.0, "{}", p.h);
}

#[test]
fn text_waits_for_fonts_and_changes_nothing_meanwhile() {
    let f = fonts(&[]);
    let mut h = h_with(&f);
    let r = h.ed.create_text(
        Point::ZERO,
        &TextContent::plain("Ferrum"),
        &props(12.0),
        0.0,
        0.0,
    );
    assert!(matches!(r, Err(TextError::Pending)));
    assert!(h.ed.page_doc().walk().is_empty());
    let reqs = f.borrow_mut().take_requests();
    assert_eq!(reqs[0].family, "Inter");
    for file in &reqs[0].files {
        f.borrow_mut().load("Inter", file, font_file(file)).unwrap();
    }
    h.ed.create_text(
        Point::ZERO,
        &TextContent::plain("Ferrum"),
        &props(12.0),
        0.0,
        0.0,
    )
    .unwrap();
}

#[test]
fn resizing_text_makes_it_fixed_in_one_step() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
    let id =
        h.ed.create_text(
            Point::ZERO,
            &TextContent::plain("Ferrum"),
            &props(24.0),
            0.0,
            0.0,
        )
        .unwrap();
    let before = text_of(&mut h, id);
    let steps = h.ed.step_count();
    h.ed.edit_selection(
        &PanelEdit::Number {
            field: NumField::W,
            value: 300.0,
            relative: false,
        },
        Phase::Commit,
    );
    let after = text_of(&mut h, id);
    assert_eq!(after.w, 300.0);
    assert_eq!(after.text_props().unwrap().sizing, TextSizing::Fixed);
    assert_eq!(h.ed.step_count(), steps + 1);
    h.ed.execute(CommandId::Undo);
    assert_eq!(text_of(&mut h, id), before);
}

#[test]
fn missing_fonts_are_listed_and_replaced_in_one_step() {
    let f = fonts(&FILES);
    let mut h = h_with(&f);
    let mut comic = props(24.0);
    comic.style.font = FontName::new("Comic Sans", "Bold");
    let a =
        h.ed.create_text(Point::ZERO, &TextContent::plain("Первый"), &comic, 0.0, 0.0)
            .unwrap();
    let b =
        h.ed.create_text(
            Point::new(0.0, 100.0),
            &TextContent::plain("Второй"),
            &comic,
            0.0,
            0.0,
        )
        .unwrap();
    // Третий: Inter по умолчанию, одно слово — отсутствующим шрифтом.
    let c =
        h.ed.create_text(
            Point::new(0.0, 200.0),
            &TextContent::plain("Три слова"),
            &props(24.0),
            0.0,
            0.0,
        )
        .unwrap();
    h.ed.transact(|tx| {
        tx.format_text(
            c,
            0..3,
            &TextAttrs {
                font: Some("Comic Sans".into()),
                style: Some("Bold".into()),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![])
    })
    .unwrap();
    assert_eq!(
        h.ed.missing_fonts(),
        vec![MissingFont {
            font: FontName::new("Comic Sans", "Bold"),
            nodes: 3,
        }]
    );
    let before: Vec<Props> = [a, b, c].iter().map(|id| text_of(&mut h, *id)).collect();
    let steps = h.ed.step_count();
    let roboto = FontName::new("Roboto", "Bold");
    assert_eq!(
        h.ed.replace_font(&FontName::new("Comic Sans", "Bold"), &roboto)
            .unwrap(),
        3
    );
    assert_eq!(h.ed.step_count(), steps + 1);
    assert!(h.ed.missing_fonts().is_empty());
    let pa = text_of(&mut h, a);
    assert_eq!(pa.text_props().unwrap().style.font, roboto);
    // Ширина — по раскладке Roboto Bold, а не прежняя.
    assert_ne!(pa.w, before[0].w);
    let pc = text_of(&mut h, c);
    let tc = pc.text.unwrap();
    assert_eq!(tc.props.style.font, FontName::inter_regular());
    assert_eq!(tc.content.runs[0].attrs.font_name(&tc.props.style), roboto);
    assert_eq!(
        tc.content.runs[1].attrs.font_name(&tc.props.style),
        FontName::inter_regular()
    );
    h.ed.execute(CommandId::Undo);
    let after_undo: Vec<Props> = [a, b, c].iter().map(|id| text_of(&mut h, *id)).collect();
    assert_eq!(after_undo, before);
}

#[test]
fn replacement_waits_for_the_new_font() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
    let mut comic = props(24.0);
    comic.style.font = FontName::new("Comic Sans", "Regular");
    h.ed.create_text(Point::ZERO, &TextContent::plain("Текст"), &comic, 0.0, 0.0)
        .unwrap();
    let r = h.ed.replace_font(
        &FontName::new("Comic Sans", "Regular"),
        &FontName::new("Roboto", "Regular"),
    );
    assert!(matches!(r, Err(TextError::Pending)));
    assert_eq!(h.ed.missing_fonts().len(), 1);
}

#[test]
fn copy_paste_and_duplicate_keep_text_and_marks() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
    let id =
        h.ed.create_text(
            Point::new(5.0, 5.0),
            &TextContent::plain("Копия"),
            &props(20.0),
            0.0,
            0.0,
        )
        .unwrap();
    h.ed.transact(|tx| {
        tx.format_text(
            id,
            0..2,
            &TextAttrs {
                decoration: Some(Decoration::Underline),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![id])
    })
    .unwrap();
    let original = text_of(&mut h, id).text.unwrap();
    h.ed.execute(CommandId::Copy);
    let bytes =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::Clipboard(b) => Some(b),
                _ => None,
            })
            .expect("копия в буфер");
    h.ed.paste(&bytes).unwrap();
    let pasted = h.ed.selection()[0];
    assert_ne!(pasted, id);
    assert_eq!(text_of(&mut h, pasted).text.unwrap(), original);
    h.ed.select(&[id]);
    h.ed.execute(CommandId::Duplicate);
    let dup = h.ed.selection()[0];
    assert_ne!(dup, id);
    assert_eq!(text_of(&mut h, dup).text.unwrap(), original);
}

#[test]
fn over_limit_text_is_refused_with_an_error_and_no_step() {
    let f = fonts(&FILES[..2]);
    let mut h = h_with(&f);
    let steps = h.ed.step_count();
    let r = h.ed.create_text(
        Point::ZERO,
        &TextContent::plain(&"а".repeat(100_001)),
        &props(12.0),
        0.0,
        0.0,
    );
    let Err(TextError::Doc(e)) = r else {
        panic!("ожидалась ошибка документа");
    };
    assert!(e.to_string().contains("100 000"), "{e}");
    assert!(h.ed.page_doc().walk().is_empty());
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn replace_font_in_read_only_document_is_an_error() {
    let f = fonts(&FILES);
    let mut h = h_with(&f);
    let mut comic = props(24.0);
    comic.style.font = FontName::new("Comic Sans", "Regular");
    h.ed.create_text(Point::ZERO, &TextContent::plain("Текст"), &comic, 0.0, 0.0)
        .unwrap();
    let bytes = ferrum_doc::container::encode(h.ed.document()).unwrap();
    let future = ferrum_doc::container::testing::with_schema_version(&bytes, SCHEMA_VERSION + 1);
    let doc = ferrum_doc::container::decode(&future).unwrap().document;
    let mut ed = Editor::new(
        doc,
        None,
        kurbo::Size::new(800.0, 600.0),
        1.0,
        Platform::Windows,
    )
    .unwrap();
    ed.set_fonts(f.clone());
    let steps = ed.step_count();
    let r = ed.replace_font(
        &FontName::new("Comic Sans", "Regular"),
        &FontName::new("Roboto", "Regular"),
    );
    assert!(
        matches!(r, Err(TextError::Doc(DocError::ReadOnly))),
        "{r:?}"
    );
    assert_eq!(ed.step_count(), steps);
    assert_eq!(ed.missing_fonts().len(), 1);
}

#[test]
fn replace_font_without_changes_makes_no_step() {
    let f = fonts(&FILES);
    let mut h = h_with(&f);
    let mut comic = props(24.0);
    comic.style.font = FontName::new("Comic Sans", "Regular");
    h.ed.create_text(Point::ZERO, &TextContent::plain("Текст"), &comic, 0.0, 0.0)
        .unwrap();
    let steps = h.ed.step_count();
    let comic = FontName::new("Comic Sans", "Regular");
    // Сама на себя и шрифт, которого на странице нет: ничего не меняется.
    assert_eq!(h.ed.replace_font(&comic, &comic).unwrap(), 0);
    assert_eq!(
        h.ed.replace_font(
            &FontName::new("Roboto", "Bold"),
            &FontName::new("Roboto", "Regular")
        )
        .unwrap(),
        0
    );
    assert_eq!(h.ed.step_count(), steps);
}
