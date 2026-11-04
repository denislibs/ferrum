//! Линейки и их команды (спека подсистемы 2, раздел 9).

mod common;

use common::*;
use ferrum_doc::{Guide, GuideAxis, GuideOwner};
use ferrum_editor::*;
use kurbo::Point;

fn checked(h: &H, c: CommandId) -> Option<bool> {
    h.ed.command_state(c).checked
}

#[test]
fn rulers_are_off_by_default_and_toggle_by_command_and_shift_r() {
    let mut h = H::new();
    assert!(h.ed.overlay().rulers.is_none());
    assert_eq!(checked(&h, CommandId::ViewRulers), Some(false));
    let steps = h.ed.step_count();
    h.ed.execute(CommandId::ViewRulers);
    assert!(h.ed.overlay().rulers.is_some());
    assert_eq!(checked(&h, CommandId::ViewRulers), Some(true));
    let ev = h.ed.take_events();
    assert!(
        ev.contains(&EditorEvent::ViewChanged {
            rulers: true,
            guides: true
        }),
        "{ev:?}"
    );
    assert!(ev.contains(&EditorEvent::NeedsRedraw));
    assert_eq!(h.ed.step_count(), steps, "настройка вида — не шаг");
    assert!(!h.ed.document().can_undo());
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    h.ed.handle(Input::KeyDown(KeyEvent::new("KeyR", shift)));
    assert!(h.ed.overlay().rulers.is_none());
    // Простая R — по-прежнему прямоугольник.
    h.ed.handle(Input::KeyDown(KeyEvent::new("KeyR", Modifiers::NONE)));
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
    assert!(h.ed.overlay().rulers.is_none());
}

#[test]
fn set_view_does_not_report_back() {
    let mut h = H::new();
    h.ed.take_events();
    h.ed.set_view(Some(true), Some(false));
    assert_eq!(checked(&h, CommandId::ViewRulers), Some(true));
    assert_eq!(checked(&h, CommandId::ViewGuides), Some(false));
    let ev = h.ed.take_events();
    assert!(
        !ev.iter()
            .any(|e| matches!(e, EditorEvent::ViewChanged { .. })),
        "настройка пришла от интерфейса: {ev:?}"
    );
    assert!(ev.contains(&EditorEvent::NeedsRedraw));
}

#[test]
fn guides_command_is_grey_without_rulers_and_hides_marks() {
    let mut h = H::new();
    let page = h.ed.page();
    h.ed.document_mut_for_tests()
        .set_guides(
            page,
            vec![],
            &[(GuideOwner::Page(page), vec![Guide::new(GuideAxis::X, 50.0)])],
        )
        .unwrap();
    h.ed.set_view(None, None);
    assert!(
        h.ed.overlay().guides.is_empty(),
        "как в Figma: без линеек не видны"
    );
    assert!(!h.ed.command_state(CommandId::ViewGuides).enabled);
    let events = h.ed.take_events().len();
    h.ed.execute(CommandId::ViewGuides);
    assert_eq!(
        checked(&h, CommandId::ViewGuides),
        Some(true),
        "серая не выполняется"
    );
    assert!(h.ed.take_events().len() <= events);

    h.ed.execute(CommandId::ViewRulers);
    assert_eq!(h.ed.overlay().guides.len(), 1);
    assert!(h.ed.command_state(CommandId::ViewGuides).enabled);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    h.ed.handle(Input::KeyDown(KeyEvent::new("KeyG", shift)));
    assert_eq!(checked(&h, CommandId::ViewGuides), Some(false));
    assert!(h.ed.overlay().guides.is_empty());
    assert!(h.ed.overlay().rulers.is_some(), "линейки остаются");
}

#[test]
fn origin_is_the_top_level_frame_under_selection_else_page_zero() {
    let mut h = H::new();
    h.ed.set_view(Some(true), None);
    let origin = |h: &H| h.ed.overlay().rulers.unwrap().origin;
    let band = |h: &H| h.ed.overlay().rulers.unwrap().band;
    let f = h.frame(None, 100.0, 50.0, 300.0, 200.0);
    let inner = h.frame(Some(f), 10.0, 10.0, 100.0, 100.0);
    let deep = h.rect(Some(inner), 5.0, 6.0, 20.0, 30.0);
    let loose = h.rect(None, 600.0, 600.0, 40.0, 40.0);
    assert_eq!(origin(&h), Point::ZERO, "без выделения — ноль страницы");
    assert_eq!(band(&h), None);

    h.ed.select(&[deep]);
    assert_eq!(
        origin(&h),
        Point::new(100.0, 50.0),
        "фрейм верхнего уровня, а не вложенный"
    );
    let b = band(&h).unwrap();
    assert_eq!((b.x0, b.y0, b.x1, b.y1), (115.0, 66.0, 135.0, 96.0));

    h.ed.select(&[f]);
    assert_eq!(origin(&h), Point::new(100.0, 50.0), "сам фрейм");

    h.ed.select(&[loose]);
    assert_eq!(origin(&h), Point::ZERO, "узел вне фрейма");

    h.ed.select(&[deep, loose]);
    assert_eq!(origin(&h), Point::ZERO, "разные корни");
    let b = band(&h).unwrap();
    assert_eq!((b.x0, b.x1), (115.0, 640.0), "полоса — общая рамка");

    // Двигаем фрейм — начало отсчёта за ним, а не из кэша.
    h.ed.select(&[deep]);
    h.set(
        f,
        ferrum_doc::PropsPatch {
            x: Some(10.0),
            ..Default::default()
        },
    );
    assert_eq!(origin(&h), Point::new(10.0, 50.0));
    // Повёрнутый фрейм — ноль страницы: оси линейки с его осями не совпадают.
    h.set(
        f,
        ferrum_doc::PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    assert_eq!(origin(&h), Point::ZERO);
}

#[test]
fn hidden_interface_hides_rulers() {
    let mut h = H::new();
    h.ed.set_view(Some(true), None);
    let mut s = h.ed.settings().clone();
    s.ui_hidden = true;
    h.ed.set_settings(s);
    assert!(h.ed.overlay().rulers.is_none());
    assert_eq!(
        checked(&h, CommandId::ViewRulers),
        Some(true),
        "настройка та же"
    );
}

#[test]
fn rulers_in_menus() {
    let menu = main_menu();
    let flat = format!("{menu:?}");
    assert!(flat.contains("ViewRulers") && flat.contains("ViewGuides"));
    let mut h = H::new();
    let items =
        h.ed.context_menu(ContextTarget::Canvas(Point::new(500.0, 500.0)));
    assert!(
        items
            .iter()
            .any(|i| matches!(i, MenuNode::Command(CommandId::ViewRulers))),
        "«Линейки» в меню пустого места (B-121): {items:?}"
    );
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.ed.select(&[r]);
    let items =
        h.ed.context_menu(ContextTarget::Canvas(Point::new(120.0, 120.0)));
    assert!(!items
        .iter()
        .any(|i| matches!(i, MenuNode::Command(CommandId::ViewRulers))));
}

#[test]
fn held_key_does_not_toggle_rulers_or_guides() {
    let mut h = H::new();
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    for (code, cmd) in [
        ("KeyR", CommandId::ViewRulers),
        ("KeyG", CommandId::ViewGuides),
    ] {
        let before = checked(&h, cmd);
        let mut e = KeyEvent::new(code, shift);
        e.repeat = true;
        h.ed.handle(Input::KeyDown(e));
        assert_eq!(checked(&h, cmd), before, "{code}: повтор не переключает");
        h.ed.handle(Input::KeyDown(KeyEvent::new(code, shift)));
        assert_ne!(checked(&h, cmd), before, "{code}: нажатие переключает");
    }
}
