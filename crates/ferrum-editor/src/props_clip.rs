//! «Копировать свойства» и «Вставить свойства» (спека подсистемы 2, раздел 6):
//! заливки, обводки, стиль обводки, прозрачность, скругление и стиль текста.
//! Копия кладётся во внутренний буфер редактора и в системный — текстом с
//! меткой `PROPS_MARKER`: base64 того же формата, что у копии узлов (один узел
//! без детей и с теми изображениями, на которые ссылаются его заливки). Вставка
//! берёт свойства из системного буфера — копии свойств или копии узлов (свойства
//! её первого узла, как в Figma), — а без них из внутреннего буфера и применяет к
//! каждому выделенному то, что подходит его типу, одним шагом. Эффектов и режимов
//! наложения в модели пока нет.

use std::sync::Arc;

use ferrum_doc::{
    base64_decode, base64_encode, NodeId, NodeKind, Props, PropsPatch, TextContent, TextData,
};
use kurbo::Rect;

use crate::clipboard::{encodable, ClipNode, ClipPayload, ClipboardError};
use crate::editor::{Editor, EditorEvent};
use crate::panel::{field_applies, NumField};
use crate::text::TextError;
use crate::text_style::TextStylePatch;

/// Метка копии свойств в `text/plain`.
pub const PROPS_MARKER: &str = "ferrum-props:v1:";
/// Метка копии узлов текстом (запасной путь буфера обмена интерфейса).
pub const NODES_MARKER: &str = "ferrum-clipboard:v1:";
/// Предел текста свойств или узлов из буфера: base64 предела вставки.
pub const MAX_PROPS_TEXT: usize = crate::clipboard::MAX_PAYLOAD / 3 * 4 + 64;

/// Нечего вставить: свойства не копировали.
pub const NO_PROPS: &str = "Сначала скопируйте свойства";

/// Свойства для буфера: без геометрии, имени, детей и сети; у текста — стиль
/// первого участка как стиль узла и пустое содержимое (переносится только стиль).
fn strip(p: &Props) -> Props {
    let mut out = Props::defaults(p.kind);
    out.opacity = p.opacity;
    out.corner_radius = p.corner_radius;
    out.fills = p.fills.clone();
    out.strokes = p.strokes.clone();
    out.stroke_style = p.stroke_style;
    out.boolean_op = p.boolean_op;
    if let Some(t) = &p.text {
        let mut props = t.props.clone();
        let d = props.style.clone();
        if let Some(a) = t.content.runs.first().map(|r| &r.attrs) {
            props.style.font = a.font_name(&d);
            props.style.size = a.size.unwrap_or(d.size);
            props.style.line_height = a.line_height.unwrap_or(d.line_height);
            props.style.letter_spacing = a.letter_spacing.unwrap_or(d.letter_spacing);
            props.style.decoration = a.decoration.unwrap_or(d.decoration);
            props.style.case = a.case.unwrap_or(d.case);
            // Видимый цвет — у участка: он и копируется (как в Figma).
            if let Some(fill) = &a.fill {
                out.fills = vec![fill.clone()];
            }
        }
        out.text = Some(Arc::new(TextData {
            props,
            content: TextContent::plain(""),
        }));
    }
    out
}

/// Есть ли у узла этого типа заливки и обводки (и стиль обводки).
fn has_fills(kind: NodeKind) -> bool {
    !matches!(kind, NodeKind::Group | NodeKind::Line)
}

fn has_strokes(kind: NodeKind) -> bool {
    kind != NodeKind::Group
}

fn has_stroke_style(kind: NodeKind) -> bool {
    !matches!(kind, NodeKind::Group | NodeKind::Text)
}

/// Правка узла типа `kind` свойствами `src`: только то, что есть у обоих типов.
pub(crate) fn props_patch(src: &Props, kind: NodeKind) -> PropsPatch {
    let mut p = PropsPatch {
        opacity: Some(src.opacity),
        ..Default::default()
    };
    if has_fills(src.kind) && has_fills(kind) {
        p.fills = Some(src.fills.clone());
    }
    if has_strokes(src.kind) && has_strokes(kind) {
        p.strokes = Some(src.strokes.clone());
    }
    if has_stroke_style(src.kind) && has_stroke_style(kind) {
        let s = src.stroke_style;
        p.stroke_cap_start = Some(s.cap_start);
        p.stroke_cap_end = Some(s.cap_end);
        p.stroke_join = Some(s.join);
        p.stroke_miter_limit = Some(s.miter_limit);
    }
    if field_applies(NumField::CornerRadius, src.kind)
        && field_applies(NumField::CornerRadius, kind)
    {
        p.corner_radius = Some(src.corner_radius);
    }
    p
}

/// Стиль текста-источника для всего текста цели.
fn text_patch(src: &Props) -> Option<TextStylePatch> {
    let s = &src.text.as_ref()?.props.style;
    Some(TextStylePatch {
        font: Some(s.font.clone()),
        size: Some(s.size),
        line_height: Some(s.line_height),
        letter_spacing: Some(s.letter_spacing),
        decoration: Some(s.decoration),
        case: Some(s.case),
        ..Default::default()
    })
}

/// Байты копии из текста буфера: метка свойств или узлов и base64; иначе `None`.
pub fn props_text_bytes(text: &str) -> Option<Vec<u8>> {
    if text.len() > MAX_PROPS_TEXT {
        return None;
    }
    let body = text
        .strip_prefix(PROPS_MARKER)
        .or_else(|| text.strip_prefix(NODES_MARKER))?;
    base64_decode(body.trim())
}

impl Editor {
    /// «Копировать свойства»: свойства первого выделенного — во внутренний буфер и
    /// текстом с меткой для системного (событие `ClipboardProps`).
    pub(crate) fn copy_props(&mut self) {
        let Some(&id) = self.selection.first() else {
            return;
        };
        let Some(props) = self.doc.page(self.page).ok().and_then(|p| p.props(id).ok()) else {
            return;
        };
        let props = strip(&props);
        let images: Vec<(String, Vec<u8>)> = {
            let mut hashes: Vec<&String> = props
                .fills
                .iter()
                .filter_map(|f| f.image_hash.as_ref())
                .collect();
            hashes.sort();
            hashes.dedup();
            hashes
                .into_iter()
                .filter_map(|h| Some((h.clone(), self.doc.images.get(h)?.to_vec())))
                .collect()
        };
        let payload = ClipPayload {
            nodes: vec![ClipNode {
                parent: None,
                props: props.clone(),
            }],
            images,
            bounds: Rect::ZERO,
        };
        if let Err(e) = encodable(&payload) {
            self.out.events.push(EditorEvent::Error(e.to_string()));
            return;
        }
        self.props_clip = Some(props);
        let text = format!("{PROPS_MARKER}{}", base64_encode(&payload.encode()));
        self.out.events.push(EditorEvent::ClipboardProps(text));
    }

    /// «Вставить свойства» к выделению одним шагом. `clip` — байты копии из
    /// системного буфера (свойств или узлов: берётся первый узел), без них —
    /// внутренний буфер. Данные не доверенные: при ошибке документ не меняется,
    /// причина — событием `Error`. Шрифт стиля текста ещё грузится — ничего не
    /// сделано, `take_text_pending` вернёт `true` (ядро повторит).
    pub fn paste_props(&mut self, clip: Option<&[u8]>) {
        self.begin_doc_edit();
        let (src, images) = match clip {
            Some(bytes) => match ClipPayload::decode(bytes) {
                Ok(p) => {
                    let first = p.nodes.into_iter().next().map(|n| strip(&n.props));
                    (first, p.images)
                }
                Err(e) => {
                    self.out.events.push(EditorEvent::Error(e.to_string()));
                    self.emit();
                    return;
                }
            },
            None => (self.props_clip.clone(), Vec::new()),
        };
        let Some(src) = src else {
            let message = match clip {
                Some(_) => ClipboardError::Empty.to_string(),
                None => NO_PROPS.to_string(),
            };
            self.out.events.push(EditorEvent::Error(message));
            self.emit();
            return;
        };
        let ids: Vec<NodeId> = self.alive(&self.selection);
        let extra: Vec<(NodeId, PropsPatch)> = ids
            .iter()
            .filter_map(|id| Some((*id, props_patch(&src, self.scene.node(*id)?.props.kind))))
            .collect();
        // Все тексты цели: стиль — если источник текст, цвета участков — всегда.
        let texts: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.scene
                    .node(*id)
                    .is_some_and(|n| n.props.kind == NodeKind::Text)
            })
            .collect();
        let before = self.out.steps;
        let patch = text_patch(&src).unwrap_or_default();
        match self.restyle_nodes_with(&texts, &patch, extra) {
            Ok(()) => {}
            Err(TextError::Pending) => {
                self.out.text_pending = true;
                return;
            }
            // Отказ документа `commit` уже отдал событием.
            Err(TextError::Doc(_)) => return,
        }
        if self.out.steps != before {
            // Изображения заливок — в документ, когда свойства легли. Только те,
            // на которые ссылаются перенесённые заливки: остальные картинки копии
            // узлов никому не нужны.
            for (hash, data) in images {
                let used = src
                    .fills
                    .iter()
                    .any(|f| f.image_hash.as_ref() == Some(&hash));
                if used && self.doc.images.get(&hash).is_none() {
                    self.doc.images.add(data);
                }
            }
        }
    }
}
