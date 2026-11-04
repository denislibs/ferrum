//! Направляющие: протяжка, привязка, щелчки сквозь линию, отмена (спека подсистемы 2,
//! раздел 9). Стенд: зум 1, экран совпадает с миром; линейка — 20 px сверху и слева.

mod common;

use common::*;
use ferrum_doc::{Guide, GuideAxis, GuideOwner, NodeId, PropsPatch, MAX_GUIDES};
use ferrum_editor::*;
use ferrum_render::Camera;
use kurbo::Point;

const NONE: Modifiers = Modifiers::NONE;

fn gx(v: f64) -> Guide {
    Guide::new(GuideAxis::X, v)
}
fn gy(v: f64) -> Guide {
    Guide::new(GuideAxis::Y, v)
}

fn with_rulers() -> H {
    let mut h = H::new();
    h.ed.set_view(Some(true), Some(true));
    h.ed.take_events();
    h
}

fn page_guides(h: &H) -> Vec<Guide> {
    h.ed.document().page_guides(h.ed.page())
}

fn frame_guides(h: &mut H, f: NodeId) -> Vec<Guide> {
    h.props(f).guides.map(|g| g.to_vec()).unwrap_or_default()
}

/// Направляющая страницы одним шагом через документ.
fn seed_page(h: &mut H, guides: Vec<Guide>) {
    let page = h.ed.page();
    h.ed.set_guides(GuideOwner::Page(page), guides, Phase::Commit)
        .unwrap();
}

#[test]
fn drag_from_top_ruler_creates_horizontal_page_guide_in_one_step() {
    let mut h = with_rulers();
    let steps = h.ed.step_count();
    let tool = h.ed.tool_kind();
    h.down(300.0, 10.0, NONE);
    assert_eq!(h.ed.selection().len(), 0);
    h.mv(300.0, 150.0, NONE);
    assert!(h.ed.is_busy(), "пока кнопка нажата — только превью");
    assert!(page_guides(&h).is_empty(), "документ не тронут");
    let a = h.ed.overlay().active_guide.expect("линия видна");
    assert!(a.mark.infinite);
    assert_eq!(a.value, 150.0);
    h.up(300.0, 150.0, NONE);
    assert_eq!(page_guides(&h), [gy(150.0)]);
    assert_eq!(h.ed.step_count(), steps + 1, "ровно один шаг");
    assert_eq!(h.ed.tool_kind(), tool);
    assert!(h.ed.overlay().active_guide.is_none());
    assert_eq!(h.ed.overlay().guides.len(), 1);
    h.ed.execute(CommandId::Undo);
    assert!(page_guides(&h).is_empty());
    assert!(h.ed.overlay().guides.is_empty(), "оверлей после отмены");
    h.ed.execute(CommandId::Redo);
    assert_eq!(h.ed.overlay().guides.len(), 1, "оверлей после повтора");
    assert_eq!(page_guides(&h), [gy(150.0)]);
}

#[test]
fn drag_from_left_ruler_creates_vertical_guide_and_uses_world_coordinates() {
    let mut h = with_rulers();
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(2.0);
    cam.set_center(Point::new(100.0, 100.0));
    h.ed.set_camera(cam);
    h.drag((10.0, 300.0), (500.0, 300.0), NONE);
    // Экран 500 при зуме 2 и центре 100: мир 100 + (500 − 500) / 2.
    assert_eq!(page_guides(&h), [gx(100.0)]);
}

#[test]
fn guide_goes_to_the_top_level_frame_under_the_cursor() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
    let inner = h.frame(Some(f), 50.0, 50.0, 100.0, 100.0);
    // Вглубь вложенного: всё равно фрейм верхнего уровня, смещение — от его угла.
    h.drag((10.0, 170.0), (170.0, 170.0), NONE);
    assert_eq!(frame_guides(&mut h, f), [gx(70.0)]);
    assert!(frame_guides(&mut h, inner).is_empty());
    assert!(page_guides(&h).is_empty());
    // Линия фрейма — только в его границах и едет вместе с ним.
    let m = h.ed.overlay().guides[0];
    assert!(!m.infinite);
    assert_eq!(
        (m.line.p0.x, m.line.p0.y, m.line.p1.y),
        (170.0, 100.0, 400.0)
    );
    h.set(
        f,
        PropsPatch {
            x: Some(200.0),
            ..Default::default()
        },
    );
    assert_eq!(h.ed.overlay().guides[0].line.p0.x, 270.0);
}

#[test]
fn hidden_locked_and_rotated_frames_do_not_own_guides() {
    for patch in [
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    ] {
        let mut h = with_rulers();
        let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
        h.set(f, patch);
        h.drag((10.0, 220.0), (250.0, 220.0), NONE);
        assert_eq!(page_guides(&h), [gx(250.0)]);
        assert!(frame_guides(&mut h, f).is_empty());
    }
}

#[test]
fn nested_top_level_is_found_even_without_fill() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
    h.set(
        f,
        PropsPatch {
            fills: Some(vec![]),
            ..Default::default()
        },
    );
    h.drag((10.0, 220.0), (250.0, 220.0), NONE);
    assert_eq!(frame_guides(&mut h, f), [gx(150.0)]);
}

#[test]
fn dragged_guide_moves_in_one_step_and_back_on_ruler_deletes() {
    let mut h = with_rulers();
    seed_page(&mut h, vec![gy(150.0), gx(600.0)]);
    let steps = h.ed.step_count();
    let list = |h: &H| page_guides(h);
    h.drag((300.0, 150.0), (300.0, 250.0), NONE);
    assert_eq!(list(&h), [gy(250.0), gx(600.0)], "порядок тот же");
    assert_eq!(h.ed.step_count(), steps + 1);
    // Во время протяжки прежней линии нет, новая — активная.
    h.down(300.0, 250.0, NONE);
    h.mv(300.0, 300.0, NONE);
    assert_eq!(h.ed.overlay().guides.len(), 1, "прежняя скрыта превью");
    assert!(h.ed.overlay().active_guide.is_some());
    // Над своей линейкой линии нет: отпускание удалит.
    h.mv(300.0, 12.0, NONE);
    assert!(h.ed.overlay().active_guide.is_none());
    h.up(300.0, 12.0, NONE);
    assert_eq!(list(&h), [gx(600.0)]);
    assert_eq!(h.ed.step_count(), steps + 2, "удаление — один шаг");
    h.ed.execute(CommandId::Undo);
    assert_eq!(list(&h), [gy(250.0), gx(600.0)]);
}

#[test]
fn new_guide_released_over_its_ruler_is_not_created() {
    let mut h = with_rulers();
    let steps = h.ed.step_count();
    h.down(300.0, 10.0, NONE);
    h.mv(300.0, 15.0, NONE);
    h.up(300.0, 15.0, NONE);
    assert!(page_guides(&h).is_empty());
    assert_eq!(h.ed.step_count(), steps);
    assert!(!h.ed.document().can_undo());
}

#[test]
fn guide_moves_between_page_and_frame_in_one_step() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 300.0, 300.0);
    seed_page(&mut h, vec![gx(600.0)]);
    let steps = h.ed.step_count();
    h.drag((600.0, 300.0), (250.0, 300.0), NONE);
    assert!(page_guides(&h).is_empty());
    assert_eq!(frame_guides(&mut h, f), [gx(150.0)]);
    assert_eq!(h.ed.step_count(), steps + 1);
    // И обратно.
    h.drag((250.0, 300.0), (700.0, 300.0), NONE);
    assert_eq!(page_guides(&h), [gx(700.0)]);
    assert!(frame_guides(&mut h, f).is_empty());
    h.ed.execute(CommandId::Undo);
    assert_eq!(frame_guides(&mut h, f), [gx(150.0)]);
    h.ed.execute(CommandId::Undo);
    assert_eq!(page_guides(&h), [gx(600.0)]);
}

#[test]
fn escape_and_blur_cancel_without_a_trace() {
    let mut h = with_rulers();
    seed_page(&mut h, vec![gy(150.0)]);
    let steps = h.ed.step_count();
    h.down(300.0, 150.0, NONE);
    h.mv(300.0, 300.0, NONE);
    h.key("Escape", NONE);
    assert!(!h.ed.is_busy());
    assert_eq!(page_guides(&h), [gy(150.0)]);
    assert_eq!(h.ed.overlay().guides.len(), 1, "линия снова на месте");
    h.up(300.0, 300.0, NONE);
    assert_eq!(
        page_guides(&h),
        [gy(150.0)],
        "отпускание после отмены ничего не пишет"
    );
    assert_eq!(h.ed.step_count(), steps);

    h.down(300.0, 10.0, NONE);
    h.mv(300.0, 300.0, NONE);
    h.ev(Input::Blur);
    assert!(!h.ed.is_busy());
    assert_eq!(page_guides(&h), [gy(150.0)]);
    assert_eq!(h.ed.step_count(), steps);
    assert!(h.ed.overlay().active_guide.is_none());
}

#[test]
fn nan_pointer_up_cancels() {
    let mut h = with_rulers();
    h.down(300.0, 10.0, NONE);
    h.mv(300.0, 300.0, NONE);
    let steps = h.ed.step_count();
    h.ev(Input::PointerUp(PointerEvent::new(
        (f64::NAN, 0.0),
        NONE,
        1.0,
    )));
    assert!(!h.ed.is_busy());
    assert!(page_guides(&h).is_empty());
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn click_on_guide_selects_what_is_under_it() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 200.0, 200.0);
    seed_page(&mut h, vec![gx(200.0)]);
    let steps = h.ed.step_count();
    h.click(200.0, 150.0, NONE);
    assert_eq!(h.sel(), [r], "щелчок прошёл к узлу");
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(page_guides(&h), [gx(200.0)]);
    h.click(200.0, 700.0, NONE);
    assert!(
        h.sel().is_empty(),
        "щелчок по линии на пустом месте снимает выделение"
    );
}

#[test]
fn modifiers_and_tools_keep_nodes_reachable_under_a_guide() {
    let mut h = with_rulers();
    let a = h.rect(None, 100.0, 100.0, 200.0, 200.0);
    let b = h.rect(None, 400.0, 100.0, 200.0, 200.0);
    seed_page(&mut h, vec![gx(500.0)]);
    h.click(150.0, 150.0, NONE);
    // Shift — добавление в выделение, а не линия.
    h.click(
        500.0,
        150.0,
        Modifiers {
            shift: true,
            ..NONE
        },
    );
    assert_eq!(h.sel(), [a, b]);
    // Другой инструмент рисует, а не тянет линию.
    h.ed.select(&[]);
    h.ed.set_tool(ToolKind::Rect);
    let before = h.children(None).len();
    h.drag((500.0, 500.0), (560.0, 560.0), NONE);
    assert_eq!(h.children(None).len(), before + 1);
    assert_eq!(page_guides(&h), [gx(500.0)]);
}

#[test]
fn pressing_inside_a_selected_node_moves_it_not_the_guide() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 200.0, 200.0);
    seed_page(&mut h, vec![gx(200.0)]);
    h.click(150.0, 150.0, NONE);
    assert_eq!(h.sel(), [r]);
    h.drag((200.0, 150.0), (260.0, 150.0), NONE);
    assert_eq!(page_guides(&h), [gx(200.0)], "линия на месте");
    assert_eq!(h.props(r).x, 160.0, "узел сдвинут");
}

#[test]
fn guide_of_a_selected_frame_stays_grabbable() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 400.0, 400.0);
    h.ed.set_guides(GuideOwner::Frame(f), vec![gx(100.0)], Phase::Commit)
        .unwrap();
    h.click(150.0, 150.0, NONE);
    assert_eq!(h.sel(), [f]);
    h.drag((200.0, 300.0), (300.0, 300.0), NONE);
    assert_eq!(frame_guides(&mut h, f), [gx(200.0)]);
    assert_eq!(h.props(f).x, 100.0);
}

#[test]
fn selection_handles_win_over_a_guide() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 200.0, 200.0);
    seed_page(&mut h, vec![gx(300.0)]);
    h.ed.select(&[r]);
    h.hover(300.0, 200.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { angle } if angle == 0.0));
    h.drag((300.0, 200.0), (350.0, 200.0), NONE);
    assert_eq!(h.props(r).w, 250.0, "ручка рамки");
    assert_eq!(page_guides(&h), [gx(300.0)]);
}

#[test]
fn hover_shows_resize_cursor_on_ruler_and_guide_without_node_hover() {
    let mut h = with_rulers();
    let r = h.rect(None, 0.0, 0.0, 500.0, 500.0);
    seed_page(&mut h, vec![gx(200.0)]);
    h.hover(100.0, 10.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { angle } if angle == 90.0));
    assert_eq!(h.ed.overlay().hover, None, "узел под линейкой не наводится");
    h.hover(10.0, 100.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { angle } if angle == 0.0));
    h.hover(10.0, 10.0);
    assert_eq!(h.ed.cursor(), Cursor::Default, "угол");
    h.hover(200.0, 300.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { angle } if angle == 0.0));
    assert_eq!(h.ed.overlay().hover, None);
    h.hover(260.0, 300.0);
    assert_eq!(h.ed.cursor(), Cursor::Default);
    assert_eq!(h.ed.overlay().hover, Some(r));
}

#[test]
fn ruler_click_does_not_reach_tool_or_clear_selection() {
    let mut h = with_rulers();
    let r = h.rect(None, 0.0, 0.0, 500.0, 500.0);
    h.ed.select(&[r]);
    h.click(100.0, 10.0, NONE);
    assert_eq!(h.sel(), [r], "щелчок по линейке не снимает выделение");
    h.ed.set_tool(ToolKind::Rect);
    let before = h.children(None).len();
    h.drag((100.0, 10.0), (100.0, 15.0), NONE);
    assert_eq!(
        h.children(None).len(),
        before,
        "рисование под линейкой не начинается"
    );
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
}

#[test]
fn rulers_off_or_read_only_means_no_gesture() {
    let mut h = H::new();
    let r = h.rect(None, 0.0, 0.0, 500.0, 500.0);
    h.click(100.0, 10.0, NONE);
    assert_eq!(h.sel(), [r], "без линеек это обычный холст");
    let mut h = with_rulers();
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    h.down(100.0, 10.0, NONE);
    assert!(!h.ed.is_busy());
    h.mv(100.0, 200.0, NONE);
    h.up(100.0, 200.0, NONE);
    assert!(page_guides(&h).is_empty());
    let mut h = with_rulers();
    h.ed.set_view(None, Some(false));
    h.down(100.0, 10.0, NONE);
    assert!(
        !h.ed.is_busy(),
        "скрытые направляющие из линейки не тянутся"
    );
    h.up(100.0, 10.0, NONE);
}

#[test]
fn hidden_guides_are_not_hit_and_do_not_snap() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(303.0)]);
    h.ed.set_view(None, Some(false));
    h.hover(303.0, 400.0);
    assert_eq!(h.ed.cursor(), Cursor::Default);
    h.drag((125.0, 125.0), (327.0, 125.0), NONE);
    assert_eq!(h.props(r).x, 302.0, "без привязки: целые пиксели");
}

// ——— Привязка ———

#[test]
fn moving_node_snaps_edge_and_center_to_guides_within_4_px() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(303.0), gy(407.0)]);
    h.drag((125.0, 125.0), (327.0, 125.0), NONE);
    // Левый край 302 → 303.
    assert_eq!(h.props(r).x, 303.0);
    h.ed.execute(CommandId::Undo);
    // Правый край: 150 + 149 = 299, до линии 4 — на пороге; 5 — уже нет.
    h.drag((125.0, 125.0), (274.0, 125.0), NONE);
    assert_eq!(
        h.props(r).x + h.props(r).w,
        303.0,
        "на самом пороге привязка есть"
    );
    h.ed.execute(CommandId::Undo);
    h.drag((125.0, 125.0), (273.0, 125.0), NONE);
    assert_eq!(h.props(r).x, 248.0, "дальше порога — нет");
    h.ed.execute(CommandId::Undo);
    // По y и центр: центр 125 + 281 = 406 → 407.
    h.drag((125.0, 125.0), (125.0, 406.0), NONE);
    assert_eq!(h.props(r).y + 25.0, 407.0);
}

#[test]
fn snap_threshold_is_in_screen_pixels() {
    let mut h = with_rulers();
    let r = h.rect(None, 250.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(300.0)]);
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(4.0);
    cam.set_center(Point::new(300.0, 125.0));
    h.ed.set_camera(cam);
    let to_screen = |x: f64| 500.0 + (x - 300.0) * 4.0;
    // Хотим левый край на 298 (на экране 8 px от линии): без привязки.
    h.ed.select(&[r]);
    h.drag((to_screen(275.0), 400.0), (to_screen(323.0), 400.0), NONE);
    assert_eq!(h.props(r).x, 298.0);
    h.ed.execute(CommandId::Undo);
    // На 299.5 (2 px на экране): привязка.
    h.drag((to_screen(275.0), 400.0), (to_screen(324.5), 400.0), NONE);
    assert_eq!(h.props(r).x, 300.0);
}

#[test]
fn snapped_position_shows_a_snap_line() {
    let mut h = with_rulers();
    h.rect(None, 100.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(303.0)]);
    h.down(125.0, 125.0, NONE);
    h.mv(327.0, 125.0, NONE);
    let lines = h.ed.overlay().snap_lines;
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!((lines[0].p0.x, lines[0].p1.x), (303.0, 303.0));
    h.up(327.0, 125.0, NONE);
}

#[test]
fn resizing_and_drawing_snap_to_guides() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    seed_page(&mut h, vec![gx(303.0), gy(407.0)]);
    h.ed.select(&[r]);
    // Правая сторона ближе порога к линии.
    h.drag((200.0, 150.0), (301.0, 150.0), NONE);
    assert_eq!(h.props(r).x + h.props(r).w, 303.0);
    h.ed.select(&[]);
    h.ed.set_tool(ToolKind::Rect);
    h.drag((600.0, 300.0), (701.0, 405.0), NONE);
    let made = *h.children(None).last().unwrap();
    assert_eq!(h.props(made).y + h.props(made).h, 407.0, "рисование");
    h.ed.set_tool(ToolKind::Rect);
    h.drag((302.0, 500.0), (340.0, 540.0), NONE);
    let made = *h.children(None).last().unwrap();
    // Начало фигуры — точка нажатия; привязка — у конца.
    assert_eq!(h.props(made).x, 302.0);
}

#[test]
fn frame_guide_snaps_only_opposite_the_frame_and_not_the_frame_itself() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 200.0, 200.0);
    h.ed.set_guides(GuideOwner::Frame(f), vec![gx(50.0)], Phase::Commit)
        .unwrap();
    // Узел напротив фрейма: линия x = 150 действует.
    let r = h.rect(None, 400.0, 150.0, 20.0, 20.0);
    h.drag((410.0, 160.0), (162.0, 160.0), NONE);
    // Узел перенесён: снимем его с родителя фрейма — важен только x.
    assert_eq!(
        h.props(r).x,
        50.0,
        "левый край на линии (в координатах фрейма)"
    );
    h.ed.execute(CommandId::Undo);
    // Ниже фрейма линии нет: края узла свободны, целые пиксели.
    let low = h.rect(None, 400.0, 500.0, 20.0, 20.0);
    h.drag((410.0, 510.0), (162.0, 510.0), NONE);
    assert_eq!(h.props(low).x, 152.0);
    // Сам фрейм к своей линии не привязывается: она едет вместе с ним.
    h.drag((110.0, 110.0), (118.0, 110.0), NONE);
    assert_eq!(h.props(f).x, 108.0);
}

#[test]
fn guide_itself_snaps_to_node_edges_and_other_guides() {
    let mut h = with_rulers();
    h.rect(None, 100.0, 100.0, 50.0, 50.0);
    seed_page(&mut h, vec![gx(400.0)]);
    h.drag((10.0, 500.0), (152.0, 500.0), NONE);
    assert_eq!(page_guides(&h), [gx(400.0), gx(150.0)], "к краю узла");
    h.drag((10.0, 500.0), (402.0, 500.0), NONE);
    let list = page_guides(&h);
    assert_eq!(
        list.len(),
        2,
        "к другой линии — то же место, повтор отброшен"
    );
}

#[test]
fn guide_without_snapping_lands_on_whole_pixels() {
    let mut h = with_rulers();
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(4.0);
    cam.set_center(Point::new(100.0, 100.0));
    h.ed.set_camera(cam);
    h.drag((10.0, 300.0), (503.0, 300.0), NONE);
    assert_eq!(page_guides(&h), [gx(101.0)], "100,75 → 101");
}

// ——— Отмена и страницы ———

#[test]
fn undo_across_page_switch_returns_to_the_page() {
    let mut h = with_rulers();
    let first = h.ed.page();
    h.drag((10.0, 300.0), (400.0, 300.0), NONE);
    let second = h.ed.create_page().unwrap();
    assert!(
        h.ed.overlay().guides.is_empty(),
        "на новой странице своих нет"
    );
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.page(), first);
    assert!(h.ed.overlay().guides.is_empty());
    h.ed.execute(CommandId::Redo);
    assert_eq!(h.ed.overlay().guides.len(), 1);
    h.ed.set_page(second).unwrap();
    assert!(h.ed.overlay().guides.is_empty());
    h.ed.set_page(first).unwrap();
    assert_eq!(h.ed.overlay().guides.len(), 1);
}

#[test]
fn guides_copy_with_duplicate_and_paste_of_a_frame() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 200.0, 200.0);
    h.ed.set_guides(GuideOwner::Frame(f), vec![gx(50.0)], Phase::Commit)
        .unwrap();
    h.ed.select(&[f]);
    h.ed.execute(CommandId::Duplicate);
    let copy = h.sel()[0];
    assert_ne!(copy, f);
    assert_eq!(frame_guides(&mut h, copy), [gx(50.0)]);
    assert_eq!(h.ed.overlay().guides.len(), 2, "и видна сразу");
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.overlay().guides.len(), 1);
    // Удаление фрейма убирает линии, отмена возвращает.
    h.ed.select(&[f]);
    h.ed.execute(CommandId::Delete);
    assert!(h.ed.overlay().guides.is_empty());
    h.ed.execute(CommandId::Undo);
    assert_eq!(h.ed.overlay().guides.len(), 1);
    assert_eq!(frame_guides(&mut h, f), [gx(50.0)]);
}

#[test]
fn only_top_level_frames_show_guides() {
    let mut h = with_rulers();
    let outer = h.frame(None, 0.0, 0.0, 600.0, 600.0);
    let f = h.frame(None, 700.0, 100.0, 200.0, 200.0);
    h.ed.set_guides(GuideOwner::Frame(f), vec![gx(50.0)], Phase::Commit)
        .unwrap();
    assert_eq!(h.ed.overlay().guides.len(), 1);
    h.ed.transact(|tx| {
        tx.move_node(f, Some(outer), 0)?;
        Ok(vec![])
    })
    .unwrap();
    assert!(
        h.ed.overlay().guides.is_empty(),
        "вложенный фрейм линий не показывает"
    );
    assert_eq!(frame_guides(&mut h, f), [gx(50.0)], "данные на месте");
}

// ——— `set_guides` ———

#[test]
fn protocol_preview_commit_and_cancel() {
    let mut h = with_rulers();
    let page = h.ed.page();
    let owner = GuideOwner::Page(page);
    let steps = h.ed.step_count();
    h.ed.set_guides(owner, vec![gx(10.0)], Phase::Preview)
        .unwrap();
    assert_eq!(h.ed.overlay().guides.len(), 1, "превью в кадре");
    assert!(page_guides(&h).is_empty(), "в документе пусто");
    h.ed.set_guides(owner, vec![gx(10.0), gy(5.0)], Phase::Preview)
        .unwrap();
    assert_eq!(h.ed.overlay().guides.len(), 2);
    assert_eq!(h.ed.step_count(), steps);
    h.ed.set_guides(owner, vec![], Phase::Cancel).unwrap();
    assert!(h.ed.overlay().guides.is_empty());
    assert_eq!(h.ed.step_count(), steps);
    assert!(!h.ed.document().can_undo());
    h.ed.set_guides(owner, vec![gx(10.0)], Phase::Preview)
        .unwrap();
    h.ed.set_guides(owner, vec![gx(11.0)], Phase::Commit)
        .unwrap();
    assert_eq!(page_guides(&h), [gx(11.0)]);
    assert_eq!(h.ed.step_count(), steps + 1);
    h.ed.execute(CommandId::Undo);
    assert!(page_guides(&h).is_empty());
}

#[test]
fn protocol_refuses_bad_data_without_a_step() {
    let mut h = with_rulers();
    let page = h.ed.page();
    let r = h.rect(None, 0.0, 0.0, 10.0, 10.0);
    let f = h.frame(None, 0.0, 0.0, 100.0, 100.0);
    let nested = h.frame(Some(f), 0.0, 0.0, 10.0, 10.0);
    let steps = h.ed.step_count();
    let many: Vec<Guide> = (0..=MAX_GUIDES).map(|i| gx(i as f64)).collect();
    for (owner, list) in [
        (GuideOwner::Page(page), many),
        (GuideOwner::Page(page), vec![gx(f64::NAN)]),
        (GuideOwner::Page(page), vec![gy(f64::INFINITY)]),
        (GuideOwner::Page(page), vec![gy(1.1e9)]),
        (GuideOwner::Frame(r), vec![gy(1.0)]),
        (GuideOwner::Frame(nested), vec![gy(1.0)]),
    ] {
        for phase in [Phase::Preview, Phase::Commit] {
            assert!(h.ed.set_guides(owner, list.clone(), phase).is_err());
        }
    }
    assert_eq!(h.ed.step_count(), steps);
    assert!(page_guides(&h).is_empty());
    assert!(
        h.ed.overlay().guides.is_empty(),
        "неверное превью не показано"
    );
}

#[test]
fn limit_on_gesture_reports_error_and_leaves_no_step() {
    let mut h = with_rulers();
    let list: Vec<Guide> = (0..MAX_GUIDES)
        .map(|i| gx(i as f64 * 2.0 + 1000.0))
        .collect();
    seed_page(&mut h, list);
    let steps = h.ed.step_count();
    h.ed.take_events();
    h.drag((10.0, 300.0), (400.0, 300.0), NONE);
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(page_guides(&h).len(), MAX_GUIDES);
    let ev = h.ed.take_events();
    assert!(
        ev.iter().any(|e| matches!(e, EditorEvent::Error(_))),
        "{ev:?}"
    );
}

#[test]
fn held_shortcut_does_not_flicker_the_toggle() {
    let mut h = with_rulers();
    let shift = Modifiers {
        shift: true,
        ..NONE
    };
    let mut e = KeyEvent::new("KeyR", shift);
    e.repeat = true;
    h.ev(Input::KeyDown(e));
    assert!(h.ed.settings().rulers, "повтор клавиши не переключает");
}

#[test]
fn remembered_hover_does_not_outlive_tool_view_or_edit_changes() {
    let mut h = with_rulers();
    seed_page(&mut h, vec![gx(200.0)]);
    let r = h.rect(None, 500.0, 500.0, 60.0, 60.0);
    let resize = |h: &H| matches!(h.ed.cursor(), Cursor::Resize { .. });
    // Инструмент сменился, указатель не двигался.
    h.hover(200.0, 300.0);
    assert!(resize(&h));
    h.ed.set_tool(ToolKind::Rect);
    assert!(
        !resize(&h),
        "у «Прямоугольника» своя стрелка, линия не берётся"
    );
    h.ed.set_tool(ToolKind::Move);
    // Показ выключен.
    h.hover(200.0, 300.0);
    assert!(resize(&h));
    h.ed.set_view(None, Some(false));
    assert!(!resize(&h));
    h.ed.set_view(None, Some(true));
    // Линейка: выключена или направляющие скрыты.
    h.hover(300.0, 10.0);
    assert!(resize(&h));
    h.ed.set_view(None, Some(false));
    assert_eq!(h.ed.cursor(), Cursor::Default, "из линейки не тянется");
    h.ed.set_view(None, Some(true));
    assert!(resize(&h));
    h.ed.set_view(Some(false), None);
    assert!(!resize(&h));
    h.ed.set_view(Some(true), None);
    // В правке вектора нажатие на линейку ничего не тянет — и курсор это говорит.
    h.ed.select(&[r]);
    h.ed.execute(CommandId::EditObject);
    h.hover(300.0, 10.0);
    assert_eq!(h.ed.cursor(), Cursor::Default);
    h.down(300.0, 10.0, NONE);
    assert!(!h.ed.is_busy());
    h.up(300.0, 10.0, NONE);
}

// ——— Раунд правок 1 ———

#[test]
fn guide_of_a_locked_frame_is_not_grabbed() {
    let mut h = with_rulers();
    let f = h.frame(None, 100.0, 100.0, 400.0, 400.0);
    h.ed.set_guides(GuideOwner::Frame(f), vec![gx(50.0)], Phase::Commit)
        .unwrap();
    h.set(
        f,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    let steps = h.ed.step_count();
    h.hover(150.0, 300.0);
    assert!(
        !matches!(h.ed.cursor(), Cursor::Resize { .. }),
        "курсор протяжки у заблокированной линии"
    );
    h.drag((150.0, 200.0), (180.0, 200.0), NONE);
    assert_eq!(frame_guides(&mut h, f), [gx(50.0)]);
    assert!(page_guides(&h).is_empty());
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn hidden_ui_does_not_delete_a_guide_on_the_unseen_ruler() {
    let mut h = with_rulers();
    seed_page(&mut h, vec![gy(150.0)]);
    h.ed.execute(CommandId::ToggleUi);
    h.drag((300.0, 150.0), (300.0, 12.0), NONE);
    assert_eq!(page_guides(&h), [gy(12.0)], "линия перенесена, не удалена");
}

#[test]
fn snap_hint_redraws_when_it_changes_and_clears_with_the_gesture() {
    let mut h = with_rulers();
    h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.down(10.0, 300.0, NONE);
    h.mv(152.0, 300.0, NONE);
    h.ed.take_events();
    h.mv(152.0, 500.0, NONE);
    assert!(h.ed.take_events().contains(&EditorEvent::NeedsRedraw));
    h.up(152.0, 500.0, NONE);
    assert!(h.ed.take_events().contains(&EditorEvent::NeedsRedraw));
    assert!(h.ed.overlay().snap_lines.is_empty());
}

#[test]
fn hiding_guides_mid_drag_cancels_the_gesture() {
    let mut h = with_rulers();
    let steps = h.ed.step_count();
    h.down(300.0, 10.0, NONE);
    h.mv(300.0, 150.0, NONE);
    h.ed.set_view(None, Some(false));
    assert!(!h.ed.is_busy());
    assert!(h.ed.overlay().active_guide.is_none());
    h.up(300.0, 150.0, NONE);
    assert!(page_guides(&h).is_empty());
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn refused_commit_drops_the_preview() {
    let mut h = with_rulers();
    let owner = GuideOwner::Page(h.ed.page());
    h.ed.set_guides(owner, vec![gx(10.0)], Phase::Preview)
        .unwrap();
    assert_eq!(h.ed.overlay().guides.len(), 1);
    assert!(h
        .ed
        .set_guides(owner, vec![gx(f64::NAN)], Phase::Commit)
        .is_err());
    assert!(h.ed.overlay().guides.is_empty(), "превью снято");
}

#[test]
fn cursor_follows_the_release_point_not_the_press_point() {
    let mut h = with_rulers();
    h.hover(10.0, 100.0);
    h.down(10.0, 100.0, NONE);
    h.mv(300.0, 10.0, NONE);
    h.up(300.0, 10.0, NONE);
    assert!(
        matches!(h.ed.cursor(), Cursor::Resize { angle } if angle == 90.0),
        "{:?}",
        h.ed.cursor()
    );
}

#[test]
fn set_view_in_hidden_ui_keeps_a_visible_guide_drag() {
    let mut h = with_rulers();
    seed_page(&mut h, vec![gy(150.0)]);
    h.ed.execute(CommandId::ToggleUi);
    h.down(300.0, 150.0, NONE);
    h.mv(300.0, 200.0, NONE);
    assert!(h.ed.is_busy());
    // Другая вкладка прислала тот же вид: линия на экране, жест идёт.
    h.ed.set_view(Some(true), Some(true));
    assert!(h.ed.is_busy(), "линия видна — тянуть её можно");
    h.up(300.0, 200.0, NONE);
    assert_eq!(page_guides(&h), [gy(200.0)]);
}

#[test]
fn ruler_press_that_starts_no_guide_cancels_a_busy_tool_gesture() {
    let mut h = with_rulers();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.set_view(None, Some(false));
    h.drag_hold((125.0, 125.0), (225.0, 125.0), NONE);
    assert!(h.ed.is_busy());
    // Потерянное отпускание, затем нажатие на линейку, которая линию не начнёт.
    h.down(300.0, 10.0, NONE);
    assert!(!h.ed.is_busy(), "жест инструмента отменён");
    assert_eq!(h.props(r).x, 100.0, "узел вернулся");
    h.up(300.0, 10.0, NONE);
    assert_eq!(h.props(r).x, 100.0, "отпускание потерянный жест не пишет");
}
