//! Буфер обмена в правке текста (подсистема 3, этап 2): копия фрагмента — наш
//! формат и обычный текст, вставка со стилями в другой текст, вырезание одним
//! шагом, обычный текст вне правки — новый текстовый узел.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::clipboard::ClipPayload;
use ferrum_editor::*;

/// Байты и обычный текст последней копии фрагмента.
fn take_text_clip(h: &mut H) -> Option<(Vec<u8>, String)> {
    h.ed.take_events().into_iter().find_map(|e| match e {
        EditorEvent::ClipboardText { bytes, plain } => Some((bytes, plain)),
        _ => None,
    })
}

/// «Привет» кеглем 24, «При» — жирным; правка, выделено «ивет» (2..6).
fn bold_text(h: &mut H) -> NodeId {
    let id = add_text(h, 100.0, 100.0, "Привет", 24.0);
    h.ed.transact(|tx| {
        tx.format_text(
            id,
            0..3,
            &TextAttrs {
                style: Some("Bold".into()),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![id])
    })
    .unwrap();
    h.key("Enter", none());
    h.key("Home", none());
    h.key("ArrowRight", none());
    h.key("ArrowRight", none());
    h.key("End", shift());
    id
}

#[test]
fn copy_puts_a_fragment_with_resolved_style_and_plain_text() {
    let mut h = text_h();
    bold_text(&mut h);
    h.ed.take_events();
    h.ed.execute(CommandId::Copy);
    let (bytes, plain) = take_text_clip(&mut h).expect("копия фрагмента");
    assert_eq!(plain, "ивет");
    let p = ClipPayload::decode(&bytes).unwrap();
    assert_eq!(p.nodes.len(), 1);
    let t = p.nodes[0].props.text.as_ref().unwrap();
    assert_eq!(t.content.text(), "ивет\n");
    assert_eq!(t.props.sizing, TextSizing::AutoWidth);
    let first = &t.content.runs[0].attrs;
    assert_eq!(first.style.as_deref(), Some("Bold"));
    assert_eq!(first.size, Some(24.0), "стиль — все ключи, не от умолчаний");
    assert_eq!(first.font.as_deref(), Some("Inter"));
    // Без выделения копировать нечего.
    h.key("ArrowRight", none());
    h.ed.execute(CommandId::Copy);
    assert!(take_text_clip(&mut h).is_none());
}

#[test]
fn fragment_pastes_with_its_style_into_another_text_in_one_step() {
    let mut h = text_h();
    bold_text(&mut h);
    h.ed.execute(CommandId::Copy);
    let (bytes, _) = take_text_clip(&mut h).unwrap();
    h.key("Escape", none());
    let other = add_text(&mut h, 100.0, 300.0, "ab", 12.0);
    h.key("Enter", none());
    h.key("End", none());
    let steps = h.ed.step_count();
    h.ed.paste(&bytes).unwrap();
    assert_eq!(text_of(&mut h, other), "abивет\n");
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(sel_range(&h), (6, 6));
    let c = h.props(other).text.unwrap().content.clone();
    // Кегль источника (24) отличается от умолчания цели (12) — остаётся разметкой.
    assert_eq!(c.attrs_at(2).size, Some(24.0));
    assert_eq!(c.attrs_at(2).style.as_deref(), Some("Bold"));
    assert_eq!(c.attrs_at(4).style, None, "обычное начертание — как у узла");
    // Шрифт совпал с умолчанием цели — разметки нет.
    assert_eq!(c.attrs_at(2).font, None);
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, other), "ab\n");
}

#[test]
fn cut_removes_the_fragment_and_undo_brings_it_back() {
    let mut h = text_h();
    let id = bold_text(&mut h);
    h.ed.execute(CommandId::Cut);
    assert_eq!(take_text_clip(&mut h).unwrap().1, "ивет");
    assert_eq!(text_of(&mut h, id), "Пр\n");
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, id), "Привет\n");
}

#[test]
fn non_text_payload_ends_editing_and_pastes_nodes() {
    let mut h = text_h();
    let r = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.select(&[r]);
    let bytes = h.ed.copy().unwrap();
    add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    h.key("Enter", none());
    h.ed.paste(&bytes).unwrap();
    assert!(!h.ed.is_editing_text());
    assert_eq!(h.ed.page_doc().walk().len(), 3);
}

#[test]
fn plain_text_outside_editing_makes_a_text_node_in_the_view_center() {
    let mut h = text_h();
    h.ed.paste_plain("Строка\r\nвторая").unwrap();
    let id = h.sel()[0];
    let p = h.props(id);
    assert_eq!(p.kind, NodeKind::Text);
    assert_eq!(p.text.as_ref().unwrap().content.text(), "Строка\nвторая\n");
    assert_eq!(p.text.unwrap().props.sizing, TextSizing::AutoWidth);
    let c = h.ed.camera().center();
    assert!((p.x + p.w / 2.0 - c.x).abs() <= 1.0 && (p.y + p.h / 2.0 - c.y).abs() <= 1.0);
    assert_eq!(h.ed.step_count(), 1);
    // В правке — в позицию курсора.
    h.key("Enter", none());
    h.key("Home", ctrl());
    h.ed.paste_plain("> ").unwrap();
    assert_eq!(text_of(&mut h, id), "> Строка\nвторая\n");
}

#[test]
fn fragment_with_line_breaks_continues_the_list_of_the_split_paragraph() {
    for kind in [ListKind::Bullet, ListKind::Ordered] {
        let mut h = text_h();
        add_text(&mut h, 100.0, 100.0, "a\nb", 24.0);
        h.key("Enter", none());
        h.key("Home", ctrl());
        for _ in 0..3 {
            h.key("ArrowRight", shift());
        }
        h.ed.execute(CommandId::Copy);
        let (bytes, _) = take_text_clip(&mut h).expect("копия");
        h.key("Escape", none());
        let id = add_text(&mut h, 100.0, 300.0, "xyz", 24.0);
        let list = TextAttrs {
            list: Some(kind),
            ..Default::default()
        };
        h.ed.transact(|tx| {
            tx.format_text(id, 3..4, &list, &[])?;
            Ok(vec![id])
        })
        .unwrap();
        h.key("Enter", none());
        h.key("Home", none());
        h.key("ArrowRight", none());
        h.key("ArrowRight", none());
        h.ed.paste(&bytes).unwrap();
        let c = h.props(id).text.unwrap().content.clone();
        assert_eq!(c.text(), "xya\nbz\n");
        assert_eq!(c.attrs_at(3).list, Some(kind), "разрезанный абзац");
        assert_eq!(c.attrs_at(6).list, Some(kind));
    }
}

/// Два пункта списка «a», «b» (нужный вид) и обычный абзац «xyz» в другом узле;
/// возвращает байты копии обоих пунктов и узел-цель с курсором после «xy».
fn list_fragment_and_target(h: &mut H, src: ListKind, dst: Option<ListKind>) -> (Vec<u8>, NodeId) {
    let s = add_text(h, 100.0, 100.0, "a\nb", 24.0);
    let list = TextAttrs {
        list: Some(src),
        ..Default::default()
    };
    h.ed.transact(|tx| {
        tx.format_text(s, 0..4, &list, &[])?;
        Ok(vec![s])
    })
    .unwrap();
    h.key("Enter", none());
    h.key("Home", ctrl());
    for _ in 0..3 {
        h.key("ArrowRight", shift());
    }
    h.ed.execute(CommandId::Copy);
    let (bytes, _) = take_text_clip(h).expect("копия");
    h.key("Escape", none());
    let t = add_text(h, 100.0, 300.0, "xyz", 24.0);
    if let Some(k) = dst {
        let list = TextAttrs {
            list: Some(k),
            ..Default::default()
        };
        h.ed.transact(|tx| {
            tx.format_text(t, 3..4, &list, &[])?;
            Ok(vec![t])
        })
        .unwrap();
    }
    h.key("Enter", none());
    h.key("Home", none());
    h.key("ArrowRight", none());
    h.key("ArrowRight", none());
    (bytes, t)
}

#[test]
fn pasted_list_items_keep_their_list_in_a_plain_paragraph() {
    let mut h = text_h();
    let (bytes, id) = list_fragment_and_target(&mut h, ListKind::Bullet, None);
    h.ed.paste(&bytes).unwrap();
    let c = h.props(id).text.unwrap().content.clone();
    assert_eq!(c.text(), "xya\nbz\n");
    assert_eq!(c.attrs_at(3).list, Some(ListKind::Bullet));
    assert_eq!(
        c.attrs_at(6).list,
        None,
        "конец разрезанного абзаца — прежний"
    );
}

#[test]
fn pasted_ordered_items_stay_ordered_inside_a_bullet_item() {
    let mut h = text_h();
    let (bytes, id) = list_fragment_and_target(&mut h, ListKind::Ordered, Some(ListKind::Bullet));
    h.ed.paste(&bytes).unwrap();
    let c = h.props(id).text.unwrap().content.clone();
    assert_eq!(c.attrs_at(3).list, Some(ListKind::Ordered));
    assert_eq!(c.attrs_at(6).list, Some(ListKind::Bullet));
}

#[test]
fn cut_while_fonts_load_waits_and_leaves_clipboard_and_text_alone() {
    let mut h = text_h();
    let id = bold_text(&mut h);
    h.ed.take_events();
    assert!(!h.ed.take_text_pending());
    let fonts = ferrum_text::Fonts::shared();
    h.ed.set_fonts(fonts);
    h.ed.execute(CommandId::Cut);
    let events = h.ed.take_events();
    assert!(!events
        .iter()
        .any(|e| matches!(e, EditorEvent::ClipboardText { .. })));
    // Не ошибка: ядро поставит «Вырезать» в очередь и повторит по приходу шрифтов.
    assert!(!events.iter().any(|e| matches!(e, EditorEvent::Error(_))));
    assert!(h.ed.take_text_pending());
    assert!(!h.ed.take_text_pending(), "признак снимается чтением");
    assert_eq!(h.props(id).text.unwrap().content.text(), "Привет\n");
}

#[test]
fn every_paste_is_its_own_undo_step() {
    let mut h = text_h();
    bold_text(&mut h);
    h.ed.execute(CommandId::Copy);
    let (bytes, _) = take_text_clip(&mut h).unwrap();
    h.key("Escape", none());
    let other = add_text(&mut h, 100.0, 300.0, "ab", 12.0);
    h.key("Enter", none());
    h.key("End", none());
    let steps = h.ed.step_count();
    // Время не идёт: склейка набора сработала бы.
    h.ed.paste(&bytes).unwrap();
    h.ed.paste(&bytes).unwrap();
    h.ed.text_input(TextInputKind::Paste, "12").unwrap();
    h.ed.text_input(TextInputKind::Paste, "34").unwrap();
    h.ed.text_input(TextInputKind::Insert, "x").unwrap();
    h.ed.text_input(TextInputKind::Insert, "y").unwrap();
    assert_eq!(text_of(&mut h, other), "abиветивет1234xy\n");
    assert_eq!(
        h.ed.step_count(),
        steps + 5,
        "каждая вставка — свой шаг, набор после вставки — новый"
    );
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, other), "abиветивет1234\n");
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, other), "abиветивет12\n");
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, other), "abиветивет\n");
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, other), "abивет\n");
}
