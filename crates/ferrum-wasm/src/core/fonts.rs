//! Шрифты и текст в ядре worker'а (подсистема 3, этап 1): списки шрифтов
//! источников, запросы и загрузка байтов, отсутствующие шрифты страницы и их
//! замена, тестовое создание текста. Правки текста, которым нужны ещё не
//! загруженные шрифты, ждут в очереди и выполняются, когда байты придут.
//!
//! Всё, что приходит из основного потока, не доверенное: имена и списки
//! ограничены (спека 8), неверное — `Error`, сбой разбора шрифта — `Log`.

use std::collections::VecDeque;

use ferrum_doc::{FontName, LetterSpacing, LineHeight, TextContent, TextProps, TextSizing};
use ferrum_editor::TextError;
use ferrum_scene::Scene;
use ferrum_text::{FaceInfo, FamilyInfo, FontStatus, SharedFonts, SourceKind, MAX_FAMILY_NAME};
use kurbo::Point;
use serde_bytes::ByteBuf;

use super::Core;
use crate::protocol::{
    Command, CoreEvent, FontChoice, FontFamilyDto, FontNameDto, FontSource, MissingFontDto,
    TextSizingDto, MAX_FONT_FACES, MAX_FONT_FAMILIES, MAX_TEXT_BYTES, MAX_TEXT_DATA_BYTES,
};

/// Что ядро уже сообщило об отсутствующих шрифтах и каталоге.
#[derive(Default)]
pub(crate) struct FontState {
    /// Правки, ждущие шрифтов, по порядку прихода.
    pub waiting: VecDeque<Command>,
    pub missing_sent: Option<Vec<MissingFontDto>>,
    pub catalog_sent: Option<Vec<FontChoice>>,
    /// Поколение реестра, при котором посчитаны отсутствующие (`None` — пересчитать).
    pub generation: Option<u64>,
}

/// Ответ экспорту и копированию PNG, пока шрифты текстов не загружены.
pub(crate) const FONTS_LOADING: &str = "Шрифты ещё загружаются — повторите через мгновение";

/// Грузится ли (или ещё не известен) шрифт какого-нибудь текста сцены, с учётом
/// запасного Inter. Такой текст в PNG не попал бы; байты при этом запрошены.
pub(crate) fn fonts_loading(scene: &Scene, fonts: &SharedFonts) -> bool {
    let mut seen: Vec<FontName> = Vec::new();
    let mut reg = fonts.borrow_mut();
    let mut loading = false;
    for (_, n) in scene.iter() {
        let Some(t) = &n.props.text else { continue };
        for f in t.content.fonts(&t.props.style) {
            if seen.contains(&f) {
                continue;
            }
            // Все шрифты опрашиваются: каждый незагруженный запрашивает байты.
            if matches!(
                reg.resolve(&f).status,
                FontStatus::Loading | FontStatus::Pending
            ) {
                loading = true;
            }
            seen.push(f);
        }
    }
    loading
}

fn source(s: FontSource) -> SourceKind {
    match s {
        FontSource::Bundled => SourceKind::Bundled,
        FontSource::System => SourceKind::System,
    }
}

fn source_dto(s: SourceKind) -> FontSource {
    match s {
        SourceKind::System => FontSource::System,
        // Вшитый в wasm Onest для интерфейса — тоже «вшитый».
        SourceKind::Bundled | SourceKind::Embedded => FontSource::Bundled,
    }
}

fn name_ok(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max
}

fn font_name(f: &FontNameDto) -> Result<FontName, String> {
    if !name_ok(&f.family, MAX_FAMILY_NAME) || !name_ok(&f.style, MAX_FAMILY_NAME) {
        return Err("Неверное имя шрифта".into());
    }
    Ok(FontName::new(f.family.clone(), f.style.clone()))
}

/// Семейства из команды: имена и списки в пределах, у начертаний без веса —
/// вес и курсив по имени (Chrome не сообщает их в `FontData`).
fn families(list: Vec<FontFamilyDto>) -> Result<Vec<FamilyInfo>, String> {
    if list.len() > MAX_FONT_FAMILIES {
        return Err(format!("Шрифтов больше {MAX_FONT_FAMILIES} семейств"));
    }
    list.into_iter()
        .map(|f| {
            if !name_ok(&f.family, MAX_FAMILY_NAME) || f.faces.len() > MAX_FONT_FACES {
                return Err("Неверный список шрифтов".to_string());
            }
            let faces = f
                .faces
                .into_iter()
                .map(|x| {
                    if !name_ok(&x.style, MAX_FAMILY_NAME) || !name_ok(&x.file, MAX_TEXT_BYTES) {
                        return Err("Неверный список шрифтов".to_string());
                    }
                    let (guess_w, guess_i) = ferrum_text::style_name::guess_attrs(&x.style);
                    let finite = |v: Option<f64>| v.filter(|v| v.is_finite());
                    Ok(FaceInfo {
                        weight: finite(x.weight).map_or(guess_w, |v| v.clamp(1.0, 1000.0) as f32),
                        italic: x.italic.unwrap_or(guess_i),
                        stretch: finite(x.stretch).map_or(1.0, |v| v.clamp(0.5, 2.0) as f32),
                        index: x.index.unwrap_or(0),
                        style: x.style,
                        file: x.file,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FamilyInfo {
                family: f.family,
                faces,
            })
        })
        .collect()
}

impl Core {
    /// Команды шрифтов и текста; остальные — не сюда.
    pub(crate) fn font_command(&mut self, cmd: Command) {
        match cmd {
            Command::FontsAvailable {
                source: s,
                families: list,
            } => match families(list) {
                Ok(list) => {
                    self.fonts.borrow_mut().add_catalog(source(s), list);
                    // Шрифты страницы, которые до списка были «неизвестны», — запросить.
                    self.preload_page_fonts();
                    self.after_fonts_changed();
                }
                Err(e) => self.error(e),
            },
            Command::LoadFont {
                family,
                file,
                bytes,
            } => {
                if !name_ok(&family, MAX_FAMILY_NAME) || !name_ok(&file, MAX_TEXT_BYTES) {
                    self.error("Неверная команда загрузки шрифта");
                    return;
                }
                // Размер и разбор проверяет реестр; он же снимает уже загруженный слот.
                let result = match bytes {
                    Some(b) => self.fonts.borrow_mut().load(&family, &file, b.into_vec()),
                    None => {
                        self.fonts.borrow_mut().load_failed(&family, &file);
                        Err(format!("Шрифт «{family}» не загружен: файл недоступен"))
                    }
                };
                if let Err(message) = result {
                    self.events.push(CoreEvent::Log { message });
                }
                self.after_fonts_changed();
            }
            cmd @ (Command::ReplaceMissingFont { .. } | Command::TestCreateText { .. }) => {
                self.text_command(cmd)
            }
            _ => {}
        }
    }

    /// Правка текста; ждущая шрифтов — в очередь (повтор — `after_fonts_changed`).
    fn text_command(&mut self, cmd: Command) {
        let result = match &cmd {
            Command::ReplaceMissingFont { from, to } => match (font_name(from), font_name(to)) {
                (Ok(from), Ok(to)) => self.editor.replace_font(&from, &to).map(|_| ()),
                (Err(e), _) | (_, Err(e)) => {
                    self.error(e);
                    return;
                }
            },
            Command::TestCreateText {
                x,
                y,
                text,
                font,
                size,
                sizing,
                width,
                height,
            } => {
                if text.len() > MAX_TEXT_DATA_BYTES {
                    self.error("Текст длиннее 64 КБ");
                    return;
                }
                let mut props = TextProps::default();
                if let Some(f) = font {
                    match font_name(f) {
                        Ok(f) => props.style.font = f,
                        Err(e) => {
                            self.error(e);
                            return;
                        }
                    }
                }
                if let Some(s) = size.filter(|s| s.is_finite() && *s > 0.0) {
                    props.style.size = s.min(10_000.0);
                }
                props.style.line_height = LineHeight::Auto;
                props.style.letter_spacing = LetterSpacing::Percent(0.0);
                props.sizing = match sizing.unwrap_or(TextSizingDto::AutoWidth) {
                    TextSizingDto::AutoWidth => TextSizing::AutoWidth,
                    TextSizingDto::AutoHeight => TextSizing::AutoHeight,
                    TextSizingDto::Fixed => TextSizing::Fixed,
                };
                let finite = |v: &Option<f64>| v.filter(|v| v.is_finite()).unwrap_or(0.0);
                if !(x.is_finite() && y.is_finite()) {
                    self.error("Неверная точка текста");
                    return;
                }
                self.editor
                    .create_text(
                        Point::new(*x, *y),
                        &TextContent::plain(text),
                        &props,
                        finite(width),
                        finite(height),
                    )
                    .map(|_| ())
            }
            _ => return,
        };
        match result {
            Ok(()) => {}
            // Шрифты ещё грузятся: повторить после изменения реестра.
            Err(TextError::Pending) => self.font_state.waiting.push_back(cmd),
            // Документ не принял правку (только чтение, пределы): сообщение
            // пользователю, в очередь не ставим.
            Err(TextError::Doc(e)) => self.error(e.to_string()),
        }
        self.push_font_requests();
    }

    /// Реестр изменился (пришёл список, загружен файл): кадр заново, правки,
    /// ждавшие шрифтов, — ещё раз по порядку, списки для интерфейса — если сменились.
    fn after_fonts_changed(&mut self) {
        self.dirty = true;
        let waiting: Vec<Command> = self.font_state.waiting.drain(..).collect();
        for cmd in waiting {
            self.text_command(cmd);
        }
        self.font_state.generation = None;
        self.push_font_requests();
    }

    /// Шрифты текущей страницы: на ней — не вытеснять, незагруженные — запросить
    /// сразу (а не при первом рисовании). Вызывается при смене страницы и документа.
    pub(crate) fn preload_page_fonts(&mut self) {
        let fonts = self.editor.page_fonts();
        {
            let mut reg = self.fonts.borrow_mut();
            reg.set_in_use(fonts.iter().map(|f| f.family.clone()));
            for f in &fonts {
                reg.resolve(f);
            }
        }
        self.font_state.generation = None;
        self.push_font_requests();
    }

    /// Запросы байтов и сообщения реестра — событиями; отсутствующие шрифты и
    /// каталог — если изменились.
    pub(crate) fn push_font_requests(&mut self) {
        let (requests, errors) = {
            let mut reg = self.fonts.borrow_mut();
            (reg.take_requests(), reg.take_errors())
        };
        for r in requests {
            self.events.push(CoreEvent::FontRequest {
                family: r.family,
                source: source_dto(r.source),
                files: r.files,
            });
        }
        for message in errors {
            self.events.push(CoreEvent::Log { message });
        }
        self.push_missing_fonts();
    }

    /// `MissingFonts` и `FontCatalog`, если сменились. Пересчёт — при правке
    /// документа, смене страницы и изменении реестра, а не на каждом кадре.
    pub(crate) fn push_missing_fonts(&mut self) {
        let generation = self.fonts.borrow().generation();
        if self.font_state.generation == Some(generation) {
            return;
        }
        self.font_state.generation = Some(generation);
        let list: Vec<MissingFontDto> = self
            .editor
            .missing_fonts()
            .into_iter()
            .map(|m| MissingFontDto {
                family: m.font.family,
                style: m.font.style,
                nodes: m.nodes as u32,
            })
            .collect();
        if self.font_state.missing_sent.as_ref() != Some(&list) {
            self.font_state.missing_sent = Some(list.clone());
            self.events.push(CoreEvent::MissingFonts { list });
        }
        let catalog: Vec<FontChoice> = self
            .fonts
            .borrow()
            .families()
            .into_iter()
            .map(|(family, s, styles)| FontChoice {
                family,
                source: source_dto(s),
                styles,
            })
            .collect();
        if self.font_state.catalog_sent.as_ref() != Some(&catalog) {
            self.font_state.catalog_sent = Some(catalog.clone());
            self.events
                .push(CoreEvent::FontCatalog { families: catalog });
        }
    }

    /// Смена страницы: замена шрифта относилась к прошлой странице — отбросить.
    pub(crate) fn drop_page_scoped_font_edits(&mut self) {
        self.font_state
            .waiting
            .retain(|c| !matches!(c, Command::ReplaceMissingFont { .. }));
    }

    /// Документ правился: отсутствующие шрифты пересчитать при следующей пачке.
    pub(crate) fn fonts_document_changed(&mut self) {
        self.font_state.generation = None;
    }

    /// Байты для `LoadFont` тестов: `ByteBuf` из вектора.
    #[doc(hidden)]
    pub fn load_font_for_tests(&mut self, family: &str, file: &str, bytes: Vec<u8>) {
        self.command(Command::LoadFont {
            family: family.into(),
            file: file.into(),
            bytes: Some(ByteBuf::from(bytes)),
        });
    }
}
