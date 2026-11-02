//! «Копировать свойства» и «Вставить свойства» (спека подсистемы 2, раздел 6):
//! внутренний буфер и текст с меткой для системного, к каждому узлу — то, что
//! подходит его типу, одним шагом; копия узлов тоже несёт свойства (первого
//! узла); стиль текста; не доверенные данные.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::props_clip::{props_text_bytes, NO_PROPS, PROPS_MARKER};
use ferrum_editor::*;

fn styled(h: &mut H) -> NodeId {
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.set(
        r,
        PropsPatch {
            opacity: Some(0.5),
            corner_radius: Some(12.0),
            fills: Some(vec![Fill::solid(Color::rgba(200, 10, 20, 255))]),
            strokes: Some(vec![Stroke {
                color: Color::rgba(0, 0, 255, 255),
                width: 3.0,
                align: StrokeAlign::Outside,
            }]),
            stroke_join: Some(StrokeJoin::Round),
            ..Default::default()
        },
    );
    r
}

/// «Копировать свойства» выделения `ids`: текст для системного буфера.
fn copy_props(h: &mut H, ids: &[NodeId]) -> String {
    h.ed.select(ids);
    h.ed.take_events();
    h.ed.execute(CommandId::CopyProps);
    h.ed.take_events()
        .into_iter()
        .find_map(|e| match e {
            EditorEvent::ClipboardProps(t) => Some(t),
            _ => None,
        })
        .expect("текст свойств")
}

fn errors(h: &mut H) -> Vec<String> {
    h.ed.take_events()
        .into_iter()
        .filter_map(|e| match e {
            EditorEvent::Error(m) => Some(m),
            _ => None,
        })
        .collect()
}

#[test]
fn paste_applies_what_fits_each_type_in_one_step() {
    let mut h = H::new();
    let src = styled(&mut h);
    let text = copy_props(&mut h, &[src]);
    assert!(text.starts_with(PROPS_MARKER));
    let e = h.add(None, NodeKind::Ellipse, patch(300.0, 100.0, 40.0, 40.0));
    let line = h.add(None, NodeKind::Line, patch(300.0, 300.0, 100.0, 0.0));
    let f = h.frame(None, 500.0, 100.0, 100.0, 100.0);
    h.ed.select(&[e, line, f]);
    let steps = h.ed.step_count();
    let before = h.state();
    h.ed.paste_props(None);
    let s = h.props(src);
    let pe = h.props(e);
    assert_eq!(
        (pe.fills.clone(), pe.strokes.clone(), pe.opacity),
        (s.fills.clone(), s.strokes.clone(), 0.5)
    );
    assert_eq!(pe.stroke_style.join, StrokeJoin::Round);
    assert_eq!(pe.corner_radius, 0.0, "у эллипса скругления нет");
    assert_eq!((pe.x, pe.w), (300.0, 40.0), "геометрия не меняется");
    let pl = h.props(line);
    assert!(pl.fills.is_empty(), "у линии нет заливок");
    assert_eq!(pl.strokes, s.strokes);
    let pf = h.props(f);
    assert_eq!(
        (pf.corner_radius, pf.fills.clone()),
        (12.0, s.fills.clone())
    );
    assert_eq!(h.sel(), [e, line, f], "выделение то же");
    h.assert_one_step(steps, &before);
}

#[test]
fn system_clipboard_text_carries_props_to_another_document() {
    let mut h = H::new();
    let src = styled(&mut h);
    let text = copy_props(&mut h, &[src]);
    let bytes = props_text_bytes(&text).expect("метка и base64");
    let mut other = H::new();
    let r = other.rect(None, 0.0, 0.0, 10.0, 10.0);
    other.ed.select(&[r]);
    other.ed.paste_props(Some(&bytes));
    let p = other.props(r);
    assert_eq!((p.opacity, p.corner_radius), (0.5, 12.0));
    assert_eq!(p.fills, h.props(src).fills);
    // Чужой текст — не наши данные: `None`, вставка возьмёт внутренний буфер.
    assert_eq!(props_text_bytes("просто текст"), None);
    assert_eq!(props_text_bytes(&format!("{PROPS_MARKER}не base64!")), None);
}

#[test]
fn node_copy_gives_the_props_of_its_first_node() {
    let mut h = H::new();
    let src = styled(&mut h);
    h.ed.select(&[src]);
    h.ed.take_events();
    h.ed.execute(CommandId::Copy);
    let bytes =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::Clipboard(b) => Some(b),
                _ => None,
            })
            .unwrap();
    let r = h.rect(None, 400.0, 400.0, 30.0, 30.0);
    h.ed.select(&[r]);
    h.ed.paste_props(Some(&bytes));
    assert_eq!(h.props(r).corner_radius, 12.0);
    assert_eq!((h.props(r).x, h.props(r).w), (400.0, 30.0));
}

#[test]
fn nothing_copied_or_hostile_data_is_an_error_without_a_step() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[r]);
    let steps = h.ed.step_count();
    h.ed.take_events();
    h.ed.paste_props(None);
    assert_eq!(errors(&mut h), [NO_PROPS]);
    h.ed.paste_props(Some(b"FERRUMCB\x01\x00\x00\x00\xff\xff\xff\xff"));
    assert_eq!(errors(&mut h).len(), 1);
    h.ed.paste_props(Some("не данные".as_bytes()));
    assert_eq!(errors(&mut h).len(), 1);
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn text_style_goes_to_texts_with_fills_in_one_step() {
    let mut h = text_h();
    let a = add_text(&mut h, 100.0, 100.0, "Образец", 30.0);
    h.set(
        a,
        PropsPatch {
            fills: Some(vec![Fill::solid(Color::rgba(0, 128, 0, 255))]),
            ..Default::default()
        },
    );
    let b = add_text(&mut h, 100.0, 300.0, "Цель", 12.0);
    let r = h.rect(None, 400.0, 400.0, 30.0, 30.0);
    copy_props(&mut h, &[a]);
    h.ed.select(&[b, r]);
    let steps = h.ed.step_count();
    let before = h.state();
    h.ed.paste_props(None);
    assert!(!h.ed.take_text_pending());
    let pb = h.props(b);
    let t = pb.text.as_ref().unwrap();
    assert_eq!(t.props.style.size, 30.0);
    assert_eq!(pb.fills, h.props(a).fills);
    assert!(pb.w > 50.0, "рамка подогнана под новый кегль: {}", pb.w);
    assert_eq!(text_of(&mut h, b), "Цель\n", "содержимое не меняется");
    assert_eq!(
        h.props(r).fills,
        h.props(a).fills,
        "у фигуры — заливка, стиля текста нет"
    );
    h.assert_one_step(steps, &before);
}

#[test]
fn copy_and_paste_props_follow_state() {
    let mut h = H::new();
    assert!(!h.ed.command_state(CommandId::CopyProps).enabled);
    assert!(!h.ed.command_state(CommandId::PasteProps).enabled);
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    h.ed.select(&[r]);
    assert!(h.ed.command_state(CommandId::CopyProps).enabled);
    assert!(h.ed.command_state(CommandId::PasteProps).enabled);
    h.ed.take_events();
    h.ed.execute(CommandId::PasteProps);
    assert!(h
        .ed
        .take_events()
        .contains(&EditorEvent::PasteRequested(PasteMode::Props)));
    h.key("KeyC", ctrl().with_alt());
    assert!(h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::ClipboardProps(_))));
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    assert!(
        h.ed.command_state(CommandId::CopyProps).enabled,
        "копировать можно"
    );
    assert!(!h.ed.command_state(CommandId::PasteProps).enabled);
}

/// Текст с двумя участками разных цветов (красный, зелёный).
fn two_colour_text(h: &mut H, x: f64, y: f64) -> NodeId {
    let run = |s: &str, c: Color| TextRun {
        text: s.to_string(),
        attrs: TextAttrs {
            fill: Some(Fill::solid(c)),
            ..Default::default()
        },
    };
    let content = TextContent {
        runs: vec![
            run("Кра", Color::rgba(255, 0, 0, 255)),
            run("зел\n", Color::rgba(0, 255, 0, 255)),
        ],
    };
    h.ed.create_text(
        kurbo::Point::new(x, y),
        &content,
        &TextProps::default(),
        0.0,
        0.0,
    )
    .unwrap()
}

#[test]
fn copy_from_text_takes_the_visible_colour_of_its_first_run() {
    let mut h = text_h();
    let t = two_colour_text(&mut h, 100.0, 100.0);
    h.set(
        t,
        PropsPatch {
            fills: Some(vec![Fill::solid(Color::rgba(0, 0, 0, 255))]),
            ..Default::default()
        },
    );
    let r = h.rect(None, 400.0, 400.0, 30.0, 30.0);
    copy_props(&mut h, &[t]);
    h.ed.select(&[r]);
    h.ed.paste_props(None);
    assert_eq!(
        h.props(r).fills,
        vec![Fill::solid(Color::rgba(255, 0, 0, 255))]
    );
}

#[test]
fn pasted_fill_recolours_every_run_of_a_text_in_one_step() {
    let mut h = text_h();
    let t = two_colour_text(&mut h, 100.0, 100.0);
    let src = h.rect(None, 400.0, 400.0, 30.0, 30.0);
    let blue = vec![Fill::solid(Color::rgba(0, 0, 255, 255))];
    h.set(
        src,
        PropsPatch {
            fills: Some(blue.clone()),
            ..Default::default()
        },
    );
    copy_props(&mut h, &[src]);
    h.ed.select(&[t]);
    let steps = h.ed.step_count();
    let before = h.state();
    h.ed.paste_props(None);
    let p = h.props(t);
    assert_eq!(p.fills, blue);
    let content = &p.text.as_ref().unwrap().content;
    assert!(content.runs.iter().all(|r| r.attrs.fill.is_none()));
    assert_eq!(text_of(&mut h, t), "Кразел\n");
    h.assert_one_step(steps, &before);
}
