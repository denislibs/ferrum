//! Ядро worker'а без браузера: редактор, рендерер и картинки документа, ввод из
//! кольцевого буфера, команды и события протокола, решение «нужен ли кадр» и живое
//! состояние. Бэкенд рендера и часы даёт вызывающий: в браузере — поверхность
//! WebGPU, WebGL2 или CPU и `performance.now()` (модуль `web`), в тестах —
//! `CpuBackend`.
//!
//! Кадр рисуется только когда изменились сцена, камера или оверлей (событие
//! редактора `NeedsRedraw`) или прошлый кадр был неполным
//! (`FrameStats::needs_another_frame`: растры фреймов не догнали, картинки ещё
//! декодируются).

mod panels;

use std::collections::{HashSet, VecDeque};

use ferrum_doc::{container, fixture, Document, NodeId};
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::{CommandId, Cursor, Editor, EditorEvent, Input, Platform, ToolKind};
use ferrum_render::{decode_png, FrameStats, OverlayTheme, RenderBackend, Renderer, Rgba};
use kurbo::{Point, Size};
use serde_bytes::ByteBuf;

use crate::images::DocImages;
use crate::input::{decode_all, Record};
use crate::layers::Layers;
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES, SELECTION_IDS_MAX,
};
use crate::summary::Summary;

/// Сколько последних кадров учитывают медиана и 95-й процентиль времени кадра.
pub const FRAME_WINDOW: usize = 120;
/// Фон холста, если у страницы свой не задан: токен `canvas` дизайн-системы
/// (#1a1918 в тёмной теме, #e9e7e4 в светлой).
pub const CANVAS_DARK: Rgba = Rgba::new([26.0 / 255.0, 25.0 / 255.0, 24.0 / 255.0, 1.0]);
pub const CANVAS_LIGHT: Rgba = Rgba::new([233.0 / 255.0, 231.0 / 255.0, 228.0 / 255.0, 1.0]);
/// Масштаб «Скопировать как PNG».
pub const COPY_PNG_SCALE: f64 = 1.0;

/// Значения выделения для живого состояния и ключ, при котором они посчитаны.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SelectionInfo {
    key: (u64, u64, usize),
    count: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rotation: f64,
}

pub struct Core {
    editor: Editor,
    renderer: Renderer,
    images: DocImages,
    backend: Backend,
    platform: Platform,
    theme: Theme,
    dirty: bool,
    events: Vec<CoreEvent>,
    /// Запрошены пиксели следующего кадра (`ReadPixels`): `Some(reference)`.
    pixels: Option<bool>,
    inputs: u64,
    skipped: u64,
    frames: u64,
    frame_ms: f64,
    times: VecDeque<f64>,
    stamps: VecDeque<f64>,
    selection: Option<SelectionInfo>,
    atlas: (usize, u64),
    /// Панели (этап 5): дерево слоёв, выделенные для строк, что пересчитать.
    layers: Layers,
    selected: HashSet<NodeId>,
    ui: UiDirty,
    /// Выделение меняет панель слоёв: предков не раскрывать, к строке не прокручивать.
    from_panel: bool,
    reveal: Option<u32>,
    summary: Option<Summary>,
    /// Фон страницы во время жеста палитры: в кадре, но ещё не в документе.
    background_preview: Option<Option<ferrum_doc::Color>>,
}

/// Что из панелей пересчитать после пачки событий редактора.
#[derive(Debug, Default, Clone, Copy)]
struct UiDirty {
    layers: bool,
    properties: bool,
    pages: bool,
}

fn tool_kind(t: Tool) -> ToolKind {
    match t {
        Tool::Move => ToolKind::Move,
        Tool::Frame => ToolKind::Frame,
        Tool::Rect => ToolKind::Rect,
        Tool::Ellipse => ToolKind::Ellipse,
        Tool::Line => ToolKind::Line,
        Tool::Hand => ToolKind::Hand,
        Tool::Image => ToolKind::Image,
    }
}

fn tool(k: ToolKind) -> Tool {
    match k {
        ToolKind::Move => Tool::Move,
        ToolKind::Frame => Tool::Frame,
        ToolKind::Rect => Tool::Rect,
        ToolKind::Ellipse => Tool::Ellipse,
        ToolKind::Line => Tool::Line,
        ToolKind::Hand => Tool::Hand,
        ToolKind::Image => Tool::Image,
    }
}

fn cursor(c: Cursor) -> (CursorKind, f64) {
    match c {
        Cursor::Default => (CursorKind::Default, 0.0),
        Cursor::Crosshair => (CursorKind::Crosshair, 0.0),
        Cursor::Grab => (CursorKind::Grab, 0.0),
        Cursor::Grabbing => (CursorKind::Grabbing, 0.0),
        Cursor::Duplicate => (CursorKind::Duplicate, 0.0),
        Cursor::Resize { angle } => (CursorKind::Resize, angle),
        Cursor::Rotate { angle } => (CursorKind::Rotate, angle),
    }
}

/// Событие выделения: точное число и не больше `SELECTION_IDS_MAX` идентификаторов
/// (лишние отбрасывает и сама функция, чтобы предел не зависел от вызывающего).
fn selection_event(count: usize, ids: impl Iterator<Item = String>) -> CoreEvent {
    CoreEvent::Selection {
        count: count as u32,
        ids: ids.take(SELECTION_IDS_MAX).collect(),
    }
}

/// Команда в тексте ошибки: чужая строка может быть любой длины.
fn clip(text: &str) -> String {
    let mut it = text.chars();
    let head: String = it.by_ref().take(64).collect();
    if it.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

fn overlay_theme(t: Theme) -> OverlayTheme {
    match t {
        Theme::Dark => OverlayTheme::dark(),
        Theme::Light => OverlayTheme::light(),
    }
}

impl Core {
    /// Ядро с пустым документом для холста `viewport` (CSS-пиксели) и `dpr`.
    pub fn new(platform: Platform, viewport: Size, dpr: f64, backend: Backend) -> Core {
        let editor = Editor::new(Document::new(), None, viewport, dpr, platform)
            .expect("в новом документе есть страница");
        let mut core = Core {
            editor,
            renderer: Renderer::new(),
            images: DocImages::default(),
            backend,
            platform,
            theme: Theme::Dark,
            dirty: true,
            events: Vec::new(),
            pixels: None,
            inputs: 0,
            skipped: 0,
            frames: 0,
            frame_ms: 0.0,
            times: VecDeque::with_capacity(FRAME_WINDOW),
            stamps: VecDeque::new(),
            selection: None,
            atlas: (0, 0),
            layers: Layers::new(),
            selected: HashSet::new(),
            ui: UiDirty::default(),
            from_panel: false,
            reveal: None,
            summary: None,
            background_preview: None,
        };
        // Таблица команд и пресеты нужны интерфейсу сразу, до документа.
        core.push_keymap();
        core.push_presets();
        core
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    /// Дерево слоёв панели (для тестов: кэши и счётчики).
    pub fn layers(&self) -> &Layers {
        &self.layers
    }

    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    /// Бэкенд сменился (запасной путь после ошибки GPU): кадр нужен заново.
    pub fn set_backend(&mut self, backend: Backend) {
        self.backend = backend;
        self.dirty = true;
    }

    /// Размер кадра в пикселях устройства.
    pub fn device_size(&self) -> (u16, u16) {
        self.editor.camera().device_size()
    }

    /// Заменяет документ: новый редактор с тем же холстом, темой, раскладкой клавиш
    /// и настройками — они принадлежат пользователю, а не документу.
    fn replace_document(&mut self, doc: Document) -> Result<(), String> {
        let cam = self.editor.camera();
        let (viewport, dpr) = (cam.viewport(), cam.dpr());
        let mut editor =
            Editor::new(doc, None, viewport, dpr, self.platform).map_err(|e| e.to_string())?;
        editor.set_theme(overlay_theme(self.theme));
        editor.set_settings(self.editor.settings().clone());
        *editor.keymap_mut() = self.editor.keymap().clone();
        editor.take_events();
        self.editor = editor;
        self.images = DocImages::default();
        self.selection = None;
        self.layers.reset();
        self.selected.clear();
        self.summary = None;
        self.dirty = true;
        Ok(())
    }

    fn push_opened(&mut self, warnings: Vec<String>) {
        let pages = self.editor.document().pages().len() as u32;
        let nodes = self.editor.scene().len() as u32;
        let name = panels::event_name(&self.editor.document().name());
        self.events.push(CoreEvent::Opened {
            pages,
            nodes,
            warnings,
            name,
        });
        self.push_document();
        self.events.push(CoreEvent::Selection {
            count: 0,
            ids: vec![],
        });
        self.events.push(CoreEvent::Tool {
            tool: tool(self.editor.tool_kind()),
        });
        self.ui = UiDirty {
            layers: true,
            properties: true,
            pages: true,
        };
        self.flush_ui();
    }

    fn push_document(&mut self) {
        let doc = self.editor.document();
        self.events.push(CoreEvent::Document {
            can_undo: doc.can_undo(),
            can_redo: doc.can_redo(),
            steps: self.editor.step_count() as f64,
        });
    }

    fn error(&mut self, message: impl Into<String>) {
        self.events.push(CoreEvent::Error {
            message: message.into(),
        });
    }

    /// Открывает файл `.ferrum`. При ошибке текущий документ остаётся как был.
    pub fn open(&mut self, bytes: &[u8]) {
        match container::decode(bytes) {
            Ok(opened) => match self.replace_document(opened.document) {
                Ok(()) => self.push_opened(opened.warnings),
                Err(e) => self.error(e),
            },
            Err(e) => self.error(format!("Не удалось открыть файл: {e}")),
        }
    }

    /// Новый документ — страница-пример из `nodes` узлов (генератор бенчмарков:
    /// фреймы 1440 × 1024 по 99 фигур). Камера — на первом фрейме при зуме 50 %.
    pub fn demo(&mut self, nodes: u32) {
        if nodes > MAX_DEMO_NODES {
            self.error(format!("Пример не больше {MAX_DEMO_NODES} узлов"));
            return;
        }
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        if let Err(e) = doc.transact(page, vec![], |p| {
            fixture::fill(p, nodes as usize)?;
            Ok(vec![])
        }) {
            self.error(e.to_string());
            return;
        }
        doc.clear_history();
        if let Err(e) = self.replace_document(doc) {
            self.error(e);
            return;
        }
        let mut cam = self.editor.camera().clone();
        cam.set_zoom(0.5);
        cam.set_center(Point::new(720.0, 512.0));
        self.editor.set_camera(cam);
        self.editor.take_events();
        self.push_opened(vec![]);
    }

    /// Разбирает записи кольцевого буфера и отдаёт события редактору по порядку.
    /// Возвращает число принятых записей.
    pub fn apply_input(&mut self, bytes: &[u8]) -> usize {
        let decoded = decode_all(bytes);
        self.skipped += decoded.skipped as u64;
        let n = decoded.records.len();
        for rec in decoded.records {
            match rec {
                Record::Input(input) => self.editor.handle(input),
                Record::Resize { width, height, dpr } => {
                    self.editor.set_viewport(Size::new(width, height), dpr);
                    self.dirty = true;
                }
            }
        }
        self.inputs += n as u64;
        self.drain_editor();
        n
    }

    /// Одно событие ввода (тесты и бенчмарк без кодирования).
    pub fn handle(&mut self, input: Input) {
        self.editor.handle(input);
        self.inputs += 1;
        self.drain_editor();
    }

    /// Команда основного потока.
    pub fn command(&mut self, cmd: Command) {
        match cmd {
            Command::Open { bytes } => self.open(&bytes),
            Command::Demo { nodes } => self.demo(nodes),
            Command::Execute { command } => match CommandId::from_key(&command) {
                Some(id) => self.editor.execute(id),
                None => self.error(format!("Неизвестная команда «{}»", clip(&command))),
            },
            Command::SetTool { tool } => self.editor.set_tool(tool_kind(tool)),
            Command::SetTheme { theme } => {
                self.theme = theme;
                self.editor.set_theme(overlay_theme(theme));
            }
            Command::Paste { bytes } => {
                // Ошибку вставки редактор сам отдаёт событием `Error`.
                let _ = self.editor.paste(&bytes);
            }
            Command::PlaceImage { bytes } => self.place_image(bytes.into_vec()),
            Command::Save => match container::encode(self.editor.document()) {
                Ok(bytes) => self.events.push(CoreEvent::Saved {
                    bytes: ByteBuf::from(bytes),
                }),
                Err(e) => self.error(format!("Не удалось сохранить: {e}")),
            },
            Command::ReadPixels { reference } => {
                self.pixels = Some(reference);
                self.dirty = true;
            }
            other => self.panel_command(other),
        }
        self.drain_editor();
    }

    fn place_image(&mut self, bytes: Vec<u8>) {
        let Some(pixmap) = decode_png(&bytes) else {
            self.error("Не удалось прочитать изображение: нужен PNG не больше 8192 px");
            return;
        };
        let (w, h) = (pixmap.width() as f64, pixmap.height() as f64);
        let hash = self.editor.add_image(bytes);
        self.images.insert(&hash, pixmap);
        self.editor.place_image(&hash, w, h);
    }

    /// Запрос пикселей следующего кадра: `Some(reference)` один раз.
    pub fn take_pixel_request(&mut self) -> Option<bool> {
        self.pixels.take()
    }

    /// Переводит события редактора в события протокола.
    fn drain_editor(&mut self) {
        // Кэш имён слоёв учитывает каждую применённую к сцене правку (иначе при
        // активном поиске каждая правка обходила бы страницу заново). Строки и
        // совпадения кэш пересобирает сам: их метка — `Scene::structure_stamp`.
        let applied = self.editor.take_applied();
        self.layers.note_applied(&applied, self.editor.scene());
        let mut document = false;
        // Выделение, как курсор и документ, — состояние: в пачке важно последнее.
        // Строки идентификаторов строятся один раз, для него, а не для каждого щелчка.
        let mut selection = None;
        for e in self.editor.take_events() {
            match e {
                EditorEvent::NeedsRedraw => self.dirty = true,
                EditorEvent::DocumentChanged => {
                    document = true;
                    self.ui.layers = true;
                    self.ui.properties = true;
                }
                EditorEvent::SelectionChanged(ids) => selection = Some(ids),
                EditorEvent::ToolChanged(k) => self.events.push(CoreEvent::Tool { tool: tool(k) }),
                EditorEvent::CursorChanged(c) => {
                    // Наведение меняет курсор часто: в пачке важен только последний.
                    self.events
                        .retain(|e| !matches!(e, CoreEvent::Cursor { .. }));
                    let (cursor, angle) = cursor(c);
                    self.events.push(CoreEvent::Cursor { cursor, angle });
                }
                EditorEvent::PageChanged(p) => {
                    self.events.push(CoreEvent::Page { id: p.to_string() });
                    self.background_preview = None;
                    self.layers.reset();
                    self.ui.layers = true;
                    self.ui.pages = true;
                    self.dirty = true;
                }
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
                    bytes: ByteBuf::from(bytes),
                }),
                EditorEvent::PasteRequested => self.events.push(CoreEvent::PasteRequested),
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
                EditorEvent::RequestImage => self.events.push(CoreEvent::RequestImage),
                EditorEvent::ToggleUi => self.events.push(CoreEvent::ToggleUi),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
            }
        }
        if let Some(ids) = selection {
            self.events
                .retain(|e| !matches!(e, CoreEvent::Selection { .. }));
            self.events.push(selection_event(
                ids.len(),
                ids.iter().take(SELECTION_IDS_MAX).map(NodeId::to_string),
            ));
            self.selected = ids.iter().copied().collect();
            // Выделение с холста: раскрыть предков и прокрутить панель к строке.
            if !self.from_panel && !ids.is_empty() {
                self.reveal = self.layers.reveal(self.editor.scene(), &ids);
            }
            self.ui.layers = true;
            self.ui.properties = true;
        }
        if document {
            self.events
                .retain(|e| !matches!(e, CoreEvent::Document { .. }));
            self.push_document();
        }
        self.flush_ui();
    }

    fn copy_png(&mut self, ids: &[NodeId]) {
        let before = self.images.placeholders_served();
        // Экспорт декодирует всё нужное сразу, без бюджета кадра.
        let provider = self.images.export_provider(&self.editor.document().images);
        let result =
            self.renderer
                .export_png_nodes(self.editor.scene(), ids, COPY_PNG_SCALE, &provider);
        // Заглушка в экспорте — это неверная картинка: лучше ошибка, чем серый квадрат.
        if self.images.placeholders_served() != before {
            self.error(
                "Не удалось скопировать: часть изображений не читается или не помещается в память",
            );
            return;
        }
        match result {
            Ok(png) => self.events.push(CoreEvent::ClipboardPng {
                bytes: ByteBuf::from(png),
            }),
            Err(e) => self.error(e.to_string()),
        }
    }

    /// Кадр не дошёл до экрана (поверхность пропустила его): нарисовать снова.
    pub fn request_frame(&mut self) {
        self.dirty = true;
    }

    /// Событие от вызывающего (готовность, пиксели, ошибки поверхности) — в общую
    /// очередь с событиями редактора.
    pub fn push_event(&mut self, e: CoreEvent) {
        self.events.push(e);
    }

    /// Нужен ли кадр.
    pub fn wants_frame(&self) -> bool {
        self.dirty
    }

    /// Фон кадра: цвет фона страницы, если задан, иначе холст темы.
    pub fn background(&self) -> Rgba {
        let page = match self.background_preview {
            Some(preview) => preview,
            None => self.editor.page_background(),
        };
        if let Some(c) = page {
            let k = |v: u8| v as f32 / 255.0;
            return Rgba::new([k(c.r), k(c.g), k(c.b), k(c.a)]);
        }
        match self.theme {
            Theme::Dark => CANVAS_DARK,
            Theme::Light => CANVAS_LIGHT,
        }
    }

    /// Рисует кадр в `backend`. Кадр нужен снова, только если этот неполный.
    pub fn draw(&mut self, backend: &mut impl RenderBackend) -> FrameStats {
        self.images.begin_frame();
        let overlay = self.editor.overlay();
        let provider = self.images.provider(&self.editor.document().images);
        let stats = self.renderer.render(
            backend,
            self.editor.scene(),
            self.editor.camera(),
            &overlay,
            &provider,
            Some(self.background()),
        );
        self.update_selection(&overlay.selection);
        self.dirty = stats.needs_another_frame;
        self.frames += 1;
        stats
    }

    /// Тот же кадр, нарисованный `vello_cpu` отдельным рендерером, — эталон для
    /// сравнения с GPU в тестах (`ReadPixels { reference: true }`). Свой рендерер:
    /// кэши и счётчики кадров экрана не трогаются. Без растрового кэша и без
    /// предела времени: эталон не зависит от истории кадров. Только для тестов.
    /// Картинки — как при экспорте (без бюджета кадра), чтобы эталон не показывал
    /// заглушку там, где у GPU картинка уже в атласе; но кэш `DocImages` он трогает
    /// (порядок вытеснения, счётчики декодирования и бюджет следующего кадра).
    pub fn draw_reference(&self) -> ferrum_render::Pixmap {
        let (w, h) = self.device_size();
        let mut backend = ferrum_render::CpuBackend::new(w, h);
        let mut renderer = Renderer::with_raster_cache_bytes(0);
        renderer.set_raster_build_time(None);
        let provider = self.images.export_provider(&self.editor.document().images);
        renderer.render(
            &mut backend,
            self.editor.scene(),
            self.editor.camera(),
            &self.editor.overlay(),
            &provider,
            Some(self.background()),
        );
        backend.render()
    }

    /// Рамка выделения для живого состояния. Пересчитывается только при смене
    /// сцены или выделения: панорамирование с 100 000 выделенными не обходит их на
    /// каждом кадре.
    fn update_selection(&mut self, ids: &[NodeId]) {
        let scene = self.editor.scene();
        let key = (
            scene.change_counter(),
            self.editor.selection_version(),
            ids.len(),
        );
        if self.selection.is_some_and(|s| s.key == key) {
            return;
        }
        let mut info = SelectionInfo {
            key,
            count: ids.len() as f64,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            rotation: 0.0,
        };
        if let [one] = ids {
            // Те же значения, что в сводке панели (у группы — рамка детей, поворот
            // как в Figma): во время жеста панель читает их отсюда.
            if let Some(slot) = scene.slot(*one) {
                let p = ferrum_editor::panel_pose(scene, slot);
                (info.x, info.y, info.w, info.h, info.rotation) = (p.x, p.y, p.w, p.h, p.rotation);
            }
        } else if let Some(b) = SelectionBox::of(scene, ids) {
            let r = b.to_world.transform_rect_bbox(b.rect);
            (info.x, info.y, info.w, info.h) = (r.x0, r.y0, r.width(), r.height());
        }
        self.selection = Some(info);
    }

    /// Время кадра (рисование и отправка в GPU), измеренное вызывающим, и момент
    /// его окончания — для статистики.
    pub fn frame_done(&mut self, now_ms: f64, frame_ms: f64) {
        self.frame_ms = frame_ms;
        if self.times.len() == FRAME_WINDOW {
            self.times.pop_front();
        }
        self.times.push_back(frame_ms);
        self.stamps.push_back(now_ms);
        while self.stamps.front().is_some_and(|t| now_ms - t > 1000.0) {
            self.stamps.pop_front();
        }
    }

    /// Состояние атласа GPU: картинок и промахов (заглушек из-за нехватки места).
    pub fn set_atlas(&mut self, images: usize, misses: u64) {
        self.atlas = (images, misses);
    }

    /// Медиана и 95-й процентиль времени кадра по окну.
    pub fn frame_percentiles(&self) -> (f64, f64) {
        if self.times.is_empty() {
            return (0.0, 0.0);
        }
        let mut t: Vec<f64> = self.times.iter().copied().collect();
        t.sort_by(f64::total_cmp);
        let at = |q: f64| t[((t.len() - 1) as f64 * q).round() as usize];
        (at(0.5), at(0.95))
    }

    pub fn take_events(&mut self) -> Vec<CoreEvent> {
        std::mem::take(&mut self.events)
    }

    /// Живое состояние для основного потока.
    pub fn live(&self) -> LiveState {
        let cam = self.editor.camera();
        let (dw, dh) = cam.device_size();
        let (p50, p95) = self.frame_percentiles();
        let doc = self.editor.document();
        let sel = self.selection.unwrap_or(SelectionInfo {
            key: (0, 0, 0),
            count: 0.0,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            rotation: 0.0,
        });
        LiveState {
            frames: self.frames as f64,
            frame_ms: self.frame_ms,
            frame_p50: p50,
            frame_p95: p95,
            fps: self.stamps.len() as f64,
            backend: self.backend.code(),
            selection_count: sel.count,
            selection_x: sel.x,
            selection_y: sel.y,
            selection_w: sel.w,
            selection_h: sel.h,
            selection_rotation: sel.rotation,
            camera_x: cam.center().x,
            camera_y: cam.center().y,
            zoom: cam.zoom(),
            viewport_w: cam.viewport().width,
            viewport_h: cam.viewport().height,
            dpr: cam.dpr(),
            device_w: dw as f64,
            device_h: dh as f64,
            busy: if self.editor.is_busy() { 1.0 } else { 0.0 },
            steps: self.editor.step_count() as f64,
            can_undo: if doc.can_undo() { 1.0 } else { 0.0 },
            can_redo: if doc.can_redo() { 1.0 } else { 0.0 },
            nodes: self.editor.scene().len() as f64,
            inputs: self.inputs as f64,
            skipped: self.skipped as f64,
            atlas_images: self.atlas.0 as f64,
            atlas_misses: self.atlas.1 as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_event_caps_ids_but_keeps_count() {
        let ids = (0..1001).map(|i| i.to_string());
        let CoreEvent::Selection { count, ids } = selection_event(1001, ids) else {
            panic!()
        };
        assert_eq!((count, ids.len()), (1001, SELECTION_IDS_MAX));
    }

    #[test]
    fn clip_shortens_long_commands() {
        assert_eq!(clip("edit.undo"), "edit.undo");
        let c = clip(&"я".repeat(100));
        assert_eq!(c.chars().count(), 65);
        assert!(c.ends_with('…'));
    }
}
