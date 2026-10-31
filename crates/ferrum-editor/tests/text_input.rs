//! Ввод в текст (подсистема 3, этап 2): набор и удаление, склейка ввода в один
//! шаг (пауза, позиция, стиль), абзац со списком, рост рамки «авто», IME —
//! превью без документа, инструмент T, пустой текст при выходе, отмена в правке.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn type_text(h: &mut H, s: &str) {
    for c in s.chars() {
        h.ed.set_time(h.t);
        h.t += 100.0;
        h.ed.text_input(TextInputKind::Insert, &c.to_string())
            .unwrap();
    }
}

#[test]
fn typing_inserts_at_the_caret_and_merges_into_one_step() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let steps = h.ed.step_count();
    let w0 = h.props(id).w;
    h.key("Enter", none());
    h.key("End", none());
    type_text(&mut h, ", мир");
    assert_eq!(text_of(&mut h, id), "Привет, мир\n");
    assert_eq!(sel_range(&h), (11, 11));
    assert_eq!(h.ed.step_count(), steps + 1, "ввод подряд — один шаг");
    // «Авто по ширине»: рамка выросла той же правкой.
    assert!(h.props(id).w > w0 + 30.0);
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, id), "Привет\n");
    assert_eq!(h.props(id).w, w0);
    assert!(h.ed.is_editing_text(), "отмена в правке не выходит из неё");
}

#[test]
fn pause_caret_move_and_style_break_the_step() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "", 24.0);
    let steps = h.ed.step_count();
    h.key("Enter", none());
    type_text(&mut h, "ab");
    h.t += 1500.0;
    type_text(&mut h, "c");
    assert_eq!(
        h.ed.step_count(),
        steps + 2,
        "пауза больше секунды — новый шаг"
    );
    h.key("ArrowLeft", none());
    type_text(&mut h, "x");
    assert_eq!(text_of(&mut h, id), "abxc\n");
    assert_eq!(h.ed.step_count(), steps + 3, "другая позиция — новый шаг");
    // Удаление после набора — тоже новый шаг; Backspace подряд — один.
    h.ed.text_input(TextInputKind::DeleteBackward, "").unwrap();
    h.ed.text_input(TextInputKind::DeleteBackward, "").unwrap();
    assert_eq!(text_of(&mut h, id), "ac\n");
    assert_eq!(h.ed.step_count(), steps + 4);
}

#[test]
fn delete_by_cluster_word_line_and_selection() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "один два три", 24.0);
    h.key("Enter", none());
    h.key("End", none());
    h.key("Backspace", ctrl());
    assert_eq!(text_of(&mut h, id), "один два \n");
    h.key("Home", none());
    h.key("Delete", none());
    assert_eq!(text_of(&mut h, id), "дин два \n");
    h.key("ArrowRight", shift());
    h.key("ArrowRight", shift());
    h.key("Backspace", none());
    assert_eq!(text_of(&mut h, id), "н два \n");
    // Последний перевод строки не удаляется.
    h.key("End", ctrl());
    h.key("Delete", none());
    assert_eq!(text_of(&mut h, id), "н два \n");
    // Набор поверх выделения заменяет его.
    h.key("KeyA", ctrl());
    type_text(&mut h, "Ок");
    assert_eq!(text_of(&mut h, id), "Ок\n");
}

#[test]
fn typing_takes_the_style_on_the_left_and_paragraph_keeps_the_list() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    let bold = TextAttrs {
        style: Some("Bold".into()),
        ..Default::default()
    };
    let list = TextAttrs {
        list: Some(ListKind::Bullet),
        ..Default::default()
    };
    h.ed.transact(|tx| {
        tx.format_text(id, 0..1, &bold, &[])?;
        tx.format_text(id, 2..3, &list, &[])?;
        Ok(vec![id])
    })
    .unwrap();
    h.key("Enter", none());
    h.key("Home", none());
    h.key("ArrowRight", none());
    type_text(&mut h, "x");
    h.ed.text_input(TextInputKind::Paragraph, "").unwrap();
    let c = h.props(id).text.unwrap().content.clone();
    assert_eq!(c.text(), "ax\nb\n");
    assert_eq!(c.attrs_at(1).style.as_deref(), Some("Bold"));
    assert_eq!(
        c.attrs_at(2).list,
        Some(ListKind::Bullet),
        "новый пункт списка"
    );
    assert_eq!(c.attrs_at(4).list, Some(ListKind::Bullet));
    // Backspace в начале пункта снимает список, а не склеивает абзацы.
    h.key("Home", none());
    h.key("Backspace", none());
    let c = h.props(id).text.unwrap().content.clone();
    assert_eq!(c.text(), "ax\nb\n");
    assert_eq!(c.attrs_at(4).list, None);
}

#[test]
fn centered_auto_width_grows_both_ways() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    let mut props = h.props(id).text.unwrap().props.clone();
    props.align_h = AlignH::Center;
    h.set(
        id,
        PropsPatch {
            text: Some(props),
            ..Default::default()
        },
    );
    let before = h.props(id);
    h.key("Enter", none());
    h.key("End", none());
    type_text(&mut h, "cdef");
    let after = h.props(id);
    let grown = after.w - before.w;
    assert!(grown > 20.0);
    assert!((after.x - (before.x - grown / 2.0)).abs() < 1e-6);
}

#[test]
fn composition_is_a_preview_until_commit() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    let steps = h.ed.step_count();
    h.key("Enter", none());
    h.key("End", none());
    h.ed.text_input(TextInputKind::Compose, "к").unwrap();
    h.ed.text_input(TextInputKind::Compose, "ку").unwrap();
    assert_eq!(text_of(&mut h, id), "ab\n", "в документе составления нет");
    let scene_text =
        h.ed.scene()
            .node(id)
            .unwrap()
            .props
            .text
            .as_ref()
            .unwrap()
            .content
            .text();
    assert_eq!(scene_text, "abку\n");
    let o = h.ed.overlay().text_edit.unwrap();
    assert_eq!(o.composition.len(), 1);
    assert!(info(&h).composing);
    h.ed.text_input(TextInputKind::Commit, "кур").unwrap();
    assert_eq!(text_of(&mut h, id), "abкур\n");
    assert!(!h.ed.scene().has_preview());
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(sel_range(&h), (5, 5));
    // Отменённое составление (пустой текст) ничего не оставляет.
    h.ed.text_input(TextInputKind::Compose, "x").unwrap();
    h.ed.text_input(TextInputKind::Compose, "").unwrap();
    assert!(!h.ed.scene().has_preview());
    assert_eq!(text_of(&mut h, id), "abкур\n");
}

#[test]
fn text_tool_click_makes_a_draft_until_the_first_char() {
    let mut h = text_h();
    h.key("KeyT", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Text);
    h.click(200.0, 150.0, none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert!(h.ed.is_editing_text() && info(&h).draft);
    assert!(h.ed.page_doc().walk().is_empty(), "черновик не в документе");
    assert_eq!(h.ed.step_count(), 0);
    assert!(h.ed.overlay().text_edit.unwrap().caret.is_some());
    type_text(&mut h, "Hi");
    let id = info(&h).id;
    assert!(!info(&h).draft);
    assert_eq!(text_of(&mut h, id), "Hi\n");
    let p = h.props(id);
    assert_eq!((p.x, p.y), (200.0, 150.0));
    assert_eq!(p.text.unwrap().props.sizing, TextSizing::AutoWidth);
    assert_eq!(h.ed.step_count(), 1, "создание и набор — один шаг");
    h.key("Escape", none());
    assert_eq!(h.sel(), vec![id]);
    h.key("KeyZ", ctrl());
    assert!(h.ed.page_doc().walk().is_empty());
}

#[test]
fn empty_new_text_vanishes_without_a_step() {
    let mut h = text_h();
    h.key("KeyT", none());
    h.click(200.0, 150.0, none());
    h.key("Escape", none());
    assert!(!h.ed.is_editing_text());
    assert!(h.ed.page_doc().walk().is_empty());
    assert!(h.ed.scene().is_empty());
    assert_eq!(h.ed.step_count(), 0);
    // Протяжка — фиксированный размер по рамке.
    h.key("KeyT", none());
    h.drag((100.0, 100.0), (300.0, 160.0), none());
    type_text(&mut h, "x");
    let id = info(&h).id;
    let p = h.props(id);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 200.0, 60.0));
    assert_eq!(p.text.unwrap().props.sizing, TextSizing::Fixed);
}

#[test]
fn emptied_text_is_removed_on_exit_with_the_same_step() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    let steps = h.ed.step_count();
    h.key("Enter", none());
    h.key("Backspace", none());
    h.key("Escape", none());
    assert!(h.ed.scene().node(id).is_none());
    assert!(h.sel().is_empty());
    assert_eq!(h.ed.step_count(), steps + 1);
    h.key("KeyZ", ctrl());
    assert_eq!(text_of(&mut h, id), "ab\n");
}

#[test]
fn ending_or_losing_the_edit_clears_the_ime_preview() {
    // Выход по Esc во время составления.
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    h.key("Enter", none());
    h.key("End", none());
    h.ed.text_input(TextInputKind::Compose, "ку").unwrap();
    assert!(h.ed.scene().has_preview());
    h.key("Escape", none());
    assert!(!h.ed.is_editing_text());
    assert!(!h.ed.scene().has_preview());
    assert_eq!(text_of(&mut h, id), "ab\n");
    // Правимый узел исчез (чужая правка) во время составления.
    h.key("Enter", none());
    h.key("End", none());
    h.ed.text_input(TextInputKind::Compose, "ку").unwrap();
    assert!(h.ed.scene().has_preview());
    h.ed.transact(|tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    assert!(!h.ed.is_editing_text());
    assert!(!h.ed.scene().has_preview(), "превью не переживает правку");
    assert!(h.ed.scene().node(id).is_none());
}

#[test]
fn text_tool_does_not_start_in_a_read_only_document() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    let bytes = ferrum_doc::container::encode(h.ed.document()).unwrap();
    let future =
        ferrum_doc::container::testing::with_schema_version(&bytes, ferrum_doc::SCHEMA_VERSION + 1);
    let doc = ferrum_doc::container::decode(&future).unwrap().document;
    let mut ed = Editor::new(
        doc,
        None,
        kurbo::Size::new(800.0, 600.0),
        1.0,
        Platform::Windows,
    )
    .unwrap();
    ed.set_fonts(loaded_fonts());
    ed.set_tool(ToolKind::Text);
    let down = |x: f64| PointerEvent::new(kurbo::Point::new(x, 400.0), none(), 0.0);
    ed.handle(Input::PointerDown(down(500.0)));
    ed.handle(Input::PointerUp(down(500.0)));
    assert!(!ed.is_editing_text());
    assert!(!ed.scene().has_preview());
    assert!(ed.scene().node(id).is_some());
}
