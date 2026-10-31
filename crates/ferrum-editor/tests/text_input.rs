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
fn pause_caret_move_and_kind_break_the_step() {
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

/// Рамка 400×300 в (100, 100) с заданным поворотом и отражением.
fn posed_frame(h: &mut H, rotation: f64, flip_x: bool) -> NodeId {
    let f = h.frame(None, 100.0, 100.0, 400.0, 300.0);
    h.set(
        f,
        PropsPatch {
            rotation: Some(rotation),
            flip_x: Some(flip_x),
            ..Default::default()
        },
    );
    f
}

/// Линейная часть мировой матрицы узла — единичная (текст не повёрнут и не зеркален).
fn assert_upright(h: &H, id: NodeId) {
    let c = h.ed.scene().node(id).unwrap().world.as_coeffs();
    let want = [1.0, 0.0, 0.0, 1.0];
    for (g, w) in c[..4].iter().zip(want) {
        assert!((g - w).abs() < 1e-9, "{c:?}");
    }
}

#[test]
fn text_tool_in_a_rotated_or_flipped_frame_matches_the_screen_gesture() {
    for (rotation, flip) in [(30.0, false), (0.0, true), (30.0, true)] {
        // Щелчок: текст «авто» встаёт в точку щелчка и не повёрнут.
        let mut h = text_h();
        let frame = posed_frame(&mut h, rotation, flip);
        h.key("KeyT", none());
        h.click(300.0, 250.0, none());
        type_text(&mut h, "x");
        let id = info(&h).id;
        assert_eq!(h.parent(id), Some(frame));
        assert_upright(&h, id);
        let o = h.ed.scene().node(id).unwrap().world * kurbo::Point::ZERO;
        assert!(
            close(o.x, 300.0) && close(o.y, 250.0),
            "{rotation} {flip}: {o:?}"
        );
        // Протяжка: рамка текста совпадает с нарисованной на экране.
        let mut h = text_h();
        let frame = posed_frame(&mut h, rotation, flip);
        h.key("KeyT", none());
        h.drag((250.0, 220.0), (360.0, 270.0), none());
        type_text(&mut h, "x");
        let id = info(&h).id;
        assert_eq!(h.parent(id), Some(frame));
        assert_upright(&h, id);
        let n = h.ed.scene().node(id).unwrap();
        assert_eq!((n.props.w, n.props.h), (110.0, 50.0));
        let r = n
            .world
            .transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, 110.0, 50.0));
        assert!(
            close(r.x0, 250.0) && close(r.y0, 220.0) && close(r.x1, 360.0) && close(r.y1, 270.0),
            "{rotation} {flip}: {r:?}"
        );
    }
}

#[test]
fn multiline_insert_into_a_list_item_makes_list_paragraphs() {
    for kind in [ListKind::Bullet, ListKind::Ordered] {
        let mut h = text_h();
        let id = add_text(&mut h, 100.0, 100.0, "xyz", 24.0);
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
        h.ed.text_input(TextInputKind::Insert, "a\nb").unwrap();
        let c = h.props(id).text.unwrap().content.clone();
        assert_eq!(c.text(), "xya\nbz\n");
        assert_eq!(c.attrs_at(3).list, Some(kind), "разрезанный абзац");
        assert_eq!(c.attrs_at(6).list, Some(kind));
        // Вставка из буфера — так же.
        h.ed.text_input(TextInputKind::Paste, "1\r\n2").unwrap();
        let c = h.props(id).text.unwrap().content.clone();
        assert_eq!(c.text(), "xya\nb1\n2z\n");
        assert_eq!(c.attrs_at(6).list, Some(kind));
        assert_eq!(c.attrs_at(9).list, Some(kind));
    }
}

#[test]
fn delete_while_the_font_loads_is_reported_for_retry_instead_of_dropped() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "abc", 24.0);
    h.key("Enter", none());
    h.key("KeyA", ctrl());
    h.ed.take_events();
    // Шрифты исчезли из реестра (грузятся заново): правка откладывается.
    let fonts = ferrum_text::Fonts::shared();
    {
        let faces = [
            "inter/Inter-Variable.ttf",
            "inter/Inter-Italic-Variable.ttf",
            "roboto/Roboto-Variable.ttf",
            "roboto/Roboto-Italic-Variable.ttf",
        ]
        .iter()
        .flat_map(|r| ferrum_text::read_faces(&font_file(r), r))
        .collect::<Vec<_>>();
        let mut f = fonts.borrow_mut();
        f.add_catalog(ferrum_text::SourceKind::Bundled, ferrum_text::group(faces));
        f.take_requests();
    }
    h.ed.set_fonts(fonts);
    h.key("Backspace", none());
    let events = h.ed.take_events();
    // Не молча: ядро узнаёт об отложенной клавише и повторит её по приходу шрифтов.
    assert!(h.ed.take_text_pending(), "{events:?}");
    assert!(!events.iter().any(|e| matches!(e, EditorEvent::Error(_))));
    assert_eq!(text_of(&mut h, id), "abc\n", "документ не менялся");
    assert!(h.ed.is_editing_text());
}

#[test]
fn draft_without_its_parent_ends_the_edit_instead_of_retrying() {
    let mut h = text_h();
    let frame = h.frame(None, 100.0, 100.0, 400.0, 300.0);
    h.key("KeyT", none());
    h.click(300.0, 250.0, none());
    assert!(info(&h).draft);
    h.ed.transact(|tx| {
        tx.delete_nodes(&[frame])?;
        Ok(vec![])
    })
    .unwrap();
    assert!(
        !h.ed.is_editing_text(),
        "черновик без родителя — правка кончена"
    );
    h.ed.take_events();
    h.ed.text_input(TextInputKind::Insert, "x").unwrap();
    assert!(h.ed.take_events().is_empty());
    assert!(h.ed.page_doc().walk().is_empty());
}

#[test]
fn refused_exit_delete_does_not_count_a_step() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "", 24.0);
    let steps = h.ed.step_count();
    h.key("Enter", none());
    assert!(h.ed.is_editing_text());
    // Документ стал «только чтение» посреди правки: удаление пустого текста
    // при выходе не записывается — и шаг не считается.
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    h.ed.take_events();
    h.key("Escape", none());
    assert!(!h.ed.is_editing_text());
    assert!(h.ed.scene().node(id).is_some());
    assert_eq!(h.ed.step_count(), steps, "счётчик шагов идёт за историей");
    assert!(h
        .ed
        .take_events()
        .iter()
        .any(|e| matches!(e, EditorEvent::Error(_))));
}
