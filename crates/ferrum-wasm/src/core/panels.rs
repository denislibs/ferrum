//! Команды панелей (этап 5): слои, свойства, страницы, экспорт, раскладка,
//! настройки — и события для них. Всё, что приходит из основного потока, не
//! доверенное: идентификаторы, строки и списки проверяются и ограничиваются,
//! неверное — событие `Error` на русском, документ не меняется.

use ferrum_doc::{Color, Fill, FillKind, NodeId, PageId, ScaleMode, Stroke, StrokeAlign};
use ferrum_editor::scale::{Anchor, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR};
use ferrum_editor::{
    browser_reserved, frame_presets, BindError, CommandId, DropTarget, KeymapError, NumField,
    PanelEdit, Phase, Shortcut, COMMANDS, MAX_NAME_CHARS,
};
use ferrum_render::{decode_png, export_area, ExportBudget, ExportError, SvgOptions};
use ferrum_scene::shape::COORD_LIMIT;
use serde_bytes::ByteBuf;

use super::Core;
use crate::protocol::{
    Align, Command, ConflictInfo, CoreEvent, DropPosition, Edit, EditPhase, ExportFormat, Field,
    FillDto, KeymapEntry, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto,
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{cap_of, join_of, summarize, MAX_PAINTS};

/// Наибольший размер картинки из буфера обмена (`PasteImage`).
pub const MAX_PASTE_BYTES: usize = 64 << 20;
/// Правка или перенос по версии выделения, а выделение с начала жеста сменилось.
pub const STALE_SELECTION: &str = "Выделение изменилось — правка не применена";

/// Имя для события: имя страницы или документа из файла может быть любой длины.
pub(super) fn event_name(name: &str) -> String {
    name.chars().take(MAX_NAME_CHARS).collect()
}

/// Страницы PDF (спека подсистемы 3, раздел 11): у выделения — каждый узел
/// верхнего уровня, без выделения — каждый видимый фрейм верхнего уровня, а
/// если фреймов нет — вся страница одной; по порядку чтения (сверху вниз, слева
/// направо, как «Export frames to PDF» в Figma). Скрытый фрейм страницей не
/// становится (как в Figma: скрытый слой не экспортируется); фреймы есть, но
/// все скрыты — `None`, «нечего экспортировать».
fn pdf_pages(
    scene: &ferrum_scene::Scene,
    selection: &[NodeId],
    all: &[NodeId],
) -> Option<Vec<Vec<NodeId>>> {
    let tops: Vec<NodeId> = if selection.is_empty() {
        let frames: Vec<NodeId> = all
            .iter()
            .copied()
            .filter(|id| {
                scene
                    .node(*id)
                    .is_some_and(|n| n.props.kind == ferrum_doc::NodeKind::Frame)
            })
            .collect();
        if frames.is_empty() {
            return Some(vec![all.to_vec()]);
        }
        // Видимость — как у экспорта: у скрытого узла нет `subtree_bounds`.
        let visible: Vec<NodeId> = frames
            .into_iter()
            .filter(|id| scene.node(*id).is_some_and(|n| n.subtree_bounds.is_some()))
            .collect();
        if visible.is_empty() {
            return None;
        }
        visible
    } else {
        ferrum_editor::geometry::top_level(scene, selection)
    };
    if tops.is_empty() {
        return Some(vec![all.to_vec()]);
    }
    // Ключ — та же область, что станет страницей (у фрейма — его форма, а не
    // потомки, выступающие за край): строка — по верху с точностью до пикселя.
    let mut keyed: Vec<(f64, f64, NodeId)> = tops
        .into_iter()
        .map(|id| {
            let b = export_area(scene, id).unwrap_or_default();
            (b.y0.round(), b.x0, id)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    Some(keyed.into_iter().map(|(_, _, id)| vec![id]).collect())
}

/// Имя файла экспорта: без знаков, которые не любят файловые системы; у PNG
/// масштаб — суффиксом «@2x», при 1× — без него (как в Figma).
fn file_name(name: &str, scale: f64, format: ExportFormat) -> String {
    let base: String = name
        .chars()
        .take(MAX_NAME_CHARS)
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let base = base.trim();
    let base = if base.is_empty() { "Ferrum" } else { base };
    match format {
        // Как в Figma: при 1× суффикса нет («Слой.png»), иначе «Слой@2x.png».
        ExportFormat::Png if scale == 1.0 => format!("{base}.png"),
        ExportFormat::Png => format!("{base}@{scale}x.png"),
        _ => format!("{base}.{}", format.extension()),
    }
}

/// Ошибка экспорта для интерфейса: законченная фраза по-русски. Тексты
/// `TooBig`, `Timeout`, `Empty` и `TooLarge` уже говорят, что не так и что
/// делать, — без приставки «Не удалось экспортировать».
fn export_message(e: &ExportError) -> String {
    match e {
        ExportError::TooBig
        | ExportError::Timeout
        | ExportError::Empty
        | ExportError::TooLarge(..) => e.to_string(),
        ExportError::NotFound => "Нечего экспортировать: слой не найден".to_string(),
        ExportError::BadScale => "Масштаб экспорта — 1×, 2× или 3×".to_string(),
        ExportError::Write(why) => format!("Не удалось экспортировать: файл не собран ({why})"),
        ExportError::Encode(why) => {
            format!("Не удалось экспортировать: PNG не закодирован ({why})")
        }
    }
}

/// Ошибка «Копировать как SVG»: те же законченные фразы, что у экспорта (без
/// второй приставки «Не удалось скопировать: Нечего экспортировать…»); свои —
/// только предел буфера обмена и сбой сборки.
pub(super) fn copy_svg_message(e: &ExportError) -> String {
    match e {
        ExportError::TooBig => {
            "Не удалось скопировать: SVG больше 64 МБ — экспортируйте его в файл".to_string()
        }
        ExportError::Write(why) => format!("Не удалось скопировать: SVG не собран ({why})"),
        e => export_message(e),
    }
}

fn field(f: Field) -> NumField {
    match f {
        Field::X => NumField::X,
        Field::Y => NumField::Y,
        Field::W => NumField::W,
        Field::H => NumField::H,
        Field::Rotation => NumField::Rotation,
        Field::Opacity => NumField::Opacity,
        Field::CornerRadius => NumField::CornerRadius,
        Field::StrokeWidth => NumField::StrokeWidth,
    }
}

fn phase(p: EditPhase) -> Phase {
    match p {
        EditPhase::Preview => Phase::Preview,
        EditPhase::Commit => Phase::Commit,
        EditPhase::Cancel => Phase::Cancel,
    }
}

fn color(s: &str) -> Result<Color, String> {
    if s.len() > 16 {
        return Err("Неверный цвет".into());
    }
    Color::parse(s).map_err(|e| e.to_string())
}

/// Хэш изображения — 64 шестнадцатеричных знака (blake3).
fn image_hash(h: &str) -> Option<String> {
    (h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).then(|| h.to_ascii_lowercase())
}

fn fill(d: &FillDto) -> Result<Fill, String> {
    let opacity = if d.opacity.is_finite() {
        d.opacity.clamp(0.0, 1.0)
    } else {
        return Err("Неверная прозрачность заливки".into());
    };
    let mut f = match d.kind {
        PaintKind::Solid => Fill::solid(color(&d.color)?),
        PaintKind::Image => {
            let hash = d
                .image_hash
                .as_deref()
                .and_then(image_hash)
                .ok_or("У заливки изображением нет хэша")?;
            let mut f = Fill::image(hash);
            f.scale_mode = match d.scale_mode.unwrap_or(ScaleModeDto::Fill) {
                ScaleModeDto::Fill => ScaleMode::Fill,
                ScaleModeDto::Fit => ScaleMode::Fit,
                ScaleModeDto::Tile => ScaleMode::Tile,
            };
            f.color = color(&d.color)?;
            f
        }
    };
    debug_assert!(f.kind == FillKind::Solid || f.image_hash.is_some());
    f.opacity = opacity;
    f.visible = d.visible;
    Ok(f)
}

fn stroke(d: &StrokeDto) -> Result<Stroke, String> {
    if !d.width.is_finite() {
        return Err("Неверная толщина обводки".into());
    }
    Ok(Stroke {
        color: color(&d.color)?,
        width: d.width.clamp(0.0, COORD_LIMIT),
        align: match d.align {
            Align::Inside => StrokeAlign::Inside,
            Align::Center => StrokeAlign::Center,
            Align::Outside => StrokeAlign::Outside,
        },
    })
}

fn panel_edit(e: &Edit) -> Result<PanelEdit, String> {
    Ok(match e {
        Edit::Number {
            field: f,
            value,
            relative,
        } => {
            if !value.is_finite() {
                return Err("Значение поля — не число".into());
            }
            PanelEdit::Number {
                field: field(*f),
                value: *value,
                relative: *relative,
            }
        }
        Edit::Fills { fills } => {
            if fills.len() > MAX_PAINTS {
                return Err(format!("Заливок не больше {MAX_PAINTS}"));
            }
            PanelEdit::Fills(fills.iter().map(fill).collect::<Result<_, _>>()?)
        }
        Edit::Strokes { strokes } => {
            if strokes.len() > MAX_PAINTS {
                return Err(format!("Обводок не больше {MAX_PAINTS}"));
            }
            PanelEdit::Strokes(strokes.iter().map(stroke).collect::<Result<_, _>>()?)
        }
        Edit::ClipContent { value } => PanelEdit::ClipContent(*value),
        Edit::Size { w, h } => {
            if !w.is_finite() || !h.is_finite() {
                return Err("Размер — не число".into());
            }
            PanelEdit::Size { w: *w, h: *h }
        }
        Edit::StrokeStyle {
            cap_start,
            cap_end,
            join,
            miter_limit,
        } => {
            if cap_start.is_none() && cap_end.is_none() && join.is_none() && miter_limit.is_none() {
                return Err("В правке стиля обводки нет изменений".into());
            }
            // Недоверенный ввод: предел вне диапазона — отказ, а не подгонка
            // (панель зажимает значения сама, `limits.ts`).
            if let Some(m) = miter_limit {
                if !(m.is_finite() && ferrum_doc::MITER_LIMIT_RANGE.contains(m)) {
                    return Err("Предел острого угла — от 1 до 100".into());
                }
            }
            PanelEdit::StrokeStyle {
                cap_start: cap_start.map(cap_of),
                cap_end: cap_end.map(cap_of),
                join: join.map(join_of),
                miter_limit: *miter_limit,
            }
        }
        Edit::Scale { factor, anchor } => {
            // Недоверенный ввод: вне пределов — отказ (панель зажимает сама).
            if !(factor.is_finite() && (MIN_SCALE_FACTOR..=MAX_SCALE_FACTOR).contains(factor)) {
                return Err("Масштаб — от 1 до 10 000 %".into());
            }
            let Some(anchor) = Anchor::new(anchor[0], anchor[1]) else {
                return Err("Точка масштаба — угол, середина стороны или центр рамки".into());
            };
            PanelEdit::Scale {
                factor: *factor,
                anchor,
            }
        }
    })
}

impl Core {
    /// Строка из команды не длиннее `MAX_TEXT_BYTES`.
    fn text(&mut self, s: &str) -> bool {
        if s.len() > MAX_TEXT_BYTES {
            self.error("Слишком длинная строка в команде");
            return false;
        }
        true
    }

    fn node(&mut self, s: &str) -> Option<NodeId> {
        match s.parse() {
            Ok(id) if s.len() <= 64 => Some(id),
            _ => {
                self.error("Неверный идентификатор слоя");
                None
            }
        }
    }

    fn nodes(&mut self, list: &[String]) -> Option<Vec<NodeId>> {
        if list.len() > SELECTION_IDS_MAX {
            self.error(format!("Не больше {SELECTION_IDS_MAX} слоёв в команде"));
            return None;
        }
        list.iter().map(|s| self.node(s)).collect()
    }

    fn page_id(&mut self, s: &str) -> Option<PageId> {
        match s.parse::<PageId>() {
            Ok(id) if s.len() <= 64 && self.editor.document().pages().iter().any(|p| p.0 == id) => {
                Some(id)
            }
            _ => {
                self.error("Страница не найдена");
                None
            }
        }
    }

    pub(super) fn panel_command(&mut self, cmd: Command) {
        match cmd {
            Command::LayersWindow { start, count } => {
                self.layers.set_window(start, count);
                self.ui.layers = true;
            }
            Command::ExpandLayer { id, expanded } => {
                if let Some(id) = self.node(&id) {
                    self.layers.set_expanded(id, expanded);
                    self.ui.layers = true;
                }
            }
            Command::FilterLayers { query } => {
                if self.text(&query) {
                    self.layers.set_filter(&query);
                    self.ui.layers = true;
                }
            }
            Command::PrepareSearch => self.layers.prepare_search(self.editor.scene()),
            Command::Select { ids, mode } => {
                if let Some(ids) = self.nodes(&ids) {
                    self.select(ids, mode);
                }
            }
            Command::HoverLayer { id } => match id {
                None => self.editor.set_hover(None),
                Some(s) => {
                    if let Some(id) = self.node(&s) {
                        self.editor.set_hover(Some(id));
                    }
                }
            },
            Command::RenameLayer { id, name } => {
                if self.text(&name) {
                    if let Some(id) = self.node(&id) {
                        self.editor.rename_node(id, &name);
                        // Пустое имя ничего не меняет: панель должна вернуть прежнее.
                        self.layers.resend();
                        self.ui.layers = true;
                    }
                }
            }
            Command::MoveLayers {
                ids,
                target,
                position,
                selection_version,
            } => {
                // Без id (выделение больше предела id) — только если выделение то же,
                // что в начале перетаскивания.
                if ids.is_none()
                    && selection_version
                        .is_some_and(|v| v != self.editor.selection_version() as f64)
                {
                    self.error(STALE_SELECTION);
                    self.layers.resend();
                    self.ui.layers = true;
                    return;
                }
                let ids = match ids {
                    Some(list) => self.nodes(&list),
                    None => Some(self.editor.selection().to_vec()),
                };
                if let (Some(ids), Some(t)) = (ids, self.node(&target)) {
                    let target = match position {
                        DropPosition::Above => DropTarget::Above(t),
                        DropPosition::Below => DropTarget::Below(t),
                        DropPosition::Into => DropTarget::Into(t),
                    };
                    if !self.editor.move_layers(&ids, target) {
                        // Перенос не удался (в себя, в лист): строки вернуть как были.
                        self.layers.resend();
                        self.ui.layers = true;
                    }
                }
            }
            Command::SetLayerFlags {
                ids,
                visible,
                locked,
            } => {
                if let Some(ids) = self.nodes(&ids) {
                    self.editor.set_flags(&ids, visible, locked);
                }
            }
            Command::EditSelection {
                edit,
                phase: p,
                ids,
                selection_version,
            } => match (panel_edit(&edit), ids.as_deref().map(|l| self.nodes(l))) {
                // Правка без id (выделение больше предела id) — только если выделение то же.
                (Ok(_), None)
                    if selection_version
                        .is_some_and(|v| v != self.editor.selection_version() as f64) =>
                {
                    self.editor.cancel_gesture();
                    // Ошибка — только о записи: превью и отмена жеста молчат.
                    if p == EditPhase::Commit {
                        self.error(STALE_SELECTION);
                    }
                }
                (Ok(e), None) => self.editor.edit_selection(&e, phase(p)),
                (Ok(e), Some(Some(ids))) => self.editor.edit_nodes(&e, phase(p), &ids),
                (Ok(_), Some(None)) => {
                    // Ошибку об идентификаторах уже выдал `nodes`; жест не висит.
                    if p != EditPhase::Preview {
                        self.editor.cancel_gesture();
                    }
                }
                (Err(m), _) => {
                    // Жест не должен остаться висеть из-за неверного шага.
                    if p != EditPhase::Preview {
                        self.editor.cancel_gesture();
                    }
                    self.error(format!("Неверная правка: {m}"));
                }
            },
            Command::AddPage => {
                match self.editor.create_page() {
                    Ok(_) => self.touch(),
                    Err(e) => self.error(e.to_string()),
                }
                self.ui.pages = true;
            }
            Command::RenamePage { id, name } => {
                if self.text(&name) {
                    if let Some(id) = self.page_id(&id) {
                        let before = self.editor.document().pages();
                        match self.editor.rename_page(id, &name) {
                            Ok(()) => {
                                if self.editor.document().pages() != before {
                                    self.touch();
                                }
                            }
                            Err(e) => self.error(e.to_string()),
                        }
                    }
                }
                self.ui.pages = true;
            }
            Command::DeletePage { id } => {
                if let Some(id) = self.page_id(&id) {
                    match self.editor.delete_page(id) {
                        Ok(()) => self.touch(),
                        Err(e) => self.error(e.to_string()),
                    }
                }
                self.ui.pages = true;
            }
            Command::SetPage { id } => {
                if let Some(id) = self.page_id(&id) {
                    if id != self.editor.page() {
                        if let Err(e) = self.editor.set_page(id) {
                            self.error(e.to_string());
                        }
                    }
                }
            }
            Command::SetPageBackground { color: c, phase } => {
                // Отмена жеста не читает цвет: неверный не должен оставить превью.
                let c = if phase == EditPhase::Cancel { None } else { c };
                let parsed = match c.as_deref().map(color).transpose() {
                    Ok(v) => v,
                    Err(m) => {
                        self.error(m);
                        return;
                    }
                };
                self.dirty = true;
                match phase {
                    // Фон страницы не в истории отмены, но и сотни записей в документ
                    // за один жест палитры не нужны: превью живёт только в кадре.
                    EditPhase::Preview => self.background_preview = Some(parsed),
                    EditPhase::Cancel => self.background_preview = None,
                    EditPhase::Commit => {
                        self.background_preview = None;
                        let page = self.editor.page();
                        let before = self.editor.page_background();
                        match self.editor.set_page_background(page, parsed) {
                            Ok(()) => {
                                if self.editor.page_background() != before {
                                    self.touch();
                                }
                            }
                            Err(e) => self.error(e.to_string()),
                        }
                        self.ui.pages = true;
                    }
                }
            }
            Command::SetDocumentName { name, quiet } => {
                if self.text(&name) {
                    let before = self.editor.document().name();
                    match self.editor.set_document_name(&name) {
                        Ok(name) => {
                            if name != before && quiet != Some(true) {
                                self.touch();
                            }
                            self.events.push(CoreEvent::DocumentName {
                                name: event_name(&name),
                            });
                        }
                        Err(e) => self.error(e.to_string()),
                    }
                }
            }
            Command::AddFramePreset { index } => match frame_presets().get(index as usize) {
                Some(p) => self.editor.add_frame_preset(p),
                None => self.error("Нет такого пресета"),
            },
            Command::SetZoom { zoom } => {
                if zoom.is_finite() && zoom > 0.0 {
                    let mut cam = self.editor.camera().clone();
                    cam.set_zoom(zoom);
                    self.editor.set_camera(cam);
                } else {
                    self.error("Неверный масштаб");
                }
            }
            Command::Export {
                scale,
                format,
                outline_text,
                svg_ids,
            } => {
                let svg = SvgOptions {
                    outline_text: outline_text.unwrap_or(true),
                    ids: svg_ids.unwrap_or(false),
                };
                self.export(scale, format.unwrap_or_default(), svg)
            }
            Command::PasteImage { bytes } => self.paste_image(bytes),
            Command::Bind {
                command,
                shortcut,
                replace,
            } => self.bind(&command, &shortcut, replace),
            Command::Unbind { command, shortcut } => {
                if let (Some(id), Some(s)) = (self.command_id(&command), self.shortcut(&shortcut)) {
                    self.editor.keymap_mut().unbind(id, &s);
                    self.push_keymap();
                }
            }
            Command::ResetKeymap { command } => {
                match command {
                    None => self.editor.keymap_mut().reset_all(),
                    Some(c) => match self.command_id(&c) {
                        Some(id) => self.editor.keymap_mut().reset(id),
                        None => return,
                    },
                }
                self.push_keymap();
            }
            Command::ImportKeymap { json } => match self.editor.keymap_mut().import_json(&json) {
                Ok(warnings) => {
                    self.events.push(CoreEvent::KeymapImported { warnings });
                    self.push_keymap();
                }
                // Текст разбора JSON — английский: пользователю — своими словами.
                Err(KeymapError::BadFormat(_)) => self.error(
                    "Не удалось импортировать раскладку: это не файл раскладки Ferrum (JSON)",
                ),
                Err(e) => self.error(format!("Не удалось импортировать раскладку: {e}")),
            },
            Command::SetSettings { pixel_snap } => {
                let mut s = self.editor.settings().clone();
                s.pixel_snap = pixel_snap;
                self.editor.set_settings(s);
            }
            // Команды этапа 4 разбирает `Core::command`. Список полный, без `_`:
            // новая команда без ветки здесь — ошибка сборки, а не молчаливый пропуск.
            Command::Open { .. }
            | Command::Demo { .. }
            | Command::OpenWithJournal { .. }
            | Command::FlushJournal
            | Command::Execute { .. }
            | Command::SetTool { .. }
            | Command::SetTheme { .. }
            | Command::Paste { .. }
            | Command::PlaceImage { .. }
            | Command::Save
            | Command::Snapshot { .. }
            | Command::MarkSaved { .. }
            | Command::ReadPixels { .. }
            | Command::ReadMeasurements
            | Command::DemoText { .. }
            | Command::FontsAvailable { .. }
            | Command::LoadFont { .. }
            | Command::ReplaceMissingFont { .. }
            | Command::TestCreateText { .. }
            | Command::TextEditStart { .. }
            | Command::TextEditEnd
            | Command::TextInput { .. }
            | Command::TextKey { .. }
            | Command::SetTextStyle { .. }
            | Command::SetTextProps { .. } => {
                debug_assert!(false, "команда ядра — в `Core::command`");
            }
        }
    }

    fn select(&mut self, ids: Vec<NodeId>, mode: SelectMode) {
        let next = match mode {
            SelectMode::Replace => {
                self.layers.set_anchor(ids.last().copied());
                ids
            }
            SelectMode::Toggle => {
                let mut cur = self.editor.selection().to_vec();
                for id in &ids {
                    match cur.iter().position(|c| c == id) {
                        Some(i) => {
                            cur.remove(i);
                        }
                        None => cur.push(*id),
                    }
                }
                self.layers.set_anchor(ids.last().copied());
                cur
            }
            SelectMode::Range => {
                let Some(to) = ids.last().copied() else {
                    return;
                };
                let from = self.layers.anchor().unwrap_or(to);
                self.layers.range(self.editor.scene(), from, to)
            }
        };
        self.from_panel = true;
        self.editor.select(&next);
        self.drain_editor();
        self.from_panel = false;
    }

    fn command_id(&mut self, key: &str) -> Option<CommandId> {
        let id = CommandId::from_key(key);
        if id.is_none() {
            self.error(format!("Неизвестная команда «{}»", super::clip(key)));
        }
        id
    }

    fn shortcut(&mut self, s: &str) -> Option<Shortcut> {
        if s.len() > 64 {
            self.error("Неверное сочетание");
            return None;
        }
        match Shortcut::parse(s) {
            Ok(sc) => Some(sc),
            Err(e) => {
                self.error(e.to_string());
                None
            }
        }
    }

    fn bind(&mut self, command: &str, shortcut: &str, replace: bool) {
        let (Some(id), Some(s)) = (self.command_id(command), self.shortcut(shortcut)) else {
            return;
        };
        match self.editor.keymap_mut().bind(id, s.clone(), replace) {
            Ok(()) => self.push_keymap(),
            Err(BindError::Conflict(c)) => self.events.push(CoreEvent::BindConflict {
                command: id.key().to_string(),
                shortcut: s.to_string(),
                commands: c.commands.iter().map(|c| c.key().to_string()).collect(),
            }),
            Err(e @ BindError::TooMany) => self.error(e.to_string()),
        }
    }

    /// Таблица команд и раскладка — после каждого изменения раскладки.
    pub(super) fn push_keymap(&mut self) {
        let km = self.editor.keymap();
        let platform = km.platform();
        let commands = COMMANDS
            .iter()
            .map(|c| KeymapEntry {
                key: c.key.to_string(),
                name: c.name.to_string(),
                group: c.key.split('.').next().unwrap_or("").to_string(),
                bindings: km.bindings(c.id).iter().map(|s| s.to_string()).collect(),
                labels: km
                    .bindings(c.id)
                    .iter()
                    .map(|s| s.label(platform))
                    .collect(),
                defaults: km
                    .default_bindings(c.id)
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                overridden: km.is_overridden(c.id),
            })
            .collect();
        let conflicts = km
            .conflicts()
            .into_iter()
            .map(|c| ConflictInfo {
                shortcut: c.shortcut.to_string(),
                label: c.shortcut.label(platform),
                commands: c.commands.iter().map(|id| id.key().to_string()).collect(),
            })
            .collect();
        let reserved = browser_reserved(platform)
            .iter()
            .map(|s| s.to_string())
            .collect();
        let overrides = km.export_json();
        self.events
            .retain(|e| !matches!(e, CoreEvent::Keymap { .. }));
        self.events.push(CoreEvent::Keymap {
            commands,
            conflicts,
            reserved,
            overrides,
        });
    }

    pub(super) fn push_presets(&mut self) {
        let presets = frame_presets()
            .iter()
            .map(|p| PresetInfo {
                group: p.group.clone(),
                name: p.name.clone(),
                width: p.width,
                height: p.height,
            })
            .collect();
        self.events.push(CoreEvent::Presets { presets });
    }

    fn export(&mut self, scale: f64, format: ExportFormat, svg: SvgOptions) {
        if !EXPORT_SCALES.contains(&scale) {
            self.error("Масштаб экспорта — 1×, 2× или 3×");
            return;
        }
        let scene = self.editor.scene();
        let selection = self.editor.selection().to_vec();
        let (ids, name) = if selection.is_empty() {
            let page = self.editor.page();
            let name = self
                .editor
                .document()
                .pages()
                .into_iter()
                .find(|p| p.0 == page)
                .map(|p| p.1)
                .unwrap_or_default();
            (
                scene
                    .roots()
                    .iter()
                    .map(|s| scene.at(*s).id)
                    .collect::<Vec<_>>(),
                name,
            )
        } else {
            let name = match selection.as_slice() {
                [one] => scene
                    .node(*one)
                    .map(|n| n.props.name.clone())
                    .unwrap_or_default(),
                _ => "Выделение".to_string(),
            };
            (selection.clone(), name)
        };
        if ids.is_empty() {
            self.error("Нечего экспортировать: страница пуста");
            return;
        }
        // Текст, чей шрифт ещё грузится, в экспорт не попал бы: лучше ошибка.
        if super::fonts::fonts_loading(self.editor.scene(), &self.fonts) {
            self.error(super::fonts::FONTS_LOADING);
            return;
        }
        let before = self.images.placeholders_served();
        let budget = ExportBudget::new(self.export_clock.as_deref());
        let result = match format {
            ExportFormat::Png => {
                let provider = self.images.export_provider(&self.editor.document().images);
                self.renderer
                    .export_png_nodes(self.editor.scene(), &ids, scale, &provider)
            }
            ExportFormat::Svg => self
                .renderer
                .export_svg(
                    self.editor.scene(),
                    &ids,
                    &self.editor.document().images,
                    svg,
                    &budget,
                )
                .map(String::into_bytes),
            ExportFormat::Pdf => match pdf_pages(self.editor.scene(), &selection, &ids) {
                Some(pages) => {
                    let provider = self.images.export_provider(&self.editor.document().images);
                    self.renderer
                        .export_pdf(self.editor.scene(), &pages, &provider, &budget)
                }
                None => Err(ExportError::Empty),
            },
        };
        if self.images.placeholders_served() != before {
            self.error("Не удалось экспортировать: часть изображений не читается или не помещается в память");
            return;
        }
        match result {
            Ok(bytes) => self.events.push(CoreEvent::Exported {
                name: file_name(&name, scale, format),
                bytes: ByteBuf::from(bytes),
                mime: format.mime().to_string(),
            }),
            Err(e) => self.error(export_message(&e)),
        }
    }

    fn paste_image(&mut self, bytes: ByteBuf) {
        if bytes.len() > MAX_PASTE_BYTES {
            self.error("Не удалось вставить изображение: файл больше 64 МБ");
            return;
        }
        let Some(pixmap) = decode_png(&bytes) else {
            self.error("Не удалось вставить изображение: нужен PNG не больше 8192 px");
            return;
        };
        let (w, h) = (pixmap.width() as f64, pixmap.height() as f64);
        let hash = self.editor.add_image(bytes.into_vec());
        self.images.insert(&hash, pixmap);
        self.editor.insert_image(&hash, w, h);
    }

    /// События панелей после пачки: окно слоёв, сводка, страницы — только если
    /// изменились.
    pub(super) fn flush_ui(&mut self) {
        let ui = std::mem::take(&mut self.ui);
        if ui.layers {
            if self.reveal.is_some() {
                self.layers.resend();
            }
            if let Some(w) = self.layers.window(self.editor.scene(), &self.selected) {
                self.events
                    .retain(|e| !matches!(e, CoreEvent::Layers { .. }));
                self.events.push(CoreEvent::Layers {
                    start: w.start,
                    total: w.total,
                    rows: w.rows,
                    reveal: self.reveal.take(),
                });
            }
        }
        if ui.properties {
            let s = summarize(self.editor.scene(), self.editor.selection());
            if self.summary.as_ref() != Some(&s) {
                self.summary = Some(s.clone());
                self.events
                    .retain(|e| !matches!(e, CoreEvent::Properties { .. }));
                self.events.push(CoreEvent::Properties { summary: s });
            }
        }
        if ui.pages {
            let doc = self.editor.document();
            let pages = doc
                .pages()
                .into_iter()
                .map(|(id, name)| PageInfo {
                    id: id.to_string(),
                    name: event_name(&name),
                })
                .collect();
            let background = self.editor.page_background().map(|c| c.to_hex());
            self.events
                .retain(|e| !matches!(e, CoreEvent::Pages { .. }));
            self.events.push(CoreEvent::Pages {
                pages,
                current: self.editor.page().to_string(),
                background,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Каждая ошибка экспорта — законченная фраза по-русски, без двойного «Не
    /// удалось экспортировать: Экспорт…» и без паники (решение 24 плана).
    #[test]
    fn export_errors_are_clear_russian_messages() {
        let cases = [
            (
                ExportError::TooBig,
                "Экспорт слишком велик: файл или его содержимое больше 256 МБ. Экспортируйте часть документа",
            ),
            (
                ExportError::Timeout,
                "Экспорт идёт дольше 30 секунд и прерван: экспортируйте часть документа",
            ),
            (
                ExportError::Empty,
                "Нечего экспортировать: у узла нулевой размер или он скрыт",
            ),
            (
                ExportError::NotFound,
                "Нечего экспортировать: слой не найден",
            ),
            (ExportError::BadScale, "Масштаб экспорта — 1×, 2× или 3×"),
            (
                ExportError::Write("шрифт не встраивается".into()),
                "Не удалось экспортировать: файл не собран (шрифт не встраивается)",
            ),
            (
                ExportError::Encode("нет памяти".into()),
                "Не удалось экспортировать: PNG не закодирован (нет памяти)",
            ),
        ];
        for (e, text) in cases {
            assert_eq!(export_message(&e), text, "{e:?}");
        }
        assert!(export_message(&ExportError::TooLarge(20_000, 10)).starts_with("Картинка 20000×10"));
    }

    /// Ошибки «Копировать как SVG» — без двойной приставки.
    #[test]
    fn copy_svg_errors_have_one_prefix() {
        assert_eq!(
            copy_svg_message(&ExportError::Timeout),
            ExportError::Timeout.to_string()
        );
        assert_eq!(
            copy_svg_message(&ExportError::Empty),
            ExportError::Empty.to_string()
        );
        assert_eq!(
            copy_svg_message(&ExportError::TooBig),
            "Не удалось скопировать: SVG больше 64 МБ — экспортируйте его в файл"
        );
        assert_eq!(
            copy_svg_message(&ExportError::Write("x".into())),
            "Не удалось скопировать: SVG не собран (x)"
        );
        assert_eq!(
            copy_svg_message(&ExportError::NotFound),
            "Нечего экспортировать: слой не найден"
        );
    }

    /// Имя файла: как в Figma — при 1× без «@1x», при 2× и 3× — с суффиксом;
    /// у SVG и PDF масштаба в имени нет.
    #[test]
    fn png_name_has_no_suffix_at_1x() {
        assert_eq!(file_name("Кнопка", 1.0, ExportFormat::Png), "Кнопка.png");
        assert_eq!(file_name("Кнопка", 2.0, ExportFormat::Png), "Кнопка@2x.png");
        assert_eq!(file_name("Кнопка", 3.0, ExportFormat::Png), "Кнопка@3x.png");
        assert_eq!(file_name("Кнопка", 1.0, ExportFormat::Svg), "Кнопка.svg");
        assert_eq!(file_name(" a/b ", 1.0, ExportFormat::Pdf), "a_b.pdf");
    }

    /// Страница документа с узлами для `pdf_pages`.
    struct Doc {
        doc: ferrum_doc::Document,
        page: PageId,
    }

    impl Doc {
        fn new() -> Doc {
            let doc = ferrum_doc::Document::new();
            let page = doc.pages()[0].0;
            Doc { doc, page }
        }

        fn add(
            &mut self,
            parent: Option<NodeId>,
            kind: ferrum_doc::NodeKind,
            (x, y, w, h): (f64, f64, f64, f64),
            visible: bool,
        ) -> NodeId {
            let patch = ferrum_doc::PropsPatch {
                x: Some(x),
                y: Some(y),
                w: Some(w),
                h: Some(h),
                visible: Some(visible),
                // Без обрезки: потомок за краем расширяет границы поддерева, а
                // страница PDF — всё равно форма фрейма.
                clip_content: (kind == ferrum_doc::NodeKind::Frame).then_some(false),
                fills: Some(vec![Fill::solid(Color::BLACK)]),
                ..Default::default()
            };
            self.doc
                .transact(self.page, vec![], |p| {
                    let id = p.create_node(parent, None, kind)?;
                    p.set_props(id, &patch)?;
                    Ok(vec![id])
                })
                .unwrap()[0]
        }

        fn scene(&mut self) -> ferrum_scene::Scene {
            self.doc.take_changes(self.page);
            ferrum_scene::Scene::from_page(self.doc.page(self.page).unwrap())
        }
    }

    fn roots(scene: &ferrum_scene::Scene) -> Vec<NodeId> {
        scene.roots().iter().map(|s| scene.at(*s).id).collect()
    }

    /// Порядок страниц — по области самой страницы (форме фрейма), а не по
    /// потомку, что выступает за край фрейма вверх: сверху вниз, затем слева
    /// направо. Фреймы созданы не по порядку чтения.
    #[test]
    fn pdf_pages_follow_reading_order_of_frame_shapes() {
        use ferrum_doc::NodeKind::{Frame, Rect};
        let mut d = Doc::new();
        let low_right = d.add(None, Frame, (500.0, 400.0, 100.0, 100.0), true);
        let top_right = d.add(None, Frame, (500.0, 0.0, 100.0, 100.0), true);
        // Фрейм ниже `top_left`, но его потомок выступает вверх за край: по
        // границам поддерева он оказался бы первым.
        let low_left = d.add(None, Frame, (0.0, 200.0, 100.0, 100.0), true);
        d.add(Some(low_left), Rect, (0.0, -500.0, 50.0, 550.0), true);
        let top_left = d.add(None, Frame, (0.0, 0.0, 100.0, 100.0), true);
        let scene = d.scene();
        let pages = pdf_pages(&scene, &[], &roots(&scene)).unwrap();
        assert_eq!(
            pages,
            [
                vec![top_left],
                vec![top_right],
                vec![low_left],
                vec![low_right]
            ]
        );
        // С выделением — тот же порядок чтения.
        let pages = pdf_pages(&scene, &[low_right, low_left, top_left], &roots(&scene)).unwrap();
        assert_eq!(pages, [vec![top_left], vec![low_left], vec![low_right]]);
    }

    /// (поведение) Скрытый фрейм страницей не становится; фреймы есть, но все
    /// скрыты — «нечего экспортировать», а не пустые страницы. Без фреймов —
    /// вся страница одной.
    #[test]
    fn pdf_pages_skip_hidden_frames() {
        use ferrum_doc::NodeKind::{Frame, Rect};
        let mut d = Doc::new();
        d.add(None, Frame, (0.0, 0.0, 100.0, 100.0), false);
        d.add(None, Rect, (0.0, 300.0, 100.0, 100.0), true);
        let scene = d.scene();
        assert_eq!(pdf_pages(&scene, &[], &roots(&scene)), None);
        let shown = d.add(None, Frame, (200.0, 0.0, 100.0, 100.0), true);
        let scene = d.scene();
        assert_eq!(
            pdf_pages(&scene, &[], &roots(&scene)),
            Some(vec![vec![shown]])
        );
        let mut d = Doc::new();
        let only = d.add(None, Rect, (0.0, 0.0, 10.0, 10.0), true);
        let scene = d.scene();
        assert_eq!(
            pdf_pages(&scene, &[], &roots(&scene)),
            Some(vec![vec![only]])
        );
    }
}
