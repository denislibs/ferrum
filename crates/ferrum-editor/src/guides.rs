//! Линейки и направляющие в редакторе (спека подсистемы 2, раздел 9): где они на
//! холсте, начало отсчёта линеек и кэш линий. Рисует их рендер, данные — в документе.
//!
//! Как в Figma, направляющие видны, пока включены линейки: «Вид → Направляющие»
//! дополнительно прячет их при включённых линейках.
//!
//! Жест: нажатие на линейке или на линии ведёт `GuideDrag`, пока кнопка нажата живёт
//! только превью, отпускание пишет один шаг (`Document::set_guides`), Esc и потеря
//! фокуса — отмена без следов (правило всех жестов, `tools/mod.rs`).

use std::cell::RefCell;
use std::sync::Arc;

use ferrum_doc::{dedup_guides, DocError, DocResult, Guide, GuideAxis, GuideOwner, NodeKind};
use ferrum_render::{ActiveGuide, GuideMark, RulersOverlay, RULER_SIZE};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Point, Rect};

use crate::editor::{Editor, EditorEvent};
use crate::geometry::world_box;
use crate::input::{PointerButton, PointerEvent};
use crate::panel::Phase;
use crate::snap::{Snapper, SNAP_THRESHOLD_PX};
use crate::tools::{sync, Cursor, ToolKind, DRAG_THRESHOLD_PX};

/// Допуск попадания по направляющей — экранные пиксели. Меньше допуска узлов (4):
/// узкий узел на линии остаётся достижимым.
pub const GUIDE_HIT_PX: f64 = 3.0;
/// Сколько выделенных узлов проверяется на «нажатие в выделенном».
const MAX_LEAF_CHECK: usize = 256;

/// Направляющая на холсте.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacedGuide {
    pub owner: GuideOwner,
    pub guide: Guide,
    pub mark: GuideMark,
    /// Фрейм повёрнут: линия косая, к ней нет ни привязки, ни попадания.
    pub rotated: bool,
}

/// Линии текущей страницы; считаются заново, только когда изменился ключ.
#[derive(Debug, Default)]
pub(crate) struct GuideCache {
    /// Номер пересчёта: `FrameKey` сравнивает его, а не список.
    pub revision: u64,
    key: Option<(u64, u64, u64)>,
    pub lines: Arc<Vec<PlacedGuide>>,
    /// Фреймы верхнего уровня с направляющими и метка сцены (поколение и штамп
    /// структуры), при которой они найдены: обход корней — по смене метки, то есть
    /// после правки структуры (штамп растёт и от правок, не меняющих состав корней),
    /// а не на каждый кадр.
    owners: Vec<Slot>,
    owners_key: Option<(u64, u64)>,
    /// Разобранный список страницы: его ключ меняется реже сцены (перетаскивание узла
    /// двигает сцену каждый кадр, а разбор 1000 значений Loro того не стоит).
    page_list: Vec<Guide>,
    page_key: Option<(ferrum_doc::PageId, u64)>,
}

/// Что под указателем без жеста.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum GuideHover {
    /// Линейка, из которой тянется направляющая этой оси.
    Ruler(GuideAxis),
    /// Угол между линейками: ничего не делает.
    Corner,
    /// Направляющая.
    Guide(PlacedGuide),
}

/// Протяжка направляющей. Новая (из линейки) и существующая — одно и то же; у
/// существующей короткое нажатие без сдвига достаётся инструменту как щелчок: линия
/// не мешает выбрать узел под ней.
#[derive(Debug, Clone)]
pub(crate) struct GuideDrag {
    axis: GuideAxis,
    /// Направляющая, которую тянут; `None` — новая.
    source: Option<PlacedGuide>,
    press: PointerEvent,
    /// Указатель отошёл от точки нажатия дальше порога.
    started: bool,
    /// Текущее положение в мире (с привязкой).
    at: f64,
    /// Фрейм верхнего уровня под указателем: будущий владелец и длина линии.
    frame: Option<Slot>,
    /// Указатель на своей линейке: отпускание удалит направляющую.
    over_ruler: bool,
    pub lines: Vec<Line>,
}

/// Начало отсчёта и полоса выделения: считаются обходом выделения, поэтому до
/// смены сцены или выделения хранятся (`Editor::overlay` зовётся на каждый кадр).
pub(crate) type RulersCache = RefCell<Option<((u64, u64, u64), RulersOverlay)>>;

/// Верхний узел цепочки родителей.
fn root_of(scene: &Scene, mut slot: Slot) -> Slot {
    while let Some(p) = scene.at(slot).parent {
        slot = p;
    }
    slot
}

/// Фрейм без поворота и наклона: его оси — оси мира.
pub(crate) fn axis_aligned(scene: &Scene, slot: Slot) -> bool {
    let [a, b, c, d, ..] = scene.at(slot).world.as_coeffs();
    b.abs() < 1e-9 && c.abs() < 1e-9 && a > 0.0 && d > 0.0
}

/// Линия фрейма в мире: от края до края его рамки. У повёрнутого — косая.
/// Поведение: смещение линии хранится как есть, и после уменьшения фрейма линия
/// может оказаться за его рамкой — она рисуется там же (на `w`/`h` фрейма) и остаётся
/// доступной; не удаляется и не прижимается к рамке.
fn frame_line(scene: &Scene, slot: Slot, g: Guide) -> Line {
    let n = scene.at(slot);
    let (w, h) = (n.props.w, n.props.h);
    let (a, b) = match g.axis {
        GuideAxis::X => (Point::new(g.offset, 0.0), Point::new(g.offset, h)),
        GuideAxis::Y => (Point::new(0.0, g.offset), Point::new(w, g.offset)),
    };
    Line::new(n.world * a, n.world * b)
}

/// Линия страницы: концы задают только её ось, рисуется через весь холст.
pub(crate) fn page_line(g: Guide) -> Line {
    match g.axis {
        GuideAxis::X => Line::new((g.offset, 0.0), (g.offset, 1.0)),
        GuideAxis::Y => Line::new((0.0, g.offset), (1.0, g.offset)),
    }
}

impl Editor {
    /// Линейки на холсте: включены, и интерфейс не скрыт (Ctrl/Cmd+\ прячет и их).
    pub fn rulers_visible(&self) -> bool {
        self.settings.rulers && !self.settings.ui_hidden
    }

    /// Направляющие на холсте и как цели привязки: как в Figma — только при линейках.
    pub fn guides_visible(&self) -> bool {
        self.settings.rulers && self.settings.guides
    }

    /// Показ линеек и направляющих — настройка вида, хранит её интерфейс.
    /// `None` — не менять. Шага истории нет.
    pub fn set_view(&mut self, rulers: Option<bool>, guides: Option<bool>) {
        if let Some(v) = rulers {
            self.settings.rulers = v;
        }
        if let Some(v) = guides {
            self.settings.guides = v;
        }
        // Невидимую линию не тянут и не пишут.
        if self.guide_drag.is_some() && !self.guides_visible() {
            self.cancel_guides();
        }
        self.emit();
    }

    /// Сообщает интерфейсу новый показ: настройку хранит он.
    pub(crate) fn push_view(&mut self) {
        self.out
            .events
            .push(crate::editor::EditorEvent::ViewChanged {
                rulers: self.settings.rulers,
                guides: self.settings.guides,
            });
    }

    /// Начало отсчёта линеек — левый верхний угол фрейма верхнего уровня, под которым
    /// всё выделение (без поворота), иначе ноль страницы; полоса — рамка выделения.
    pub(crate) fn rulers_overlay(&self) -> Option<RulersOverlay> {
        if !self.rulers_visible() {
            return None;
        }
        let key = (
            self.scene_epoch,
            self.scene.change_counter(),
            self.selection.version(),
        );
        if let Some((k, r)) = *self.rulers_cache.borrow() {
            if k == key {
                return Some(r);
            }
        }
        let mut band: Option<Rect> = None;
        let mut top: Option<Option<Slot>> = None;
        for id in self.selection.iter() {
            let Some(slot) = self.scene.slot(*id) else {
                continue;
            };
            if let Some(b) = world_box(&self.scene, slot) {
                band = Some(band.map_or(b, |u| u.union(b)));
            }
            let root = root_of(&self.scene, slot);
            top = match top {
                None => Some(Some(root)),
                Some(Some(t)) if t == root => top,
                _ => Some(None),
            };
        }
        let origin = top
            .flatten()
            .filter(|s| self.scene.at(*s).props.kind == NodeKind::Frame)
            .filter(|s| axis_aligned(&self.scene, *s))
            .and_then(|s| world_box(&self.scene, s))
            .map_or(Point::ZERO, |b| b.origin());
        let r = RulersOverlay { origin, band };
        *self.rulers_cache.borrow_mut() = Some((key, r));
        Some(r)
    }

    /// Пересчитывает линии, если изменились сцена, страница или её направляющие.
    pub(crate) fn refresh_guides(&mut self) {
        let key = (
            self.scene.generation(),
            self.scene.change_counter(),
            self.doc.guides_revision(),
        );
        if self.guides.key == Some(key) {
            return;
        }
        // Линии могли измениться: взятая прежде за наведённую — уже не та.
        if matches!(self.guide_hover, Some(GuideHover::Guide(_))) {
            self.guide_hover = None;
        }
        let owners_key = (self.scene.generation(), self.scene.structure_stamp());
        if self.guides.owners_key != Some(owners_key) {
            let scene = &self.scene;
            self.guides.owners = scene
                .roots()
                .iter()
                .copied()
                .filter(|s| {
                    let p = &scene.at(*s).props;
                    p.kind == NodeKind::Frame && p.guides.is_some()
                })
                .collect();
            self.guides.owners_key = Some(owners_key);
        }
        let mut lines = Vec::new();
        let preview = self.guides_preview.as_ref();
        let page_key = (self.page, self.doc.guides_revision());
        if self.guides.page_key != Some(page_key) {
            self.guides.page_list = self.doc.page_guides(self.page);
            self.guides.page_key = Some(page_key);
        }
        let of_page: &[Guide] = match preview {
            Some((GuideOwner::Page(p), list)) if *p == self.page => list,
            _ => &self.guides.page_list,
        };
        for &g in of_page {
            lines.push(PlacedGuide {
                owner: GuideOwner::Page(self.page),
                guide: g,
                mark: GuideMark {
                    line: page_line(g),
                    infinite: true,
                },
                rotated: false,
            });
        }
        // Превью фрейма без направляющих (первая у него) тоже рисуется.
        let previewed = preview.and_then(|(o, list)| match o {
            GuideOwner::Frame(id) => self.scene.slot(*id).map(|s| (s, list.as_slice())),
            GuideOwner::Page(_) => None,
        });
        let slots = self.guides.owners.iter().copied().chain(
            previewed
                .map(|(s, _)| s)
                .filter(|s| !self.guides.owners.contains(s)),
        );
        for slot in slots {
            let Some(n) = self.scene.node_at(slot) else {
                continue;
            };
            let list: &[Guide] = match previewed {
                Some((s, list)) if s == slot => list,
                _ => n.props.guides.as_deref().map_or(&[], |g| g.as_slice()),
            };
            if !self.scene.is_visible(slot) {
                continue;
            }
            let rotated = !axis_aligned(&self.scene, slot);
            for g in list {
                lines.push(PlacedGuide {
                    owner: GuideOwner::Frame(n.id),
                    guide: *g,
                    mark: GuideMark {
                        line: frame_line(&self.scene, slot, *g),
                        infinite: false,
                    },
                    rotated,
                });
            }
        }
        // Те же линии — та же ревизия: перетаскивание узла не перерисовывает их заново.
        if *self.guides.lines != lines {
            self.guides.lines = Arc::new(lines);
            self.guides.revision += 1;
        }
        self.guides.key = Some(key);
    }

    /// Метки направляющих для оверлея.
    pub(crate) fn guide_marks(&self) -> Vec<GuideMark> {
        if !self.guides_visible() {
            return Vec::new();
        }
        self.guides.lines.iter().map(|g| g.mark).collect()
    }

    /// Какая линейка под экранной точкой.
    pub(crate) fn over_ruler(&self, p: Point) -> Option<GuideHover> {
        if !self.rulers_visible() {
            return None;
        }
        match (p.x < RULER_SIZE, p.y < RULER_SIZE) {
            (true, true) => Some(GuideHover::Corner),
            // Сверху — горизонтальная, смещение по y; слева — вертикальная.
            (false, true) => Some(GuideHover::Ruler(GuideAxis::Y)),
            (true, false) => Some(GuideHover::Ruler(GuideAxis::X)),
            (false, false) => None,
        }
    }

    /// Ближайшая направляющая под экранной точкой. Косые линии не ловятся.
    fn guide_at(&self, p: Point) -> Option<PlacedGuide> {
        if !self.guides_visible() || self.over_ruler(p).is_some() {
            return None;
        }
        let world = self.camera.screen_to_world(p);
        let tol = GUIDE_HIT_PX / self.camera.zoom();
        let mut best: Option<(f64, PlacedGuide)> = None;
        for g in self.guides.lines.iter().filter(|g| !g.rotated) {
            let line = g.mark.line;
            let (d, cross, a, b) = match g.guide.axis {
                GuideAxis::X => ((world.x - line.p0.x).abs(), world.y, line.p0.y, line.p1.y),
                GuideAxis::Y => ((world.y - line.p0.y).abs(), world.x, line.p0.x, line.p1.x),
            };
            // Линия заблокированного фрейма не берётся: она едет вместе с ним.
            if let GuideOwner::Frame(id) = g.owner {
                if self.scene.node(id).is_none_or(|n| n.props.locked) {
                    continue;
                }
            }
            if d > tol || (!g.mark.infinite && (cross < a.min(b) || cross > a.max(b))) {
                continue;
            }
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, *g));
            }
        }
        best.map(|(_, g)| g)
    }

    /// Рамка выделенных узлов, кроме фреймов: нажатие в ней двигает их, а не линию
    /// (у выделенного фрейма его направляющие остаются доступными).
    fn pressing_selected_leaf(&self, world: Point) -> bool {
        self.selection.iter().any(|id| {
            self.scene
                .slot(*id)
                .filter(|s| self.scene.at(*s).props.kind != NodeKind::Frame)
                .and_then(|s| world_box(&self.scene, s))
                .is_some_and(|b| b.contains(world))
        })
    }

    /// Можно ли взяться за направляющую в этой точке, а не отдать её инструменту:
    /// только «Перемещение» без модификаторов, без правок, не над ручками рамки.
    fn grabbable_guide(&self, e: &PointerEvent) -> Option<PlacedGuide> {
        let m = e.modifiers;
        if self.tool.kind() != ToolKind::Move
            || self.doc.is_read_only()
            || self.text_edit.is_some()
            || self.vector_edit.is_some()
            || self.is_busy()
            || m.shift
            || m.alt
            || m.ctrl
            || m.meta
            || matches!(
                self.tool.cursor(),
                Cursor::Resize { .. } | Cursor::Rotate { .. }
            )
        {
            return None;
        }
        let g = self.guide_at(e.point)?;
        // Обход выделения на каждое движение: при тысячах выделенных он не окупается,
        // и такое выделение просто не защищает нажатие от линии.
        if self.selection.len() <= MAX_LEAF_CHECK
            && self.pressing_selected_leaf(self.camera.screen_to_world(e.point))
        {
            return None;
        }
        Some(g)
    }

    /// Указатель над линейкой без жеста: инструмент его не видит, наведения нет.
    /// `true` — событие обработано.
    pub(crate) fn ruler_hover_update(&mut self, e: &PointerEvent) -> bool {
        let busy = self.is_busy() || self.text_edit.as_ref().is_some_and(|t| t.drag.is_some());
        let over = if busy { None } else { self.over_ruler(e.point) };
        match over {
            Some(h) => {
                self.guide_hover = Some(h);
                self.hover = None;
                self.text_hover = false;
                true
            }
            None => {
                self.guide_hover = None;
                false
            }
        }
    }

    /// Указатель над направляющей: наведения на узел под ней нет, курсор — протяжки.
    pub(crate) fn guide_hover_update(&mut self, e: &PointerEvent) {
        if let Some(g) = self.grabbable_guide(e) {
            self.guide_hover = Some(GuideHover::Guide(g));
            self.hover = None;
        }
    }

    /// После жеста указатель стоит на новом месте: наведение считается заново.
    pub(crate) fn rehover_guides(&mut self, e: &PointerEvent) {
        self.guide_hover = None;
        if !self.ruler_hover_update(e) {
            self.guide_hover_update(e);
        }
    }

    pub(crate) fn guide_cursor(&self) -> Option<Cursor> {
        let resize = |axis| Cursor::Resize {
            angle: if axis == GuideAxis::X { 0.0 } else { 90.0 },
        };
        if let Some(d) = &self.guide_drag {
            return Some(resize(d.axis));
        }
        // Наведение запомнено при прошлом движении: инструмент, показ или правка с тех
        // пор могли смениться, а указатель ещё не двигался.
        let editing = self.text_edit.is_some() || self.vector_edit.is_some();
        match self.guide_hover? {
            GuideHover::Ruler(_) | GuideHover::Corner if !self.rulers_visible() => None,
            GuideHover::Ruler(axis) => {
                let usable = !editing && !self.doc.is_read_only() && self.guides_visible();
                Some(if usable {
                    resize(axis)
                } else {
                    Cursor::Default
                })
            }
            GuideHover::Corner => Some(Cursor::Default),
            GuideHover::Guide(g) => {
                (self.tool.kind() == ToolKind::Move && !editing && self.guides_visible())
                    .then(|| resize(g.guide.axis))
            }
        }
    }

    /// Нажатие левой кнопкой: `true` — его забрала линейка или направляющая. Нажатие
    /// на линейку инструменту не достаётся никогда; правка текста и вектора от него
    /// не кончается.
    pub(crate) fn guide_press(&mut self, e: &PointerEvent) -> bool {
        // Прежнее нажатие без отпускания (событие потеряно): жест отменяется.
        self.cancel_guides();
        if let Some(over) = self.over_ruler(e.point) {
            let editing = self.text_edit.is_some() || self.vector_edit.is_some();
            // Нажатие забрала линейка: потерянный жест инструмента (отпускание не
            // дошло) кончается, даже если линия не начнётся.
            if self.tool.is_busy() {
                self.cancel_gesture();
            }
            if let GuideHover::Ruler(axis) = over {
                if !editing && !self.doc.is_read_only() && self.guides_visible() {
                    self.guide_drag = Some(GuideDrag {
                        axis,
                        source: None,
                        press: *e,
                        started: false,
                        at: 0.0,
                        frame: None,
                        over_ruler: true,
                        lines: Vec::new(),
                    });
                }
            }
            return true;
        }
        let Some(g) = self.grabbable_guide(e) else {
            return false;
        };
        self.guide_drag = Some(GuideDrag {
            axis: g.guide.axis,
            source: Some(g),
            press: *e,
            started: false,
            at: 0.0,
            frame: None,
            over_ruler: false,
            lines: Vec::new(),
        });
        true
    }

    /// Фрейм верхнего уровня под точкой мира: видимый, не заблокированный, без
    /// поворота. Он и станет владельцем направляющей.
    fn top_frame_under(&self, world: Point) -> Option<Slot> {
        // Не `hit_test_all`: у фрейма без заливки попадание только по краю и детям.
        self.scene
            .query_rect(Rect::from_points(world, world))
            .into_iter()
            .filter(|id| !self.scene.is_draft(*id))
            .filter_map(|id| self.scene.slot(id))
            .map(|s| root_of(&self.scene, s))
            .filter(|&s| {
                let n = self.scene.at(s);
                n.props.kind == NodeKind::Frame
                    && self.scene.is_visible(s)
                    && !n.props.locked
                    && axis_aligned(&self.scene, s)
                    && world_box(&self.scene, s).is_some_and(|b| b.contains(world))
            })
            .max_by_key(|&s| self.scene.order_key(s))
    }

    pub(crate) fn guide_drag_move(&mut self, e: &PointerEvent) {
        let Some(mut d) = self.guide_drag.take() else {
            return;
        };
        if !d.started && (e.point - d.press.point).hypot() >= DRAG_THRESHOLD_PX {
            d.started = true;
            // Прежняя линия — в превью, пока жест идёт.
            if let Some(g) = d.source {
                let rest = self.owner_guides_without(g);
                self.guides_preview = Some((g.owner, rest));
                self.guides.key = None;
            }
        }
        if d.started {
            let world = self.camera.screen_to_world(e.point);
            d.frame = self.top_frame_under(world);
            // Удаление — только на видимой линейке (при скрытом интерфейсе её нет).
            d.over_ruler = self.rulers_visible()
                && match d.axis {
                    GuideAxis::X => e.point.x < RULER_SIZE,
                    GuideAxis::Y => e.point.y < RULER_SIZE,
                };
            let raw = match d.axis {
                GuideAxis::X => world.x,
                GuideAxis::Y => world.y,
            };
            d.lines.clear();
            let mut snapped = None;
            if self.settings.snapping && !d.over_ruler {
                let parent = d.frame.map(|s| self.scene.at(s).id);
                let snapper = Snapper::for_parent(&self.scene, parent, &[])
                    .with_guides(self.guides.lines.clone())
                    .skipping_guide(d.source);
                let threshold = SNAP_THRESHOLD_PX / self.camera.zoom();
                let moving = Rect::from_points(world, world);
                let view = self.camera.visible_world_rect();
                let s = match d.axis {
                    GuideAxis::X => snapper.snap(&self.scene, &[raw], &[], moving, threshold, view),
                    GuideAxis::Y => snapper.snap(&self.scene, &[], &[raw], moving, threshold, view),
                };
                snapped = match d.axis {
                    GuideAxis::X => s.dx,
                    GuideAxis::Y => s.dy,
                };
                d.lines = s.lines;
            }
            d.at = match snapped {
                Some(delta) => raw + delta,
                None if self.settings.pixel_snap => raw.round(),
                None => raw,
            };
        }
        self.guide_drag = Some(d);
    }

    fn owner_guides_without(&self, g: PlacedGuide) -> Vec<Guide> {
        self.owner_guides(g.owner)
            .into_iter()
            .filter(|x| *x != g.guide)
            .collect()
    }

    /// Список владельца из документа и сцены.
    fn owner_guides(&self, owner: GuideOwner) -> Vec<Guide> {
        match owner {
            GuideOwner::Page(p) => self.doc.page_guides(p),
            GuideOwner::Frame(id) => self
                .scene
                .node(id)
                .and_then(|n| n.props.guides.as_ref())
                .map_or_else(Vec::new, |g| g.to_vec()),
        }
    }

    pub(crate) fn guide_drag_up(&mut self, e: &PointerEvent) {
        self.guide_drag_move(e);
        let Some(d) = self.guide_drag.take() else {
            return;
        };
        self.guides_preview = None;
        self.guides.key = None;
        if !d.started {
            // Щелчок по линии — как щелчок по тому, что под ней. Щелчок по линейке — ничего.
            if d.source.is_some() {
                self.press_tool(&d.press);
                let mut up = *e;
                up.button = PointerButton::Primary;
                if self.tool.is_busy() {
                    self.release_tool(&up);
                }
            }
            return;
        }
        if d.over_ruler {
            if let Some(g) = d.source {
                let rest = self.owner_guides_without(g);
                self.write_guides(&[(g.owner, rest)]);
            }
            return;
        }
        let (owner, offset) = match d.frame {
            Some(slot) => {
                let n = self.scene.at(slot);
                let origin = world_box(&self.scene, slot).map_or(Point::ZERO, |b| b.origin());
                let base = match d.axis {
                    GuideAxis::X => origin.x,
                    GuideAxis::Y => origin.y,
                };
                (GuideOwner::Frame(n.id), d.at - base)
            }
            None => (GuideOwner::Page(self.page), d.at),
        };
        let guide = Guide::new(d.axis, offset);
        if !guide.is_valid() {
            return;
        }
        let edits = match d.source {
            Some(g) if g.owner == owner => {
                let mut list = self.owner_guides(owner);
                if let Some(slot) = list.iter_mut().find(|x| **x == g.guide) {
                    *slot = guide;
                }
                vec![(owner, list)]
            }
            Some(g) => {
                let rest = self.owner_guides_without(g);
                let mut list = self.owner_guides(owner);
                list.push(guide);
                vec![(g.owner, rest), (owner, list)]
            }
            None => {
                let mut list = self.owner_guides(owner);
                list.push(guide);
                vec![(owner, list)]
            }
        };
        self.write_guides(&edits);
    }

    /// Запись одним шагом; отказ (предел) — сообщение, шага нет.
    fn write_guides(&mut self, edits: &[(GuideOwner, Vec<Guide>)]) {
        if let Err(e) = self.commit_guides(self.page, self.selection.to_vec(), edits) {
            self.out.events.push(EditorEvent::Error(e.to_string()));
        }
    }

    fn commit_guides(
        &mut self,
        page: ferrum_doc::PageId,
        selection: Vec<ferrum_doc::NodeId>,
        edits: &[(GuideOwner, Vec<Guide>)],
    ) -> DocResult<()> {
        let before = self.doc.last_step();
        let result = self.doc.set_guides(page, selection, edits);
        let synced = sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        if self.doc.last_step() != before {
            self.out.steps += 1;
            if !synced {
                self.out.events.push(EditorEvent::DocumentChanged);
            }
        }
        result
    }

    /// `SetGuides` из протокола (спека подсистемы 2, раздел 11): превью — без
    /// шага, запись — один шаг, отмена — назад к документу. Фрейм — только верхнего
    /// уровня на текущей странице; сверх предела и неверные смещения — отказ. Страница
    /// может быть не загружена: сначала загрузка, затем проверка «только для чтения».
    pub fn set_guides(
        &mut self,
        owner: GuideOwner,
        guides: Vec<Guide>,
        phase: Phase,
    ) -> DocResult<()> {
        let result = self.set_guides_inner(owner, guides, phase);
        // Отказ при записи: превью, оставшееся от жеста, снимается.
        if result.is_err() && phase == Phase::Commit {
            self.cancel_guides();
            self.emit();
        }
        result
    }

    fn set_guides_inner(
        &mut self,
        owner: GuideOwner,
        guides: Vec<Guide>,
        phase: Phase,
    ) -> DocResult<()> {
        if phase == Phase::Cancel {
            self.cancel_guides();
            self.emit();
            return Ok(());
        }
        ferrum_doc::check_guides(&guides)?;
        let page = match owner {
            GuideOwner::Page(p) => p,
            GuideOwner::Frame(id) => {
                let slot = self
                    .scene
                    .slot(id)
                    .ok_or_else(|| DocError::NodeNotFound(id.to_string()))?;
                let n = self.scene.at(slot);
                if n.props.kind != NodeKind::Frame || n.parent.is_some() {
                    return Err(DocError::InvalidGuides(
                        "они бывают у страницы и фрейма верхнего уровня".into(),
                    ));
                }
                self.page
            }
        };
        self.doc.page(page)?;
        if self.doc.is_read_only() {
            return Err(DocError::ReadOnly);
        }
        match phase {
            Phase::Preview => {
                if page != self.page {
                    return Err(DocError::InvalidGuides(
                        "превью только на текущей странице".into(),
                    ));
                }
                // Жест протокола не живёт вместе с жестом холста.
                if self.guide_drag.is_some() || self.tool.is_busy() {
                    self.cancel_gesture();
                }
                self.guides_preview = Some((owner, dedup_guides(guides)));
                self.guides.key = None;
            }
            Phase::Commit => {
                self.begin_doc_edit();
                self.guides_preview = None;
                self.guides.key = None;
                let selection = if page == self.page {
                    self.selection.to_vec()
                } else {
                    Vec::new()
                };
                let result = self.commit_guides(page, selection, &[(owner, guides)]);
                self.emit();
                return result;
            }
            Phase::Cancel => unreachable!("разобрано выше"),
        }
        self.emit();
        Ok(())
    }

    /// Отмена жеста: превью и протяжки больше нет, документ не менялся.
    /// Перетаскивание направляющей опирается на один из этих узлов: фрейм под указателем или фрейм,
    /// с которого она снята.
    pub(crate) fn guide_drag_touches(&self, gone: &impl Fn(ferrum_doc::NodeId) -> bool) -> bool {
        self.guide_drag.as_ref().is_some_and(|d| {
            d.frame
                .and_then(|s| self.scene.node_at(s))
                .is_some_and(|n| gone(n.id))
                || matches!(d.source.map(|g| g.owner), Some(GuideOwner::Frame(f)) if gone(f))
        })
    }

    pub(crate) fn cancel_guides(&mut self) {
        let dragged = self.guide_drag.take().is_some();
        let previewed = self.guides_preview.take().is_some();
        if dragged || previewed {
            self.guides.key = None;
        }
    }

    /// Линия, которую тянут: пока указатель не на своей линейке.
    pub(crate) fn active_guide(&self) -> Option<ActiveGuide> {
        let d = self.guide_drag.as_ref()?;
        if !d.started || d.over_ruler {
            return None;
        }
        let guide = Guide::new(d.axis, d.at);
        let origin = self.rulers_overlay().map_or(Point::ZERO, |r| r.origin);
        let value = d.at
            - match d.axis {
                GuideAxis::X => origin.x,
                GuideAxis::Y => origin.y,
            };
        let box_ = d.frame.and_then(|s| {
            self.scene
                .node_at(s)
                .and_then(|_| world_box(&self.scene, s))
        });
        let mark = match box_ {
            Some(b) => GuideMark {
                line: match d.axis {
                    GuideAxis::X => Line::new((d.at, b.y0), (d.at, b.y1)),
                    GuideAxis::Y => Line::new((b.x0, d.at), (b.x1, d.at)),
                },
                infinite: false,
            },
            None => GuideMark {
                line: page_line(guide),
                infinite: true,
            },
        };
        Some(ActiveGuide { mark, value })
    }
}
