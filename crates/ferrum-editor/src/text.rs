//! Текстовые узлы в редакторе (подсистема 3, этап 1): размер по раскладке в
//! той же транзакции, что и правка текста (спека 5), создание текста (тестовая
//! команда — инструмента T нет до этапа 2), отсутствующие шрифты и их замена
//! одним шагом (спека 4), ресайз текста рамкой.
//!
//! Раскладке нужны байты шрифтов. Если какой-то шрифт ещё грузится, правка не
//! делается: `TextPending` — ядро повторит её, когда шрифты придут.

use std::sync::Arc;

use ferrum_doc::{
    DocResult, FontName, NodeId, NodeKind, Props, PropsPatch, TextContent, TextData, TextProps,
    TextSizing, Tx,
};
use ferrum_text::{FontStatus, Fonts, LayoutRequest, SharedFonts};
use kurbo::Point;

use crate::editor::Editor;

/// Правка ждёт шрифтов: их байты запрошены (`Fonts::take_requests`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPending;

/// Правка одного текста при замене шрифта: новые свойства (если сменилось
/// умолчание), содержимое и размер по новой раскладке (у авторазмера).
struct FontEdit {
    id: NodeId,
    props: Option<TextProps>,
    content: TextContent,
    size: Option<(f64, f64)>,
}

/// Отсутствующий шрифт текущей страницы и сколько текстов им набрано.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingFont {
    pub font: FontName,
    pub nodes: usize,
}

/// Размер рамки по раскладке: `AutoWidth` — ширина и высота текста, `AutoHeight`
/// — высота при ширине рамки, `Fixed` — без изменений (`None`). Шрифт грузится —
/// `Err(TextPending)`.
pub fn fit_size(fonts: &mut Fonts, p: &Props) -> Result<Option<(f64, f64)>, TextPending> {
    let Some(t) = &p.text else {
        return Ok(None);
    };
    if t.props.sizing == TextSizing::Fixed {
        return Ok(None);
    }
    let layout = fonts.layout(&LayoutRequest::for_node(t, &p.fills, p.w, p.h));
    if layout.loading {
        return Err(TextPending);
    }
    // Ширина и высота — целыми вверх, как у рамки текста в Figma.
    let h = layout.height.ceil();
    Ok(Some(match t.props.sizing {
        TextSizing::AutoWidth => (layout.width.ceil(), h),
        _ => (p.w, h),
    }))
}

/// Ресайз текста рамкой (ручки, поля W/H, масштаб группы) переводит его в
/// `Fixed`: размер больше не по тексту. Этап 2 добавит правила Figma для
/// горизонтального ресайза (`AutoHeight`) вместе с режимом размера в панели.
pub(crate) fn sizing_after_resize(before: &Props, patch: &PropsPatch) -> PropsPatch {
    let Some(t) = &before.text else {
        return patch.clone();
    };
    let resized = patch.w.is_some_and(|w| w != before.w) || patch.h.is_some_and(|h| h != before.h);
    if !resized || t.props.sizing == TextSizing::Fixed || patch.text.is_some() {
        return patch.clone();
    }
    PropsPatch {
        text: Some(TextProps {
            sizing: TextSizing::Fixed,
            ..t.props.clone()
        }),
        ..patch.clone()
    }
}

/// `set_props` с правилом ресайза текста (`sizing_after_resize`).
pub(crate) fn set_props_resizing(tx: &Tx, id: NodeId, patch: &PropsPatch) -> DocResult<()> {
    let before = tx.props(id)?;
    tx.set_props(id, &sizing_after_resize(&before, patch))
}

/// Новый шрифт участков после замены `from` → `to`: участок, набранный `from`
/// (с учётом умолчания узла), получает `to`; остальные не меняют вида, даже
/// если умолчание узла сменилось.
fn replaced_content(
    content: &TextContent,
    old_default: &TextProps,
    new_default: &TextProps,
    from: &FontName,
    to: &FontName,
) -> TextContent {
    let mut out = content.clone();
    for run in &mut out.runs {
        let before = run.attrs.font_name(&old_default.style);
        let wanted = if &before == from { to.clone() } else { before };
        let after = run.attrs.font_name(&new_default.style);
        if after != wanted {
            run.attrs.font = Some(wanted.family.clone());
            run.attrs.style = Some(wanted.style.clone());
        }
        // Разметка, совпавшая с умолчанием узла, снимается: участок «как у узла».
        if run.attrs.font.as_deref() == Some(new_default.style.font.family.as_str())
            && run.attrs.style.as_deref() == Some(new_default.style.font.style.as_str())
        {
            run.attrs.font = None;
            run.attrs.style = None;
        }
    }
    // Соседние участки с одинаковой разметкой — один участок.
    let mut merged: Vec<ferrum_doc::TextRun> = Vec::with_capacity(out.runs.len());
    for run in out.runs {
        match merged.last_mut() {
            Some(last) if last.attrs == run.attrs => last.text.push_str(&run.text),
            _ => merged.push(run),
        }
    }
    TextContent { runs: merged }
}

impl Editor {
    /// Общий реестр шрифтов ядра (тот же у рендера).
    pub fn set_fonts(&mut self, fonts: SharedFonts) {
        self.fonts = fonts;
    }

    pub fn fonts(&self) -> &SharedFonts {
        &self.fonts
    }

    /// Текстовый узел в точке мира `at` (левый верхний угол) на странице, одним
    /// шагом отмены, с размером по раскладке; узел выделяется. Нужно тестам и
    /// стенду — инструмент T появится на этапе 2. `width` — ширина рамки для
    /// `AutoHeight` и `Fixed`, `height` — высота для `Fixed`.
    pub fn create_text(
        &mut self,
        at: Point,
        content: &TextContent,
        props: &TextProps,
        width: f64,
        height: f64,
    ) -> Result<Option<NodeId>, TextPending> {
        let mut p = Props::defaults(NodeKind::Text);
        (p.x, p.y, p.w, p.h) = (at.x, at.y, width.max(0.0), height.max(0.0));
        p.text = Some(Arc::new(TextData {
            props: props.clone(),
            content: content.clone(),
        }));
        if let Some((w, h)) = fit_size(&mut self.fonts.borrow_mut(), &p)? {
            (p.w, p.h) = (w, h);
        }
        let result = self.transact(|tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_props(
                id,
                &PropsPatch {
                    x: Some(p.x),
                    y: Some(p.y),
                    w: Some(p.w),
                    h: Some(p.h),
                    text: Some(props.clone()),
                    ..Default::default()
                },
            )?;
            tx.set_text_content(id, content)?;
            Ok(vec![id])
        });
        Ok(result.ok().and_then(|ids| ids.first().copied()))
    }

    /// Шрифты текстов текущей страницы, без повторов.
    pub fn page_fonts(&self) -> Vec<FontName> {
        let mut out: Vec<FontName> = Vec::new();
        for (_, n) in self.scene.iter() {
            if let Some(t) = &n.props.text {
                for f in t.content.fonts(&t.props.style) {
                    if !out.contains(&f) {
                        out.push(f);
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// Отсутствующие шрифты текущей страницы (спека 4): нет ни во вшитых, ни в
    /// системных. Пока список вшитых не пришёл — пусто.
    pub fn missing_fonts(&self) -> Vec<MissingFont> {
        let fonts = self.fonts.borrow();
        let mut out: Vec<MissingFont> = Vec::new();
        for (_, n) in self.scene.iter() {
            let Some(t) = &n.props.text else {
                continue;
            };
            let mut seen: Vec<FontName> = Vec::new();
            for f in t.content.fonts(&t.props.style) {
                if fonts.status(&f) != FontStatus::Missing || seen.contains(&f) {
                    continue;
                }
                match out.iter_mut().find(|m| m.font == f) {
                    Some(m) => m.nodes += 1,
                    None => out.push(MissingFont {
                        font: f.clone(),
                        nodes: 1,
                    }),
                }
                seen.push(f);
            }
        }
        out.sort_by(|a, b| a.font.cmp(&b.font));
        out
    }

    /// Замена шрифта `from` на `to` во всех текстах текущей страницы одним шагом
    /// отмены (спека 4); размер текстов с авторазмером — по новой раскладке в той
    /// же транзакции. Возвращает число изменённых узлов; новый шрифт ещё
    /// грузится — `Err(TextPending)` и ничего не меняется.
    pub fn replace_font(&mut self, from: &FontName, to: &FontName) -> Result<usize, TextPending> {
        let mut edits: Vec<FontEdit> = Vec::new();
        let mut pending = false;
        for (_, n) in self.scene.iter() {
            let Some(t) = &n.props.text else {
                continue;
            };
            if !t.content.fonts(&t.props.style).contains(from) {
                continue;
            }
            let mut props = t.props.clone();
            if &props.style.font == from {
                props.style.font = to.clone();
            }
            let content = replaced_content(&t.content, &t.props, &props, from, to);
            let mut p = n.props.clone();
            p.text = Some(Arc::new(TextData {
                props: props.clone(),
                content: content.clone(),
            }));
            let size = match fit_size(&mut self.fonts.borrow_mut(), &p) {
                Ok(s) => s,
                Err(TextPending) => {
                    pending = true;
                    None
                }
            };
            edits.push(FontEdit {
                id: n.id,
                props: (props != t.props).then_some(props),
                content,
                size,
            });
        }
        if pending {
            return Err(TextPending);
        }
        if edits.is_empty() {
            return Ok(0);
        }
        let count = edits.len();
        let selection = self.selection.to_vec();
        let _ = self.transact(|tx| {
            for e in &edits {
                tx.set_props(
                    e.id,
                    &PropsPatch {
                        text: e.props.clone(),
                        w: e.size.map(|s| s.0),
                        h: e.size.map(|s| s.1),
                        ..Default::default()
                    },
                )?;
                tx.set_text_content(e.id, &e.content)?;
            }
            Ok(selection)
        });
        Ok(count)
    }
}
