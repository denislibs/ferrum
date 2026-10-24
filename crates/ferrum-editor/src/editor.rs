//! Редактор без браузера: принимает события ввода и команды, правит документ
//! транзакциями, держит сцену, камеру, выделение и инструмент, отдаёт оверлей,
//! курсор и события для интерфейса.
//!
//! Редактор владеет документом и сценой, а не заимствует их: сцена должна совпадать
//! с документом после каждой правки, отмены и повтора, и это можно гарантировать,
//! только если править документ больше некому. Панели (этап 5) правят через
//! `Editor::transact`, который сам обновляет сцену. В worker'е (этап 4) редактор —
//! единственный владелец всего состояния ядра, рендер получает `scene()`,
//! `camera()` и `overlay()` по ссылке.

use std::collections::HashSet;

use ferrum_doc::{DocError, DocResult, Document, NodeId, PageDoc, PageId, Tx};
use ferrum_render::{Camera, Overlay, OverlayTheme};
use ferrum_scene::Scene;
use kurbo::{Point, Size, Vec2};

use crate::commands::CommandId;
use crate::input::{Input, KeyEvent, Platform, PointerButton, PointerEvent, WheelEvent};
use crate::keymap::Keymap;
use crate::tools::{
    sync, Ctx, Cursor, DrawTool, Flow, HandTool, Outbox, SelectTool, Selection, Shape, Tool,
    ToolKind, ToolOverlay,
};

/// Множитель зума колесом с Ctrl/Cmd и щипком: `exp(-deltaY · k)`.
pub const WHEEL_ZOOM_SPEED: f64 = 0.01;

/// Настройки редактора (хранятся у пользователя, не в документе).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub platform: Platform,
    /// Привязка положения и размеров к целым пикселям (по умолчанию включена).
    pub pixel_snap: bool,
    /// Привязка к краям и центрам соседей.
    pub snapping: bool,
    /// Пиксельная сетка при зуме от 800%.
    pub pixel_grid: bool,
}

impl Settings {
    pub fn new(platform: Platform) -> Settings {
        Settings {
            platform,
            pixel_snap: true,
            snapping: true,
            pixel_grid: true,
        }
    }
}

/// События для интерфейса. Забираются `take_events` раз за кадр.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorEvent {
    SelectionChanged(Vec<NodeId>),
    ToolChanged(ToolKind),
    CursorChanged(Cursor),
    /// Изменились сцена, камера или оверлей — нужен новый кадр.
    NeedsRedraw,
    /// Документ изменён правкой, отменой или повтором.
    DocumentChanged,
    PageChanged(PageId),
    /// Готовы байты для системного буфера обмена (формат `web application/x-ferrum`).
    Clipboard(Vec<u8>),
    /// Интерфейсу пора прочитать системный буфер и вызвать `paste`.
    PasteRequested,
    /// Скопировать как PNG: интерфейс экспортирует узлы и кладёт картинку в буфер.
    ExportPng(Vec<NodeId>),
    /// Выбрать файл изображения и вызвать `place_image`.
    RequestImage,
    /// Скрыть или показать интерфейс (Ctrl+\).
    ToggleUi,
    /// Сообщение об ошибке для пользователя.
    Error(String),
}

fn finite_point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

/// Все числа события конечны.
fn input_is_finite(input: &Input) -> bool {
    match input {
        Input::PointerDown(e) | Input::PointerMove(e) | Input::PointerUp(e) => {
            finite_point(e.point) && e.time_ms.is_finite()
        }
        Input::Wheel(e) => finite_point(e.point) && e.delta.x.is_finite() && e.delta.y.is_finite(),
        Input::Pinch { point, scale } => finite_point(*point) && scale.is_finite(),
        _ => true,
    }
}

/// Что видел интерфейс в прошлый раз — события шлются только при изменении.
/// Выделение — номером версии: сравнение списков в 10 000 узлов на каждое
/// событие стоило бы больше самого события.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    selection: u64,
    tool: ToolKind,
    cursor: Cursor,
    frame: FrameKey,
}

#[derive(Debug, Clone, PartialEq)]
struct FrameKey {
    scene: u64,
    scene_epoch: u64,
    camera: Camera,
    /// Версия выделения; выделение инструмента на время жеста — в `tool`.
    selection: u64,
    hover: Option<NodeId>,
    tool: ToolOverlay,
    theme: OverlayTheme,
    pixel_grid: bool,
}

pub struct Editor {
    pub(crate) doc: Document,
    pub(crate) page: PageId,
    pub(crate) scene: Scene,
    pub(crate) camera: Camera,
    pub(crate) selection: Selection,
    /// Растёт при каждой замене сцены целиком (переключение страницы).
    scene_epoch: u64,
    pub(crate) hover: Option<NodeId>,
    pub(crate) tool: Box<dyn Tool>,
    /// Инструмент, который рука временно заменила на время удержания клавиши,
    /// и физическая клавиша удержания.
    held: Option<(Box<dyn Tool>, String)>,
    /// Панорамирование средней кнопкой поверх любого инструмента.
    pan: Option<HandTool>,
    pub(crate) keymap: Keymap,
    pub(crate) settings: Settings,
    theme: OverlayTheme,
    pub(crate) out: Outbox,
    last_pointer: Option<PointerEvent>,
    seen: Seen,
}

impl Editor {
    /// Редактор для страницы `page` (по умолчанию первой) с вьюпортом в CSS-пикселях.
    pub fn new(
        mut doc: Document,
        page: Option<PageId>,
        viewport: Size,
        dpr: f64,
        platform: Platform,
    ) -> DocResult<Editor> {
        let page = match page.or_else(|| doc.pages().first().map(|p| p.0)) {
            Some(p) => p,
            None => return Err(DocError::PageNotFound("—".into())),
        };
        let scene = Scene::from_page(doc.page(page)?);
        // Изменения, накопленные до редактора, уже в сцене.
        for (p, _) in doc.pages() {
            doc.take_changes(p);
        }
        let camera = Camera::new(viewport, dpr);
        let theme = OverlayTheme::dark();
        let tool: Box<dyn Tool> = Box::new(SelectTool::new());
        let seen = Seen {
            selection: 0,
            tool: ToolKind::Move,
            cursor: Cursor::Default,
            frame: FrameKey {
                scene: scene.change_counter(),
                scene_epoch: 0,
                camera: camera.clone(),
                selection: 0,
                hover: None,
                tool: ToolOverlay::default(),
                theme: theme.clone(),
                pixel_grid: true,
            },
        };
        Ok(Editor {
            doc,
            page,
            scene,
            camera,
            selection: Selection::default(),
            scene_epoch: 0,
            hover: None,
            tool,
            held: None,
            pan: None,
            keymap: Keymap::new(platform),
            settings: Settings::new(platform),
            theme,
            out: Outbox::default(),
            last_pointer: None,
            seen,
        })
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Текущая страница документа (для чтения панелями и тестами).
    pub fn page_doc(&mut self) -> &PageDoc {
        self.doc
            .page(self.page)
            .expect("текущая страница загружена при открытии")
    }

    pub fn page(&self) -> PageId {
        self.page
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn selection(&self) -> &[NodeId] {
        &self.selection
    }

    pub fn tool_kind(&self) -> ToolKind {
        self.tool.kind()
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    pub fn keymap_mut(&mut self) -> &mut Keymap {
        &mut self.keymap
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn set_settings(&mut self, settings: Settings) {
        if settings.platform != self.settings.platform {
            // Раскладка другой платформы — с теми же пользовательскими отличиями.
            let overrides = self.keymap.export_json();
            self.keymap = Keymap::new(settings.platform);
            let _ = self.keymap.import_json(&overrides);
        }
        self.settings = settings;
        self.emit();
    }

    pub fn set_theme(&mut self, theme: OverlayTheme) {
        self.theme = theme;
        self.emit();
    }

    /// Шагов истории, созданных редактором (жесты и команды с изменениями).
    pub fn step_count(&self) -> u64 {
        self.out.steps
    }

    /// Идёт ли жест (кнопка нажата).
    pub fn is_busy(&self) -> bool {
        self.tool.is_busy() || self.pan.is_some()
    }

    pub fn take_events(&mut self) -> Vec<EditorEvent> {
        std::mem::take(&mut self.out.events)
    }

    /// Камера целиком (восстановление вида страницы, тесты).
    pub fn set_camera(&mut self, camera: Camera) {
        self.camera = camera;
        self.refresh_gesture();
        self.emit();
    }

    pub fn set_viewport(&mut self, viewport: Size, dpr: f64) {
        self.camera.set_viewport(viewport);
        self.camera.set_dpr(dpr);
        self.emit();
    }

    /// Курсор над холстом.
    pub fn cursor(&self) -> Cursor {
        if self.pan.is_some() {
            Cursor::Grabbing
        } else {
            self.tool.cursor()
        }
    }

    /// Описание оверлея для рендера.
    pub fn overlay(&self) -> Overlay {
        let t = self.tool.overlay();
        Overlay {
            theme: self.theme.clone(),
            selection: t.selection.unwrap_or_else(|| self.selection.to_vec()),
            hover: if self.is_busy() { None } else { self.hover },
            marquee: t.marquee,
            snap_lines: t.snap_lines,
            frame_labels: true,
            pixel_grid: self.settings.pixel_grid,
        }
    }

    /// Разделяет редактор на инструмент и контекст для него.
    pub(crate) fn split(&mut self) -> (&mut Box<dyn Tool>, Ctx<'_>) {
        let Editor {
            doc,
            page,
            scene,
            camera,
            selection,
            scene_epoch,
            hover,
            settings,
            out,
            tool,
            ..
        } = self;
        (
            tool,
            Ctx {
                doc,
                page: *page,
                scene,
                camera,
                selection,
                scene_epoch: *scene_epoch,
                hover,
                settings,
                out,
            },
        )
    }

    pub(crate) fn ctx(&mut self) -> Ctx<'_> {
        self.split().1
    }

    /// Обрабатывает одно событие ввода.
    pub fn handle(&mut self, input: Input) {
        // Нечисловые координаты и дельты не доходят до инструментов: из них иначе
        // получились бы NaN в положении и повороте узлов. Событие пропускается целиком.
        if !input_is_finite(&input) {
            // Отпускание, которое потерялось, иначе оставило бы жест открытым до
            // следующего нажатия: без координат применить его нельзя, поэтому отменяем.
            if matches!(input, Input::PointerUp(_)) {
                self.cancel_gesture();
            }
            return;
        }
        match input {
            Input::PointerDown(e) => self.pointer_down(e),
            Input::PointerMove(e) => self.pointer_move(e),
            Input::PointerUp(e) => self.pointer_up(e),
            Input::PointerCancel => self.cancel_gesture(),
            Input::Wheel(e) => self.wheel(e),
            Input::Pinch { point, scale } => {
                self.camera.zoom_at(point, scale);
                self.refresh_gesture();
            }
            Input::KeyDown(e) => self.key_down(e),
            Input::KeyUp(e) => self.key_up(e),
            Input::Blur => {
                self.cancel_gesture();
                self.release_hold();
            }
        }
        self.emit();
    }

    fn pointer_down(&mut self, e: PointerEvent) {
        self.last_pointer = Some(e);
        match e.button {
            PointerButton::Middle => {
                self.cancel_gesture();
                let mut hand = HandTool::new();
                hand.pointer_down(&mut self.ctx(), &e);
                self.pan = Some(hand);
            }
            PointerButton::Secondary => {}
            PointerButton::Primary => {
                if self.pan.is_some() {
                    return;
                }
                // Нажатие без отпускания предыдущего (событие потеряно) — прежний
                // жест отменяется, а не записывается.
                if self.tool.is_busy() {
                    self.cancel_gesture();
                }
                let (tool, mut cx) = self.split();
                tool.pointer_down(&mut cx, &e);
            }
        }
    }

    fn pointer_move(&mut self, e: PointerEvent) {
        self.last_pointer = Some(e);
        if let Some(mut hand) = self.pan.take() {
            hand.pointer_move(&mut self.ctx(), &e);
            self.pan = Some(hand);
            return;
        }
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
    }

    fn pointer_up(&mut self, e: PointerEvent) {
        self.last_pointer = Some(e);
        if e.button == PointerButton::Middle {
            if let Some(mut hand) = self.pan.take() {
                hand.pointer_up(&mut self.ctx(), &e);
            }
            return;
        }
        if e.button != PointerButton::Primary || !self.tool.is_busy() {
            return;
        }
        let (tool, mut cx) = self.split();
        if tool.pointer_up(&mut cx, &e) == Flow::Finished {
            self.set_tool(ToolKind::Move);
        }
    }

    fn wheel(&mut self, e: WheelEvent) {
        if e.modifiers.ctrl || e.modifiers.meta {
            self.camera
                .zoom_at(e.point, (-e.delta.y * WHEEL_ZOOM_SPEED).exp());
        } else {
            // Колесо мыши с Shift прокручивает вбок.
            let d = if e.modifiers.shift && e.delta.x == 0.0 {
                Vec2::new(e.delta.y, 0.0)
            } else {
                e.delta
            };
            self.camera.pan_by(-d);
        }
        self.refresh_gesture();
    }

    /// Камера сдвинулась во время жеста: та же экранная точка теперь над другим
    /// местом мира, жест пересчитывается.
    fn refresh_gesture(&mut self) {
        if let (true, Some(e)) = (self.tool.is_busy(), self.last_pointer) {
            let (tool, mut cx) = self.split();
            tool.pointer_move(&mut cx, &e);
        }
    }

    fn key_down(&mut self, e: KeyEvent) {
        if self.is_busy() {
            // Во время жеста клавиши только меняют модификаторы; Esc отменяет жест.
            // Esc здесь — физическая клавиша (раздел 7.1), а не команда раскладки:
            // переназначение команды «Отмена» не должно лишать жест выхода.
            if e.code == "Escape" {
                self.cancel_gesture();
            } else {
                let (tool, mut cx) = self.split();
                tool.key(&mut cx, e.modifiers);
            }
            return;
        }
        let Some(cmd) = self.keymap.lookup(&e) else {
            return;
        };
        if cmd == CommandId::HandHold {
            if !e.repeat && self.held.is_none() && self.tool.kind() != ToolKind::Hand {
                let prev = std::mem::replace(&mut self.tool, Box::new(HandTool::new()));
                self.held = Some((prev, e.code.clone()));
                self.hover = None;
            }
            return;
        }
        self.run(cmd);
    }

    fn key_up(&mut self, e: KeyEvent) {
        if self.held.as_ref().is_some_and(|(_, code)| *code == e.code) {
            self.release_hold();
            return;
        }
        if self.tool.is_busy() {
            let (tool, mut cx) = self.split();
            tool.key(&mut cx, e.modifiers);
        }
    }

    /// Возвращает инструмент, заменённый рукой на время удержания клавиши.
    fn release_hold(&mut self) {
        if let Some((prev, _)) = self.held.take() {
            if self.tool.is_busy() {
                let (tool, mut cx) = self.split();
                tool.cancel(&mut cx);
            }
            self.tool = prev;
        }
    }

    /// Отменяет жест: превью убирается, документ и история не меняются.
    pub fn cancel_gesture(&mut self) {
        if let Some(mut hand) = self.pan.take() {
            hand.cancel(&mut self.ctx());
        }
        if self.tool.is_busy() {
            let (tool, mut cx) = self.split();
            tool.cancel(&mut cx);
        }
    }

    /// Выбирает инструмент (тулбар, горячая клавиша). Жест текущего отменяется.
    pub fn set_tool(&mut self, kind: ToolKind) {
        self.cancel_gesture();
        self.held = None;
        self.hover = None;
        self.tool = match kind {
            ToolKind::Move => Box::new(SelectTool::new()),
            ToolKind::Frame => Box::new(DrawTool::new(Shape::Frame)),
            ToolKind::Rect => Box::new(DrawTool::new(Shape::Rect)),
            ToolKind::Ellipse => Box::new(DrawTool::new(Shape::Ellipse)),
            ToolKind::Line => Box::new(DrawTool::new(Shape::Line)),
            ToolKind::Hand => Box::new(HandTool::new()),
            // Изображению нужны хэш и размер: интерфейс сначала выбирает файл.
            ToolKind::Image => {
                self.out.events.push(EditorEvent::RequestImage);
                Box::new(SelectTool::new())
            }
        };
        self.emit();
    }

    /// Инструмент размещения изображения: клик ставит его в исходном размере,
    /// перетаскивание задаёт размер с сохранением пропорций.
    pub fn place_image(&mut self, hash: &str, width: f64, height: f64) {
        self.set_tool(ToolKind::Move);
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if !ok(width) || !ok(height) {
            self.out
                .events
                .push(EditorEvent::Error("У изображения нет размера".into()));
            return;
        }
        self.tool = Box::new(DrawTool::new(Shape::Image {
            hash: hash.to_string(),
            width,
            height,
        }));
        self.emit();
    }

    /// Выполняет команду (меню, палитра, горячая клавиша). Во время жеста
    /// выполняется только Esc — он отменяет жест.
    pub fn execute(&mut self, cmd: CommandId) {
        if self.is_busy() {
            if cmd == CommandId::Escape {
                self.cancel_gesture();
            }
        } else {
            self.run(cmd);
        }
        self.emit();
    }

    /// Правка документа из панелей одним шагом отмены; сцена обновляется сама.
    pub fn transact<F>(&mut self, f: F) -> DocResult<Vec<NodeId>>
    where
        F: FnOnce(&Tx) -> DocResult<Vec<NodeId>>,
    {
        self.cancel_gesture();
        let before = self.selection.to_vec();
        let result = self.doc.transact(self.page, before, f);
        let changed = sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        if let Ok(sel) = &result {
            if changed {
                self.out.steps += 1;
            }
            let sel = self.alive(sel);
            self.selection.set(sel);
        }
        self.emit();
        result
    }

    /// Выделение из панели слоёв. Неизвестные узлы пропускаются.
    pub fn select(&mut self, ids: &[NodeId]) {
        self.cancel_gesture();
        let sel = self.alive(ids);
        self.selection.set(sel);
        self.emit();
    }

    /// Подсветка наведения из панели слоёв.
    pub fn set_hover(&mut self, id: Option<NodeId>) {
        if !self.is_busy() {
            self.hover = id;
            self.emit();
        }
    }

    /// Узлы из `ids`, которые есть в сцене и не черновики, без повторов, в порядке
    /// входа. Повторы — множеством: перебор списка был квадратичным (выделение
    /// 20 000 узлов — 94 мс).
    pub(crate) fn alive(&self, ids: &[NodeId]) -> Vec<NodeId> {
        let mut out = Vec::with_capacity(ids.len());
        let mut seen = HashSet::with_capacity(ids.len());
        for id in ids {
            if self.scene.slot(*id).is_some() && !self.scene.is_draft(*id) && seen.insert(*id) {
                out.push(*id);
            }
        }
        out
    }

    /// Кладёт байты изображения в документ и возвращает их хэш — для
    /// `place_image` и `insert_image`. Байты не попадают в историю отмены.
    pub fn add_image(&mut self, bytes: Vec<u8>) -> String {
        self.doc.images.add(bytes)
    }

    /// Новая страница в конце списка (в историю отмены не попадает, как и в
    /// `ferrum-doc`). Текущая страница не меняется.
    pub fn add_page(&mut self, name: &str) -> DocResult<PageId> {
        self.doc.add_page(name)
    }

    /// Переключает страницу: сцена строится заново, выделение снимается.
    pub fn set_page(&mut self, page: PageId) -> DocResult<()> {
        self.cancel_gesture();
        // Изменения всех страниц уже в сценах или не нужны: новая сцена строится
        // по документу целиком.
        for (p, _) in self.doc.pages() {
            self.doc.take_changes(p);
        }
        let pd = self.doc.page(page)?;
        self.scene = Scene::from_page(pd);
        self.scene_epoch += 1;
        self.page = page;
        self.selection.clear();
        self.hover = None;
        self.out.events.push(EditorEvent::PageChanged(page));
        self.emit();
        Ok(())
    }

    pub(crate) fn undo_redo(&mut self, redo: bool) {
        // Жест держит превью и черновики на узлах, которые отмена может убрать.
        self.cancel_gesture();
        let result = if redo {
            self.doc.redo()
        } else {
            self.doc.undo()
        };
        sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        match result {
            Ok(Some(restore)) => {
                if restore.page != self.page {
                    if let Ok(pd) = self.doc.page(restore.page) {
                        self.scene = Scene::from_page(pd);
                        self.scene_epoch += 1;
                        self.page = restore.page;
                        self.out.events.push(EditorEvent::PageChanged(restore.page));
                    }
                }
                let sel = self.alive(&restore.selection);
                self.selection.set(sel);
            }
            Ok(None) => {}
            Err(e) => self.out.events.push(EditorEvent::Error(e.to_string())),
        }
    }

    /// Шлёт события об изменившемся выделении, инструменте, курсоре и кадре.
    pub(crate) fn emit(&mut self) {
        // Наведённый узел могли удалить правкой, отменой или повтором.
        if self.hover.is_some_and(|h| self.scene.slot(h).is_none()) {
            self.hover = None;
        }
        let now = Seen {
            selection: self.selection.version(),
            tool: self.tool.kind(),
            cursor: self.cursor(),
            frame: FrameKey {
                scene: self.scene.change_counter(),
                scene_epoch: self.scene_epoch,
                camera: self.camera.clone(),
                selection: self.selection.version(),
                hover: if self.is_busy() { None } else { self.hover },
                tool: self.tool.overlay(),
                theme: self.theme.clone(),
                pixel_grid: self.settings.pixel_grid,
            },
        };
        if now.selection != self.seen.selection {
            self.out
                .events
                .push(EditorEvent::SelectionChanged(self.selection.to_vec()));
        }
        if now.tool != self.seen.tool {
            self.out.events.push(EditorEvent::ToolChanged(now.tool));
        }
        if now.cursor != self.seen.cursor {
            self.out.events.push(EditorEvent::CursorChanged(now.cursor));
        }
        if now.frame != self.seen.frame {
            self.out.events.push(EditorEvent::NeedsRedraw);
        }
        self.seen = now;
    }

    /// Точка мира в центре вьюпорта (для вставки изображения и пресетов).
    pub(crate) fn view_center(&self) -> Point {
        self.camera.center()
    }
}
