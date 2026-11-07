//! Метки комментариев на холсте (спека подсистемы 4, разделы 10.2 и 10.3). Сами обсуждения
//! живут на сервере, интерфейс присылает метки (`set_comment_pins`), а редактор их раскладывает
//! по сцене и камере, собирает слипшиеся в группы, проверяет попадание и ведёт жесты меток:
//! открыть, перетащить свою, поставить новую в режиме «Комментарий». Метки не трогают ни
//! документ, ни историю, ни `own_change_counter` сцены: это не правка, а оверлей.
//!
//! Привязка: метка над узлом едет с верхним узлом страницы (ребёнком страницы) под точкой —
//! `x`, `y` в его координатах; на пустом месте — в мире. Каждый кадр мировая точка метки —
//! матрица узла в сцене (со своим превью жеста и слоем чужого живого превью) на `(x, y)`; узла
//! нет или он скрыт — последняя мировая точка с сервера `(wx, wy)` **(поведение)**.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use ferrum_doc::{NodeId, PageId};
use ferrum_render::{pin_contains, Camera, PinKind, PinMark, Rgba, MAX_ZOOM, PIN_SIZE};
use kurbo::{Point, Rect};

use crate::editor::{Editor, EditorEvent};
use crate::input::PointerEvent;
use crate::tools::{ToolKind, DRAG_THRESHOLD_PX, HIT_TOLERANCE_PX};

/// Меток держит редактор: больше — лишние не принимаются (страница из 1000 обсуждений — в
/// десять раз меньше).
pub const COMMENT_PINS_MAX: usize = 10_000;
/// Метки ближе этого на экране (CSS-пиксели, между точками меток) слипаются в группу: капли
/// перекрылись бы.
pub const PIN_CLUSTER_DISTANCE: f64 = PIN_SIZE;
/// Поле вокруг меток группы при щелчке по ней — CSS-пиксели: крайние метки не у края холста.
pub const PIN_CLUSTER_FIT_PADDING: f64 = 4.0 * PIN_SIZE;

/// Где метка: страница, верхний узел (или мир), точка в его координатах и в мире.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PinAnchor {
    pub page: PageId,
    pub node: Option<NodeId>,
    pub local: Point,
    pub world: Point,
}

/// Метка обсуждения от интерфейса.
#[derive(Debug, Clone, PartialEq)]
pub struct CommentPin {
    pub id: Arc<str>,
    pub anchor: PinAnchor,
    /// Инициалы (до трёх графем) и цвет автора.
    pub initials: Arc<str>,
    pub color: Rgba,
    pub replies: u32,
    pub unread: bool,
    pub resolved: bool,
    /// Можно ли тащить (автор обсуждения или владелец файла — решает сервер).
    pub movable: bool,
}

/// Нажатие на метку, группу или холст в режиме комментариев — до отпускания.
#[derive(Debug, Clone)]
enum Press {
    Pin {
        id: Arc<str>,
        /// Экранная точка нажатия (порог) и мировая под ней: сдвиг метки — от неё, поэтому
        /// метка идёт за указателем и когда камера едет посреди жеста.
        start: Point,
        grab: Point,
        /// Мировая точка метки на нажатие и куда её тянут (после порога).
        from: Point,
        to: Option<Point>,
        movable: bool,
    },
    Cluster {
        members: Vec<Point>,
    },
    Place {
        start: Point,
        moved: bool,
    },
}

/// Что под точкой экрана: метка или группа.
#[derive(Debug, Clone)]
enum Target {
    Pin(Arc<str>),
    /// Мировые точки меток группы — для «вписать в экран».
    Cluster(Vec<Point>),
}

#[derive(Debug, Clone)]
struct Hit {
    /// Точка метки на экране, CSS-пиксели.
    at: Point,
    kind: PinKind,
    target: Target,
}

/// Раскладка меток для кадра и попадания.
#[derive(Debug, Default)]
pub(crate) struct Layout {
    marks: Arc<[PinMark]>,
    /// По порядку рисования: верхняя — последняя.
    hits: Vec<Hit>,
    /// Экранная точка открытой метки (или черновика) и видна ли она на холсте.
    anchor: Option<(Point, bool)>,
}

/// Ревизия меток, сцена (счётчик со всеми слоями превью, `generation`, `scene_epoch`), страница и
/// камера: раскладка та же, пока они те же.
type LayoutKey = (u64, u64, u64, u64, PageId, Camera);

pub(crate) struct Comments {
    /// Хозяин сказал, что комментарии есть (файл на сервере и роль пришла).
    available: bool,
    /// Shift+C: метки видны.
    visible: bool,
    show_resolved: bool,
    /// По идентификатору: порядок рисования стабилен от кадра к кадру.
    pins: BTreeMap<Arc<str>, CommentPin>,
    open: Option<Arc<str>>,
    draft: Option<PinAnchor>,
    press: Option<Press>,
    /// Над меткой без нажатия: курсор — стрелка, а не прицел.
    hover: bool,
    /// Перейти к обсуждению, когда его метка или страница дойдут.
    focus: Option<Arc<str>>,
    /// Растёт при всём, что видно у меток, кроме сцены и камеры.
    pub(crate) revision: u64,
    layout: RefCell<Option<(LayoutKey, Arc<Layout>)>>,
    sent_anchor: Option<(Point, bool)>,
    sent_state: Option<(bool, bool, bool)>,
}

impl Default for Comments {
    fn default() -> Self {
        Comments {
            available: false,
            visible: true,
            show_resolved: false,
            pins: BTreeMap::new(),
            open: None,
            draft: None,
            press: None,
            hover: false,
            focus: None,
            revision: 0,
            layout: RefCell::new(None),
            sent_anchor: None,
            sent_state: None,
        }
    }
}

/// Мировая точка метки: узел есть и виден — по его матрице в сцене, иначе последняя мировая.
fn pin_world(scene: &ferrum_scene::Scene, a: &PinAnchor) -> Point {
    if let Some(slot) = a.node.and_then(|id| scene.slot(id)) {
        if scene.is_visible(slot) {
            let p = scene.at(slot).world * a.local;
            if p.x.is_finite() && p.y.is_finite() {
                return p;
            }
        }
    }
    a.world
}

/// Группы меток на экране. Первый проход: метка ложится в первую группу, чья первая метка
/// ближе `PIN_CLUSTER_DISTANCE`, иначе начинает свою (сетка ячеек — O(n)). Дальше группы, чьи
/// середины ближе того же, сливаются (не больше `CLUSTER_PASSES` проходов): иначе ряд меток давал
/// бы цепочку перекрытых кругов. Сцепления «каждая с соседней» нет: строка из 1000 меток с шагом
/// меньше капли не становится одной группой через весь экран.
fn cluster(points: &[Point]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = (0..points.len()).map(|i| vec![i]).collect();
    let mut centers: Vec<Point> = points.to_vec();
    for _ in 0..CLUSTER_PASSES {
        let merged = seed_groups(&centers);
        if merged.len() == groups.len() {
            break;
        }
        let next: Vec<Vec<usize>> = merged
            .iter()
            .map(|m| m.iter().flat_map(|g| groups[*g].iter().copied()).collect())
            .collect();
        centers = next
            .iter()
            .map(|g| {
                let n = g.len() as f64;
                g.iter()
                    .fold(Point::ZERO, |a, i| a + points[*i].to_vec2() / n)
            })
            .collect();
        groups = next;
    }
    groups
}

/// Проходов слияния групп в `cluster`.
const CLUSTER_PASSES: usize = 4;

/// Один проход: точка — в группу первой точки ближе `PIN_CLUSTER_DISTANCE`, иначе своя.
fn seed_groups(points: &[Point]) -> Vec<Vec<usize>> {
    let cell = |p: Point| {
        (
            (p.x / PIN_CLUSTER_DISTANCE).floor() as i64,
            (p.y / PIN_CLUSTER_DISTANCE).floor() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let (cx, cy) = cell(*p);
        let mut found = None;
        'look: for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(list) = grid.get(&(cx + dx, cy + dy)) {
                    for g in list {
                        if points[groups[*g][0]].distance(*p) < PIN_CLUSTER_DISTANCE {
                            found = Some(*g);
                            break 'look;
                        }
                    }
                }
            }
        }
        match found {
            Some(g) => groups[g].push(i),
            None => {
                grid.entry((cx, cy)).or_default().push(groups.len());
                groups.push(vec![i]);
            }
        }
    }
    groups
}

impl Editor {
    fn comments_bump(&mut self) {
        self.comments.revision += 1;
    }

    /// Хозяин сказал, есть ли комментарии (файл на сервере, роль пришла). Без них метки не
    /// рисуются, команды «Комментарий» и Shift+C недоступны, режим комментариев кончается.
    pub fn set_comments_available(&mut self, on: bool) {
        if self.comments.available == on {
            return;
        }
        self.comments.available = on;
        if !on {
            self.comments.press = None;
            self.comments.open = None;
            self.comments.draft = None;
            self.comments.focus = None;
            if self.tool.kind() == ToolKind::Comment {
                self.set_tool(ToolKind::Move);
            }
        }
        self.comments_bump();
        self.emit();
    }

    pub fn comments_available(&self) -> bool {
        self.comments.available
    }

    /// Видны ли метки (Shift+C).
    pub fn comments_visible(&self) -> bool {
        self.comments.visible
    }

    pub(crate) fn toggle_comments(&mut self) {
        self.comments.visible = !self.comments.visible;
        if !self.comments.visible {
            self.comments.press = None;
            self.comments.hover = false;
        }
        self.comments_bump();
    }

    /// Вход в режим комментариев показывает метки **(поведение, как в Figma)**.
    pub(crate) fn comment_mode_entered(&mut self) {
        if !self.comments.visible {
            self.comments.visible = true;
            self.comments_bump();
        }
    }

    pub fn set_comments_show_resolved(&mut self, on: bool) {
        if self.comments.show_resolved != on {
            self.comments.show_resolved = on;
            self.comments_bump();
            self.emit();
        }
    }

    /// Метки: `replace` — весь набор, иначе добавить и заменить по идентификатору. Сверх
    /// `COMMENT_PINS_MAX` новые не принимаются.
    pub fn set_comment_pins(&mut self, pins: Vec<CommentPin>, replace: bool) {
        if replace {
            self.comments.pins.clear();
        }
        for p in pins {
            let known = self.comments.pins.contains_key(&p.id);
            if !known && self.comments.pins.len() >= COMMENT_PINS_MAX {
                continue;
            }
            self.comments.pins.insert(p.id.clone(), p);
        }
        self.forget_missing_pins();
        self.comments_bump();
        self.retry_comment_focus();
        self.emit();
    }

    pub fn remove_comment_pins(&mut self, ids: &[&str]) {
        let mut any = false;
        for id in ids {
            any |= self.comments.pins.remove(*id).is_some();
        }
        if any {
            self.forget_missing_pins();
            self.comments_bump();
            self.emit();
        }
    }

    /// Открытая и тянущаяся метки, которых больше нет, — закрыты, жест отменён.
    fn forget_missing_pins(&mut self) {
        let pins = &self.comments.pins;
        if self
            .comments
            .open
            .as_ref()
            .is_some_and(|id| !pins.contains_key(id))
        {
            self.comments.open = None;
        }
        if matches!(&self.comments.press, Some(Press::Pin { id, .. }) if !pins.contains_key(id)) {
            self.comments.press = None;
        }
    }

    pub fn comment_pin(&self, id: &str) -> Option<&CommentPin> {
        self.comments.pins.get(id)
    }

    pub fn comment_pin_count(&self) -> usize {
        self.comments.pins.len()
    }

    /// Открытое обсуждение: его метка выделена, за ней следуют события `CommentAnchor`.
    pub fn set_comment_open(&mut self, id: Option<&str>) {
        let id: Option<Arc<str>> = id.map(Arc::from);
        if self.comments.open != id {
            self.comments.open = id;
            self.comments_bump();
        }
        // Открыть другое — переход к прежнему, если он ещё ждал, не нужен.
        self.comments.focus = None;
        self.emit();
    }

    pub fn comment_open(&self) -> Option<&str> {
        self.comments.open.as_deref()
    }

    /// Черновик нового комментария: его метка, пока пишут текст.
    pub fn set_comment_draft(&mut self, draft: Option<PinAnchor>) {
        if self.comments.draft != draft {
            self.comments.draft = draft;
            self.comments_bump();
        }
        self.emit();
    }

    /// Перейти к обсуждению (панель, ссылка): страница метки, камера по центру метки (зум тот
    /// же), обсуждение открыто, событие `CommentOpened`. Метки или страницы ещё нет — переход,
    /// как только они дойдут.
    pub fn comment_focus(&mut self, id: &str) {
        self.comments.focus = Some(Arc::from(id));
        self.retry_comment_focus();
        self.emit();
    }

    pub(crate) fn retry_comment_focus(&mut self) {
        let Some(id) = self.comments.focus.clone() else {
            return;
        };
        let Some(page) = self.comments.pins.get(&id).map(|p| p.anchor.page) else {
            return;
        };
        if page != self.page {
            if self.doc.page(page).is_err() {
                return;
            }
            if self.set_page(page).is_err() {
                return;
            }
        }
        self.comments.focus = None;
        let pin = &self.comments.pins[&id];
        let world = pin_world(&self.scene, &pin.anchor);
        self.camera.set_center(world);
        if self.comments.open.as_ref() != Some(&id) {
            self.comments.open = Some(id.clone());
        }
        self.comments_bump();
        let screen = self.camera.world_to_screen(world);
        self.out
            .events
            .push(EditorEvent::CommentOpened { id, screen });
    }

    /// Ключ раскладки (`LayoutKey`).
    fn layout_key(&self) -> LayoutKey {
        (
            self.comments.revision,
            self.scene.change_counter(),
            self.scene.generation(),
            self.scene_epoch,
            self.page,
            self.camera.clone(),
        )
    }

    /// Раскладка меток этой страницы: из кэша, пока не сменились метки, сцена или камера.
    pub(crate) fn comment_layout(&self) -> Arc<Layout> {
        let key = self.layout_key();
        if let Some((k, l)) = &*self.comments.layout.borrow() {
            if *k == key {
                return l.clone();
            }
        }
        let l = Arc::new(self.build_comment_layout());
        *self.comments.layout.borrow_mut() = Some((key, l.clone()));
        l
    }

    fn build_comment_layout(&self) -> Layout {
        let c = &self.comments;
        if !c.available {
            return Layout::default();
        }
        let page = self.page;
        let cam = &self.camera;
        let dragging = match &c.press {
            Some(Press::Pin {
                id, to: Some(to), ..
            }) => Some((id.clone(), *to)),
            _ => None,
        };
        // Открытая и тянущаяся — отдельно, поверх групп.
        let mut singles: Vec<(Arc<str>, Point)> = Vec::new();
        let mut ids: Vec<&CommentPin> = Vec::new();
        let mut points: Vec<Point> = Vec::new();
        // Метки за краем холста не раскладываются и не рисуются (поле — на группу у края).
        let reach = Rect::from_origin_size(Point::ZERO, cam.viewport())
            .inflate(2.0 * PIN_SIZE, 2.0 * PIN_SIZE);
        if c.visible {
            for p in c.pins.values() {
                if p.anchor.page != page {
                    continue;
                }
                let open = c.open.as_ref() == Some(&p.id);
                if p.resolved && !c.show_resolved && !open {
                    continue;
                }
                let world = match &dragging {
                    Some((id, to)) if *id == p.id => *to,
                    _ => pin_world(&self.scene, &p.anchor),
                };
                if open || dragging.as_ref().is_some_and(|(id, _)| *id == p.id) {
                    singles.push((p.id.clone(), world));
                    continue;
                }
                let s = cam.world_to_screen(world);
                if !(s.x.is_finite() && s.y.is_finite() && reach.contains(s)) {
                    continue;
                }
                ids.push(p);
                points.push(s);
            }
        } else if let Some(open) = c.open.as_ref().and_then(|id| c.pins.get(id)) {
            // Метки скрыты (Shift+C), но обсуждение открыто: его метку видно, как в Figma.
            if open.anchor.page == page {
                singles.push((open.id.clone(), pin_world(&self.scene, &open.anchor)));
            }
        }
        let screens: Vec<Point> = points.clone();
        let mut marks: Vec<PinMark> = Vec::new();
        let mut hits: Vec<Hit> = Vec::new();
        let pin_kind = |p: &CommentPin, open: bool| PinKind::Pin {
            initials: p.initials.clone(),
            color: p.color,
            replies: p.replies,
            unread: p.unread,
            resolved: p.resolved,
            open,
        };
        for g in cluster(&screens) {
            if g.len() == 1 {
                let p = ids[g[0]];
                let kind = pin_kind(p, false);
                marks.push(PinMark {
                    at: cam.screen_to_world(screens[g[0]]),
                    kind: kind.clone(),
                });
                hits.push(Hit {
                    at: screens[g[0]],
                    kind,
                    target: Target::Pin(p.id.clone()),
                });
                continue;
            }
            let n = g.len() as f64;
            let center = g
                .iter()
                .fold(Point::ZERO, |a, i| a + screens[*i].to_vec2() / n);
            // Группа — кругом над серединой капель её меток.
            let center = center + kurbo::Vec2::new(PIN_SIZE / 2.0, -PIN_SIZE / 2.0);
            let kind = PinKind::Cluster {
                count: g.len() as u32,
                unread: g.iter().any(|i| ids[*i].unread),
            };
            marks.push(PinMark {
                at: cam.screen_to_world(center),
                kind: kind.clone(),
            });
            hits.push(Hit {
                at: center,
                kind,
                target: Target::Cluster(
                    g.iter().map(|i| cam.screen_to_world(screens[*i])).collect(),
                ),
            });
        }
        let viewport = Rect::from_origin_size(Point::ZERO, cam.viewport());
        let mut anchor = None;
        for (id, world) in singles {
            let p = &c.pins[&id];
            let open = c.open.as_ref() == Some(&id);
            let kind = pin_kind(p, open);
            let s = cam.world_to_screen(world);
            if open {
                anchor = Some((s, viewport.contains(s)));
            }
            marks.push(PinMark {
                at: world,
                kind: kind.clone(),
            });
            hits.push(Hit {
                at: s,
                kind,
                target: Target::Pin(id),
            });
        }
        if let Some(d) = c.draft.filter(|d| d.page == page) {
            let world = pin_world(&self.scene, &d);
            let s = cam.world_to_screen(world);
            anchor = Some((s, viewport.contains(s)));
            marks.push(PinMark {
                at: world,
                kind: PinKind::Draft,
            });
        }
        // Открытое обсуждение на другой странице — не видно.
        if anchor.is_none() && (c.open.is_some() || c.draft.is_some()) {
            anchor = Some((Point::ZERO, false));
        }
        Layout {
            marks: marks.into(),
            hits,
            anchor,
        }
    }

    /// Метки для оверлея.
    pub(crate) fn comment_marks(&self) -> Arc<[PinMark]> {
        self.comment_layout().marks.clone()
    }

    /// Что под экранной точкой: верхняя метка или группа.
    fn comment_hit(&self, p: Point) -> Option<Hit> {
        if !self.comments.available {
            return None;
        }
        let l = self.comment_layout();
        l.hits
            .iter()
            .rev()
            .find(|h| pin_contains(&h.kind, h.at, p))
            .cloned()
    }

    /// Метки перехватывают нажатие раньше выделения на холсте: в режиме комментариев — всегда,
    /// иначе — у «Перемещения» и «Масштаба», когда метки видны **(поведение)**: рисующий
    /// инструмент рисует и поверх метки. Рука (и удержание Space) метки не трогает.
    pub(crate) fn comment_press(&mut self, e: &PointerEvent) -> bool {
        let mode = self.tool.kind() == ToolKind::Comment;
        if !self.comments.available || (!mode && !self.tool.kind().is_select_like()) {
            return false;
        }
        let hit = self.comment_hit(e.point);
        let press = match hit {
            Some(Hit {
                target: Target::Pin(id),
                ..
            }) => {
                let p = &self.comments.pins[&id];
                Press::Pin {
                    from: pin_world(&self.scene, &p.anchor),
                    movable: p.movable,
                    id,
                    start: e.point,
                    grab: self.camera.screen_to_world(e.point),
                    to: None,
                }
            }
            Some(Hit {
                target: Target::Cluster(members),
                ..
            }) => Press::Cluster { members },
            None if mode => Press::Place {
                start: e.point,
                moved: false,
            },
            None => return false,
        };
        self.comments.press = Some(press);
        self.comments.hover = false;
        true
    }

    /// Движение с нажатой меткой: тянется своя (за порогом), остальное ждёт отпускания.
    /// `false` — нажатия на метку нет.
    pub(crate) fn comment_drag(&mut self, e: &PointerEvent) -> bool {
        let world = self.camera.screen_to_world(e.point);
        let Some(press) = &mut self.comments.press else {
            return false;
        };
        let mut moved = false;
        match press {
            Press::Pin {
                start,
                grab,
                from,
                to,
                movable,
                ..
            } => {
                let d = e.point - *start;
                if *movable && (to.is_some() || d.hypot() > DRAG_THRESHOLD_PX) {
                    let next = *from + (world - *grab);
                    if *to != Some(next) {
                        *to = Some(next);
                        moved = true;
                    }
                }
            }
            Press::Place { start, moved: m } => {
                *m |= (e.point - *start).hypot() > DRAG_THRESHOLD_PX;
            }
            Press::Cluster { .. } => {}
        }
        if moved {
            self.comments_bump();
        }
        true
    }

    /// Отпускание: щелчок по метке открывает её, протянутая метка встаёт на место (событие
    /// `CommentMoved`), щелчок по группе вписывает её метки в экран, щелчок по холсту в режиме
    /// комментариев — место нового (`CommentPlace`). `false` — нажатия на метку не было.
    pub(crate) fn comment_release(&mut self, e: &PointerEvent) -> bool {
        let Some(press) = self.comments.press.take() else {
            return false;
        };
        match press {
            Press::Pin { id, to: None, .. } => {
                let world = self
                    .comments
                    .pins
                    .get(&id)
                    .map(|p| pin_world(&self.scene, &p.anchor));
                if let Some(world) = world {
                    self.comments.open = Some(id.clone());
                    self.comments.focus = None;
                    let screen = self.camera.world_to_screen(world);
                    self.out
                        .events
                        .push(EditorEvent::CommentOpened { id, screen });
                }
            }
            Press::Pin {
                id, to: Some(to), ..
            } => {
                let anchor = self.anchor_at(to);
                // Сразу на новом месте, пока сервер не ответил; отказ — хозяин пришлёт метку
                // заново **(поведение)**.
                if let Some(p) = self.comments.pins.get_mut(&id) {
                    p.anchor = anchor;
                }
                self.out
                    .events
                    .push(EditorEvent::CommentMoved { id, anchor });
            }
            Press::Cluster { members, .. } => self.zoom_to_pins(&members),
            Press::Place { moved: false, .. } => {
                let anchor = self.anchor_at(self.camera.screen_to_world(e.point));
                self.out.events.push(EditorEvent::CommentPlace {
                    anchor,
                    screen: e.point,
                });
            }
            Press::Place { moved: true, .. } => {}
        }
        self.comments_bump();
        true
    }

    /// Жест метки отменён (Esc, потеря фокуса): протянутая метка возвращается, события нет.
    pub(crate) fn cancel_comment_press(&mut self) -> bool {
        if self.comments.press.take().is_some() {
            self.comments_bump();
            return true;
        }
        false
    }

    pub(crate) fn comment_pressed(&self) -> bool {
        self.comments.press.is_some()
    }

    /// Курсор у меток: тянется — «взята», над меткой — стрелка.
    pub(crate) fn comment_cursor(&self) -> Option<crate::tools::Cursor> {
        use crate::tools::Cursor;
        match &self.comments.press {
            Some(Press::Pin { to: Some(_), .. }) => Some(Cursor::Grabbing),
            Some(Press::Pin { .. } | Press::Cluster { .. }) => Some(Cursor::Default),
            _ if self.comments.hover => Some(Cursor::Default),
            _ => None,
        }
    }

    /// Наведение на метку (курсор): без нажатия, только где метки ловят нажатие.
    pub(crate) fn comment_hover(&mut self, e: &PointerEvent) {
        let kind = self.tool.kind();
        let catches =
            self.comments.available && (kind == ToolKind::Comment || kind.is_select_like());
        self.comments.hover = catches && self.comment_hit(e.point).is_some();
    }

    /// Привязка точки мира: верхний узел страницы под ней (по тому, что видно) и точка в его
    /// координатах; мимо узлов — мир. Вложенный фрейм — к верхнему **(поведение)**.
    pub(crate) fn anchor_at(&self, world: Point) -> PinAnchor {
        let tol = HIT_TOLERANCE_PX / self.camera.zoom();
        let top = self.scene.hit_test(world, tol).and_then(|id| {
            let mut slot = self.scene.slot(id)?;
            while let Some(p) = self.scene.at(slot).parent {
                slot = p;
            }
            Some(slot)
        });
        if let Some(slot) = top {
            let n = self.scene.at(slot);
            if n.world.determinant().abs() > 1e-12 {
                let local = n.world.inverse() * world;
                if local.x.is_finite() && local.y.is_finite() {
                    return PinAnchor {
                        page: self.page,
                        node: Some(n.id),
                        local,
                        world,
                    };
                }
            }
        }
        PinAnchor {
            page: self.page,
            node: None,
            local: world,
            world,
        }
    }

    /// Щелчок по группе: её метки — в экран с полем, но не меньше чем вдвое ближе: совпавшие
    /// метки иначе не разошлись бы ни от одного щелчка **(поведение)**.
    fn zoom_to_pins(&mut self, members: &[Point]) {
        let Some(b) = members
            .iter()
            .map(|p| Rect::from_points(*p, *p))
            .reduce(|a, b| a.union(b))
        else {
            return;
        };
        let before = self.camera.zoom();
        self.camera.fit(b, PIN_CLUSTER_FIT_PADDING);
        if self.camera.zoom() < before * 2.0 {
            self.camera.set_zoom((before * 2.0).min(MAX_ZOOM));
            self.camera.set_center(b.center());
        }
    }

    /// Состояние меток и точка открытой — интерфейсу, только при изменении (из `emit`).
    pub(crate) fn emit_comments(&mut self) {
        let state = (
            self.tool.kind() == ToolKind::Comment,
            self.comments.visible,
            self.comments.available,
        );
        if self.comments.sent_state != Some(state) {
            self.comments.sent_state = Some(state);
            self.out.events.push(EditorEvent::CommentsState {
                mode: state.0,
                visible: state.1,
                available: state.2,
            });
        }
        let anchor = if self.comments.open.is_some() || self.comments.draft.is_some() {
            self.comment_layout().anchor
        } else {
            None
        };
        if anchor != self.comments.sent_anchor {
            // Открытого не стало — последний раз «не видно» на прежнем месте.
            let (screen, visible) = anchor.unwrap_or((
                self.comments.sent_anchor.map_or(Point::ZERO, |a| a.0),
                false,
            ));
            self.comments.sent_anchor = anchor;
            self.out
                .events
                .push(EditorEvent::CommentAnchor { screen, visible });
        }
    }

    /// Вид меток для ключа кадра: ревизия; сцена и камера — уже в ключе.
    pub(crate) fn comments_revision(&self) -> u64 {
        self.comments.revision
    }

    /// Только для тестов и замеров: экранные точки и виды меток раскладки (группа — с числом).
    #[doc(hidden)]
    pub fn comment_layout_for_tests(&self) -> Vec<(Point, PinKind)> {
        self.comment_layout()
            .hits
            .iter()
            .map(|h| (h.at, h.kind.clone()))
            .collect()
    }
}
