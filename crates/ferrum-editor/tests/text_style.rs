//! Стиль текста из панели и сочетаний (подсистема 3, этап 2): сводка со
//! «Смешанным», правка всего текста узлов и диапазона в правке, превью до
//! отпускания, стиль следующего ввода у курсора, жирный, подчёркивание,
//! выравнивание, списки и режим размера.

mod common;

use common::text::*;
use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn content(h: &mut H, id: NodeId) -> TextContent {
    h.props(id).text.unwrap().content.clone()
}

fn size(v: f64) -> TextStylePatch {
    TextStylePatch {
        size: Some(v),
        ..Default::default()
    }
}

/// «Привет мир», «При» — жирным.
fn mixed_text(h: &mut H) -> NodeId {
    let id = add_text(h, 100.0, 100.0, "Привет мир", 24.0);
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
    id
}

#[test]
fn summary_marks_mixed_values() {
    let mut h = text_h();
    mixed_text(&mut h);
    let s = h.ed.text_style_summary().unwrap();
    assert_eq!(s.family.value, "Inter");
    assert!(!s.family.mixed);
    assert!(s.style.mixed, "жирный и обычный — «Смешанное»");
    assert_eq!(s.size.value, 24.0);
    assert!(!s.size.mixed);
    assert!(!s.range);
    // В правке — только выделенный диапазон.
    h.key("Enter", none());
    h.key("Home", none());
    h.key("ArrowRight", shift());
    let s = h.ed.text_style_summary().unwrap();
    assert!(s.range && !s.style.mixed);
    assert_eq!(s.style.value, "Bold");
    // Ничего не выделено (не текст) — сводки нет.
    h.key("Escape", none());
    h.ed.select(&[]);
    assert!(h.ed.text_style_summary().is_none());
}

#[test]
fn whole_text_edit_sets_the_default_and_clears_marks_in_one_step() {
    let mut h = text_h();
    let id = mixed_text(&mut h);
    let w0 = h.props(id).w;
    let steps = h.ed.step_count();
    let regular = TextStylePatch {
        font: Some(FontName::new("Inter", "Regular")),
        size: Some(32.0),
        ..Default::default()
    };
    h.ed.edit_text_style(&regular, Phase::Commit, None).unwrap();
    let p = h.props(id);
    let t = p.text.as_ref().unwrap();
    assert_eq!(t.props.style.size, 32.0);
    assert_eq!(t.content.runs.len(), 1, "разметка жирного снята");
    assert!(p.w > w0, "«авто по ширине» выросла той же правкой");
    assert_eq!(h.ed.step_count(), steps + 1);
    h.ed.execute(CommandId::Undo);
    assert_eq!(
        content(&mut h, id).attrs_at(0).style.as_deref(),
        Some("Bold")
    );
    assert_eq!(h.props(id).w, w0);
}

#[test]
fn range_edit_marks_only_the_selection_with_preview_until_commit() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет мир", 24.0);
    h.key("Enter", none());
    h.key("Home", none());
    for _ in 0..3 {
        h.key("ArrowRight", shift());
    }
    let steps = h.ed.step_count();
    h.ed.edit_text_style(&size(40.0), Phase::Preview, None)
        .unwrap();
    assert_eq!(
        content(&mut h, id).attrs_at(0).size,
        None,
        "в документе — нет"
    );
    let scene = h.ed.scene().node(id).unwrap().props.text.clone().unwrap();
    assert_eq!(scene.content.attrs_at(0).size, Some(40.0));
    h.ed.edit_text_style(&size(40.0), Phase::Cancel, None)
        .unwrap();
    assert!(!h.ed.scene().has_preview());
    h.ed.edit_text_style(&size(40.0), Phase::Commit, None)
        .unwrap();
    let c = content(&mut h, id);
    assert_eq!(c.attrs_at(2).size, Some(40.0));
    assert_eq!(c.attrs_at(3).size, None);
    assert_eq!(h.ed.step_count(), steps + 1);
    assert_eq!(sel_range(&h), (0, 3), "выделение остаётся");
    // Значение, равное умолчанию узла, снимает разметку.
    h.ed.edit_text_style(&size(24.0), Phase::Commit, None)
        .unwrap();
    assert_eq!(content(&mut h, id).runs.len(), 1);
}

#[test]
fn caret_style_applies_to_the_next_typing() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "ab", 24.0);
    h.key("Enter", none());
    h.key("End", none());
    let steps = h.ed.step_count();
    h.key("KeyB", ctrl());
    assert_eq!(h.ed.step_count(), steps, "у курсора документ не меняется");
    assert_eq!(h.ed.text_style_summary().unwrap().style.value, "Bold");
    h.ed.text_input(TextInputKind::Insert, "c").unwrap();
    let c = content(&mut h, id);
    assert_eq!(c.attrs_at(2).style.as_deref(), Some("Bold"));
    assert_eq!(c.attrs_at(1).style, None);
}

#[test]
fn shortcuts_toggle_bold_underline_strike_and_align() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    h.key("Enter", none());
    h.key("KeyB", ctrl());
    assert_eq!(
        content(&mut h, id).attrs_at(0).style.as_deref(),
        Some("Bold")
    );
    h.key("KeyB", ctrl());
    assert_eq!(
        content(&mut h, id).runs.len(),
        1,
        "снова обычный — как у узла"
    );
    h.key("KeyI", ctrl());
    assert_eq!(
        content(&mut h, id).attrs_at(0).style.as_deref(),
        Some("Italic")
    );
    h.key("KeyU", ctrl());
    assert_eq!(
        content(&mut h, id).attrs_at(0).decoration,
        Some(Decoration::Underline)
    );
    h.key("KeyX", ctrl().with_shift());
    assert_eq!(
        content(&mut h, id).attrs_at(0).decoration,
        Some(Decoration::Strike)
    );
    h.key("KeyT", ctrl().with_alt());
    assert_eq!(h.props(id).text.unwrap().props.align_h, AlignH::Center);
    assert!(h.ed.is_editing_text());
}

#[test]
fn lists_apply_to_paragraphs_and_tab_changes_the_level() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "один\nдва\nтри", 24.0);
    h.key("Enter", none());
    h.key("Home", ctrl());
    h.key("ArrowDown", shift());
    h.ed.edit_text_style(
        &TextStylePatch {
            list: Some(ListKind::Bullet),
            ..Default::default()
        },
        Phase::Commit,
        None,
    )
    .unwrap();
    let c = content(&mut h, id);
    assert_eq!(c.attrs_at(4).list, Some(ListKind::Bullet));
    assert_eq!(c.attrs_at(8).list, Some(ListKind::Bullet));
    assert_eq!(c.attrs_at(12).list, None, "третий абзац не задет");
    h.key("Tab", none());
    assert_eq!(content(&mut h, id).attrs_at(4).indent, Some(1));
    h.key("Tab", shift());
    assert_eq!(content(&mut h, id).attrs_at(4).indent, None);
    let s = h.ed.text_style_summary().unwrap();
    assert_eq!(s.list.value, ListKind::Bullet);
}

#[test]
fn sizing_mode_back_to_auto_refits_the_frame() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "Привет", 24.0);
    let w0 = h.props(id).w;
    h.ed.edit_selection(&PanelEdit::Size { w: 400.0, h: 200.0 }, Phase::Commit);
    assert_eq!(h.props(id).w, 400.0);
    h.ed.edit_text_props(
        &TextPropsPatch {
            sizing: Some(TextSizing::AutoWidth),
            ..Default::default()
        },
        Phase::Commit,
        None,
    )
    .unwrap();
    let p = h.props(id);
    assert_eq!(p.text.unwrap().props.sizing, TextSizing::AutoWidth);
    assert_eq!(p.w, w0);
}

#[test]
fn new_text_takes_the_style_chosen_before_typing() {
    let mut h = text_h();
    h.key("KeyT", none());
    h.click(200.0, 150.0, none());
    h.ed.edit_text_style(&size(40.0), Phase::Commit, None)
        .unwrap();
    assert_eq!(h.ed.step_count(), 0, "черновик — без документа");
    assert_eq!(h.ed.text_style_summary().unwrap().size.value, 40.0);
    h.ed.text_input(TextInputKind::Insert, "x").unwrap();
    let id = info(&h).id;
    assert_eq!(h.props(id).text.unwrap().props.style.size, 40.0);
}

#[test]
fn style_change_at_the_caret_breaks_the_typing_merge() {
    let mut h = text_h();
    let id = add_text(&mut h, 100.0, 100.0, "", 24.0);
    h.key("Enter", none());
    let steps = h.ed.step_count();
    h.ed.text_input(TextInputKind::Insert, "a").unwrap();
    h.ed.text_input(TextInputKind::Insert, "b").unwrap();
    assert_eq!(h.ed.step_count(), steps + 1, "обычный ввод склеивается");
    h.key("KeyB", ctrl());
    h.ed.text_input(TextInputKind::Insert, "c").unwrap();
    assert_eq!(h.ed.step_count(), steps + 2, "другой стиль — новый шаг");
    h.ed.text_input(TextInputKind::Insert, "d").unwrap();
    assert_eq!(h.ed.step_count(), steps + 2, "тот же стиль — склеивается");
    let c = content(&mut h, id);
    assert_eq!(c.attrs_at(1).style, None);
    assert_eq!(c.attrs_at(2).style.as_deref(), Some("Bold"));
    assert_eq!(c.attrs_at(3).style.as_deref(), Some("Bold"));
}
