//! Состояние команд (спека подсистемы 2, раздел 3): доступна ли команда и
//! отмечена ли — для каждой группы ситуаций; события только с изменениями;
//! недоступная команда ничего не делает.

mod common;

use std::collections::BTreeMap;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

use CommandId as C;

fn on(h: &H, id: CommandId) -> bool {
    h.ed.command_state(id).enabled
}

fn checked(h: &H, id: CommandId) -> Option<bool> {
    h.ed.command_state(id).checked
}

/// Состояния из событий с прошлого вызова (последнее у каждой команды).
/// История без шагов создания стенда.
fn fresh(h: &mut H) {
    h.ed.document_mut_for_tests().clear_history();
}

fn changed(h: &mut H) -> BTreeMap<CommandId, CommandState> {
    let mut out = BTreeMap::new();
    for e in h.ed.take_events() {
        if let EditorEvent::CommandStates(list) = e {
            out.extend(list);
        }
    }
    out
}

#[test]
fn only_toggles_have_a_check_mark() {
    let mut h = text_h();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    let t = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    for sel in [vec![], vec![a], vec![a, t]] {
        h.ed.select(&sel);
        for (c, s) in COMMANDS.iter().zip(h.ed.command_states()) {
            assert_eq!(s.checked.is_some(), c.toggle, "{}", c.key);
            assert_eq!(s, h.ed.command_state(c.id), "{}", c.key);
        }
    }
}

#[test]
fn nothing_selected() {
    let mut h = H::new();
    h.rect(None, 0.0, 0.0, 50.0, 50.0);
    h.ed.select(&[]);
    for c in [
        C::Copy,
        C::Cut,
        C::Duplicate,
        C::Delete,
        C::Group,
        C::Ungroup,
        C::FrameSelection,
        C::Flatten,
        C::OutlineStroke,
        C::BooleanUnion,
        C::BringForward,
        C::SendToBack,
        C::NudgeLeft,
        C::ToggleVisible,
        C::ToggleLock,
        C::FlipHorizontal,
        C::CopyAsPng,
        C::CopyAsSvg,
        C::ZoomToSelection,
        C::EditObject,
        C::TextBold,
        C::TextAlignCenter,
    ] {
        assert!(!on(&h, c), "{c:?} без выделения недоступна");
    }
    for c in [
        C::Paste,
        C::SelectAll,
        C::ZoomToFit,
        C::ZoomIn,
        C::ToolRect,
        C::Escape,
        C::Save,
        C::PixelGrid,
        C::UiMainMenu,
        C::UiSettings,
    ] {
        assert!(on(&h, c), "{c:?} доступна всегда");
    }
}

#[test]
fn one_shape_selected() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a]);
    for c in [
        C::Copy,
        C::Cut,
        C::Duplicate,
        C::Delete,
        C::Group,
        C::FrameSelection,
        C::Flatten,
        C::BringToFront,
        C::NudgeDownBig,
        C::FlipVertical,
        C::ZoomToSelection,
        C::EditObject,
    ] {
        assert!(on(&h, c), "{c:?}");
    }
    // Разгруппировать нечего; булевой операции нужен второй слой; текста нет.
    for c in [C::Ungroup, C::BooleanUnion, C::BooleanExclude, C::TextBold] {
        assert!(!on(&h, c), "{c:?}");
    }
    // Заблокированный — не правится.
    h.set(
        a,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    h.ed.select(&[a]);
    assert!(!on(&h, C::EditObject));
}

#[test]
fn groups_and_booleans() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    let b = h.rect(None, 30.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a, b]);
    assert!(on(&h, C::BooleanUnion) && on(&h, C::BooleanSubtract));
    assert_eq!(checked(&h, C::BooleanUnion), Some(false));
    h.ed.execute(C::BooleanSubtract);
    let g = h.sel()[0];
    // Выделена булева группа: операция отмечена, разгруппировать можно.
    assert_eq!(checked(&h, C::BooleanSubtract), Some(true));
    assert_eq!(checked(&h, C::BooleanUnion), Some(false));
    assert!(on(&h, C::Ungroup) && on(&h, C::BooleanIntersect));
    // Группа вместе с прямоугольником — ничего не отмечено.
    let c = h.rect(None, 200.0, 0.0, 50.0, 50.0);
    h.ed.select(&[g, c]);
    assert_eq!(checked(&h, C::BooleanSubtract), Some(false));
    assert!(on(&h, C::Ungroup));
    h.ed.select(&[c]);
    h.ed.execute(C::Group);
    assert!(on(&h, C::Ungroup), "обычная группа");
    assert!(!on(&h, C::BooleanUnion), "один слой — не булева группа");
}

#[test]
fn history() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    fresh(&mut h);
    h.ed.select(&[a]);
    assert!(!on(&h, C::Undo) && !on(&h, C::Redo));
    h.ed.execute(C::NudgeRight);
    assert!(on(&h, C::Undo) && !on(&h, C::Redo));
    h.ed.execute(C::Undo);
    assert!(on(&h, C::Redo));
}

#[test]
fn text_editing_and_text_selection() {
    let mut h = text_h();
    let t = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.ed.select(&[t]);
    assert!(on(&h, C::TextBold) && on(&h, C::TextAlignJustify));
    assert!(on(&h, C::EditObject));
    h.key("Enter", none());
    assert!(h.ed.is_editing_text());
    assert!(on(&h, C::Copy) && on(&h, C::Cut) && on(&h, C::TextUnderline));
    assert!(!on(&h, C::EditObject), "уже в правке");
}

#[test]
fn vector_editing_keeps_point_commands() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a]);
    assert!(h.ed.start_vector_edit(a));
    assert!(on(&h, C::Delete) && on(&h, C::NudgeLeft) && on(&h, C::Undo));
    assert!(!on(&h, C::EditObject));
}

#[test]
fn read_only_document_disables_edits_silently() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    h.ed.select(&[a]);
    fresh(&mut h);
    h.ed.document_mut_for_tests().force_read_only_for_tests();
    h.ed.select(&[a]);
    let steps = h.ed.step_count();
    for c in [
        C::Delete,
        C::Cut,
        C::Paste,
        C::Duplicate,
        C::Group,
        C::NudgeUp,
        C::ToggleLock,
        C::EditObject,
    ] {
        assert!(!on(&h, c), "{c:?} в документе только для чтения");
    }
    assert!(on(&h, C::Copy) && on(&h, C::CopyAsPng) && on(&h, C::ZoomToSelection));
    // Недоступная команда — ни шага, ни сообщения.
    let before = h.state();
    h.ed.take_events();
    h.key("Delete", none());
    h.ed.execute(C::Duplicate);
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    assert!(h.ed.take_events().is_empty());
}

#[test]
fn disabled_commands_do_nothing() {
    let mut h = H::new();
    h.rect(None, 0.0, 0.0, 50.0, 50.0);
    fresh(&mut h);
    h.ed.select(&[]);
    h.ed.take_events();
    let steps = h.ed.step_count();
    for c in [C::Delete, C::CopyAsPng, C::Group, C::Copy, C::Undo] {
        h.ed.execute(c);
    }
    let ev = h.ed.take_events();
    assert!(ev.is_empty(), "ни событий буфера, ни ошибок: {ev:?}");
    assert_eq!(h.ed.step_count(), steps);
}

#[test]
fn view_and_settings_toggles() {
    let mut h = H::new();
    assert_eq!(checked(&h, C::PixelGrid), Some(true));
    h.ed.execute(C::PixelGrid);
    assert_eq!(checked(&h, C::PixelGrid), Some(false));
    assert_eq!(checked(&h, C::ToggleUi), Some(false));
    h.ed.execute(C::ToggleUi);
    assert_eq!(checked(&h, C::ToggleUi), Some(true));
    assert_eq!(checked(&h, C::UiPixelSnap), Some(true));
    assert_eq!(checked(&h, C::UiThemeDark), Some(true));
    let mut s = h.ed.settings().clone();
    s.pixel_snap = false;
    s.theme = ThemeChoice::System;
    h.ed.set_settings(s);
    assert_eq!(checked(&h, C::UiPixelSnap), Some(false));
    assert_eq!(
        [C::UiThemeDark, C::UiThemeLight, C::UiThemeSystem].map(|c| checked(&h, c)),
        [Some(false), Some(false), Some(true)]
    );
}

/// Инструмент и вид не входят в `StatesKey`: ни одно состояние от них не
/// зависит. Если зависимость появится, свежий расчёт здесь разойдётся с прежним —
/// тогда нужно новое поле в `StatesKey`, иначе события о ней не придут.
#[test]
fn tool_and_view_do_not_change_states() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    fresh(&mut h);
    h.ed.select(&[a]);
    h.ed.take_events();
    let before = h.ed.command_states();
    for t in [
        ToolKind::Rect,
        ToolKind::Ellipse,
        ToolKind::Frame,
        ToolKind::Text,
        ToolKind::Hand,
        ToolKind::Line,
        ToolKind::Arrow,
        ToolKind::Move,
    ] {
        h.ed.set_tool(t);
        assert_eq!(h.ed.command_states(), before, "{t:?}");
    }
    for c in [C::ZoomIn, C::ZoomIn, C::ZoomOut, C::Zoom100, C::ZoomToFit] {
        h.ed.execute(c);
        assert_eq!(h.ed.command_states(), before, "{c:?}");
    }
    assert!(changed(&mut h).is_empty());
}

#[test]
fn events_carry_only_changes_after_steps_that_may_change_them() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 50.0, 50.0);
    fresh(&mut h);
    h.ed.select(&[]);
    h.ed.reset_command_states();
    h.ed.set_hover(None);
    // Первое — все команды таблицы.
    assert_eq!(changed(&mut h).len(), COMMANDS.len());
    // Наведение и движение указателя состояний не меняют — событий нет.
    h.hover(300.0, 300.0);
    h.hover(20.0, 20.0);
    assert!(changed(&mut h).is_empty());
    // Выделение: только то, что стало доступно.
    h.ed.select(&[a]);
    let ch = changed(&mut h);
    assert!(ch.contains_key(&C::Delete) && ch.contains_key(&C::Group));
    assert!(!ch.contains_key(&C::Paste) && !ch.contains_key(&C::ZoomIn));
    assert!(ch.values().all(|s| s.enabled));
    // Жест: во время перетаскивания событий нет, после отпускания — отмена доступна.
    h.down(20.0, 20.0, none());
    h.mv(60.0, 20.0, none());
    assert!(
        changed(&mut h).is_empty(),
        "во время жеста состояния не считаются"
    );
    h.up(60.0, 20.0, none());
    let ch = changed(&mut h);
    assert_eq!(ch.get(&C::Undo).map(|s| s.enabled), Some(true));
    // Вид: отметка пиксельной сетки — одна команда.
    h.ed.execute(C::PixelGrid);
    let ch = changed(&mut h);
    assert_eq!(ch.keys().copied().collect::<Vec<_>>(), vec![C::PixelGrid]);
}
