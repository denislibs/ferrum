//! Буфер обмена в правке текста (спека подсистемы 3, раздел 7): копирование кладёт
//! обычный текст и наш формат (`web application/x-ferrum`) — копию из одного
//! текстового узла с выделенным фрагментом («фрагмент текста»: формат тот же,
//! версия прежняя). Вставка в текст — участки со стилями из нашего формата
//! (стиль переводится из умолчаний исходного узла в умолчания правимого), иначе
//! обычный текст в стиле позиции курсора. Обычный текст вне правки — новый
//! текстовый узел в центре видимой части (как в Figma).

use std::sync::Arc;

use ferrum_doc::{
    Fill, NodeKind, Props, TextAttrs, TextContent, TextData, TextRun, TextSizing, TextStyle,
};
use ferrum_text::text_color;
use kurbo::{Point, Vec2};

use crate::clipboard::{encodable, ClipNode, ClipPayload};
use crate::editor::{Editor, EditorEvent};
use crate::geometry::world_box;
use crate::text::{fit_patch, fit_size, TextError};
use crate::text_input::{char_at, clean_text, splice};

/// Заливка текста по умолчанию для разметки участка: сплошная цвета текста узла.
fn default_fill(fills: &[Fill]) -> Fill {
    Fill::solid(text_color(fills))
}

/// Разметка участка со всеми ключами стиля символов: так фрагмент не зависит от
/// умолчаний исходного узла.
fn resolved(a: &TextAttrs, d: &TextStyle, fill: &Fill) -> TextAttrs {
    TextAttrs {
        font: Some(a.font.clone().unwrap_or_else(|| d.font.family.clone())),
        style: Some(a.style.clone().unwrap_or_else(|| d.font.style.clone())),
        size: Some(a.size.unwrap_or(d.size)),
        fill: Some(a.fill.clone().unwrap_or_else(|| fill.clone())),
        line_height: Some(a.line_height.unwrap_or(d.line_height)),
        letter_spacing: Some(a.letter_spacing.unwrap_or(d.letter_spacing)),
        decoration: Some(a.decoration.unwrap_or(d.decoration)),
        case: Some(a.case.unwrap_or(d.case)),
        list: a.list,
        indent: a.indent,
    }
}

/// Обратно: ключи, совпавшие с умолчаниями узла, снимаются.
fn relative(a: &TextAttrs, d: &TextStyle, fill: &Fill) -> TextAttrs {
    let keep = |same: bool| !same;
    TextAttrs {
        font: a.font.clone().filter(|f| keep(*f == d.font.family)),
        style: a.style.clone().filter(|s| keep(*s == d.font.style)),
        size: a.size.filter(|s| keep(*s == d.size)),
        fill: a.fill.clone().filter(|f| keep(f == fill)),
        line_height: a.line_height.filter(|v| keep(*v == d.line_height)),
        letter_spacing: a.letter_spacing.filter(|v| keep(*v == d.letter_spacing)),
        decoration: a.decoration.filter(|v| keep(*v == d.decoration)),
        case: a.case.filter(|v| keep(*v == d.case)),
        list: a.list,
        indent: a.indent,
    }
}

/// Участки текстовых узлов копии подряд, стиль — в осях `target`: у каждого
/// узла последний перевод строки — между узлами, у последнего — отбрасывается.
/// `None` — в копии есть не только тексты.
fn payload_runs(payload: &ClipPayload, target: &Props) -> Option<Vec<TextRun>> {
    let t = target.text.as_ref()?;
    let target_fill = default_fill(&target.fills);
    let mut out: Vec<TextRun> = Vec::new();
    let roots: Vec<&ClipNode> = payload
        .nodes
        .iter()
        .filter(|n| n.parent.is_none())
        .collect();
    if payload.nodes.len() != roots.len() {
        return None;
    }
    for (i, n) in roots.iter().enumerate() {
        let src = n
            .props
            .text
            .as_ref()
            .filter(|_| n.props.kind == NodeKind::Text)?;
        let fill = default_fill(&n.props.fills);
        let len = src.content.char_len();
        let take = if i + 1 == roots.len() {
            len.saturating_sub(1)
        } else {
            len
        };
        for r in src.content.slice(0..take) {
            let full = resolved(&r.attrs, &src.props.style, &fill);
            out.push(TextRun {
                text: r.text,
                attrs: relative(&full, &t.props.style, &target_fill),
            });
        }
    }
    Some(out)
}

/// Участки, разрезанные по переводам строк: у «\n» — заданные список и уровень,
/// у остальных символов абзацной разметки нет.
fn relist(
    runs: &[TextRun],
    list: Option<ferrum_doc::ListKind>,
    indent: Option<u8>,
) -> Vec<TextRun> {
    let mut out = Vec::new();
    for r in runs {
        let plain = TextAttrs {
            list: None,
            indent: None,
            ..r.attrs.clone()
        };
        for (i, piece) in r.text.split('\n').enumerate() {
            if i > 0 {
                out.push(TextRun {
                    text: "\n".into(),
                    attrs: TextAttrs {
                        list,
                        indent,
                        ..plain.clone()
                    },
                });
            }
            if !piece.is_empty() {
                out.push(TextRun {
                    text: piece.to_string(),
                    attrs: plain.clone(),
                });
            }
        }
    }
    out
}

impl Editor {
    /// Копия выделенного фрагмента правимого текста: байты нашего формата и
    /// обычный текст. `None` — выделения нет.
    pub(crate) fn copy_text(&mut self) -> Option<(Vec<u8>, String)> {
        let t = self.text_edit.as_ref()?;
        let range = t.range();
        if range.is_empty() || t.draft.is_some() {
            return None;
        }
        let id = t.id;
        let slot = self.scene.slot(id)?;
        let mut props = self.doc.page(self.page).ok()?.props(id).ok()?;
        let text = props.text.clone()?;
        let fill = default_fill(&props.fills);
        let runs: Vec<TextRun> = text
            .content
            .slice(range)
            .into_iter()
            .map(|r| TextRun {
                attrs: resolved(&r.attrs, &text.props.style, &fill),
                text: r.text,
            })
            .collect();
        let plain: String = runs.iter().map(|r| r.text.as_str()).collect();
        let mut content = TextContent { runs };
        content.normalize();
        // Фрагмент вне правки вставится текстом «авто по ширине» в точке исходного.
        let before = props.clone();
        let mut tp = text.props.clone();
        tp.sizing = TextSizing::AutoWidth;
        props.text = Some(Arc::new(TextData {
            props: tp,
            content: TextContent {
                runs: {
                    let mut r = content.runs.clone();
                    r.push(TextRun {
                        text: "\n".into(),
                        attrs: TextAttrs::default(),
                    });
                    r
                },
            },
        }));
        if let Ok(fit) = fit_patch(&mut self.fonts.borrow_mut(), &before, &props) {
            fit.apply_to(&mut props);
        }
        let n = self.scene.at(slot);
        let origin = n.world * Point::ZERO;
        (props.x, props.y) = (origin.x, origin.y);
        let bounds = world_box(&self.scene, slot).unwrap_or_default();
        let payload = ClipPayload {
            nodes: vec![ClipNode {
                parent: None,
                props,
            }],
            images: Vec::new(),
            bounds,
        };
        if let Err(e) = encodable(&payload) {
            self.out.events.push(EditorEvent::Error(e.to_string()));
            return None;
        }
        Some((payload.encode(), plain))
    }

    /// Вставка нашего формата в правимый текст. `false` — в копии не только
    /// тексты: правка кончается, копия вставляется узлами.
    pub(crate) fn paste_into_text(&mut self, payload: &ClipPayload) -> bool {
        let Some(t) = self.text_edit.as_ref() else {
            return false;
        };
        let id = t.id;
        let range = t.range();
        let before = match &t.draft {
            Some(d) => d.props.clone(),
            None => match self.doc.page(self.page).ok().and_then(|p| p.props(id).ok()) {
                Some(p) => p,
                None => return false,
            },
        };
        let Some(runs) = payload_runs(payload, &before) else {
            return false;
        };
        let Some(content) = before.text.as_ref().map(|x| x.content.clone()) else {
            return false;
        };
        // Каждый «\n» вставки берёт список и уровень разрезаемого абзаца (как при
        // наборе и вставке обычного текста), а не абзаца-источника.
        let runs = if runs.iter().any(|r| r.text.contains('\n')) {
            let end = (range.end..content.char_len())
                .find(|i| char_at(&content, *i) == Some('\n'))
                .unwrap_or(content.char_len().saturating_sub(1));
            let p = content.attrs_at(end);
            let (list, indent) = (p.list, p.indent);
            relist(&runs, list, indent)
        } else {
            runs
        };
        let next = splice(&content, range.clone(), &runs);
        if let Err(e) = next.check_limits() {
            self.out.events.push(EditorEvent::Error(e.to_string()));
            return true;
        }
        let caret = range.start + runs.iter().map(|r| r.text.chars().count()).sum::<usize>();
        let r = self.apply_text_edit(&before, next, range.start, caret, 5, |tx, id| {
            if !range.is_empty() {
                tx.delete_text(id, range.clone())?;
            }
            tx.insert_runs(id, range.start, &runs)
        });
        if let Err(TextError::Pending) = r {
            self.out
                .events
                .push(EditorEvent::Error(TextError::Pending.to_string()));
        }
        self.emit();
        true
    }

    /// Обычный текст из буфера обмена: в правке — в позицию курсора, вне её —
    /// новым текстом «авто по ширине» в центре видимой части (или выделенного
    /// фрейма), одним шагом.
    pub fn paste_plain(&mut self, text: &str) -> Result<(), TextError> {
        if self.text_edit.is_some() {
            return self.text_input(crate::TextInputKind::Paste, text);
        }
        let text = clean_text(text);
        if text.is_empty() {
            return Ok(());
        }
        let content = TextContent::plain(&text);
        content.check_limits()?;
        let mut p = Props::defaults(NodeKind::Text);
        p.text = Some(Arc::new(TextData {
            props: Default::default(),
            content,
        }));
        if let Some((w, h)) = fit_size(&mut self.fonts.borrow_mut(), &p)? {
            (p.w, p.h) = (w, h);
        }
        let (target, target_world, target_box) = self.paste_target();
        let center = target_box.map_or(self.view_center(), |b| b.center());
        let origin = target_world.inverse() * (center - Vec2::new(p.w / 2.0, p.h / 2.0));
        (p.x, p.y) = (origin.x.round(), origin.y.round());
        self.cancel_gesture();
        // Отказ документа (только чтение) `commit` сам отдаёт событием `Error`.
        self.ctx()
            .commit(|tx, _| Ok(vec![tx.create_with_props(target, None, &p)?]));
        self.emit();
        Ok(())
    }
}
