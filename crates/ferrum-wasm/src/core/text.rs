//! Правка текста в ядре worker'а (подсистема 3, этап 2): команды ввода, клавиш,
//! стиля и свойств текста и события для поля ввода (`TextCaret`) и секции «Текст»
//! (`TextStyleSummary`). Всё из основного потока — недоверенное (спека 8): текст
//! ввода ≤ 64 КБ, клавиша ≤ 64 байт, имена шрифтов ≤ 256 байт, числа конечны и в
//! пределах, неизвестные ключи правки отвергает разбор (`deny_unknown_fields`).
//! Правка, которой нужны ещё не загруженные шрифты, ждёт в общей очереди правок
//! шрифтов (порядок команд текста сохраняется).

use ferrum_doc::{
    AlignH, AlignV, Decoration, FontName, LetterSpacing, LineHeight, ListKind, NodeId, TextCase,
    TextSizing,
};
use ferrum_editor::{
    EnterAt, Modifiers, Phase, TextError, TextInputKind, TextPropsPatch, TextStylePatch,
    TextStyleSummary,
};
use ferrum_text::MAX_FAMILY_NAME;

use super::Core;
use crate::protocol::{
    AlignHDto, AlignVDto, Command, CoreEvent, DecorationDto, EditPhase, ListKindDto, MixedSpacing,
    MixedString, Num, SpacingDto, SpacingUnit, TextCaseDto, TextInputKindDto, TextPropsPatchDto,
    TextSizingDto, TextStyleDto, TextStylePatchDto, MAX_FONT_SIZE, MAX_KEY_CODE_BYTES,
    MAX_TEXT_DATA_BYTES, SELECTION_IDS_MAX,
};

/// Что последним ушло полю ввода и панели.
#[derive(Default)]
pub(crate) struct TextState {
    pub caret_sent: Option<CoreEvent>,
    pub style_sent: Option<Option<TextStyleDto>>,
}

fn kind(k: TextInputKindDto) -> TextInputKind {
    match k {
        TextInputKindDto::Insert => TextInputKind::Insert,
        TextInputKindDto::Paragraph => TextInputKind::Paragraph,
        TextInputKindDto::Paste => TextInputKind::Paste,
        TextInputKindDto::Compose => TextInputKind::Compose,
        TextInputKindDto::Commit => TextInputKind::Commit,
        TextInputKindDto::DeleteBackward => TextInputKind::DeleteBackward,
        TextInputKindDto::DeleteForward => TextInputKind::DeleteForward,
        TextInputKindDto::DeleteWordBackward => TextInputKind::DeleteWordBackward,
        TextInputKindDto::DeleteWordForward => TextInputKind::DeleteWordForward,
        TextInputKindDto::DeleteLineBackward => TextInputKind::DeleteLineBackward,
        TextInputKindDto::DeleteLineForward => TextInputKind::DeleteLineForward,
    }
}

fn phase(p: EditPhase) -> Phase {
    match p {
        EditPhase::Preview => Phase::Preview,
        EditPhase::Commit => Phase::Commit,
        EditPhase::Cancel => Phase::Cancel,
    }
}

/// Число конечно и в пределах — иначе ошибка (недоверенный ввод не «подгоняется»).
fn finite_in(v: f64, lo: f64, hi: f64, what: &str) -> Result<f64, String> {
    if !v.is_finite() {
        Err(format!("{what} — не число"))
    } else if v < lo || v > hi {
        Err(format!("{what} — вне пределов {lo}…{hi}"))
    } else {
        Ok(v)
    }
}

fn line_height(d: SpacingDto) -> Result<LineHeight, String> {
    Ok(match d.unit {
        SpacingUnit::Auto => LineHeight::Auto,
        SpacingUnit::Px => LineHeight::Px(finite_in(d.value, 0.0, MAX_FONT_SIZE, "Интерлиньяж")?),
        SpacingUnit::Percent => {
            LineHeight::Percent(finite_in(d.value, 0.0, 10_000.0, "Интерлиньяж")?)
        }
    })
}

fn letter_spacing(d: SpacingDto) -> Result<LetterSpacing, String> {
    Ok(match d.unit {
        SpacingUnit::Auto => return Err("Трекинг не бывает «авто»".into()),
        SpacingUnit::Px => LetterSpacing::Px(finite_in(
            d.value,
            -MAX_FONT_SIZE,
            MAX_FONT_SIZE,
            "Трекинг",
        )?),
        SpacingUnit::Percent => {
            LetterSpacing::Percent(finite_in(d.value, -1000.0, 1000.0, "Трекинг")?)
        }
    })
}

/// Правка стиля из команды: имена и числа проверены.
fn style_patch(d: &TextStylePatchDto) -> Result<TextStylePatch, String> {
    let name = |s: &str| !s.is_empty() && s.len() <= MAX_FAMILY_NAME;
    let font = match (&d.family, &d.style) {
        (None, None) => None,
        (Some(f), s) => {
            let s = s.as_deref().unwrap_or("Regular");
            if !name(f) || !name(s) {
                return Err("Неверное имя шрифта".into());
            }
            Some(FontName::new(f.clone(), s))
        }
        (None, Some(_)) => return Err("Начертание без семейства".into()),
    };
    Ok(TextStylePatch {
        font,
        size: d
            .size
            .map(|v| finite_in(v, 1.0, MAX_FONT_SIZE, "Размер шрифта"))
            .transpose()?,
        line_height: d.line_height.map(line_height).transpose()?,
        letter_spacing: d.letter_spacing.map(letter_spacing).transpose()?,
        decoration: d.decoration.map(|v| match v {
            DecorationDto::None => Decoration::None,
            DecorationDto::Underline => Decoration::Underline,
            DecorationDto::Strike => Decoration::Strike,
        }),
        case: d.case.map(|v| match v {
            TextCaseDto::None => TextCase::None,
            TextCaseDto::Upper => TextCase::Upper,
            TextCaseDto::Lower => TextCase::Lower,
            TextCaseDto::Title => TextCase::Title,
            TextCaseDto::SmallCaps => TextCase::SmallCaps,
        }),
        list: d.list.map(|v| match v {
            ListKindDto::None => ListKind::None,
            ListKindDto::Bullet => ListKind::Bullet,
            ListKindDto::Ordered => ListKind::Ordered,
        }),
        indent_by: None,
    })
}

fn props_patch(d: &TextPropsPatchDto) -> Result<TextPropsPatch, String> {
    Ok(TextPropsPatch {
        sizing: d.sizing.map(|v| match v {
            TextSizingDto::AutoWidth => TextSizing::AutoWidth,
            TextSizingDto::AutoHeight => TextSizing::AutoHeight,
            TextSizingDto::Fixed => TextSizing::Fixed,
        }),
        align_h: d.align_h.map(|v| match v {
            AlignHDto::Left => AlignH::Left,
            AlignHDto::Center => AlignH::Center,
            AlignHDto::Right => AlignH::Right,
            AlignHDto::Justify => AlignH::Justify,
        }),
        align_v: d.align_v.map(|v| match v {
            AlignVDto::Top => AlignV::Top,
            AlignVDto::Middle => AlignV::Middle,
            AlignVDto::Bottom => AlignV::Bottom,
        }),
        paragraph_spacing: d
            .paragraph_spacing
            .map(|v| finite_in(v, 0.0, MAX_FONT_SIZE, "Интервал абзацев"))
            .transpose()?,
        paragraph_indent: d
            .paragraph_indent
            .map(|v| finite_in(v, -MAX_FONT_SIZE, MAX_FONT_SIZE, "Отступ абзаца"))
            .transpose()?,
        truncate: None,
        max_lines: None,
    })
}

fn spacing_dto_lh(v: LineHeight) -> SpacingDto {
    match v {
        LineHeight::Auto => SpacingDto {
            unit: SpacingUnit::Auto,
            value: 0.0,
        },
        LineHeight::Px(value) => SpacingDto {
            unit: SpacingUnit::Px,
            value,
        },
        LineHeight::Percent(value) => SpacingDto {
            unit: SpacingUnit::Percent,
            value,
        },
    }
}

fn spacing_dto_ls(v: LetterSpacing) -> SpacingDto {
    match v {
        LetterSpacing::Px(value) => SpacingDto {
            unit: SpacingUnit::Px,
            value,
        },
        LetterSpacing::Percent(value) => SpacingDto {
            unit: SpacingUnit::Percent,
            value,
        },
    }
}

fn mixed(value: &str, mixed: bool) -> MixedString {
    MixedString {
        value: value.to_string(),
        mixed,
    }
}

/// Сводка редактора → событие (перечисления — строками протокола).
pub(crate) fn style_dto(s: &TextStyleSummary) -> TextStyleDto {
    let deco = match s.decoration.value {
        Decoration::None => "none",
        Decoration::Underline => "underline",
        Decoration::Strike => "strike",
    };
    let case = match s.case.value {
        TextCase::None => "none",
        TextCase::Upper => "upper",
        TextCase::Lower => "lower",
        TextCase::Title => "title",
        TextCase::SmallCaps => "smallCaps",
    };
    let list = match s.list.value {
        ListKind::None => "none",
        ListKind::Bullet => "bullet",
        ListKind::Ordered => "ordered",
    };
    let sizing = match s.sizing.value {
        TextSizing::AutoWidth => "autoWidth",
        TextSizing::AutoHeight => "autoHeight",
        TextSizing::Fixed => "fixed",
    };
    let ah = match s.align_h.value {
        AlignH::Left => "left",
        AlignH::Center => "center",
        AlignH::Right => "right",
        AlignH::Justify => "justify",
    };
    let av = match s.align_v.value {
        AlignV::Top => "top",
        AlignV::Middle => "middle",
        AlignV::Bottom => "bottom",
    };
    TextStyleDto {
        family: mixed(&s.family.value, s.family.mixed),
        style: mixed(&s.style.value, s.style.mixed),
        size: Num {
            value: s.size.value,
            mixed: s.size.mixed,
        },
        line_height: MixedSpacing {
            value: spacing_dto_lh(s.line_height.value),
            mixed: s.line_height.mixed,
        },
        letter_spacing: MixedSpacing {
            value: spacing_dto_ls(s.letter_spacing.value),
            mixed: s.letter_spacing.mixed,
        },
        decoration: mixed(deco, s.decoration.mixed),
        case: mixed(case, s.case.mixed),
        list: mixed(list, s.list.mixed),
        sizing: mixed(sizing, s.sizing.mixed),
        align_h: mixed(ah, s.align_h.mixed),
        align_v: mixed(av, s.align_v.mixed),
        paragraph_spacing: Num {
            value: s.paragraph_spacing.value,
            mixed: s.paragraph_spacing.mixed,
        },
        paragraph_indent: Num {
            value: s.paragraph_indent.value,
            mixed: s.paragraph_indent.mixed,
        },
        range: s.range,
    }
}

/// Команда правки текста?
pub(crate) fn is_text_command(c: &Command) -> bool {
    matches!(
        c,
        Command::TextEditStart { .. }
            | Command::TextEditEnd
            | Command::TextInput { .. }
            | Command::TextKey { .. }
            | Command::SetTextStyle { .. }
            | Command::SetTextProps { .. }
    )
}

impl Core {
    /// Время ядра (мс): мигание курсора и склейка ввода в один шаг. Worker
    /// передаёт `performance.now()` в начале кадра и перед командой.
    pub fn set_time(&mut self, now_ms: f64) {
        self.editor.set_time(now_ms);
        if self.editor.is_editing_text() {
            self.drain_editor();
        }
    }

    fn ids(&mut self, list: &Option<Vec<String>>) -> Option<Option<Vec<NodeId>>> {
        match list {
            None => Some(None),
            Some(l) => {
                if l.len() > SELECTION_IDS_MAX {
                    self.error(format!("Не больше {SELECTION_IDS_MAX} слоёв в команде"));
                    return None;
                }
                let mut out = Vec::with_capacity(l.len());
                for s in l {
                    match s.parse::<NodeId>() {
                        Ok(id) if s.len() <= 64 => out.push(id),
                        _ => {
                            self.error("Неверный идентификатор слоя");
                            return None;
                        }
                    }
                }
                Some(Some(out))
            }
        }
    }

    /// Команда правки текста. `Err(Pending)` — ждать шрифтов (очередь —
    /// `text_command` в модуле шрифтов); неверное — `Error`, `Ok`.
    pub(crate) fn run_text(&mut self, cmd: &Command) -> Result<(), TextError> {
        match cmd {
            Command::TextEditStart { id } => {
                let Some(Some(ids)) = self.ids(&Some(vec![id.clone()])) else {
                    return Ok(());
                };
                if !self.editor.start_text_edit(ids[0], EnterAt::All) {
                    self.error("Это не текст или он заблокирован");
                }
                Ok(())
            }
            Command::TextEditEnd => {
                self.editor.end_text_edit();
                Ok(())
            }
            Command::TextInput { kind: k, data } => {
                let data = data.as_deref().unwrap_or("");
                if data.len() > MAX_TEXT_DATA_BYTES {
                    self.error("Текст ввода длиннее 64 КБ");
                    return Ok(());
                }
                if *k == TextInputKindDto::Paste && !self.editor.is_editing_text() {
                    return self.editor.paste_plain(data);
                }
                self.editor.text_input(kind(*k), data)
            }
            Command::TextKey { key, mods } => {
                if key.len() > MAX_KEY_CODE_BYTES {
                    self.error("Неверная клавиша");
                    return Ok(());
                }
                let m = Modifiers {
                    shift: mods.shift,
                    alt: mods.alt,
                    ctrl: mods.ctrl,
                    meta: mods.meta,
                };
                self.editor.text_key(key, m);
                Ok(())
            }
            Command::SetTextStyle {
                patch,
                phase: p,
                ids,
            } => {
                let Some(ids) = self.ids(ids) else {
                    return Ok(());
                };
                match style_patch(patch) {
                    Ok(patch) => self
                        .editor
                        .edit_text_style(&patch, phase(*p), ids.as_deref()),
                    Err(m) => {
                        self.editor.edit_text_style(
                            &TextStylePatch::default(),
                            Phase::Cancel,
                            None,
                        )?;
                        self.error(format!("Неверная правка: {m}"));
                        Ok(())
                    }
                }
            }
            Command::SetTextProps {
                patch,
                phase: p,
                ids,
            } => {
                let Some(ids) = self.ids(ids) else {
                    return Ok(());
                };
                match props_patch(patch) {
                    Ok(patch) => self
                        .editor
                        .edit_text_props(&patch, phase(*p), ids.as_deref()),
                    Err(m) => {
                        // Превью прошлой пачки гасим: документ не менялся.
                        self.editor.edit_text_props(
                            &TextPropsPatch::default(),
                            Phase::Cancel,
                            None,
                        )?;
                        self.error(format!("Неверная правка: {m}"));
                        Ok(())
                    }
                }
            }
            _ => Ok(()),
        }
    }

    /// `TextCaret` и `TextStyleSummary`, если сменились (после пачки событий).
    pub(crate) fn push_text_state(&mut self, properties: bool) {
        let caret = match self.editor.text_edit_info() {
            Some(i) => CoreEvent::TextCaret {
                editing: true,
                rect: Some([i.caret.x0, i.caret.y0, i.caret.width(), i.caret.height()]),
                selection: Some([i.anchor as u32, i.focus as u32]),
                draft: i.draft,
            },
            None => CoreEvent::TextCaret {
                editing: false,
                rect: None,
                selection: None,
                draft: false,
            },
        };
        let caret_changed = self.text_state.caret_sent.as_ref() != Some(&caret);
        if caret_changed {
            self.text_state.caret_sent = Some(caret.clone());
            self.events
                .retain(|e| !matches!(e, CoreEvent::TextCaret { .. }));
            self.events.push(caret);
        }
        if properties || caret_changed {
            let summary = self.editor.text_style_summary().map(|s| style_dto(&s));
            if self.text_state.style_sent.as_ref() != Some(&summary) {
                self.text_state.style_sent = Some(summary.clone());
                self.events
                    .retain(|e| !matches!(e, CoreEvent::TextStyleSummary { .. }));
                self.events.push(CoreEvent::TextStyleSummary { summary });
            }
        }
    }
}
