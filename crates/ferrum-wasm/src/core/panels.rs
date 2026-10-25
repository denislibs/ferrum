//! Команды панелей (этап 5): слои, свойства, страницы, экспорт, раскладка,
//! настройки — и события для них. Всё, что приходит из основного потока, не
//! доверенное: идентификаторы, строки и списки проверяются и ограничиваются,
//! неверное — событие `Error` на русском, документ не меняется.

use ferrum_doc::{Color, Fill, FillKind, NodeId, PageId, ScaleMode, Stroke, StrokeAlign};
use ferrum_editor::{
    browser_reserved, frame_presets, BindError, CommandId, DropTarget, KeymapError, NumField,
    PanelEdit, Phase, Shortcut, COMMANDS, MAX_NAME_CHARS,
};
use ferrum_render::decode_png;
use ferrum_scene::shape::COORD_LIMIT;
use serde_bytes::ByteBuf;

use super::Core;
use crate::protocol::{
    Align, Command, ConflictInfo, CoreEvent, DropPosition, Edit, EditPhase, Field, FillDto,
    KeymapEntry, PageInfo, PaintKind, PresetInfo, ScaleModeDto, SelectMode, StrokeDto,
    EXPORT_SCALES, MAX_TEXT_BYTES, SELECTION_IDS_MAX,
};
use crate::summary::{summarize, MAX_PAINTS};

/// Наибольший размер картинки из буфера обмена (`PasteImage`).
pub const MAX_PASTE_BYTES: usize = 64 << 20;

/// Имя для события: имя страницы или документа из файла может быть любой длины.
pub(super) fn event_name(name: &str) -> String {
    name.chars().take(MAX_NAME_CHARS).collect()
}

/// Имя файла экспорта: без знаков, которые не любят файловые системы.
fn file_name(name: &str, scale: f64) -> String {
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
    format!("{base}@{scale}x.png")
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
            } => {
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
                    if p != EditPhase::Preview {
                        self.error("Выделение изменилось — правка не применена");
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
                if let Err(e) = self.editor.create_page() {
                    self.error(e.to_string());
                }
                self.ui.pages = true;
            }
            Command::RenamePage { id, name } => {
                if self.text(&name) {
                    if let Some(id) = self.page_id(&id) {
                        if let Err(e) = self.editor.rename_page(id, &name) {
                            self.error(e.to_string());
                        }
                    }
                }
                self.ui.pages = true;
            }
            Command::DeletePage { id } => {
                if let Some(id) = self.page_id(&id) {
                    if let Err(e) = self.editor.delete_page(id) {
                        self.error(e.to_string());
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
                        if let Err(e) = self.editor.set_page_background(page, parsed) {
                            self.error(e.to_string());
                        }
                        self.ui.pages = true;
                    }
                }
            }
            Command::SetDocumentName { name } => {
                if self.text(&name) {
                    match self.editor.set_document_name(&name) {
                        Ok(name) => self.events.push(CoreEvent::DocumentName {
                            name: event_name(&name),
                        }),
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
            Command::Export { scale } => self.export(scale),
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
            // Команды этапа 4 разбирает `Core::command`.
            _ => {}
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

    fn export(&mut self, scale: f64) {
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
            (selection, name)
        };
        if ids.is_empty() {
            self.error("Нечего экспортировать: страница пуста");
            return;
        }
        let before = self.images.placeholders_served();
        let provider = self.images.export_provider(&self.editor.document().images);
        let result = self
            .renderer
            .export_png_nodes(self.editor.scene(), &ids, scale, &provider);
        if self.images.placeholders_served() != before {
            self.error("Не удалось экспортировать: часть изображений не читается или не помещается в память");
            return;
        }
        match result {
            Ok(png) => self.events.push(CoreEvent::Exported {
                name: file_name(&name, scale),
                bytes: ByteBuf::from(png),
            }),
            Err(e) => self.error(format!("Не удалось экспортировать: {e}")),
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
