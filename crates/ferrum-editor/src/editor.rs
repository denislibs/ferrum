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

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use ferrum_doc::{Changes, DocError, DocResult, Document, NodeId, PageDoc, PageId, Tx};
use ferrum_render::{Camera, Measure, Overlay, OverlayTheme, TextEditOverlay, VectorEditOverlay};
use ferrum_scene::Scene;
use ferrum_text::{Fonts, SharedFonts, TextLayout};
use kurbo::{Point, Size, Vec2};

use crate::commands::CommandId;
use crate::geometry::world_box;
use crate::input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
use crate::keymap::Keymap;
use crate::measure::{measure, AltState, MeasureCache};
use crate::panel::PanelGesture;
use crate::text_edit::{EnterAt, TextEdit};
use crate::tools::{
    sync, Ctx, Cursor, DrawTool, Flow, HandTool, Outbox, SelectTool, Selection, Shape, TextTool,
    Tool, ToolKind, ToolOverlay,
};
use crate::vector_edit::{Drag, VectorEdit, VectorTool};

/// Раскладка правимого текста и её ключ: узел, версия узла, поколение шрифтов.
pub(crate) type EditLayout = ((NodeId, u64, u64), Arc<TextLayout>);

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
    /// Копия фрагмента текста в правке: наш формат и обычный текст (`text/plain`).
    ClipboardText {
        bytes: Vec<u8>,
        plain: String,
    },
    /// Интерфейсу пора прочитать системный буфер и вызвать `paste`.
    PasteRequested,
    /// Скопировать как PNG: интерфейс экспортирует узлы и кладёт картинку в буфер.
    ExportPng(Vec<NodeId>),
    /// Выбрать файл изображения и вызвать `place_image`.
    RequestImage,
    /// Скрыть или показать интерфейс (Ctrl+\).
    ToggleUi,
    /// Сохранить файл (Ctrl/Cmd+S): запись в хранилище ведёт интерфейс.
    SaveRequested,
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
    /// Измерения с Alt: кадр нужен, когда они изменились, и не нужен, пока те же.
    measures: Vec<Measure>,
    /// Правка текста: курсор (с миганием), выделение, превью IME.
    text: Option<TextEditOverlay>,
    /// Правка вектора: сеть, выделенные точки, ручки, резинка пера.
    vector: Option<VectorEditOverlay>,
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
    /// Жест панели свойств (перетаскивание поля, палитра): свойства на начало.
    pub(crate) panel: Option<PanelGesture>,
    /// Нажат ли Alt для измерений расстояний.
    alt: AltState,
    /// Измерения для оверлея; пересчитываются в `emit`.
    measures: Vec<Measure>,
    measure_cache: Option<MeasureCache>,
    /// Реестр шрифтов для размера текста по раскладке (общий с рендером ядра).
    pub(crate) fonts: SharedFonts,
    /// Правка текста на холсте (спека подсистемы 3, раздел 7).
    pub(crate) text_edit: Option<TextEdit>,
    /// Раскладка правимого текста по ключу (узел, версия узла, поколение шрифтов).
    pub(crate) edit_layout_cache: RefCell<Option<EditLayout>>,
    /// Время ядра, мс (`set_time`): мигание курсора и склейка ввода.
    pub(crate) now_ms: f64,
    /// Указатель над правимым текстом: курсор — текстовый.
    pub(crate) text_hover: bool,
    /// Правка вектора и перо (спека подсистемы 3, раздел 9).
    pub(crate) vector_edit: Option<VectorEdit>,
    /// Поколение реестра шрифтов, с которым посчитаны контуры глифов текста в
    /// булевых группах сцены (`sync_text_outlines`).
    pub(crate) outlines_generation: u64,
    /// Источник контуров глифов застал реестр шрифтов занятым: текст булевой
    /// группы остался без формы — `sync_text_outlines` пересчитает её на
    /// следующем `emit`.
    pub(crate) outlines_missed: Rc<Cell<bool>>,
}

/// Ставит сцене источник контуров глифов для текста в булевых группах — реестр
/// шрифтов редактора. Реестр занят (сцена пересчитывается посреди раскладки) —
/// текст пока без формы, а `missed` помечается: пересчёт — на ближайшем `emit`.
pub(crate) fn install_text_outlines(
    scene: &mut Scene,
    fonts: &SharedFonts,
    missed: &Rc<Cell<bool>>,
) {
    let fonts = fonts.clone();
    let missed = missed.clone();
    scene.set_text_outlines(Some(Rc::new(move |p: &ferrum_doc::Props| {
        let Ok(mut f) = fonts.try_borrow_mut() else {
            missed.set(true);
            return None;
        };
        ferrum_render::text_outline(&mut f, p)
    })));
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
        let mut scene = Scene::from_page(doc.page(page)?);
        let fonts = Fonts::shared();
        let outlines_missed = Rc::new(Cell::new(false));
        install_text_outlines(&mut scene, &fonts, &outlines_missed);
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
                measures: Vec::new(),
                text: None,
                vector: None,
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
            panel: None,
            alt: AltState::default(),
            measures: Vec::new(),
            measure_cache: None,
            fonts,
            text_edit: None,
            edit_layout_cache: RefCell::new(None),
            now_ms: 0.0,
            text_hover: false,
            vector_edit: None,
            outlines_generation: 0,
            outlines_missed,
        })
    }

    /// Шрифты сменились (пришли байты, отказ в загрузке) — булевы группы с
    /// текстом пересчитываются с новыми контурами глифов. Вызывается с каждым
    /// `emit`; ядро зовёт его и после загрузки шрифта.
    /// Так же пересчитываются группы, чей текст застал реестр занятым.
    pub fn sync_text_outlines(&mut self) {
        // Реестр занят — дождаться следующего вызова.
        let Ok(generation) = self.fonts.try_borrow().map(|f| f.generation()) else {
            return;
        };
        if generation != self.outlines_generation || self.outlines_missed.get() {
            self.outlines_generation = generation;
            self.outlines_missed.set(false);
            self.scene.refresh_text_outlines();
        }
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Только для тестов: документ для правки мимо редактора (например, перевод
    /// в «только чтение» посреди правки текста).
    #[doc(hidden)]
    pub fn document_mut_for_tests(&mut self) -> &mut Document {
        &mut self.doc
    }

    /// Правки документа с прошлого вызова одной пачкой журнала автосохранения
    /// (этап 6); `None` — правок нет.
    pub fn take_journal(&mut self) -> DocResult<Option<Vec<u8>>> {
        self.doc.take_journal()
    }

    /// Всё нынешнее — уже в журнале (пример, только что собранный документ).
    pub fn mark_journal(&mut self) {
        self.doc.mark_journal();
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

    /// Версия выделения: растёт при каждом изменении. По ней worker не пересчитывает
    /// рамку выделения в живом состоянии, пока выделение и сцена те же.
    pub fn selection_version(&self) -> u64 {
        self.selection.version()
    }

    /// Инструмент для интерфейса: перо в правке вектора — «Перо».
    pub fn tool_kind(&self) -> ToolKind {
        if self.vector_tool() == Some(VectorTool::Pen) {
            ToolKind::Pen
        } else {
            self.tool.kind()
        }
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

    /// Непустые изменения, применённые к сцене текущей страницы с прошлого вызова,
    /// по порядку (правка, отмена и повтор, вставка, правка из панели). Новая сцена
    /// (другая страница, пересборка) сюда не попадает: у неё другой
    /// `Scene::generation`. Не больше `tools::MAX_APPLIED` последних.
    pub fn take_applied(&mut self) -> Vec<Changes> {
        std::mem::take(&mut self.out.applied)
    }

    /// Идёт ли жест (кнопка нажата).
    pub fn is_busy(&self) -> bool {
        self.tool.is_busy()
            || self.pan.is_some()
            || self.vector_edit.as_ref().is_some_and(|v| v.drag.is_some())
    }

    pub fn take_events(&mut self) -> Vec<EditorEvent> {
        std::mem::take(&mut self.out.events)
    }

    /// Было ли с прошлого вызова действие правки текста, не применённое из-за
    /// загрузки шрифтов (документ не тронут). Ядро ставит это действие в очередь
    /// правок, ждущих шрифтов, и повторит его по приходу байтов.
    pub fn take_text_pending(&mut self) -> bool {
        std::mem::take(&mut self.out.text_pending)
    }

    /// Камера целиком (восстановление вида страницы, тесты).
    pub fn set_camera(&mut self, camera: Camera) {
        self.camera = camera;
        self.refresh_gesture();
        self.rehover(None, None);
        self.emit();
    }

    pub fn set_viewport(&mut self, viewport: Size, dpr: f64) {
        self.camera.set_viewport(viewport);
        self.camera.set_dpr(dpr);
        self.rehover(None, None);
        self.emit();
    }

    /// Курсор над холстом.
    pub fn cursor(&self) -> Cursor {
        if self.pan.is_some() {
            Cursor::Grabbing
        } else if self.text_edit.is_some() {
            // Рамка с ручками во время правки скрыта: ни Resize, ни Rotate.
            if self.text_hover {
                Cursor::Text
            } else {
                Cursor::Default
            }
        } else if let Some(c) = self.vector_cursor() {
            c
        } else {
            self.tool.cursor()
        }
    }

    /// Описание оверлея для рендера.
    pub fn overlay(&self) -> Overlay {
        let mut t = self.tool.overlay();
        // Правка текста: вместо рамки с ручками — рамка текста и курсор.
        let text = self.text_overlay().map(|o| o.overlay);
        // Правка вектора: сеть с точками вместо рамки; рамка выделения точек и
        // привязки — её.
        let vector = self.vector_overlay();
        if let Some(v) = &self.vector_edit {
            if let Some(Drag::Marquee { rect, .. }) = &v.drag {
                t.marquee = *rect;
            }
            t.snap_lines = v.snap_lines.clone();
        }
        let editing = text.is_some() || self.vector_edit.is_some();
        Overlay {
            theme: self.theme.clone(),
            selection: if editing {
                Vec::new()
            } else {
                t.selection.unwrap_or_else(|| self.selection.to_vec())
            },
            hover: if self.is_busy() || editing {
                None
            } else {
                self.hover
            },
            marquee: t.marquee,
            snap_lines: t.snap_lines,
            frame_labels: true,
            pixel_grid: self.settings.pixel_grid,
            measurements: self.measures.clone(),
            text_edit: text,
            vector_edit: vector,
        }
    }

    /// Разделяет редактор на инструмент и контекст для него.
    pub(crate) fn split(&mut self) -> (&mut Box<dyn Tool>, Ctx<'_>) {
        let pen_open = self.vector_edit.as_ref().is_some_and(|v| v.pen.dirty);
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
            fonts,
            ..
        } = self;
        (
            tool,
            Ctx {
                pen_open,
                doc,
                page: *page,
                scene,
                camera,
                selection,
                scene_epoch: *scene_epoch,
                hover,
                settings,
                out,
                fonts,
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
        // Нажатие, клавиша или потеря фокуса на холсте во время жеста панели —
        // жест панели отменяется: его превью и жест холста не живут вместе.
        if !matches!(
            input,
            Input::PointerMove(_) | Input::Wheel(_) | Input::Pinch { .. }
        ) {
            self.cancel_panel();
        }
        match &input {
            Input::PointerDown(e) | Input::PointerMove(e) | Input::PointerUp(e) => {
                self.alt.pointer(e.modifiers)
            }
            Input::KeyDown(e) | Input::KeyUp(e) => self.alt.key(e, self.settings.platform),
            Input::Blur => self.alt.clear(),
            _ => {}
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
                self.rehover(Some(point), None);
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
                // Правка текста: нажатие в тексте ставит курсор; мимо — правка
                // кончается, и нажатие достаётся инструменту.
                if self.text_edit.is_some() && self.text_pointer_down(&e) {
                    return;
                }
                // Правка вектора и перо — так же.
                if self.vector_edit.is_some() && self.vector_pointer_down(&e) {
                    return;
                }
                // Нажатие без отпускания предыдущего (событие потеряно) — прежний
                // жест отменяется, а не записывается.
                if self.tool.is_busy() {
                    self.cancel_gesture();
                }
                let (tool, mut cx) = self.split();
                tool.pointer_down(&mut cx, &e);
                // Двойной щелчок по тексту (или нажатие инструментом «Текст»):
                // инструмент просит войти в правку.
                if let Some((id, world)) = self.out.edit_text.take() {
                    if self.tool.is_busy() {
                        let (tool, mut cx) = self.split();
                        tool.cancel(&mut cx);
                    }
                    // Инструмент «Текст» своё сделал — после правки снова «Перемещение».
                    if self.tool.kind() == ToolKind::Text {
                        self.set_tool(ToolKind::Move);
                        self.selection.set(vec![id]);
                    }
                    self.start_text_edit(id, EnterAt::Point(world));
                }
                // Двойной щелчок по вектору или фигуре — правка вектора.
                if let Some(id) = self.out.edit_vector.take() {
                    if self.tool.is_busy() {
                        let (tool, mut cx) = self.split();
                        tool.cancel(&mut cx);
                    }
                    self.start_vector_edit(id);
                }
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
        if let Some(t) = &self.text_edit {
            if t.drag.is_some() {
                self.text_pointer_move(&e);
                return;
            }
            self.text_hover = self.over_edited_text(e.point);
            // Инструмент не опрашивается: его ручки скрыты, hover не меняется.
            return;
        }
        if self.vector_edit.is_some() {
            self.vector_pointer_move(&e);
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
        if self.text_edit.as_ref().is_some_and(|t| t.drag.is_some()) {
            self.text_pointer_up(&e);
            return;
        }
        if self.vector_edit.is_some() {
            self.vector_pointer_up(&e);
            return;
        }
        if e.button != PointerButton::Primary || !self.tool.is_busy() {
            return;
        }
        let (tool, mut cx) = self.split();
        if tool.pointer_up(&mut cx, &e) == Flow::Finished {
            self.set_tool(ToolKind::Move);
        }
        // Инструмент «Текст» поставил новый текст: правка с курсором в нём.
        if let Some((parent, props)) = self.out.new_text.take() {
            self.start_draft_text(parent, props);
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
        self.rehover(Some(e.point), Some(e.modifiers));
    }

    /// Камера сдвинулась во время жеста: та же экранная точка теперь над другим
    /// местом мира, жест пересчитывается.
    fn refresh_gesture(&mut self) {
        if let (true, Some(e)) = (self.tool.is_busy(), self.last_pointer) {
            let (tool, mut cx) = self.split();
            tool.pointer_move(&mut cx, &e);
        }
    }

    /// Камера сменилась без жеста: под неподвижным указателем теперь другое место
    /// мира — наведение (а с ним обводка и измерения с Alt) пересчитывается тем же
    /// hit-test, что и при движении. Точка — из колеса или щипка, иначе последняя
    /// точка указателя. Наведение есть только у «Перемещения».
    fn rehover(&mut self, point: Option<Point>, modifiers: Option<Modifiers>) {
        if self.is_busy() || !self.tool.kind().is_select_like() {
            return;
        }
        let mut e = match (self.last_pointer, point) {
            (Some(e), _) => e,
            (None, Some(p)) => PointerEvent::new(p, Modifiers::NONE, 0.0),
            (None, None) => return,
        };
        if let Some(p) = point {
            e.point = p;
        }
        if let Some(m) = modifiers {
            e.modifiers = m;
        }
        self.last_pointer = Some(e);
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
    }

    fn key_down(&mut self, e: KeyEvent) {
        // Правка текста: раскладка клавиш не действует (спека подсистемы 3, 7).
        if self.text_edit.is_some() {
            self.text_key(&e.code, e.modifiers);
            return;
        }
        // Правка вектора: свои клавиши (Esc, Enter, Delete, B), остальное — по
        // раскладке (её команды правка перехватывает в `run`).
        if self.vector_edit.is_some() && self.vector_key(&e) {
            return;
        }
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
        // Enter по единственному выделенному тексту — правка всего текста (как в
        // Figma). Физическая клавиша, как Esc у жеста: не команда раскладки.
        if (e.code == "Enter" || e.code == "NumpadEnter")
            && e.modifiers == Modifiers::NONE
            && !e.repeat
        {
            if let [id] = self.selection[..] {
                if self.start_text_edit(id, EnterAt::All) {
                    return;
                }
                // Enter по вектору или фигуре — правка вектора (как в Figma).
                if self.start_vector_edit(id) {
                    return;
                }
            }
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

    /// Отменяет жест (холста или панели): превью убирается, документ и история
    /// не меняются.
    pub fn cancel_gesture(&mut self) {
        self.cancel_panel();
        self.cancel_vector_drag();
        if let Some(mut hand) = self.pan.take() {
            hand.cancel(&mut self.ctx());
        }
        if self.tool.is_busy() {
            let (tool, mut cx) = self.split();
            tool.cancel(&mut cx);
        }
    }

    /// Пролог каждой правки документа не с холста (панели, слои, вставка):
    /// открытый путь пера записывается своим шагом раньше неё (как в Figma) —
    /// иначе его запись по свойствам узла на начало пути затёрла бы правку, —
    /// затем жест отменяется. `Ctx::commit` в отладочной сборке проверяет, что
    /// пролог был: открытого пути при записи нет.
    pub(crate) fn begin_doc_edit(&mut self) {
        self.finish_pen_path();
        self.cancel_gesture();
    }

    /// Выбирает инструмент (тулбар, горячая клавиша). Жест текущего отменяется.
    pub fn set_tool(&mut self, kind: ToolKind) {
        self.cancel_gesture();
        self.end_text_edit();
        // Перо в правке — перо внутри неё; «Перемещение» в правке узла —
        // выделение точек. Остальные инструменты правку заканчивают.
        if kind == ToolKind::Pen {
            self.tool = Box::new(SelectTool::new());
            self.held = None;
            self.hover = None;
            self.start_pen();
            self.emit();
            return;
        }
        if kind == ToolKind::Move
            && self
                .vector_edit
                .as_ref()
                .is_some_and(|v| !v.from_pen_tool && v.id.is_some())
        {
            self.finish_pen_path();
            if let Some(v) = &mut self.vector_edit {
                v.tool = VectorTool::Select;
            }
            self.emit();
            return;
        }
        if let Some(id) = self.finish_vector_edit() {
            if self.scene.slot(id).is_some() {
                self.selection.set(vec![id]);
            }
        }
        self.held = None;
        self.hover = None;
        self.tool = match kind {
            ToolKind::Move => Box::new(SelectTool::new()),
            ToolKind::Scale => Box::new(SelectTool::scaling()),
            ToolKind::Frame => Box::new(DrawTool::new(Shape::Frame)),
            ToolKind::Rect => Box::new(DrawTool::new(Shape::Rect)),
            ToolKind::Ellipse => Box::new(DrawTool::new(Shape::Ellipse)),
            ToolKind::Line => Box::new(DrawTool::new(Shape::Line)),
            ToolKind::Arrow => Box::new(DrawTool::new(Shape::Arrow)),
            ToolKind::Hand => Box::new(HandTool::new()),
            ToolKind::Text => Box::new(TextTool::new()),
            ToolKind::Pen => Box::new(SelectTool::new()),
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
        // Жест панели (поле, палитра) держит узлы на начало жеста: команда могла бы
        // их удалить или сдвинуть, поэтому он отменяется.
        self.cancel_panel();
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
        self.begin_doc_edit();
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
        // Другое выделение из панели слоёв заканчивает правку текста и вектора.
        if self.text_edit.as_ref().is_some_and(|t| sel != [t.id]) {
            self.end_text_edit();
        }
        if self.vector_edit.is_some() && self.vector_edit_node().is_none_or(|id| sel != [id]) {
            self.finish_vector_edit();
        }
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
        self.finish_text_edit();
        self.finish_vector_edit();
        // Изменения всех страниц уже в сценах или не нужны: новая сцена строится
        // по документу целиком.
        for (p, _) in self.doc.pages() {
            self.doc.take_changes(p);
        }
        let pd = self.doc.page(page)?;
        self.scene = Scene::from_page(pd);
        install_text_outlines(&mut self.scene, &self.fonts, &self.outlines_missed);
        self.scene_epoch += 1;
        self.page = page;
        self.selection.clear();
        self.hover = None;
        self.out.events.push(EditorEvent::PageChanged(page));
        self.emit();
        Ok(())
    }

    pub(crate) fn undo_redo(&mut self, redo: bool) {
        // Путь пера: отмена убирает его последнюю точку, а не шаг истории.
        if !redo && self.pen_undo_point() {
            return;
        }
        // Незаписанный путь пера записывается раньше отмены: иначе он остался
        // бы поверх отменённого документа.
        self.finish_pen_path();
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
                        install_text_outlines(&mut self.scene, &self.fonts, &self.outlines_missed);
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

    /// Измерения с Alt (раздел 7.3 спеки): от рамки выделения до наведённого
    /// объекта, а без него или над родительским фреймом — до этого фрейма. Только
    /// у «Перемещения» без жеста холста или панели и не над ручками рамки; над
    /// выделенным — ничего.
    /// На движение — O(1): выделение берётся из кэша, наведённый — по слоту.
    fn update_measures(&mut self) {
        let over_box = matches!(
            self.tool.cursor(),
            Cursor::Resize { .. } | Cursor::Rotate { .. }
        );
        if !self.alt.held()
            || self.is_busy()
            || self.panel.is_some()
            || !self.tool.kind().is_select_like()
            || self.selection.is_empty()
            || over_box
        {
            self.measures.clear();
            return;
        }
        let key = (
            self.scene_epoch,
            self.scene.change_counter(),
            self.selection.version(),
        );
        if self.measure_cache.as_ref().is_none_or(|c| c.key() != key) {
            self.measure_cache = Some(MeasureCache::new(key, &self.scene, &self.selection));
        }
        let cache = self
            .measure_cache
            .as_ref()
            .expect("кэш только что заполнен");
        let target = match self.hover {
            None => cache.parent,
            Some(h) if cache.selected.contains(&h) => None,
            Some(h) => self
                .scene
                .slot(h)
                .filter(|s| self.scene.is_visible(*s))
                .and_then(|s| world_box(&self.scene, s)),
        };
        self.measures = match (cache.sbox, target) {
            (Some(s), Some(t)) => measure(s, t),
            _ => Vec::new(),
        };
    }

    /// Шлёт события об изменившемся выделении, инструменте, курсоре и кадре.
    pub(crate) fn emit(&mut self) {
        // Наведённый узел могли удалить правкой, отменой или повтором.
        if self.hover.is_some_and(|h| self.scene.slot(h).is_none()) {
            self.hover = None;
        }
        // Правимый текст удалён (отмена, чужая правка): правка кончается; курсор
        // за концом текста (его укоротили) — к концу.
        self.clamp_text_edit();
        // Правимый вектор изменился (запись, отмена, панель) — сеть заново.
        self.resync_vector_edit();
        self.sync_text_outlines();
        self.update_measures();
        let now = Seen {
            selection: self.selection.version(),
            tool: self.tool_kind(),
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
                measures: self.measures.clone(),
                text: self.text_overlay().map(|o| o.overlay),
                vector: self.vector_overlay(),
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
