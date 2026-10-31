//! Шрифты и текст в ядре worker'а (подсистема 3, этап 1): списки шрифтов
//! источников, запросы и загрузка байтов, отсутствующие шрифты страницы и их
//! замена, тестовое создание текста. Правки текста, которым нужны ещё не
//! загруженные шрифты, ждут в очереди и выполняются, когда байты придут. Пока
//! очередь не пуста, за ней встаёт и всё следующее — команды и ввод из кольца:
//! щелчок, «Вырезать» или отмена не обгоняют набор, ждущий шрифта. Очередь
//! ограничена (`MAX_WAITING`), а ответа на запрос шрифта ждут не дольше
//! `FONT_WAIT_MS` — затем шрифт считается не полученным (запасной Inter).
//!
//! Всё, что приходит из основного потока, не доверенное: имена и списки
//! ограничены (спека 8), неверное — `Error`, сбой разбора шрифта — `Log`.

use std::collections::VecDeque;

use ferrum_doc::{FontName, LetterSpacing, LineHeight, PageId, TextContent, TextProps, TextSizing};
use ferrum_editor::{Input, TextError};
use ferrum_scene::Scene;
use ferrum_text::{FaceInfo, FamilyInfo, FontStatus, SharedFonts, SourceKind, MAX_FAMILY_NAME};
use kurbo::Point;
use serde_bytes::ByteBuf;

use super::Core;
use crate::protocol::{
    Command, CoreEvent, FontChoice, FontFamilyDto, FontNameDto, FontSource, MissingFontDto,
    TextSizingDto, MAX_FONT_FACES, MAX_FONT_FAMILIES, MAX_TEXT_BYTES, MAX_TEXT_DATA_BYTES,
};

/// Сколько команд и событий ввода ждут шрифтов; сверх — очередь сбрасывается
/// с ошибкой (неотвеченный запрос шрифта не растит память без предела).
pub const MAX_WAITING: usize = 4096;
/// Сколько ждать байтов запрошенных шрифтов (мс), пока очередь не пуста; затем
/// они считаются не полученными, и очередь выполняется с запасным шрифтом.
pub const FONT_WAIT_MS: f64 = 15_000.0;

/// Ждёт шрифтов: команда основного потока или событие ввода из кольца.
#[derive(Debug)]
pub(crate) enum Waiting {
    Command(Command),
    Input(Input),
}

/// Что ядро уже сообщило об отсутствующих шрифтах и каталоге.
#[derive(Default)]
pub(crate) struct FontState {
    /// Ждущее шрифтов, по порядку прихода.
    pub waiting: VecDeque<Waiting>,
    /// Время ядра (мс), с которого очередь не пуста.
    pub waiting_since: Option<f64>,
    pub missing_sent: Option<Vec<MissingFontDto>>,
    /// Семейства текстов страницы, что ушли последними (`PageFonts`).
    pub page_sent: Option<Vec<String>>,
    pub catalog_sent: Option<Vec<FontChoice>>,
    /// При чём посчитаны отсутствующие: страница, экземпляр сцены, ревизия
    /// текстов сцены, поколение реестра (`None` — пересчитать). Правка без
    /// текстов (сдвиг, отмена геометрии) ключ не меняет — обхода сцены нет.
    pub missing_key: Option<MissingKey>,
    /// Поколение реестра, при котором собран каталог (`None` — пересобрать).
    pub catalog_key: Option<u64>,
    /// Сколько раз пересчитаны отсутствующие и каталог — для тестов.
    pub recomputed: (u64, u64),
}

/// Ключ списка отсутствующих шрифтов (см. `FontState::missing_key`).
pub(crate) type MissingKey = (PageId, u64, u64, u64);

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
/// вес и курсив по имени (Chrome не сообщает их в `FontFamilyDto`). Неверное
/// семейство или начертание пропускается, лишнее сверх пределов отбрасывается —
/// остальной список принимается (один кривой файл шрифта у пользователя не
/// должен прятать все системные шрифты). Второе значение — строки журнала.
fn families(list: Vec<FontFamilyDto>) -> (Vec<FamilyInfo>, Vec<String>) {
    let mut log = Vec::new();
    let total = list.len();
    if total > MAX_FONT_FAMILIES {
        log.push(format!(
            "Шрифтов {total} семейств, взяты первые {MAX_FONT_FAMILIES}"
        ));
    }
    let (mut bad_families, mut bad_faces, mut extra_faces) = (0usize, 0usize, 0usize);
    let mut out = Vec::with_capacity(total.min(MAX_FONT_FAMILIES));
    for f in list.into_iter().take(MAX_FONT_FAMILIES) {
        if !name_ok(&f.family, MAX_FAMILY_NAME) {
            bad_families += 1;
            continue;
        }
        extra_faces += f.faces.len().saturating_sub(MAX_FONT_FACES);
        let mut faces = Vec::with_capacity(f.faces.len().min(MAX_FONT_FACES));
        for x in f.faces.into_iter().take(MAX_FONT_FACES) {
            if !name_ok(&x.style, MAX_FAMILY_NAME) || !name_ok(&x.file, MAX_TEXT_BYTES) {
                bad_faces += 1;
                continue;
            }
            let (guess_w, guess_i) = ferrum_text::style_name::guess_attrs(&x.style);
            let finite = |v: Option<f64>| v.filter(|v| v.is_finite());
            faces.push(FaceInfo {
                weight: finite(x.weight).map_or(guess_w, |v| v.clamp(1.0, 1000.0) as f32),
                italic: x.italic.unwrap_or(guess_i),
                stretch: finite(x.stretch).map_or(1.0, |v| v.clamp(0.5, 2.0) as f32),
                index: x.index.unwrap_or(0),
                style: x.style,
                file: x.file,
            });
        }
        out.push(FamilyInfo {
            family: f.family,
            faces,
        });
    }
    if bad_families > 0 {
        log.push(format!(
            "Пропущено семейств шрифтов с неверным именем: {bad_families}"
        ));
    }
    if bad_faces > 0 {
        log.push(format!(
            "Пропущено начертаний с неверным именем или файлом: {bad_faces}"
        ));
    }
    if extra_faces > 0 {
        log.push(format!(
            "Пропущено начертаний сверх {MAX_FONT_FACES} у семейства: {extra_faces}"
        ));
    }
    (out, log)
}

impl Core {
    /// Команды шрифтов и текста; остальные — не сюда.
    pub(crate) fn font_command(&mut self, cmd: Command) {
        match cmd {
            Command::FontsAvailable {
                source: s,
                families: list,
            } => {
                let (list, log) = families(list);
                for message in log {
                    self.events.push(CoreEvent::Log { message });
                }
                self.fonts.borrow_mut().add_catalog(source(s), list);
                // Шрифты страницы, которые до списка были «неизвестны», — запросить.
                self.preload_page_fonts();
                self.after_fonts_changed();
            }
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
    pub(crate) fn text_command(&mut self, cmd: Command) {
        self.editor.take_text_pending();
        let result = match &cmd {
            Command::ReplaceMissingFont { from, to } => match (font_name(from), font_name(to)) {
                (Ok(from), Ok(to)) => self.editor.replace_font(&from, &to).map(|_| ()),
                (Err(e), _) | (_, Err(e)) => {
                    self.error(e);
                    return;
                }
            },
            c if super::text::is_text_command(c) => self.run_text(c),
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
            // Клавиша правки (удаление, формат) отложена редактором: повторить.
            Ok(()) if self.editor.take_text_pending() => self.wait(Waiting::Command(cmd)),
            Ok(()) => {}
            // Шрифты ещё грузятся: повторить после изменения реестра.
            Err(TextError::Pending) => self.wait(Waiting::Command(cmd)),
            // Документ не принял правку (только чтение, пределы): сообщение
            // пользователю, в очередь не ставим.
            Err(TextError::Doc(e)) => self.error(e.to_string()),
        }
        self.push_font_requests();
    }

    /// Ставит в очередь ждущих шрифтов. Переполнение — очередь сбрасывается
    /// целиком с ошибкой: выполнить её часть значило бы нарушить порядок ввода.
    pub(crate) fn wait(&mut self, item: Waiting) {
        let q = &mut self.font_state.waiting;
        if q.is_empty() {
            self.font_state.waiting_since = Some(self.now_ms);
        }
        // Сдвиги указателя подряд: важен последний (наведение, протяжка).
        if matches!(item, Waiting::Input(Input::PointerMove(_)))
            && matches!(q.back(), Some(Waiting::Input(Input::PointerMove(_))))
        {
            q.pop_back();
        }
        if q.len() >= MAX_WAITING {
            q.clear();
            self.font_state.waiting_since = None;
            self.error(
                "Шрифты загружаются слишком долго: ввод, сделанный за это время, не применён",
            );
            return;
        }
        q.push_back(item);
    }

    /// Ждёт ли что-нибудь шрифтов (тогда и следующее встаёт в очередь).
    pub(crate) fn fonts_awaited(&self) -> bool {
        !self.font_state.waiting.is_empty()
    }

    /// Реестр изменился (пришёл список, загружен файл): кадр заново, ждавшее
    /// шрифтов — ещё раз по порядку, списки для интерфейса — если сменились.
    fn after_fonts_changed(&mut self) {
        self.dirty = true;
        let waiting: Vec<Waiting> = self.font_state.waiting.drain(..).collect();
        self.font_state.waiting_since = None;
        for item in waiting {
            // Правка снова ждёт — всё следующее за ней тоже ждёт: порядок команд
            // и ввода (стиль, набор, щелчок) сохраняется.
            if self.font_state.waiting.is_empty() {
                self.run_waiting(item);
            } else {
                self.font_state.waiting.push_back(item);
            }
        }
        self.push_font_requests();
    }

    /// Перед журналом и снимком (интерфейс может закрываться — `pagehide`):
    /// ждущее шрифтов не должно пропасть. Ожидание прекращается (шрифты — как
    /// не полученные, запасной), очередь выполняется. Повтор может запросить новые
    /// шрифты — тогда ещё раз, не больше нескольких кругов.
    pub(crate) fn settle_waiting_fonts(&mut self) {
        for _ in 0..4 {
            if !self.fonts_awaited() {
                return;
            }
            self.give_up_waiting_fonts();
        }
    }

    /// Шрифтов ждут дольше `FONT_WAIT_MS`: запрошенные считаются не полученными
    /// (как `LoadFont` без байтов), очередь выполняется с запасным шрифтом.
    pub(crate) fn give_up_waiting_fonts(&mut self) {
        let families = self.fonts.borrow_mut().give_up_loading();
        let secs = FONT_WAIT_MS / 1000.0;
        let message = if families.is_empty() {
            format!("Шрифты не загрузились за {secs} с: текст — запасным шрифтом")
        } else {
            format!(
                "Шрифты не загрузились за {secs} с ({}): текст — запасным шрифтом",
                families.join(", ")
            )
        };
        self.events.push(CoreEvent::Log { message });
        self.after_fonts_changed();
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

    /// `MissingFonts` и `FontCatalog`, если сменились. Отсутствующие считаются
    /// заново, только когда сменились страница, сцена, тексты на ней или реестр;
    /// каталог — только когда сменился реестр.
    pub(crate) fn push_missing_fonts(&mut self) {
        let generation = self.fonts.borrow().generation();
        let scene = self.editor.scene();
        let key = (
            self.editor.page(),
            scene.generation(),
            scene.text_revision(),
            generation,
        );
        if self.font_state.missing_key != Some(key) {
            self.font_state.missing_key = Some(key);
            self.push_missing_list();
        }
        if self.font_state.catalog_key != Some(generation) {
            self.font_state.catalog_key = Some(generation);
            self.push_catalog();
        }
    }

    fn push_missing_list(&mut self) {
        self.font_state.recomputed.0 += 1;
        // Семейства страницы — для группы «В документе» выбора шрифта.
        let mut families: Vec<String> = self
            .editor
            .page_fonts()
            .into_iter()
            .map(|f| f.family)
            .collect();
        families.dedup();
        if self.font_state.page_sent.as_ref() != Some(&families) {
            self.font_state.page_sent = Some(families.clone());
            self.events.push(CoreEvent::PageFonts { families });
        }
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
    }

    fn push_catalog(&mut self) {
        self.font_state.recomputed.1 += 1;
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

    /// Сколько раз пересчитаны (отсутствующие шрифты, каталог) — для тестов.
    #[doc(hidden)]
    pub fn font_recomputes_for_tests(&self) -> (u64, u64) {
        self.font_state.recomputed
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
