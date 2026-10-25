//! Инструменты холста — конечные автоматы за трейтом `Tool` (раздел 7.1 спеки).
//! Инструмент получает события указателя и модификаторов вместе с контекстом
//! `Ctx` (документ, сцена, камера, выделение) и сам решает, когда записать правку.
//!
//! Правило всех жестов: пока кнопка нажата, изменения живут только в превью сцены
//! (`Scene::set_preview`, черновики); при отпускании записывается одна транзакция —
//! один шаг отмены; Esc убирает превью, и в истории ничего не остаётся.

mod draw;
mod hand;
mod select;

pub use draw::{DrawTool, Shape};
pub use hand::HandTool;
pub use select::SelectTool;

use ferrum_doc::{Changes, DocResult, Document, NodeId, PageId, Tx};
use ferrum_render::Camera;
use ferrum_scene::Scene;
use kurbo::{Line, Point, Rect};

use crate::editor::{EditorEvent, Settings};
use crate::input::{Modifiers, PointerEvent};

/// Допуск попадания и порог начала перетаскивания — в экранных пикселях.
pub const HIT_TOLERANCE_PX: f64 = 4.0;
pub const DRAG_THRESHOLD_PX: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Move,
    Frame,
    Rect,
    Ellipse,
    Line,
    Hand,
    Image,
}

/// Курсор над холстом. Углы — в градусах на экране: 0 — горизонтальная стрелка
/// ресайза, 90 — вертикальная; интерфейс подбирает ближайший CSS-курсор.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cursor {
    Default,
    Crosshair,
    Grab,
    Grabbing,
    /// Перетаскивание с Alt создаёт копию.
    Duplicate,
    Resize {
        angle: f64,
    },
    Rotate {
        angle: f64,
    },
}

/// Чем инструмент дополняет оверлей.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolOverlay {
    pub marquee: Option<Rect>,
    pub snap_lines: Vec<Line>,
    /// Выделение на время жеста (рисуемая фигура выделена, пока её тянут).
    pub selection: Option<Vec<NodeId>>,
}

/// Что делать редактору после отпускания кнопки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Stay,
    /// Инструмент закончил работу (фигура нарисована): вернуться к «Перемещению».
    Finished,
}

/// Выделение с номером версии. Номер растёт при каждом изменении состава, и всё,
/// что зависит от выделения, — события интерфейсу, кэш рамки и родителей у
/// инструмента — сравнивает номер за O(1), а не списки за O(n) на каждое событие.
/// Менять список можно только методами ниже: они и двигают номер.
#[derive(Debug, Default)]
pub(crate) struct Selection {
    ids: Vec<NodeId>,
    version: u64,
}

impl Selection {
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Заменяет выделение. Тот же список номер не двигает: интерфейс не должен
    /// получать «выделение изменилось», когда оно не менялось.
    pub fn set(&mut self, ids: Vec<NodeId>) {
        if self.ids != ids {
            self.ids = ids;
            self.version += 1;
        }
    }

    pub fn clear(&mut self) {
        if !self.ids.is_empty() {
            self.ids.clear();
            self.version += 1;
        }
    }

    pub fn push(&mut self, id: NodeId) {
        self.ids.push(id);
        self.version += 1;
    }

    pub fn retain(&mut self, f: impl FnMut(&NodeId) -> bool) {
        let n = self.ids.len();
        self.ids.retain(f);
        if self.ids.len() != n {
            self.version += 1;
        }
    }
}

impl std::ops::Deref for Selection {
    type Target = [NodeId];

    fn deref(&self) -> &[NodeId] {
        &self.ids
    }
}

/// Всё, с чем работает инструмент. Создаётся редактором на одно событие.
pub struct Ctx<'a> {
    pub(crate) doc: &'a mut Document,
    pub(crate) page: PageId,
    pub(crate) scene: &'a mut Scene,
    pub(crate) camera: &'a mut Camera,
    pub(crate) selection: &'a mut Selection,
    /// Номер сцены: растёт, когда редактор заменяет сцену целиком (другая
    /// страница), — у новой сцены свой счётчик изменений, и кэш по нему одному
    /// принял бы её за прежнюю.
    pub(crate) scene_epoch: u64,
    pub(crate) hover: &'a mut Option<NodeId>,
    pub(crate) settings: &'a Settings,
    pub(crate) out: &'a mut Outbox,
}

/// Исходящее: события для интерфейса, счётчики.
#[derive(Debug, Default)]
pub(crate) struct Outbox {
    pub events: Vec<EditorEvent>,
    /// Шагов истории, созданных редактором (транзакции с изменениями).
    pub steps: u64,
    next_draft: u32,
    /// Изменения, применённые к сцене текущей страницы с прошлого
    /// `Editor::take_applied` (кэши потребителя, например имён слоёв, обновляются
    /// по ним без полного обхода). Не больше `MAX_APPLIED`: при переполнении
    /// старые отбрасываются, и потребитель видит пропуск по метке сцены.
    pub applied: Vec<Changes>,
}

/// Сколько применённых пачек изменений копится до `Editor::take_applied`.
pub const MAX_APPLIED: usize = 64;

impl Ctx<'_> {
    pub fn scene(&self) -> &Scene {
        self.scene
    }

    pub fn camera(&self) -> &Camera {
        self.camera
    }

    pub fn selection(&self) -> &[NodeId] {
        self.selection
    }

    pub fn settings(&self) -> &Settings {
        self.settings
    }

    pub(crate) fn to_world(&self, screen: Point) -> Point {
        self.camera.screen_to_world(screen)
    }

    /// 4 экранных пикселя в мировых единицах.
    pub(crate) fn tolerance(&self) -> f64 {
        HIT_TOLERANCE_PX / self.camera.zoom()
    }

    pub(crate) fn view(&self) -> Rect {
        self.camera.visible_world_rect()
    }

    /// Свежий идентификатор черновика. Loro не выдаёт узлам пир `u64::MAX`
    /// (`LoroDoc::set_peer_id` его отвергает), поэтому такой `NodeId` не совпадёт
    /// ни с одним узлом документа.
    pub(crate) fn draft_id(&mut self) -> NodeId {
        draft_id(&mut self.out.next_draft)
    }

    /// Записывает правку одной транзакцией: превью убирается, транзакция
    /// выполняется по выделению «до», выделение заменяется результатом, сцена
    /// получает изменения. `f` видит сцену без превью — состояние документа. При
    /// ошибке документ откатывается сам, а интерфейс получает сообщение.
    pub(crate) fn commit<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(&Tx, &Scene) -> DocResult<Vec<NodeId>>,
    {
        self.scene.clear_preview();
        let scene: &Scene = self.scene;
        let result = self
            .doc
            .transact(self.page, self.selection.to_vec(), |tx| f(tx, scene));
        let changed = sync(self.doc, self.page, self.scene, self.out);
        match result {
            Ok(sel) => {
                if changed {
                    self.out.steps += 1;
                }
                self.selection.set(sel);
                true
            }
            Err(e) => {
                self.out.events.push(EditorEvent::Error(e.to_string()));
                false
            }
        }
    }
}

pub(crate) fn draft_id(next: &mut u32) -> NodeId {
    let n = *next;
    *next = next.wrapping_add(1);
    format!("{n}@{}", u64::MAX)
        .parse()
        .expect("идентификатор черновика корректен")
}

/// Забирает изменения всех страниц и применяет изменения текущей к сцене.
/// Изменения других страниц отбрасываются: их сцена строится заново при
/// переключении. Возвращает, было ли что-то изменено.
pub(crate) fn sync(doc: &mut Document, page: PageId, scene: &mut Scene, out: &mut Outbox) -> bool {
    let mut any = false;
    for (p, _) in doc.pages() {
        let changes = doc.take_changes(p);
        if changes.is_empty() {
            continue;
        }
        any = true;
        if p == page {
            if let Ok(pd) = doc.page(p) {
                scene.apply(&changes, pd);
                if out.applied.len() >= MAX_APPLIED {
                    out.applied.clear();
                }
                out.applied.push(changes);
            }
        }
    }
    if any {
        out.events.push(EditorEvent::DocumentChanged);
    }
    any
}

/// Инструмент холста.
pub trait Tool {
    fn kind(&self) -> ToolKind;
    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent);
    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent);
    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow;
    /// Модификаторы изменились во время жеста (нажали или отпустили Shift, Alt).
    fn key(&mut self, cx: &mut Ctx, modifiers: Modifiers);
    /// Отменить жест: превью убирается, документ не меняется.
    fn cancel(&mut self, cx: &mut Ctx);
    /// Идёт жест (кнопка нажата).
    fn is_busy(&self) -> bool;
    fn cursor(&self) -> Cursor;
    fn overlay(&self) -> ToolOverlay;
}
