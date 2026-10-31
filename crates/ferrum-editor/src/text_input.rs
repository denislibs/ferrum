//! Ввод в текст (спека подсистемы 3, раздел 7): набор, абзац, удаление по
//! кластерам, словам и строкам, вставка обычного текста, составление IME
//! (превью в сцене подчёркнуто, в документе его нет), отмена и повтор в правке,
//! новый текст инструментом T.
//!
//! Каждая правка — одна транзакция вместе с размером рамки по раскладке
//! (`AutoWidth`, `AutoHeight`); ввод подряд — один шаг отмены (`transact_merging`):
//! шаг рвут пауза больше секунды, другая позиция курсора и другой стиль.
//! Пустой текст при выходе из правки удаляется без своего шага: новый (черновик
//! инструмента T) в документ так и не попадает, а опустевший — удаляется той же
//! склейкой, что опустошила его.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use ferrum_doc::{
    DocResult, NodeId, Props, PropsPatch, TextAttrs, TextContent, TextData, TextMerge, TextRun, Tx,
};
use ferrum_scene::Draft;
use ferrum_text::{next_word_end, prev_word_start, Caret};

use crate::editor::{Editor, EditorEvent};
use crate::input::{Modifiers, Platform};
use crate::text::{fit_patch, TextError};
use crate::text_edit::{DraftText, TextEdit};
use crate::tools::sync;

/// Вид ввода из поля (`InputEvent.inputType` и события составления).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputKind {
    /// Набор (`insertText`, `insertReplacementText`).
    Insert,
    /// Новый абзац (`insertParagraph`, `insertLineBreak`).
    Paragraph,
    /// Вставка обычного текста (`insertFromPaste`, событие `paste` без данных Ferrum).
    Paste,
    /// Составление IME (`compositionupdate`): только превью.
    Compose,
    /// Конец составления (`compositionend`): текст — в документ.
    Commit,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteLineBackward,
    DeleteLineForward,
}

/// Содержимое с заменой диапазона участками.
pub(crate) fn splice(c: &TextContent, range: Range<usize>, runs: &[TextRun]) -> TextContent {
    let len = c.char_len();
    let mut out = TextContent {
        runs: c.slice(0..range.start),
    };
    out.runs.extend(runs.iter().cloned());
    out.runs.extend(c.slice(range.end.min(len)..len));
    out.normalize();
    out
}

/// Символ в позиции (`None` — за концом).
pub(crate) fn char_at(c: &TextContent, at: usize) -> Option<char> {
    let mut pos = 0;
    for r in &c.runs {
        let n = r.text.chars().count();
        if at < pos + n {
            return r.text.chars().nth(at - pos);
        }
        pos += n;
    }
    None
}

/// Разметка символов без абзацной (список и уровень — на переводе строки).
pub(crate) fn char_attrs(a: &TextAttrs) -> TextAttrs {
    TextAttrs {
        list: None,
        indent: None,
        ..a.clone()
    }
}

/// Стиль набора: заданный для курсора, иначе стиль заменяемого (первого
/// выделенного) символа, иначе символа слева, а в начале абзаца — справа.
pub(crate) fn typing_attrs(
    c: &TextContent,
    range: Range<usize>,
    caret: Option<&TextAttrs>,
) -> TextAttrs {
    if let Some(a) = caret {
        return char_attrs(a);
    }
    let at = range.start;
    let src = if !range.is_empty() {
        at
    } else if at > 0 && char_at(c, at - 1) != Some('\n') {
        at - 1
    } else {
        at
    };
    char_attrs(c.attrs_at(src))
}

/// Отпечаток стиля для склейки шагов ввода: вид правки и разметка.
fn style_print(kind: u8, attrs: &TextAttrs) -> u64 {
    let mut h = DefaultHasher::new();
    kind.hash(&mut h);
    format!("{attrs:?}").hash(&mut h);
    h.finish()
}

/// Текст из буфера обмена: переводы строк Windows и старого macOS — `\n`,
/// прочие управляющие символы (кроме табуляции) выбрасываются.
pub(crate) fn clean_text(s: &str) -> String {
    s.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect()
}

/// Правка не черновика: черновик создаётся своим шагом.
fn draft_free(t: &TextEdit) -> bool {
    t.draft.is_none()
}

impl Editor {
    /// Ввод из поля во время правки текста. Шрифты ещё грузятся —
    /// `Err(TextError::Pending)`, ничего не изменено (ядро повторит).
    pub fn text_input(&mut self, kind: TextInputKind, data: &str) -> Result<(), TextError> {
        if self.text_edit.is_none() {
            return Ok(());
        }
        let r = match kind {
            TextInputKind::Insert | TextInputKind::Commit | TextInputKind::Paste => {
                if kind == TextInputKind::Commit {
                    self.clear_composition();
                }
                let text = clean_text(data);
                if text.is_empty() {
                    Ok(())
                } else {
                    self.replace_selection(&text, false)
                }
            }
            TextInputKind::Paragraph => self.replace_selection("\n", true),
            TextInputKind::Compose => {
                self.compose(data);
                Ok(())
            }
            TextInputKind::DeleteBackward
            | TextInputKind::DeleteForward
            | TextInputKind::DeleteWordBackward
            | TextInputKind::DeleteWordForward
            | TextInputKind::DeleteLineBackward
            | TextInputKind::DeleteLineForward => self.delete(kind),
        };
        if let Some(t) = self.text_edit.as_mut() {
            t.blink_from = self.now_ms;
        }
        self.emit();
        r
    }

    /// Свойства правимого узла в документе (у черновика — его собственные).
    fn edit_doc_props(&mut self) -> Option<Props> {
        let t = self.text_edit.as_ref()?;
        if let Some(d) = &t.draft {
            return Some(d.props.clone());
        }
        let id = t.id;
        self.doc.page(self.page).ok()?.props(id).ok()
    }

    /// Заменяет выделение текстом `text` одной правкой (набор и вставка подряд —
    /// один шаг). `paragraph` — новый абзац: перевод строки получает список и
    /// уровень текущего абзаца.
    fn replace_selection(&mut self, text: &str, paragraph: bool) -> Result<(), TextError> {
        let Some(before) = self.edit_doc_props() else {
            return Ok(());
        };
        let Some(content) = before.text.as_ref().map(|t| t.content.clone()) else {
            return Ok(());
        };
        let t = self.text_edit.as_ref().expect("правка идёт");
        let range = t.range();
        let attrs = typing_attrs(&content, range.clone(), t.caret_attrs.as_ref());
        // Новые абзацы (любой `\n` во вставке) наследуют список и уровень абзаца,
        // который разрезают.
        let runs = if text.contains('\n') {
            let end = (range.end..content.char_len())
                .find(|i| char_at(&content, *i) == Some('\n'))
                .unwrap_or(content.char_len().saturating_sub(1));
            let p = content.attrs_at(end);
            let newline = TextAttrs {
                list: p.list,
                indent: p.indent,
                ..attrs.clone()
            };
            let mut runs = Vec::new();
            for (i, piece) in text.split('\n').enumerate() {
                if i > 0 {
                    runs.push(TextRun {
                        text: "\n".into(),
                        attrs: newline.clone(),
                    });
                }
                if !piece.is_empty() {
                    runs.push(TextRun {
                        text: piece.to_string(),
                        attrs: attrs.clone(),
                    });
                }
            }
            runs
        } else {
            vec![TextRun {
                text: text.to_string(),
                attrs: attrs.clone(),
            }]
        };
        let next = splice(&content, range.clone(), &runs);
        next.check_limits()?;
        let added = text.chars().count();
        let caret = range.start + added;
        let merge_kind = if paragraph { 2 } else { 1 };
        self.apply_text_edit(
            &before,
            next,
            range.start,
            caret,
            style_print(merge_kind, &attrs),
            |tx, id| {
                if !range.is_empty() {
                    tx.delete_text(id, range.clone())?;
                }
                tx.insert_runs(id, range.start, &runs)
            },
        )
    }

    /// Удаление выделения или куска перед курсором (за ним) по кластеру, слову
    /// или строке.
    fn delete(&mut self, kind: TextInputKind) -> Result<(), TextError> {
        let Some(before) = self.edit_doc_props() else {
            return Ok(());
        };
        let Some(content) = before.text.as_ref().map(|t| t.content.clone()) else {
            return Ok(());
        };
        let last = content.char_len().saturating_sub(1);
        let t = self.text_edit.as_ref().expect("правка идёт");
        let mut range = t.range();
        let focus = t.focus;
        if range.is_empty() {
            let at = focus.index;
            // Backspace в начале пункта списка снимает список, а не склеивает абзацы.
            if kind == TextInputKind::DeleteBackward
                && (at == 0 || char_at(&content, at - 1) == Some('\n'))
            {
                let end = (at..content.char_len())
                    .find(|i| char_at(&content, *i) == Some('\n'))
                    .unwrap_or(last);
                let p = content.attrs_at(end);
                if p.list.is_some_and(|l| l != ferrum_doc::ListKind::None) {
                    return self.unlist(&before, end);
                }
            }
            let Some(layout) = self.edit_layout() else {
                return Ok(());
            };
            let chars: Vec<char> = content.text().chars().collect();
            range = match kind {
                TextInputKind::DeleteBackward => layout.prev_stop(at)..at,
                TextInputKind::DeleteForward => at..layout.next_stop(at).min(last),
                TextInputKind::DeleteWordBackward => prev_word_start(&chars, at)..at,
                TextInputKind::DeleteWordForward => at..next_word_end(&chars, at).min(last),
                TextInputKind::DeleteLineBackward => {
                    let (a, _) = layout.line_bounds(focus).unwrap_or_default();
                    // В начале строки — как Backspace (склейка с прошлой).
                    if a.index == at {
                        layout.prev_stop(at)..at
                    } else {
                        a.index..at
                    }
                }
                TextInputKind::DeleteLineForward => {
                    let (_, b) = layout.line_bounds(focus).unwrap_or_default();
                    if b.index == at {
                        at..layout.next_stop(at).min(last)
                    } else {
                        at..b.index.min(last)
                    }
                }
                _ => return Ok(()),
            };
        }
        if range.is_empty() {
            return Ok(());
        }
        let next = splice(&content, range.clone(), &[]);
        let backward = matches!(
            kind,
            TextInputKind::DeleteBackward
                | TextInputKind::DeleteWordBackward
                | TextInputKind::DeleteLineBackward
        );
        let (from, caret) = if backward {
            (focus.index.max(range.end), range.start)
        } else {
            (range.start, range.start)
        };
        self.apply_text_edit(
            &before,
            next,
            from,
            caret,
            style_print(if backward { 3 } else { 4 }, &TextAttrs::default()),
            |tx, id| tx.delete_text(id, range.clone()),
        )
    }

    /// Снимает список с абзаца, кончающегося переводом строки `newline`.
    fn unlist(&mut self, before: &Props, newline: usize) -> Result<(), TextError> {
        let content = before.text.as_ref().expect("текст").content.clone();
        let mut next = content.clone();
        let mut runs = next.slice(newline..newline + 1);
        for r in &mut runs {
            r.attrs.list = None;
            r.attrs.indent = None;
        }
        next = splice(&next, newline..newline + 1, &runs);
        let caret = self.text_edit.as_ref().expect("правка идёт").focus.index;
        self.apply_text_edit(before, next, caret, caret, 0, |tx, id| {
            tx.format_text(
                id,
                newline..newline + 1,
                &TextAttrs::default(),
                &[ferrum_doc::AttrKey::List, ferrum_doc::AttrKey::Indent],
            )
        })
    }

    /// Правка текста одной транзакцией: `edit` меняет содержимое (`next` — каким
    /// оно станет: по нему считается размер рамки до транзакции), размер — той же
    /// транзакцией. Ввод подряд склеивается в один шаг. Черновик инструмента T
    /// становится узлом документа с первой правкой.
    pub(crate) fn apply_text_edit(
        &mut self,
        before: &Props,
        next: TextContent,
        from: usize,
        caret: usize,
        style: u64,
        edit: impl FnOnce(&Tx, NodeId) -> DocResult<()>,
    ) -> Result<(), TextError> {
        let mut after = before.clone();
        let text = before.text.as_ref().expect("текст");
        after.text = Some(Arc::new(TextData {
            props: text.props.clone(),
            content: next.clone(),
        }));
        let fit = fit_patch(&mut self.fonts.borrow_mut(), before, &after)?;
        let t = self.text_edit.as_ref().expect("правка идёт");
        let id = t.id;
        // Метка склейки — по узлу правки; у нового текста — по черновику до конца
        // сеанса (id узла появится только в транзакции).
        let merge = TextMerge {
            node: t.merge.map_or(id, |m| m.node),
            before: from,
            after: caret,
            style,
            at_ms: self.now_ms,
        };
        // Склейка — только с собственной прошлой правкой этого же сеанса правки.
        let merged = t.merge_steps == self.out.steps
            && t.merge.is_some_and(|prev| prev.continues(&merge))
            && draft_free(t);
        if !merged {
            self.doc.break_text_merge();
        }
        let draft = t.draft.clone();
        self.scene.clear_preview();
        let selection = self.selection.to_vec();
        let page = self.page;
        let result = match &draft {
            // Первый символ нового текста: узел создаётся той же правкой.
            Some(d) => self
                .doc
                .transact_merging(page, selection, merge, |tx| {
                    let mut props = after.clone();
                    fit.apply_to(&mut props);
                    let id = tx.create_with_props(d.parent, None, &props)?;
                    Ok(vec![id])
                })
                .map(|ids| ids[0]),
            None => self
                .doc
                .transact_merging(page, selection, merge, |tx| {
                    edit(tx, id)?;
                    if fit != PropsPatch::default() {
                        tx.set_props(id, &fit)?;
                    }
                    Ok(vec![id])
                })
                .map(|_| id),
        };
        let changed = sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        let id = match result {
            Ok(id) => id,
            Err(e) => {
                self.out.events.push(EditorEvent::Error(e.to_string()));
                return Err(TextError::Doc(e));
            }
        };
        if changed && !merged {
            self.out.steps += 1;
        }
        self.selection.set(vec![id]);
        let steps = self.out.steps;
        let t = self.text_edit.as_mut().expect("правка идёт");
        t.id = id;
        t.draft = None;
        t.anchor = caret;
        t.focus = Caret::at(caret);
        t.goal_x = None;
        t.merge = Some(merge);
        t.merge_steps = steps;
        Ok(())
    }

    /// Превью составляемого IME текста: в сцене (подчёркнуто оверлеем), не в
    /// документе. Пустое — превью убирается.
    fn compose(&mut self, data: &str) {
        let Some(before) = self.edit_doc_props() else {
            return;
        };
        let text = clean_text(data).replace('\n', " ");
        let t = self.text_edit.as_mut().expect("правка идёт");
        if text.is_empty() {
            t.composition = None;
            self.scene.clear_preview();
            self.restore_draft();
            return;
        }
        t.composition = Some(text.clone());
        let range = t.range();
        let Some(content) = before.text.as_ref().map(|x| x.content.clone()) else {
            return;
        };
        let attrs = typing_attrs(&content, range.clone(), t.caret_attrs.as_ref());
        let next = splice(&content, range, &[TextRun { text, attrs }]);
        let mut after = before.clone();
        let td = before.text.as_ref().expect("текст");
        after.text = Some(Arc::new(TextData {
            props: td.props.clone(),
            content: next.clone(),
        }));
        // Шрифт ещё грузится — превью без нового размера.
        let fit = fit_patch(&mut self.fonts.borrow_mut(), &before, &after).unwrap_or_default();
        let id = t.id;
        self.scene.set_preview(&[(
            id,
            PropsPatch {
                content: Some(next),
                ..fit
            },
        )]);
    }

    /// Составление кончилось: превью убирается (текст придёт вводом `Commit`).
    fn clear_composition(&mut self) {
        if let Some(t) = self.text_edit.as_mut() {
            if t.composition.take().is_some() {
                self.scene.clear_preview();
                self.restore_draft();
            }
        }
    }

    /// Черновик инструмента T снова в сцене (`clear_preview` убирает и черновики).
    pub(crate) fn restore_draft(&mut self) {
        if let Some(t) = &self.text_edit {
            if let Some(d) = &t.draft {
                self.scene.set_drafts(&[Draft {
                    id: t.id,
                    parent: d.parent,
                    props: d.props.clone(),
                }]);
            }
        }
    }

    /// Новый текст инструментом T: черновик в сцене и правка с курсором в нём.
    /// В документ он попадёт с первым символом.
    pub(crate) fn start_draft_text(&mut self, parent: Option<NodeId>, props: Props) {
        self.finish_text_edit();
        // Документ только для чтения: нового текста не будет (как у `start_text_edit`).
        if self.doc.is_read_only() {
            return;
        }
        let id = self.ctx().draft_id();
        let mut props = props;
        // Размер пустой строки (у «авто») — по раскладке; шрифт грузится — как есть.
        if let Ok(patch) = fit_patch(&mut self.fonts.borrow_mut(), &props, &props) {
            patch.apply_to(&mut props);
        }
        self.scene.set_drafts(&[Draft {
            id,
            parent,
            props: props.clone(),
        }]);
        self.selection.clear();
        let mut t = TextEdit::new(id, 0, 0, self.now_ms);
        t.draft = Some(DraftText { parent, props });
        self.text_edit = Some(t);
        self.emit();
    }

    /// Заканчивает правку: превью и черновик убираются; опустевший текст
    /// удаляется склейкой с правкой, что его опустошила (без своего шага).
    pub(crate) fn finish_text_edit(&mut self) {
        let Some(t) = self.text_edit.take() else {
            return;
        };
        self.text_hover = false;
        self.scene.clear_preview();
        if t.draft.is_some() {
            return;
        }
        let empty = self
            .doc
            .page(self.page)
            .ok()
            .and_then(|p| p.props(t.id).ok())
            .and_then(|p| p.text.map(|x| x.content.char_len() <= 1))
            .unwrap_or(false);
        if !empty {
            return;
        }
        let page = self.page;
        let id = t.id;
        let result = match t.merge.filter(|_| t.merge_steps == self.out.steps) {
            Some(m) => self.doc.transact_merging(
                page,
                vec![id],
                TextMerge {
                    before: m.after,
                    ..m
                },
                |tx| {
                    tx.delete_nodes(&[id])?;
                    Ok(vec![])
                },
            ),
            None => {
                self.out.steps += 1;
                self.doc.transact(page, vec![id], |tx| {
                    tx.delete_nodes(&[id])?;
                    Ok(vec![])
                })
            }
        };
        sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        match result {
            Ok(_) => self.selection.clear(),
            Err(e) => self.out.events.push(EditorEvent::Error(e.to_string())),
        }
    }

    /// Отмена и повтор в правке: правка продолжается, если текст остался.
    pub(crate) fn text_undo_redo(&mut self, redo: bool) {
        let Some(id) = self.text_edit.as_ref().map(|t| t.id) else {
            return;
        };
        let draft = self.text_edit.as_ref().is_some_and(|t| t.draft.is_some());
        if draft {
            // Черновик ещё не в документе: отмена — к прошлым правкам.
            self.finish_text_edit();
            self.undo_redo(redo);
            return;
        }
        // Превью IME — не правка: убрать до отмены.
        self.clear_composition();
        let edit = self.text_edit.take();
        self.undo_redo(redo);
        if self.scene.slot(id).is_some() {
            self.text_edit = edit;
            self.selection.set(vec![id]);
            if let Some(t) = self.text_edit.as_mut() {
                t.merge = None;
            }
            self.clamp_text_edit();
        }
    }

    /// Клавиши удаления и отмены в правке (`text_key`).
    pub(crate) fn text_edit_key(&mut self, code: &str, mods: Modifiers) -> bool {
        let platform = self.settings.platform;
        let mac = platform == Platform::Mac;
        let primary = mods.primary(platform);
        match code {
            "Backspace" | "Delete" => {
                let back = code == "Backspace";
                let kind = if mac && mods.meta {
                    if back {
                        TextInputKind::DeleteLineBackward
                    } else {
                        TextInputKind::DeleteLineForward
                    }
                } else if (mac && mods.alt) || (!mac && mods.ctrl) {
                    if back {
                        TextInputKind::DeleteWordBackward
                    } else {
                        TextInputKind::DeleteWordForward
                    }
                } else if back {
                    TextInputKind::DeleteBackward
                } else {
                    TextInputKind::DeleteForward
                };
                // Ошибку документа `apply_text_edit` уже отправила событием; шрифт ещё
                // грузится — удаление не применено, и пользователь видит причину.
                if let Err(e @ TextError::Pending) = self.text_input(kind, "") {
                    self.out.events.push(EditorEvent::Error(e.to_string()));
                }
                true
            }
            "KeyZ" if primary && !mods.alt => {
                self.text_undo_redo(mods.shift);
                true
            }
            "KeyY" if !mac && mods.ctrl && !mods.shift && !mods.alt => {
                self.text_undo_redo(true);
                true
            }
            _ => false,
        }
    }
}
