//! Стиль текста из панели «Текст» и сочетаний (спека подсистемы 3, раздел 7):
//! сводка со «Смешанным» (выделенный диапазон в правке, иначе все символы
//! выделенных текстов) и правка — диапазона в правке (у курсора без выделения —
//! стиль следующего ввода), иначе всего текста узлов. Одна правка — один шаг,
//! превью до отпускания (`Phase`, как у полей панели). Размер рамки «авто» —
//! той же правкой.
//!
//! Сочетания в правке: Ctrl/Cmd+B, I, U, Shift+Ctrl/Cmd+X — жирный, курсив,
//! подчёркивание, зачёркивание; Alt+Ctrl/Cmd+L, T, R, J — выравнивание; Tab и
//! Shift+Tab — уровень списка.

use std::ops::Range;
use std::sync::Arc;

use ferrum_doc::{
    AlignH, AlignV, AttrKey, Decoration, DocResult, FontName, LetterSpacing, LineHeight, ListKind,
    NodeId, NodeKind, Props, PropsPatch, TextAttrs, TextCase, TextContent, TextData, TextProps,
    TextSizing, TextStyle, Tx, MAX_LIST_INDENT,
};
use ferrum_text::{style_name::guess_attrs, FontStatus};

use crate::editor::{Editor, EditorEvent};
use crate::input::Modifiers;
use crate::panel::{PanelGesture, Phase};
use crate::text::{fit_patch, TextError};
use crate::text_input::{char_attrs, splice, typing_attrs};

/// Правка стиля символов (и списка абзацев); `None` — не трогать.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStylePatch {
    pub font: Option<FontName>,
    pub size: Option<f64>,
    pub line_height: Option<LineHeight>,
    pub letter_spacing: Option<LetterSpacing>,
    pub decoration: Option<Decoration>,
    pub case: Option<TextCase>,
    pub list: Option<ListKind>,
    /// Уровень списка: прибавка (Tab — +1, Shift+Tab — −1).
    pub indent_by: Option<i8>,
}

/// Правка свойств текстового узла; `None` — не трогать.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextPropsPatch {
    pub sizing: Option<TextSizing>,
    pub align_h: Option<AlignH>,
    pub align_v: Option<AlignV>,
    pub paragraph_spacing: Option<f64>,
    pub paragraph_indent: Option<f64>,
    pub truncate: Option<bool>,
    pub max_lines: Option<Option<u32>>,
}

/// Значение для панели и признак «Смешанное».
#[derive(Debug, Clone, PartialEq)]
pub struct Mixed<T> {
    pub value: T,
    pub mixed: bool,
}

/// Сводка стиля для секции «Текст» (значение первого символа и «Смешанное»).
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyleSummary {
    pub family: Mixed<String>,
    pub style: Mixed<String>,
    pub size: Mixed<f64>,
    pub line_height: Mixed<LineHeight>,
    pub letter_spacing: Mixed<LetterSpacing>,
    pub decoration: Mixed<Decoration>,
    pub case: Mixed<TextCase>,
    pub list: Mixed<ListKind>,
    pub sizing: Mixed<TextSizing>,
    pub align_h: Mixed<AlignH>,
    pub align_v: Mixed<AlignV>,
    pub paragraph_spacing: Mixed<f64>,
    pub paragraph_indent: Mixed<f64>,
    /// Правится диапазон (идёт правка текста), а не узлы целиком.
    pub range: bool,
}

/// Накопитель «первое значение и было ли отличие».
struct Acc<T>(Option<Mixed<T>>);

impl<T: PartialEq + Clone> Acc<T> {
    fn add(&mut self, v: T) {
        match &mut self.0 {
            None => {
                self.0 = Some(Mixed {
                    value: v,
                    mixed: false,
                })
            }
            Some(m) => {
                if m.value != v {
                    m.mixed = true;
                }
            }
        }
    }

    fn get(self, default: T) -> Mixed<T> {
        self.0.unwrap_or(Mixed {
            value: default,
            mixed: false,
        })
    }
}

/// Ключи разметки символов, которые меняет правка.
fn char_keys(p: &TextStylePatch) -> Vec<AttrKey> {
    let mut k = Vec::new();
    if p.font.is_some() {
        k.extend([AttrKey::Font, AttrKey::Style]);
    }
    for (on, key) in [
        (p.size.is_some(), AttrKey::Size),
        (p.line_height.is_some(), AttrKey::LineHeight),
        (p.letter_spacing.is_some(), AttrKey::LetterSpacing),
        (p.decoration.is_some(), AttrKey::Decoration),
        (p.case.is_some(), AttrKey::Case),
    ] {
        if on {
            k.push(key);
        }
    }
    k
}

/// Стиль по умолчанию узла после правки.
fn apply_style(s: &TextStyle, p: &TextStylePatch) -> TextStyle {
    TextStyle {
        font: p.font.clone().unwrap_or_else(|| s.font.clone()),
        size: p.size.unwrap_or(s.size),
        line_height: p.line_height.unwrap_or(s.line_height),
        letter_spacing: p.letter_spacing.unwrap_or(s.letter_spacing),
        decoration: p.decoration.unwrap_or(s.decoration),
        case: p.case.unwrap_or(s.case),
    }
}

/// Разметка участка после правки: ключи правки задаются, а совпавшие с
/// умолчанием узла — снимаются (участок «как у узла»).
fn apply_attrs(a: &TextAttrs, p: &TextStylePatch, d: &TextStyle) -> TextAttrs {
    let mut out = a.clone();
    if let Some(f) = &p.font {
        let same = *f == d.font;
        out.font = (!same).then(|| f.family.clone());
        out.style = (!same).then(|| f.style.clone());
    }
    if let Some(v) = p.size {
        out.size = (v != d.size).then_some(v);
    }
    if let Some(v) = p.line_height {
        out.line_height = (v != d.line_height).then_some(v);
    }
    if let Some(v) = p.letter_spacing {
        out.letter_spacing = (v != d.letter_spacing).then_some(v);
    }
    if let Some(v) = p.decoration {
        out.decoration = (v != d.decoration).then_some(v);
    }
    if let Some(v) = p.case {
        out.case = (v != d.case).then_some(v);
    }
    out
}

/// Разметка перевода строки абзаца после правки списка.
fn apply_list(a: &TextAttrs, p: &TextStylePatch) -> TextAttrs {
    let mut out = a.clone();
    if let Some(l) = p.list {
        if l == ListKind::None {
            out.list = None;
            out.indent = None;
        } else {
            out.list = Some(l);
        }
    }
    if let Some(by) = p.indent_by {
        if out.list.is_some_and(|l| l != ListKind::None) {
            let level = out.indent.unwrap_or(0) as i16 + by as i16;
            let level = level.clamp(0, MAX_LIST_INDENT as i16) as u8;
            out.indent = (level > 0).then_some(level);
        }
    }
    out
}

/// Позиции переводов строк абзацев, которых касается диапазон.
fn paragraph_ends(c: &TextContent, range: Range<usize>) -> Vec<usize> {
    let text: Vec<char> = c.text().chars().collect();
    let mut start = range.start.min(text.len());
    while start > 0 && text[start - 1] != '\n' {
        start -= 1;
    }
    let end = range.end.max(range.start);
    (start..text.len())
        .filter(|i| text[*i] == '\n')
        .take_while(|i| {
            let para_start = (0..*i)
                .rev()
                .find(|j| text[*j] == '\n')
                .map_or(0, |j| j + 1);
            para_start <= end
        })
        .collect()
}

/// Содержимое после правки стиля диапазона `range` (абзацная — у абзацев).
fn restyle(c: &TextContent, range: Range<usize>, p: &TextStylePatch, d: &TextStyle) -> TextContent {
    let mut out = c.clone();
    if !char_keys(p).is_empty() && !range.is_empty() {
        let runs: Vec<_> = c
            .slice(range.clone())
            .into_iter()
            .map(|mut r| {
                r.attrs = apply_attrs(&r.attrs, p, d);
                r
            })
            .collect();
        out = splice(&out, range.clone(), &runs);
    }
    if p.list.is_some() || p.indent_by.is_some() {
        for at in paragraph_ends(c, range) {
            let mut runs = out.slice(at..at + 1);
            for r in &mut runs {
                r.attrs = apply_list(&r.attrs, p);
            }
            out = splice(&out, at..at + 1, &runs);
        }
    }
    out
}

/// Операции документа, дающие `restyle`.
fn restyle_ops(
    tx: &Tx,
    id: NodeId,
    before: &TextContent,
    range: Range<usize>,
    p: &TextStylePatch,
    d: &TextStyle,
) -> DocResult<()> {
    if !char_keys(p).is_empty() && !range.is_empty() {
        let set = apply_attrs(&TextAttrs::default(), p, d);
        let clear: Vec<AttrKey> = char_keys(p).into_iter().filter(|k| !set.has(*k)).collect();
        tx.format_text(id, range.clone(), &set, &clear)?;
    }
    if p.list.is_some() || p.indent_by.is_some() {
        for at in paragraph_ends(before, range) {
            let a = apply_list(before.attrs_at(at), p);
            let mut clear = Vec::new();
            if a.list.is_none() {
                clear.push(AttrKey::List);
            }
            if a.indent.is_none() {
                clear.push(AttrKey::Indent);
            }
            let set = TextAttrs {
                list: a.list,
                indent: a.indent,
                ..Default::default()
            };
            tx.format_text(id, at..at + 1, &set, &clear)?;
        }
    }
    Ok(())
}

/// Имя начертания по весу и курсиву (как у Figma: «Semi Bold Italic»).
fn style_name(weight: f32, italic: bool) -> String {
    let w = match weight.round() as i32 {
        ..=150 => "Thin",
        151..=250 => "Extra Light",
        251..=350 => "Light",
        351..=450 => "Regular",
        451..=550 => "Medium",
        551..=650 => "Semi Bold",
        651..=750 => "Bold",
        751..=850 => "Extra Bold",
        _ => "Black",
    };
    match (w, italic) {
        ("Regular", true) => "Italic".into(),
        (w, true) => format!("{w} Italic"),
        (w, false) => w.into(),
    }
}

impl Editor {
    /// Узлы правки стиля: правимый текст, иначе выделенные тексты (`ids` —
    /// адресат с начала жеста панели).
    fn style_targets(&self, ids: Option<&[NodeId]>) -> Vec<NodeId> {
        if let Some(t) = &self.text_edit {
            return vec![t.id];
        }
        let ids = match ids {
            Some(ids) => self.alive(ids),
            None => self.selection.to_vec(),
        };
        ids.into_iter()
            .filter(|id| {
                self.scene
                    .slot(*id)
                    .is_some_and(|s| self.scene.at(s).props.kind == NodeKind::Text)
            })
            .collect()
    }

    /// Свойства узла в документе (у черновика — его собственные).
    fn doc_props(&mut self, id: NodeId) -> Option<Props> {
        if let Some(t) = &self.text_edit {
            if t.id == id {
                if let Some(d) = &t.draft {
                    return Some(d.props.clone());
                }
            }
        }
        self.doc.page(self.page).ok()?.props(id).ok()
    }

    /// Сводка стиля для секции «Текст»; `None` — текстов нет.
    pub fn text_style_summary(&self) -> Option<TextStyleSummary> {
        let mut family = Acc(None);
        let mut style = Acc(None);
        let mut size = Acc(None);
        let mut lh = Acc(None);
        let mut ls = Acc(None);
        let mut deco = Acc(None);
        let mut case = Acc(None);
        let mut list = Acc(None);
        let mut sizing = Acc(None);
        let mut ah = Acc(None);
        let mut av = Acc(None);
        let mut ps = Acc(None);
        let mut pi = Acc(None);
        let mut add = |a: &TextAttrs, d: &TextStyle| {
            let f = a.font_name(d);
            family.add(f.family);
            style.add(f.style);
            size.add(a.size.unwrap_or(d.size));
            lh.add(a.line_height.unwrap_or(d.line_height));
            ls.add(a.letter_spacing.unwrap_or(d.letter_spacing));
            deco.add(a.decoration.unwrap_or(d.decoration));
            case.add(a.case.unwrap_or(d.case));
        };
        let edit = self.text_edit.as_ref();
        let ids: Vec<NodeId> = match edit {
            Some(t) => vec![t.id],
            None => self.style_targets(None),
        };
        if ids.is_empty() {
            return None;
        }
        for id in &ids {
            let props = match edit.and_then(|t| t.draft.as_ref()) {
                Some(d) => &d.props,
                None => &self.scene.node(*id)?.props,
            };
            let t = props.text.as_ref()?;
            let d = &t.props.style;
            let c = &t.content;
            sizing.add(t.props.sizing);
            ah.add(t.props.align_h);
            av.add(t.props.align_v);
            ps.add(t.props.paragraph_spacing);
            pi.add(t.props.paragraph_indent);
            let range = match edit {
                Some(e) => e.range(),
                None => 0..c.char_len(),
            };
            if range.is_empty() {
                // Курсор: стиль следующего ввода.
                let caret = edit.and_then(|e| e.caret_attrs.as_ref());
                add(&typing_attrs(c, range.clone(), caret), d);
            } else {
                let mut any = false;
                for r in c.slice(range.clone()) {
                    if r.text.chars().any(|ch| ch != '\n') {
                        add(&r.attrs, d);
                        any = true;
                    }
                }
                if !any {
                    add(&char_attrs(c.attrs_at(range.start)), d);
                }
            }
            for at in paragraph_ends(c, range) {
                list.add(c.attrs_at(at).list.unwrap_or(ListKind::None));
            }
        }
        let d = TextStyle::default();
        let tp = TextProps::default();
        Some(TextStyleSummary {
            family: family.get(d.font.family.clone()),
            style: style.get(d.font.style.clone()),
            size: size.get(d.size),
            line_height: lh.get(d.line_height),
            letter_spacing: ls.get(d.letter_spacing),
            decoration: deco.get(d.decoration),
            case: case.get(d.case),
            list: list.get(ListKind::None),
            sizing: sizing.get(tp.sizing),
            align_h: ah.get(tp.align_h),
            align_v: av.get(tp.align_v),
            paragraph_spacing: ps.get(0.0),
            paragraph_indent: pi.get(0.0),
            range: edit.is_some(),
        })
    }

    /// Правка стиля текста (панель «Текст», сочетания): в правке — выделенного
    /// диапазона (у курсора — стиль следующего ввода), иначе — всего текста
    /// узлов. Превью — до `Commit`, одна запись — один шаг. Шрифт ещё грузится
    /// — `Err(Pending)` при записи (ядро повторит), превью пропускается.
    pub fn edit_text_style(
        &mut self,
        patch: &TextStylePatch,
        phase: Phase,
        ids: Option<&[NodeId]>,
    ) -> Result<(), TextError> {
        if phase == Phase::Cancel {
            self.cancel_text_preview();
            self.emit();
            return Ok(());
        }
        // Курсор без выделения: стиль следующего ввода, документ не меняется.
        let caret_only = self
            .text_edit
            .as_ref()
            .is_some_and(|t| t.range().is_empty() && t.draft.is_none());
        if caret_only && !char_keys(patch).is_empty() && phase == Phase::Commit {
            let content = self.edit_content().unwrap_or_default();
            let t = self.text_edit.as_mut().expect("правка идёт");
            let base = typing_attrs(&content, t.range(), t.caret_attrs.as_ref());
            let d = self.edit_props_style().unwrap_or_default();
            let t = self.text_edit.as_mut().expect("правка идёт");
            t.caret_attrs = Some(apply_attrs(&base, patch, &d));
            if patch.list.is_none() && patch.indent_by.is_none() {
                self.emit();
                return Ok(());
            }
        }
        let mut edits: Vec<(NodeId, Props, Props, Option<Range<usize>>)> = Vec::new();
        for id in self.style_targets(ids) {
            let Some(before) = self.doc_props(id) else {
                continue;
            };
            let Some(t) = before.text.as_ref() else {
                continue;
            };
            let edit = self.text_edit.as_ref().filter(|e| e.id == id);
            let (props, content, range) = match edit {
                // Новый текст (черновик): стиль узла целиком, без документа.
                Some(e) if e.draft.is_some() => {
                    let mut p = t.props.clone();
                    p.style = apply_style(&p.style, patch);
                    let c = restyle(
                        &t.content,
                        0..t.content.char_len(),
                        &TextStylePatch {
                            font: None,
                            size: None,
                            line_height: None,
                            letter_spacing: None,
                            decoration: None,
                            case: None,
                            ..patch.clone()
                        },
                        &p.style,
                    );
                    (p, c, None)
                }
                Some(e) => {
                    let r = e.range();
                    let char_patch = if r.is_empty() {
                        TextStylePatch {
                            list: patch.list,
                            indent_by: patch.indent_by,
                            ..Default::default()
                        }
                    } else {
                        patch.clone()
                    };
                    let c = restyle(&t.content, r.clone(), &char_patch, &t.props.style);
                    (t.props.clone(), c, Some(r))
                }
                None => {
                    // Узел целиком: умолчание узла, а разметка этих ключей снимается.
                    let mut p = t.props.clone();
                    p.style = apply_style(&p.style, patch);
                    let all = 0..t.content.char_len();
                    let c = restyle(&t.content, all, patch, &p.style);
                    (p, c, None)
                }
            };
            let mut after = before.clone();
            after.text = Some(Arc::new(TextData { props, content }));
            edits.push((id, before, after, range));
        }
        self.apply_text_changes(edits, phase, patch)
    }

    /// Правка свойств текстовых узлов (режим размера, выравнивание, интервалы).
    pub fn edit_text_props(
        &mut self,
        patch: &TextPropsPatch,
        phase: Phase,
        ids: Option<&[NodeId]>,
    ) -> Result<(), TextError> {
        if phase == Phase::Cancel {
            self.cancel_text_preview();
            self.emit();
            return Ok(());
        }
        let mut edits = Vec::new();
        for id in self.style_targets(ids) {
            let Some(before) = self.doc_props(id) else {
                continue;
            };
            let Some(t) = before.text.as_ref() else {
                continue;
            };
            let mut p = t.props.clone();
            if let Some(v) = patch.sizing {
                p.sizing = v;
            }
            if let Some(v) = patch.align_h {
                p.align_h = v;
            }
            if let Some(v) = patch.align_v {
                p.align_v = v;
            }
            if let Some(v) = patch.paragraph_spacing {
                p.paragraph_spacing = v;
            }
            if let Some(v) = patch.paragraph_indent {
                p.paragraph_indent = v;
            }
            if let Some(v) = patch.truncate {
                p.truncate = v;
            }
            if let Some(v) = patch.max_lines {
                p.max_lines = v;
            }
            let mut after = before.clone();
            after.text = Some(Arc::new(TextData {
                props: p,
                content: t.content.clone(),
            }));
            edits.push((id, before, after, None));
        }
        self.apply_text_changes(edits, phase, &TextStylePatch::default())
    }

    fn edit_props_style(&self) -> Option<TextStyle> {
        let t = self.text_edit.as_ref()?;
        match &t.draft {
            Some(d) => d.props.text.as_ref().map(|x| x.props.style.clone()),
            None => self
                .scene
                .node(t.id)?
                .props
                .text
                .as_ref()
                .map(|x| x.props.style.clone()),
        }
    }

    /// Превью или запись правок текстов: свойства, содержимое и размер рамки.
    fn apply_text_changes(
        &mut self,
        edits: Vec<(NodeId, Props, Props, Option<Range<usize>>)>,
        phase: Phase,
        patch: &TextStylePatch,
    ) -> Result<(), TextError> {
        let mut patches: Vec<(NodeId, PropsPatch, Props, Option<Range<usize>>)> = Vec::new();
        let mut pending = false;
        for (id, before, after, range) in edits {
            let fit = match fit_patch(&mut self.fonts.borrow_mut(), &before, &after) {
                Ok(f) => f,
                Err(_) => {
                    pending = true;
                    PropsPatch::default()
                }
            };
            let (bt, at) = (before.text.as_ref(), after.text.as_ref());
            let p = PropsPatch {
                text: (bt.map(|t| &t.props) != at.map(|t| &t.props))
                    .then(|| at.map(|t| t.props.clone()))
                    .flatten(),
                content: (bt.map(|t| &t.content) != at.map(|t| &t.content))
                    .then(|| at.map(|t| t.content.clone()))
                    .flatten(),
                ..fit
            };
            if p != PropsPatch::default() {
                patches.push((id, p, before, range));
            }
        }
        // Черновик правится сам по себе, без документа и превью.
        if let Some(t) = self.text_edit.as_mut() {
            if let Some(d) = t.draft.as_mut() {
                if let Some((_, p, _, _)) = patches.iter().find(|x| x.0 == t.id) {
                    if phase == Phase::Commit {
                        p.apply_to(&mut d.props);
                    }
                }
                let mut preview = d.props.clone();
                if phase == Phase::Preview {
                    if let Some((_, p, _, _)) = patches.iter().find(|x| x.0 == t.id) {
                        p.apply_to(&mut preview);
                    }
                }
                let (id, parent) = (t.id, d.parent);
                self.scene.set_drafts(&[ferrum_scene::Draft {
                    id,
                    parent,
                    props: preview,
                }]);
                self.emit();
                return Ok(());
            }
        }
        match phase {
            Phase::Cancel => unreachable!("отмена — выше"),
            Phase::Preview => {
                if pending {
                    return Ok(());
                }
                let list: Vec<(NodeId, PropsPatch)> = patches
                    .iter()
                    .map(|(id, p, _, _)| (*id, p.clone()))
                    .collect();
                self.panel = Some(PanelGesture::default());
                self.scene.set_preview(&list);
                self.emit();
                Ok(())
            }
            Phase::Commit => {
                self.cancel_text_preview();
                if pending {
                    return Err(TextError::Pending);
                }
                if patches.is_empty() {
                    self.emit();
                    return Ok(());
                }
                let selection = self.selection.to_vec();
                let style = patch.clone();
                // Отказ документа `commit` сам отдаёт событием `Error`.
                self.ctx().commit(|tx, _| {
                    for (id, p, before, range) in &patches {
                        let t = before.text.as_ref().expect("текст");
                        match range {
                            // Диапазон в правке — разметкой (позиции Loro живы у
                            // других участников), без замены содержимого.
                            Some(r) => {
                                restyle_ops(
                                    tx,
                                    *id,
                                    &t.content,
                                    r.clone(),
                                    &style,
                                    &t.props.style,
                                )?;
                                tx.set_props(
                                    *id,
                                    &PropsPatch {
                                        content: None,
                                        ..p.clone()
                                    },
                                )?;
                            }
                            None if p.content.is_some() && p.text.is_some() => {
                                let d = &p.text.as_ref().expect("проверено").style;
                                let all = 0..t.content.char_len();
                                tx.set_props(
                                    *id,
                                    &PropsPatch {
                                        content: None,
                                        ..p.clone()
                                    },
                                )?;
                                restyle_ops(tx, *id, &t.content, all, &style, d)?;
                            }
                            None => tx.set_props(*id, p)?,
                        }
                    }
                    Ok(selection)
                });
                // Ввод после правки стиля — новый шаг.
                if let Some(t) = self.text_edit.as_mut() {
                    t.merge = None;
                }
                self.emit();
                Ok(())
            }
        }
    }

    /// Убирает превью правки стиля (и черновик возвращает в сцену).
    fn cancel_text_preview(&mut self) {
        if self.panel.take().is_some() {
            self.scene.clear_preview();
        }
        self.restore_draft();
    }

    /// Сочетания форматирования в правке. Возвращает, обработано ли.
    pub(crate) fn text_format_key(&mut self, code: &str, mods: Modifiers) -> bool {
        let platform = self.settings.platform;
        let primary = mods.primary(platform);
        let result = match code {
            "KeyB" | "KeyI" if primary && !mods.alt && !mods.shift => {
                let Some(s) = self.text_style_summary() else {
                    return true;
                };
                let (w, italic) = guess_attrs(&s.style.value);
                let (w, italic) = if code == "KeyB" {
                    (if w >= 600.0 { 400.0 } else { 700.0 }, italic)
                } else {
                    (w, !italic)
                };
                let font = FontName::new(s.family.value.clone(), style_name(w, italic));
                // Начертания нет в семействе — ничего (как в Figma).
                if self.fonts.borrow().status(&font) == FontStatus::Missing {
                    return true;
                }
                self.edit_text_style(
                    &TextStylePatch {
                        font: Some(font),
                        ..Default::default()
                    },
                    Phase::Commit,
                    None,
                )
            }
            "KeyU" | "KeyX" if primary && !mods.alt && (code == "KeyU") != mods.shift => {
                let want = if code == "KeyU" {
                    Decoration::Underline
                } else {
                    Decoration::Strike
                };
                let on = self
                    .text_style_summary()
                    .is_some_and(|s| s.decoration.value == want && !s.decoration.mixed);
                self.edit_text_style(
                    &TextStylePatch {
                        decoration: Some(if on { Decoration::None } else { want }),
                        ..Default::default()
                    },
                    Phase::Commit,
                    None,
                )
            }
            "KeyL" | "KeyT" | "KeyR" | "KeyJ" if primary && mods.alt && !mods.shift => {
                let align = match code {
                    "KeyL" => AlignH::Left,
                    "KeyT" => AlignH::Center,
                    "KeyR" => AlignH::Right,
                    _ => AlignH::Justify,
                };
                self.edit_text_props(
                    &TextPropsPatch {
                        align_h: Some(align),
                        ..Default::default()
                    },
                    Phase::Commit,
                    None,
                )
            }
            "Tab" if !primary && !mods.alt => {
                // Уровень списка; вне списка Tab ничего не делает.
                self.edit_text_style(
                    &TextStylePatch {
                        indent_by: Some(if mods.shift { -1 } else { 1 }),
                        ..Default::default()
                    },
                    Phase::Commit,
                    None,
                )
            }
            _ => return false,
        };
        if let Err(e) = result {
            self.out.events.push(EditorEvent::Error(e.to_string()));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_follow_figma() {
        assert_eq!(style_name(400.0, false), "Regular");
        assert_eq!(style_name(400.0, true), "Italic");
        assert_eq!(style_name(700.0, true), "Bold Italic");
        assert_eq!(style_name(600.0, false), "Semi Bold");
    }

    #[test]
    fn paragraphs_touched_by_a_range() {
        let c = TextContent::plain("аб\nвг\nде");
        assert_eq!(paragraph_ends(&c, 1..1), vec![2]);
        assert_eq!(paragraph_ends(&c, 1..4), vec![2, 5]);
        assert_eq!(paragraph_ends(&c, 6..8), vec![8]);
    }
}
