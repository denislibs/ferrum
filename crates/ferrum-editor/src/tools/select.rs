//! Инструмент «Перемещение» (V): выделение, перемещение, рамка выделения, ресайз
//! за восемь ручек (у линии — за концы), поворот за зоной угловых ручек.
//!
//! Состояния: `Idle → Pressed → Dragging | Marquee | Resizing | Rotating | LineEnd`.
//! Из `Pressed` жест начинается, когда указатель ушёл на 3 экранных пикселя; иначе
//! отпускание — это клик.

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind, PropsPatch};
use ferrum_render::HANDLE_SIZE;
use ferrum_scene::Draft;
use kurbo::{Affine, Line, Point, Rect, Vec2};

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay, DRAG_THRESHOLD_PX};
use crate::geometry::{
    bases, normalize_deg, rect_map, resize_rect, rigid_patches, scale_patches, snap_angle,
    top_level, world_box, Base, Handle, SelectionBox,
};
use crate::input::{Modifiers, PointerEvent};
use crate::ops::{duplicate, local_delta, reposition};
use crate::selection::{
    deeper, drop_frame_at, frame_label_at, locked, resolve, root_frame_at, top_hit, Pick,
};
use crate::snap::{Snapper, SNAP_THRESHOLD_PX};

/// Двойной клик: не дольше 500 мс и не дальше 4 px от первого.
pub const DOUBLE_CLICK_MS: f64 = 500.0;
pub const DOUBLE_CLICK_PX: f64 = 4.0;
/// Ручка ловится в радиусе чуть больше её половины; край рамки — в 4 px.
const HANDLE_HIT_PX: f64 = HANDLE_SIZE / 2.0 + 2.0;
const EDGE_HIT_PX: f64 = 4.0;
/// Зона поворота — снаружи угловой ручки, до 16 px от угла.
const ROTATE_HIT_PX: f64 = 16.0;
/// Шаг поворота с Shift.
pub const ROTATE_STEP_DEG: f64 = 15.0;

/// Что оказалось под указателем при нажатии.
#[derive(Debug, Clone, PartialEq)]
enum Target {
    Node {
        id: NodeId,
        /// Изменить выделение при клике без перетаскивания.
        on_click: OnClick,
    },
    Handle(Handle),
    Rotate,
    Empty {
        /// Фрейм верхнего уровня, в котором начата рамка (её контекст).
        context: Option<NodeId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum OnClick {
    Nothing,
    /// Shift+клик по выделенному убирает его из выделения.
    Remove(NodeId),
    /// Клик по одному из нескольких выделенных оставляет только его.
    SelectOnly(NodeId),
}

#[derive(Debug, Clone)]
struct Press {
    screen: Point,
    world: Point,
    target: Target,
}

/// Узел, который двигается: исходное положение и перевод мирового сдвига к осям
/// родителя.
#[derive(Debug, Clone)]
struct Moving {
    id: NodeId,
    x: f64,
    y: f64,
    parent_world: Affine,
}

#[derive(Debug, Clone)]
struct Drag {
    start: Point,
    /// Узлы выделения верхнего уровня (без заблокированных).
    originals: Vec<Moving>,
    /// Копии при перетаскивании с Alt — черновики, двигаются вместо оригиналов.
    copies: Option<Vec<Moving>>,
    box0: Rect,
    snapper: Snapper,
    /// Родитель узла, за который взялись: если курсор отпущен над другим фреймом,
    /// узлы переносятся в него.
    grab_parent: Option<NodeId>,
    delta: Vec2,
}

#[derive(Debug, Clone)]
struct Marquee {
    start: Point,
    context: Option<NodeId>,
    /// Выделение до рамки: с Shift рамка добавляет к нему.
    base: Vec<NodeId>,
    rect: Rect,
}

#[derive(Debug, Clone)]
struct Resize {
    handle: Handle,
    sbox: SelectionBox,
    bases: Vec<Base>,
    snapper: Snapper,
    transform: Affine,
}

#[derive(Debug, Clone)]
struct Rotate {
    pivot: Point,
    start_angle: f64,
    bases: Vec<Base>,
    /// Поворот единственного узла до жеста: с Shift угол узла кратен 15°.
    single: Option<f64>,
    transform: Affine,
    angle_deg: f64,
}

#[derive(Debug, Clone)]
struct LineEnd {
    handle: Handle,
    id: NodeId,
    parent_world: Affine,
    /// Концы линии в мире до жеста.
    p0: Point,
    p1: Point,
    patch: Option<PropsPatch>,
}

#[derive(Debug, Clone)]
enum State {
    Idle,
    Pressed(Press),
    Dragging(Drag),
    Marquee(Marquee),
    Resizing(Resize),
    Rotating(Rotate),
    LineEnd(LineEnd),
}

pub struct SelectTool {
    state: State,
    cursor: Cursor,
    lines: Vec<Line>,
    last: Option<PointerEvent>,
    /// Время и точка последнего клика — для двойного клика.
    last_click: Option<(f64, Point)>,
}

impl Default for SelectTool {
    fn default() -> Self {
        SelectTool::new()
    }
}

/// Что под точкой на рамке выделения: ручка или зона поворота.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BoxHit {
    Handle(Handle),
    Rotate,
}

/// Угол на экране от центра рамки к точке — для курсора ресайза или поворота.
fn screen_angle(cx: &Ctx, sbox: &SelectionBox, p: Point) -> f64 {
    let c = cx.camera.world_to_screen(sbox.center_world());
    let s = cx.camera.world_to_screen(p);
    let v = s - c;
    if v.hypot() < 1e-9 {
        0.0
    } else {
        normalize_deg(v.y.atan2(v.x).to_degrees())
    }
}

impl SelectTool {
    pub fn new() -> SelectTool {
        SelectTool {
            state: State::Idle,
            cursor: Cursor::Default,
            lines: Vec::new(),
            last: None,
            last_click: None,
        }
    }

    /// Единственная выделенная линия.
    fn single_line(cx: &Ctx) -> Option<NodeId> {
        match cx.selection[..] {
            [id] => cx
                .scene
                .node(id)
                .filter(|n| n.props.kind == NodeKind::Line)
                .map(|_| id),
            _ => None,
        }
    }

    /// Ручка или зона поворота под экранной точкой.
    fn box_hit(cx: &Ctx, screen: Point) -> Option<(BoxHit, f64)> {
        let sel = editable(cx, cx.selection);
        if sel.is_empty() {
            return None;
        }
        let sbox = SelectionBox::of(cx.scene, &sel)?;
        let to_screen = |p: Point| cx.camera.world_to_screen(p);
        if Self::single_line(cx).is_some() {
            for h in [Handle::LineStart, Handle::LineEnd] {
                let (fx, fy) = h.frac();
                let p = sbox.point(fx, fy);
                if (to_screen(p) - screen).hypot() <= HANDLE_HIT_PX {
                    return Some((BoxHit::Handle(h), screen_angle(cx, &sbox, p)));
                }
            }
            return None;
        }
        // Сначала ручки (углы раньше середин сторон), потом стороны, потом поворот.
        let mut handles = Handle::BOX;
        handles.sort_by_key(|h| !h.is_corner());
        for h in handles {
            let (fx, fy) = h.frac();
            let p = sbox.point(fx, fy);
            if (to_screen(p) - screen).hypot() <= HANDLE_HIT_PX {
                return Some((BoxHit::Handle(h), screen_angle(cx, &sbox, p)));
            }
        }
        let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let sides = [Handle::Top, Handle::Right, Handle::Bottom, Handle::Left];
        for (i, h) in sides.iter().enumerate() {
            let a = to_screen(sbox.point(corners[i].0, corners[i].1));
            let b = to_screen(sbox.point(corners[(i + 1) % 4].0, corners[(i + 1) % 4].1));
            let t = ((screen - a).dot(b - a) / (b - a).hypot2().max(1e-12)).clamp(0.0, 1.0);
            if (a.lerp(b, t) - screen).hypot() <= EDGE_HIT_PX {
                let (fx, fy) = h.frac();
                return Some((
                    BoxHit::Handle(*h),
                    screen_angle(cx, &sbox, sbox.point(fx, fy)),
                ));
            }
        }
        let inside = {
            let local = sbox.to_world.inverse() * cx.to_world(screen);
            sbox.rect.contains(local)
        };
        if !inside {
            for (fx, fy) in corners {
                let p = sbox.point(fx, fy);
                if (to_screen(p) - screen).hypot() <= ROTATE_HIT_PX {
                    return Some((BoxHit::Rotate, screen_angle(cx, &sbox, p)));
                }
            }
        }
        None
    }

    /// Узел, который выберет клик в точке, с учётом модификаторов.
    fn pick_at(cx: &Ctx, screen: Point, mods: Modifiers, double: bool) -> Option<NodeId> {
        if let Some(frame) = frame_label_at(cx.scene, cx.camera, screen) {
            return Some(frame);
        }
        let world = cx.to_world(screen);
        let hit = top_hit(cx.scene, world, cx.tolerance())?;
        if double {
            if let Some(d) = deeper(cx.scene, hit, cx.selection) {
                return Some(d);
            }
        }
        let pick = if mods.primary(cx.settings.platform) {
            Pick::Deepest
        } else {
            Pick::Normal
        };
        resolve(cx.scene, hit, cx.selection, pick)
    }

    fn update_hover(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        if let Some((hit, angle)) = Self::box_hit(cx, e.point) {
            *cx.hover = None;
            self.cursor = match hit {
                BoxHit::Handle(_) => Cursor::Resize { angle },
                BoxHit::Rotate => Cursor::Rotate { angle },
            };
            return;
        }
        self.cursor = Cursor::Default;
        *cx.hover = Self::pick_at(cx, e.point, e.modifiers, false);
    }

    fn begin(&mut self, cx: &mut Ctx, press: Press, e: &PointerEvent) {
        let world = press.world;
        self.state = match press.target {
            Target::Node { id, .. } => {
                let ids = editable(cx, &top_level(cx.scene, cx.selection));
                let Some(box0) = union_box(cx, &ids) else {
                    self.state = State::Idle;
                    return;
                };
                let originals = moving(cx, &ids);
                let grab_parent = cx
                    .scene
                    .node(id)
                    .and_then(|n| n.parent)
                    .map(|p| cx.scene.at(p).id);
                State::Dragging(Drag {
                    start: world,
                    snapper: Snapper::new(cx.scene, &ids),
                    originals,
                    copies: None,
                    box0,
                    grab_parent,
                    delta: Vec2::ZERO,
                })
            }
            Target::Empty { context } => {
                let base = if e.modifiers.shift {
                    cx.selection.clone()
                } else {
                    Vec::new()
                };
                State::Marquee(Marquee {
                    start: world,
                    context,
                    base,
                    rect: Rect::from_points(world, world),
                })
            }
            Target::Handle(handle @ (Handle::LineStart | Handle::LineEnd)) => {
                let Some(id) = Self::single_line(cx) else {
                    self.state = State::Idle;
                    return;
                };
                let n = cx.scene.node(id).expect("линия в сцене");
                let parent_world = n.parent.map_or(Affine::IDENTITY, |p| cx.scene.at(p).world);
                State::LineEnd(LineEnd {
                    handle,
                    id,
                    parent_world,
                    p0: n.world * Point::ZERO,
                    p1: n.world * Point::new(n.props.w, 0.0),
                    patch: None,
                })
            }
            Target::Handle(handle) => {
                let ids = editable(cx, &top_level(cx.scene, cx.selection));
                let Some(sbox) = SelectionBox::of(cx.scene, &ids) else {
                    self.state = State::Idle;
                    return;
                };
                State::Resizing(Resize {
                    handle,
                    sbox,
                    bases: bases(cx.scene, &ids, true),
                    snapper: Snapper::new(cx.scene, &ids),
                    transform: Affine::IDENTITY,
                })
            }
            Target::Rotate => {
                let ids = editable(cx, &top_level(cx.scene, cx.selection));
                let Some(sbox) = SelectionBox::of(cx.scene, &ids) else {
                    self.state = State::Idle;
                    return;
                };
                let pivot = sbox.center_world();
                let single = match ids[..] {
                    [id] => cx.scene.node(id).map(|n| n.props.rotation),
                    _ => None,
                };
                State::Rotating(Rotate {
                    pivot,
                    start_angle: (world - pivot).atan2(),
                    bases: bases(cx.scene, &ids, false),
                    single,
                    transform: Affine::IDENTITY,
                    angle_deg: 0.0,
                })
            }
        };
    }

    fn update(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let p = cx.to_world(e.point);
        let mods = e.modifiers;
        let threshold = SNAP_THRESHOLD_PX / cx.camera.zoom();
        let view = cx.view();
        self.lines.clear();
        match &mut self.state {
            State::Idle | State::Pressed(_) => {}
            State::Dragging(d) => {
                // Alt включает и выключает копию прямо во время перетаскивания.
                if mods.alt != d.copies.is_some() {
                    if mods.alt {
                        d.copies = Some(make_copies(cx, &d.originals));
                    } else {
                        d.copies = None;
                        cx.scene.set_drafts(&[]);
                    }
                }
                let mut delta = p - d.start;
                let (mut lock_x, mut lock_y) = (false, false);
                if mods.shift {
                    if delta.x.abs() >= delta.y.abs() {
                        delta.y = 0.0;
                        lock_y = true;
                    } else {
                        delta.x = 0.0;
                        lock_x = true;
                    }
                }
                let moved = d.box0 + delta;
                let (mut snapped_x, mut snapped_y) = (false, false);
                if cx.settings.snapping {
                    let xs = [moved.x0, moved.center().x, moved.x1];
                    let ys = [moved.y0, moved.center().y, moved.y1];
                    let s = d.snapper.snap(
                        cx.scene,
                        if lock_x { &[] } else { &xs },
                        if lock_y { &[] } else { &ys },
                        moved,
                        threshold,
                        view,
                    );
                    if let Some(dx) = s.dx {
                        delta.x += dx;
                        snapped_x = true;
                    }
                    if let Some(dy) = s.dy {
                        delta.y += dy;
                        snapped_y = true;
                    }
                    self.lines = s.lines;
                }
                if cx.settings.pixel_snap {
                    if !snapped_x && !lock_x {
                        delta.x = (d.box0.x0 + delta.x).round() - d.box0.x0;
                    }
                    if !snapped_y && !lock_y {
                        delta.y = (d.box0.y0 + delta.y).round() - d.box0.y0;
                    }
                }
                d.delta = delta;
                let movers = d.copies.as_ref().unwrap_or(&d.originals);
                let patches: Vec<(NodeId, PropsPatch)> = movers
                    .iter()
                    .map(|m| {
                        let dl = local_delta(m.parent_world, delta);
                        (
                            m.id,
                            PropsPatch {
                                x: Some(m.x + dl.x),
                                y: Some(m.y + dl.y),
                                ..Default::default()
                            },
                        )
                    })
                    .collect();
                cx.scene.set_preview(&patches);
            }
            State::Marquee(m) => {
                m.rect = Rect::from_points(m.start, p);
                let found = crate::selection::marquee(cx.scene, m.rect, m.context);
                let mut sel = m.base.clone();
                let have: HashSet<NodeId> = sel.iter().copied().collect();
                sel.extend(found.into_iter().filter(|id| !have.contains(id)));
                *cx.selection = sel;
            }
            State::Resizing(r) => {
                let mut p = p;
                let (fx, fy) = r.handle.frac();
                if cx.settings.snapping && r.sbox.axis_aligned() && !mods.shift {
                    let xs = if fx != 0.5 { vec![p.x] } else { vec![] };
                    let ys = if fy != 0.5 { vec![p.y] } else { vec![] };
                    let s = r.snapper.snap(
                        cx.scene,
                        &xs,
                        &ys,
                        Rect::from_points(p, p),
                        threshold,
                        view,
                    );
                    p.x += s.dx.unwrap_or(0.0);
                    p.y += s.dy.unwrap_or(0.0);
                    self.lines = s.lines;
                }
                let local = r.sbox.to_world.inverse() * p;
                let new = resize_rect(r.sbox.rect, r.handle, local, mods.shift, mods.alt);
                r.transform =
                    r.sbox.to_world * rect_map(r.sbox.rect, new) * r.sbox.to_world.inverse();
                let patches = scale_patches(&r.bases, r.transform, cx.settings.pixel_snap);
                cx.scene.set_preview(&patches);
            }
            State::Rotating(r) => {
                let a = (p - r.pivot).atan2() - r.start_angle;
                // На экране ось Y вниз: поворот по часовой — это минус в модели.
                let mut deg = -a.to_degrees();
                // Нечисловой угол (вырожденная точка) не должен дойти до свойств узла.
                if !deg.is_finite() {
                    return;
                }
                if mods.shift {
                    deg = match r.single {
                        Some(rot0) => snap_angle(rot0 + deg, ROTATE_STEP_DEG) - rot0,
                        None => snap_angle(deg, ROTATE_STEP_DEG),
                    };
                }
                r.angle_deg = deg;
                r.transform = Affine::translate(r.pivot.to_vec2())
                    * Affine::rotate(-deg.to_radians())
                    * Affine::translate(-r.pivot.to_vec2());
                let patches = rigid_patches(&r.bases, r.transform, cx.settings.pixel_snap);
                cx.scene.set_preview(&patches);
                let angle = (p - r.pivot).atan2().to_degrees();
                self.cursor = Cursor::Rotate {
                    angle: normalize_deg(angle),
                };
            }
            State::LineEnd(l) => {
                let mut p = p;
                if cx.settings.pixel_snap {
                    p = Point::new(p.x.round(), p.y.round());
                }
                let (mut a, mut b) = match l.handle {
                    Handle::LineStart => (p, l.p1),
                    _ => (l.p0, p),
                };
                if mods.shift {
                    // Шаг 45° вокруг неподвижного конца.
                    let (anchor, free) = if l.handle == Handle::LineStart {
                        (b, a)
                    } else {
                        (a, b)
                    };
                    let v = free - anchor;
                    let ang = snap_angle(v.atan2().to_degrees(), 45.0).to_radians();
                    let len = v.hypot();
                    let snapped = anchor + Vec2::new(ang.cos(), ang.sin()) * len;
                    if l.handle == Handle::LineStart {
                        a = snapped;
                    } else {
                        b = snapped;
                    }
                }
                let patch = line_patch(l.parent_world, a, b);
                cx.scene.set_preview(&[(l.id, patch.clone())]);
                l.patch = Some(patch);
            }
        }
    }

    fn commit(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let state = std::mem::replace(&mut self.state, State::Idle);
        self.lines.clear();
        match state {
            State::Idle => {}
            State::Pressed(press) => {
                cx.scene.clear_preview();
                if let Target::Node { on_click, .. } = &press.target {
                    match on_click {
                        OnClick::Nothing => {}
                        OnClick::Remove(id) => cx.selection.retain(|s| s != id),
                        OnClick::SelectOnly(id) => *cx.selection = vec![*id],
                    }
                }
                self.last_click = Some((e.time_ms, e.point));
            }
            State::Dragging(d) => commit_drag(cx, d, cx.to_world(e.point)),
            State::Marquee(_) => {}
            State::Resizing(r) => {
                let patches = scale_patches(&r.bases, r.transform, cx.settings.pixel_snap);
                commit_patches(cx, patches);
            }
            State::Rotating(r) => {
                if r.angle_deg == 0.0 {
                    cx.scene.clear_preview();
                    return;
                }
                let patches = rigid_patches(&r.bases, r.transform, cx.settings.pixel_snap);
                commit_patches(cx, patches);
            }
            State::LineEnd(l) => match l.patch {
                Some(patch) => commit_patches(cx, vec![(l.id, patch)]),
                None => cx.scene.clear_preview(),
            },
        }
    }
}

/// Узлы, которые жест может менять: есть в сцене, не черновики, не заблокированы.
fn editable(cx: &Ctx, ids: &[NodeId]) -> Vec<NodeId> {
    ids.iter()
        .copied()
        .filter(|id| {
            cx.scene
                .slot(*id)
                .is_some_and(|s| !cx.scene.is_draft(*id) && !locked(cx.scene, s))
        })
        .collect()
}

fn union_box(cx: &Ctx, ids: &[NodeId]) -> Option<Rect> {
    ids.iter()
        .filter_map(|id| cx.scene.slot(*id))
        .filter_map(|s| world_box(cx.scene, s))
        .reduce(|a, b| a.union(b))
}

fn moving(cx: &Ctx, ids: &[NodeId]) -> Vec<Moving> {
    ids.iter()
        .filter_map(|id| {
            let s = cx.scene.slot(*id)?;
            let n = cx.scene.at(s);
            Some(Moving {
                id: *id,
                x: n.props.x,
                y: n.props.y,
                parent_world: n.parent.map_or(Affine::IDENTITY, |p| cx.scene.at(p).world),
            })
        })
        .collect()
}

/// Черновики-копии поддеревьев для перетаскивания с Alt. Возвращает их корни.
fn make_copies(cx: &mut Ctx, originals: &[Moving]) -> Vec<Moving> {
    let mut drafts = Vec::new();
    let mut roots = Vec::new();
    for m in originals {
        let Some(slot) = cx.scene.slot(m.id) else {
            continue;
        };
        let root = cx.draft_id();
        let parent = cx.scene.at(slot).parent.map(|p| cx.scene.at(p).id);
        drafts.push(Draft {
            id: root,
            parent,
            props: cx.scene.at(slot).props.clone(),
        });
        let mut stack = vec![(slot, root)];
        while let Some((src, dst)) = stack.pop() {
            let children = cx.scene.at(src).children.clone();
            for c in children {
                let id = cx.draft_id();
                drafts.push(Draft {
                    id,
                    parent: Some(dst),
                    props: cx.scene.at(c).props.clone(),
                });
                stack.push((c, id));
            }
        }
        roots.push(Moving {
            id: root,
            ..m.clone()
        });
    }
    cx.scene.set_drafts(&drafts);
    roots
}

fn line_patch(parent_world: Affine, a: Point, b: Point) -> PropsPatch {
    let inv = parent_world.inverse();
    let (a, b) = (inv * a, inv * b);
    let v = b - a;
    PropsPatch {
        x: Some(a.x),
        y: Some(a.y),
        w: Some(v.hypot()),
        rotation: Some(normalize_deg(-v.atan2().to_degrees())),
        ..Default::default()
    }
}

fn commit_patches(cx: &mut Ctx, patches: Vec<(NodeId, PropsPatch)>) {
    let sel = cx.selection.clone();
    cx.commit(|tx, _| {
        for (id, patch) in &patches {
            tx.set_props(*id, patch)?;
        }
        Ok(sel)
    });
}

fn commit_drag(cx: &mut Ctx, d: Drag, pointer: Point) {
    let delta = d.delta;
    if d.copies.is_some() {
        // Копии: одна транзакция создаёт их на новом месте и выделяет.
        let ids: Vec<NodeId> = d.originals.iter().map(|m| m.id).collect();
        cx.commit(|tx, scene| duplicate(tx, scene, &ids, delta));
        return;
    }
    // Узлы на тех же местах — менять нечего, шага в истории нет.
    if delta == Vec2::ZERO {
        cx.scene.clear_preview();
        return;
    }
    cx.scene.clear_preview();
    let ids: HashSet<NodeId> = d.originals.iter().map(|m| m.id).collect();
    let target = drop_frame_at(cx.scene, pointer, cx.tolerance(), &ids);
    let reparent = target != d.grab_parent;
    let sel = cx.selection.clone();
    cx.commit(|tx, scene| {
        let shift = Affine::translate(delta);
        // Переносятся узлы, чей родитель — фрейм или корень: из группы при
        // перетаскивании не выходят (группа растёт вместе с ними, как в Figma).
        let movable = |m: &Moving| {
            let Some(s) = scene.slot(m.id) else {
                return false;
            };
            scene
                .at(s)
                .parent
                .is_none_or(|p| scene.at(p).props.kind == NodeKind::Frame)
        };
        let mut moved: Vec<(Vec<u32>, &Moving)> = Vec::new();
        for m in &d.originals {
            if reparent && movable(m) {
                let s = scene.slot(m.id).expect("проверено выше");
                moved.push((scene.order_key(s), m));
            } else {
                let dl = local_delta(m.parent_world, delta);
                tx.set_props(
                    m.id,
                    &PropsPatch {
                        x: Some(m.x + dl.x),
                        y: Some(m.y + dl.y),
                        ..Default::default()
                    },
                )?;
            }
        }
        if !moved.is_empty() {
            moved.sort_by(|a, b| a.0.cmp(&b.0));
            let target_world = target
                .and_then(|t| scene.node(t))
                .map_or(Affine::IDENTITY, |n| n.world);
            let ids: Vec<NodeId> = moved.iter().map(|(_, m)| m.id).collect();
            tx.move_nodes(&ids, target, usize::MAX)?;
            for (_, m) in moved {
                let world = shift * scene.node(m.id).expect("в сцене").world;
                tx.set_props(m.id, &reposition(target_world, world))?;
            }
        }
        Ok(sel)
    });
}

impl Tool for SelectTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Move
    }

    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        self.last = Some(*e);
        let double = self.last_click.is_some_and(|(t, p)| {
            e.time_ms - t <= DOUBLE_CLICK_MS && (e.point - p).hypot() <= DOUBLE_CLICK_PX
        });
        if double {
            self.last_click = None;
        }
        let world = cx.to_world(e.point);
        let shift = e.modifiers.shift;
        let target = if let Some((hit, _)) = Self::box_hit(cx, e.point) {
            match hit {
                BoxHit::Handle(h) => Target::Handle(h),
                BoxHit::Rotate => Target::Rotate,
            }
        } else if let Some(id) = Self::pick_at(cx, e.point, e.modifiers, double) {
            let selected = cx.selection.contains(&id);
            let on_click = if !selected {
                if shift {
                    cx.selection.push(id);
                } else {
                    *cx.selection = vec![id];
                }
                OnClick::Nothing
            } else if shift {
                OnClick::Remove(id)
            } else if cx.selection.len() > 1 {
                OnClick::SelectOnly(id)
            } else {
                OnClick::Nothing
            };
            Target::Node { id, on_click }
        } else if let Some(first) = inside_selection(cx, world) {
            // Пустое место внутри общей рамки нескольких выделенных: тащим их все.
            Target::Node {
                id: first,
                on_click: OnClick::Nothing,
            }
        } else {
            if !shift {
                cx.selection.clear();
            }
            Target::Empty {
                context: root_frame_at(cx.scene, world, cx.tolerance()),
            }
        };
        *cx.hover = None;
        self.state = State::Pressed(Press {
            screen: e.point,
            world,
            target,
        });
    }

    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        self.last = Some(*e);
        if let State::Pressed(press) = &self.state {
            if (e.point - press.screen).hypot() < DRAG_THRESHOLD_PX {
                return;
            }
            let press = press.clone();
            self.begin(cx, press, e);
        }
        match self.state {
            State::Idle => self.update_hover(cx, e),
            _ => self.update(cx, e),
        }
    }

    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow {
        self.last = Some(*e);
        if !matches!(self.state, State::Idle | State::Pressed(_)) {
            self.update(cx, e);
        }
        self.commit(cx, e);
        self.cursor = Cursor::Default;
        Flow::Stay
    }

    fn key(&mut self, cx: &mut Ctx, modifiers: Modifiers) {
        if let Some(mut last) = self.last {
            last.modifiers = modifiers;
            self.last = Some(last);
            if !matches!(self.state, State::Idle | State::Pressed(_)) {
                self.update(cx, &last);
            }
        }
    }

    fn cancel(&mut self, cx: &mut Ctx) {
        if let State::Marquee(m) = &self.state {
            *cx.selection = m.base.clone();
        }
        self.state = State::Idle;
        self.lines.clear();
        self.cursor = Cursor::Default;
        cx.scene.clear_preview();
    }

    fn is_busy(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    fn cursor(&self) -> Cursor {
        match &self.state {
            State::Dragging(d) if d.copies.is_some() => Cursor::Duplicate,
            _ => self.cursor,
        }
    }

    fn overlay(&self) -> ToolOverlay {
        let mut out = ToolOverlay {
            snap_lines: self.lines.clone(),
            ..Default::default()
        };
        match &self.state {
            State::Marquee(m) => out.marquee = Some(m.rect),
            State::Dragging(d) => {
                if let Some(c) = &d.copies {
                    out.selection = Some(c.iter().map(|m| m.id).collect());
                }
            }
            _ => {}
        }
        out
    }
}

/// Первый выделенный узел, если точка внутри общей рамки нескольких выделенных.
fn inside_selection(cx: &Ctx, world: Point) -> Option<NodeId> {
    if cx.selection.len() < 2 {
        return None;
    }
    let ids = editable(cx, cx.selection);
    let b = union_box(cx, &ids)?;
    b.contains(world).then(|| ids[0])
}
