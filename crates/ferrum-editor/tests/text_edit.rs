//! Правка текста на холсте (подсистема 3, этап 2): вход двойным щелчком и Enter,
//! выход Esc и щелчком мимо, курсор и выделение мышью и клавишами, мигание,
//! раскладка клавиш в правке не действует.

mod common;

use common::text::*;
use common::*;
use ferrum_editor::*;
use kurbo::Point;

#[test]
fn double_click_enters_editing_with_caret_at_the_point() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет мир", 24.0);
    h.ed.select(&[]);
    let (x, y) = at_index(&h, id, 3);
    h.dbl(x + 1.0, y);
    assert!(h.ed.is_editing_text());
    assert_eq!(sel_range(&h), (3, 3));
    assert_eq!(h.sel(), vec![id]);
    // Вместо рамки с ручками — рамка текста и курсор.
    let o = h.ed.overlay();
    assert!(o.selection.is_empty());
    let t = o.text_edit.unwrap();
    let (cx, top, bottom) = t.caret.unwrap();
    assert!(((t.transform * Point::new(cx, 0.0)).x - x).abs() < 1e-6);
    assert!(bottom - top > 20.0);
    assert!(t.selection.is_empty());
    assert_eq!(h.ed.step_count(), 1, "вход в правку — не шаг истории");
}

#[test]
fn enter_selects_all_and_escape_exits_with_the_node_selected() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет мир", 24.0);
    h.key("Enter", none());
    assert_eq!(sel_range(&h), (0, 10));
    assert!(!h.ed.overlay().text_edit.unwrap().selection.is_empty());
    assert!(h.ed.overlay().text_edit.unwrap().caret.is_none());
    h.key("Escape", none());
    assert!(!h.ed.is_editing_text());
    assert_eq!(h.sel(), vec![id]);
    // Enter по не тексту ничего не делает.
    let r = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.select(&[r]);
    h.key("Enter", none());
    assert!(!h.ed.is_editing_text());
}

#[test]
fn arrows_move_by_cluster_word_line_and_document() {
    let mut h = text_h();
    add_text(&mut h, 100.0, 100.0, "один два\nтри", 24.0);
    h.key("Enter", none());
    // Схлопнуть выделение к началу, затем по кластерам.
    h.key("ArrowLeft", none());
    assert_eq!(sel_range(&h), (0, 0));
    h.key("ArrowRight", none());
    assert_eq!(sel_range(&h), (1, 1));
    h.key("ArrowRight", shift());
    h.key("ArrowRight", shift());
    assert_eq!(sel_range(&h), (1, 3));
    // Без Shift выделение схлопывается к своему краю.
    h.key("ArrowRight", none());
    assert_eq!(sel_range(&h), (3, 3));
    // Ctrl+→ (Windows) — конец слова, Ctrl+← — начало.
    h.key("ArrowRight", ctrl());
    assert_eq!(sel_range(&h), (4, 4));
    h.key("ArrowRight", ctrl());
    assert_eq!(sel_range(&h), (8, 8));
    h.key("ArrowLeft", ctrl());
    assert_eq!(sel_range(&h), (5, 5));
    // Home/End — строка, Ctrl+End — конец текста, ↓ — следующая строка.
    h.key("End", none());
    assert_eq!(sel_range(&h), (8, 8));
    h.key("Home", none());
    assert_eq!(sel_range(&h), (0, 0));
    h.key("ArrowDown", none());
    assert_eq!(sel_range(&h), (9, 9));
    h.key("ArrowDown", none());
    assert_eq!(sel_range(&h), (12, 12), "ниже последней строки — конец");
    h.key("Home", ctrl().with_shift());
    assert_eq!(sel_range(&h), (12, 0));
    h.key("KeyA", ctrl());
    assert_eq!(sel_range(&h), (0, 12));
}

#[test]
fn mac_uses_option_for_words_and_command_for_lines() {
    let mut h = H::with_platform(Platform::Mac);
    h.ed.set_fonts(loaded_fonts());
    add_text(&mut h, 100.0, 100.0, "один два\nтри", 24.0);
    h.key("Enter", none());
    h.key("ArrowLeft", none());
    h.key("ArrowRight", Modifiers::ALT);
    assert_eq!(sel_range(&h), (4, 4));
    h.key("ArrowRight", Modifiers::META);
    assert_eq!(sel_range(&h), (8, 8));
    h.key("ArrowDown", Modifiers::META);
    assert_eq!(sel_range(&h), (12, 12));
    h.key("ArrowUp", Modifiers::ALT);
    assert_eq!(sel_range(&h), (9, 9));
}

#[test]
fn mouse_selects_by_drag_word_paragraph_and_shift_click() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "один два\nтри", 24.0);
    h.key("Enter", none());
    let (x1, y) = at_index(&h, id, 1);
    let (x3, _) = at_index(&h, id, 3);
    // Протяжка — от позиции до позиции.
    h.drag((x1, y), (x3, y), none());
    assert_eq!(sel_range(&h), (1, 3));
    // Двойной щелчок — слово, тройной — абзац.
    let (x6, _) = at_index(&h, id, 6);
    h.dbl(x6, y);
    assert_eq!(sel_range(&h), (5, 8));
    h.t -= 550.0;
    h.click(x6, y, none());
    assert_eq!(sel_range(&h), (0, 8));
    // Shift+щелчок расширяет от якоря.
    h.click(x1, y, none());
    let (x10, y2) = at_index(&h, id, 10);
    h.click(x10, y2, shift());
    assert_eq!(sel_range(&h), (1, 10));
    assert_eq!(h.ed.step_count(), 1);
}

#[test]
fn click_outside_ends_editing_and_acts_as_a_normal_click() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let r = h.rect(None, 400.0, 400.0, 50.0, 50.0);
    h.ed.select(&[id]);
    h.key("Enter", none());
    h.click(420.0, 420.0, none());
    assert!(!h.ed.is_editing_text());
    assert_eq!(h.sel(), vec![r]);
}

#[test]
fn keymap_does_not_act_while_editing() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.key("Enter", none());
    h.key("KeyR", none());
    h.key("Delete", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert!(h.ed.scene().node(id).is_some());
    assert!(h.ed.is_editing_text());
    // Выбор инструмента извне заканчивает правку.
    h.ed.set_tool(ToolKind::Rect);
    assert!(!h.ed.is_editing_text());
}

#[test]
fn caret_blinks_at_one_hertz_and_lights_up_after_a_key() {
    let mut h = text_h();
    add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.set_time(10_000.0);
    h.key("Enter", none());
    h.key("ArrowLeft", none());
    let caret = |h: &H| h.ed.overlay().text_edit.unwrap().caret.is_some();
    assert!(caret(&h));
    h.ed.take_events();
    h.ed.set_time(10_600.0);
    assert!(!caret(&h));
    assert!(h.ed.take_events().contains(&EditorEvent::NeedsRedraw));
    h.ed.set_time(11_100.0);
    assert!(caret(&h));
    h.ed.set_time(11_600.0);
    assert!(!caret(&h));
    h.key("ArrowRight", none());
    assert!(caret(&h));
    // Поле ввода получает прямоугольник курсора и когда он погашен.
    h.ed.set_time(12_200.0);
    assert!(info(&h).caret.height() > 20.0);
}

#[test]
fn caret_rect_follows_the_camera() {
    let mut h = text_h();
    add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.key("Enter", none());
    let one = info(&h).caret;
    let mut cam = h.ed.camera().clone();
    cam.set_zoom(2.0);
    h.ed.set_camera(cam);
    let two = info(&h).caret;
    assert!((two.height() - 2.0 * one.height()).abs() < 1e-6);
}

#[test]
fn double_click_on_small_text_beats_its_handles() {
    let mut h = text_h();
    // Кегль 12: рамка 15 px высотой — точка у края в зоне ручки стороны.
    let id = add_text(&mut h, 100.0, 300.0, "Привет", 12.0);
    h.ed.select(&[]);
    h.dbl(105.0, 305.0);
    assert!(h.ed.is_editing_text());
    assert_eq!(h.sel(), vec![id]);
}

#[test]
fn press_just_off_a_corner_ends_editing_without_a_handle_gesture() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.key("Enter", none());
    let n = h.ed.scene().node(id).unwrap();
    let (w, hh) = (n.props.w, n.props.h);
    let before = h.ed.page_doc().props(id).unwrap();
    let steps = h.ed.step_count();
    // 10 px по диагонали от правого нижнего угла: зона поворота ручек.
    let (x, y) = (100.0 + w + 7.0, 100.0 + hh + 7.0);
    h.mv(x, y, none());
    assert_ne!(h.ed.cursor(), Cursor::Text);
    assert!(!matches!(
        h.ed.cursor(),
        Cursor::Rotate { .. } | Cursor::Resize { .. }
    ));
    h.down(x, y, none());
    assert!(!h.ed.is_editing_text());
    h.mv(x + 40.0, y + 40.0, none());
    h.up(x + 40.0, y + 40.0, none());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(h.ed.page_doc().props(id).unwrap(), before);
    assert!(h.sel().is_empty());
}

#[test]
fn read_only_document_refuses_editing() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
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
    assert!(!ed.start_text_edit(id, EnterAt::All));
    assert!(!ed.is_editing_text());
}

#[test]
fn held_enter_does_not_enter_editing() {
    let mut h = text_h();
    add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let mut e = KeyEvent::new("Enter", none());
    e.repeat = true;
    h.ev(Input::KeyDown(e));
    assert!(!h.ed.is_editing_text());
}

#[test]
fn text_tool_on_a_text_edits_it_at_the_point() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет мир", 24.0);
    h.ed.select(&[]);
    let nodes = h.ed.scene().len();
    let steps = h.ed.step_count();
    let (x, y) = at_index(&h, id, 3);
    h.key("KeyT", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Text);
    // Как в Figma: инструмент «Текст» по тексту — его правка, а не новый текст.
    h.click(x + 1.0, y, none());
    assert!(h.ed.is_editing_text());
    let i = info(&h);
    assert!(!i.draft, "не черновик нового текста");
    assert_eq!(i.id, id);
    assert_eq!(sel_range(&h), (3, 3));
    assert_eq!(h.sel(), vec![id]);
    assert_eq!(h.ed.scene().len(), nodes);
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    // Мимо текста — по-прежнему новый текст.
    h.key("Escape", none());
    h.key("KeyT", none());
    h.click(400.0, 400.0, none());
    assert!(info(&h).draft);
}
