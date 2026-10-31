//! Правка векторной сети на холсте (спека подсистемы 3, раздел 9): сеанс правки
//! узла (вектора или фигуры, которая станет вектором при первой правке) и перо.
//!
//! Сеанс держит сеть в координатах формы узла (`work`) и матрицу формы → мир
//! (`world`). Пока идёт жест или путь пера, сеть живёт только в превью сцены
//! (у нового вектора — черновиком); запись — одна транзакция: рамка узла и
//! положение пересчитываются по границам сети (`vector_ops::reframe`), поэтому
//! на экране ничего не сдвигается. После записи сеанс перечитывает узел:
//! номера точек при этом не меняются.
//!
//! Перо (P): щелчок — точка, протяжка — ручки (зеркально; Alt — разорвать),
//! щелчок по первой точке пути — замкнуть (петля становится областью), по
//! другой точке той же сети — соединить, Shift — шаг 45°; Esc и Enter —
//! закончить путь. Один путь — один шаг отмены; Ctrl/Cmd+Z во время пути
//! убирает последнюю точку.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use ferrum_doc::{
    DocResult, FillRule, HandleMode, NodeId, NodeKind, Props, PropsPatch, Tx, VectorNetwork,
    VectorPoint, VectorRegion, VectorSegment,
};
use ferrum_render::VectorEditOverlay;
use ferrum_scene::shape::local_transform;
use ferrum_scene::Draft;
use kurbo::{Affine, Line, Point, Rect, Vec2};

use crate::editor::{Editor, EditorEvent};
use crate::geometry::{decompose_with, snap_angle};
use crate::input::{KeyEvent, Modifiers, PointerButton, PointerEvent};
use crate::selection::{drop_frame_at, locked};
use crate::tools::{sync, Cursor, ToolKind, DRAG_THRESHOLD_PX};
use crate::vector_ops::{reframe, shape_network, HandleEnd};

/// Радиус попадания в точку и ручку, экранные пиксели.
pub const POINT_HIT_PX: f64 = 6.0;
/// Допуск попадания в сегмент, экранные пиксели.
pub const SEGMENT_HIT_PX: f64 = 4.0;

/// Инструмент внутри правки вектора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorTool {
    /// Выделение и перемещение точек, ручек и сегментов.
    Select,
    /// Перо: новые точки и пути.
    Pen,
    /// «Заливка области» (B): щелчок по замкнутой области.
    Bucket,
}

/// Выделенное в сети.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VectorSelection {
    pub points: BTreeSet<u32>,
    pub segments: BTreeSet<u32>,
    /// Ручки: сегмент и его конец.
    pub handles: BTreeSet<(u32, HandleEnd)>,
}

impl VectorSelection {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.segments.is_empty() && self.handles.is_empty()
    }

    /// Точки, которые двигает перетаскивание: выделенные и концы выделенных сегментов.
    pub fn moving_points(&self, net: &VectorNetwork) -> Vec<u32> {
        // Точки множества уже по порядку: без сегментов — без пересортировки
        // (все 100 000 точек после Ctrl/Cmd+A — каждый кадр).
        let mut out: Vec<u32> = (self.points.iter().copied())
            .filter(|&i| (i as usize) < net.points.len())
            .collect();
        if !self.segments.is_empty() {
            for &s in &self.segments {
                if let Some(seg) = net.segments.get(s as usize) {
                    out.extend([seg.start, seg.end]);
                }
            }
            out.sort_unstable();
            out.dedup();
        }
        out
    }

    /// Убирает номера за пределами сети (после отмены или чужой правки).
    pub(crate) fn clamp(&mut self, net: &VectorNetwork) {
        let (p, s) = (net.points.len() as u32, net.segments.len() as u32);
        self.points.retain(|&i| i < p);
        self.segments.retain(|&i| i < s);
        self.handles.retain(|&(i, _)| i < s);
    }
}

/// Новый вектор пера, которого ещё нет в документе.
#[derive(Debug, Clone)]
pub(crate) struct NewVector {
    pub parent: Option<NodeId>,
}

/// Путь пера, который сейчас рисуется.
#[derive(Debug, Clone, Default)]
pub(crate) struct PenPath {
    /// Последняя точка пути; `None` — следующий щелчок начинает новый путь.
    pub last: Option<u32>,
    /// Первая точка пути: щелчок по ней замыкает путь.
    pub first: Option<u32>,
    /// Сегменты пути по порядку — петля области при замыкании.
    pub chain: Vec<u32>,
    /// Ручка, вытянутая из последней точки протяжкой: начальная ручка
    /// следующего сегмента.
    pub pending_out: Option<Vec2>,
    /// В пути есть незаписанные точки.
    pub dirty: bool,
}

/// Перетаскивание в правке.
#[derive(Debug, Clone)]
pub(crate) enum Drag {
    /// Точки (и концы выделенных сегментов): `anchor` — точка под курсором в
    /// мире, по ней — привязки.
    Points {
        start_screen: Point,
        start_world: Point,
        anchor: Point,
        origin: VectorNetwork,
        moved: bool,
    },
    /// Ручка сегмента.
    Handle {
        seg: usize,
        end: HandleEnd,
        origin: VectorNetwork,
    },
    /// Изгиб сегмента (Ctrl/Cmd).
    Bend {
        seg: usize,
        t: f64,
        start_world: Point,
        origin: VectorNetwork,
    },
    /// Рамка выделения точек.
    Marquee {
        start_world: Point,
        base: VectorSelection,
        rect: Option<Rect>,
    },
    /// Протяжка пера: ручки только что поставленной точки.
    PenHandle {
        point: u32,
        /// Сегмент, который пришёл в точку (его конечная ручка — зеркальная).
        incoming: Option<usize>,
        /// Сегмент, уходящий из точки: его ручка у точки — исходящая, зеркальная
        /// входящей (замыкание и соединение).
        outgoing: Option<usize>,
        /// Точка может быть зеркальной: у неё не больше двух сегментов.
        mirror: bool,
        origin: VectorNetwork,
        start_screen: Point,
        /// Alt был нажат: зеркальность разорвана, входящая ручка не меняется.
        broken: bool,
    },
}

/// Сеанс правки вектора.
#[derive(Debug, Clone)]
pub(crate) struct VectorEdit {
    /// Узел; `None` — новый вектор пера ещё не в документе (`new`).
    pub id: Option<NodeId>,
    pub new: Option<NewVector>,
    /// Черновик сцены: новый вектор пера или сеть фигуры, которая ещё не стала
    /// вектором (сама фигура на время превью скрыта).
    pub draft: NodeId,
    /// Свойства узла на момент последней записи (у нового — начальные): по ним
    /// `reframe` пересчитывает рамку.
    pub base: Props,
    /// Матрица формы узла → мир.
    pub world: Affine,
    /// Сеть в координатах формы с незаписанными правками жеста или пути.
    /// В `Arc`: оверлей и сравнение кадров делят сеть, копия — только при правке.
    pub work: Arc<VectorNetwork>,
    /// Версия узла в сцене, с которой сеанс сверился.
    pub version: u64,
    pub sel: VectorSelection,
    pub tool: VectorTool,
    /// Сеанс начат инструментом «Перо»: конец пути — конец правки.
    pub from_pen_tool: bool,
    pub pen: PenPath,
    pub drag: Option<Drag>,
    /// Последняя точка указателя в мире (резинка пера).
    pub pointer: Option<Point>,
    /// Модификаторы последнего события (Shift — шаг 45° резинки).
    pub mods: Modifiers,
    pub snap_lines: Vec<Line>,
}

impl VectorEdit {
    pub(crate) fn to_shape(&self, world: Point) -> Point {
        self.world.inverse() * world
    }

    pub(crate) fn to_world(&self, p: Point) -> Point {
        self.world * p
    }

    /// Линейная часть мир → форма (для сдвигов).
    pub(crate) fn vec_to_shape(&self, v: Vec2) -> Vec2 {
        let inv = self.world.inverse();
        (inv * v.to_point()) - (inv * Point::ZERO)
    }
}

pub(crate) fn point_of(net: &VectorNetwork, i: u32) -> Point {
    let p = &net.points[i as usize];
    Point::new(p.x, p.y)
}

/// Точка `p` с шагом 45° от `from` (Shift).
fn snap45(from: Point, p: Point) -> Point {
    let v = p - from;
    let ang = snap_angle(v.atan2().to_degrees(), 45.0).to_radians();
    let len = v.hypot();
    from + Vec2::new(ang.cos(), ang.sin()) * len * (v.atan2() - ang).cos()
}

impl Editor {
    /// Идёт ли правка вектора.
    pub fn is_editing_vector(&self) -> bool {
        self.vector_edit.is_some()
    }

    /// Инструмент правки вектора (`None` — правки нет).
    pub fn vector_tool(&self) -> Option<VectorTool> {
        self.vector_edit.as_ref().map(|v| v.tool)
    }

    /// Выделенное в правке вектора.
    pub fn vector_selection(&self) -> Option<&VectorSelection> {
        self.vector_edit.as_ref().map(|v| &v.sel)
    }

    /// Узел правки (у нового пути пера — `None`, пока он не записан).
    pub fn vector_edit_node(&self) -> Option<NodeId> {
        self.vector_edit.as_ref().and_then(|v| v.id)
    }

    /// Сеть правки в координатах формы — с незаписанным жестом и путём пера.
    pub fn vector_work(&self) -> Option<&VectorNetwork> {
        self.vector_edit.as_ref().map(|v| &*v.work)
    }

    /// Входит в правку вектора или фигуры (прямоугольник, эллипс, линия),
    /// которая станет вектором при первой правке. `false` — не тот узел,
    /// заблокирован или документ только для чтения.
    pub fn start_vector_edit(&mut self, id: NodeId) -> bool {
        self.cancel_gesture();
        self.end_text_edit();
        self.finish_vector_edit();
        if self.doc.is_read_only() {
            return false;
        }
        let Some(slot) = self.scene.slot(id) else {
            return false;
        };
        let n = self.scene.at(slot);
        if self.scene.is_draft(id)
            || locked(&self.scene, slot)
            || !matches!(
                n.props.kind,
                NodeKind::Vector | NodeKind::Rect | NodeKind::Ellipse | NodeKind::Line
            )
        {
            return false;
        }
        let Some(net) = shape_network(&n.props) else {
            return false;
        };
        let (props, world, version) = (n.props.clone(), n.world, n.version);
        let draft = crate::tools::draft_id(&mut self.out.next_draft);
        self.selection.set(vec![id]);
        self.hover = None;
        self.vector_edit = Some(VectorEdit {
            id: Some(id),
            new: None,
            draft,
            base: props,
            world,
            work: Arc::new(net),
            version,
            sel: VectorSelection::default(),
            tool: VectorTool::Select,
            from_pen_tool: false,
            pen: PenPath::default(),
            drag: None,
            pointer: None,
            mods: Modifiers::NONE,
            snap_lines: Vec::new(),
        });
        self.emit();
        true
    }

    /// Инструмент «Перо» вне правки: сеанс без узла, первый щелчок начнёт новый
    /// вектор. В правке — перо внутри неё.
    pub(crate) fn start_pen(&mut self) {
        if let Some(v) = &mut self.vector_edit {
            v.tool = VectorTool::Pen;
            v.drag = None;
            return;
        }
        if self.doc.is_read_only() {
            return;
        }
        let draft = crate::tools::draft_id(&mut self.out.next_draft);
        self.vector_edit = Some(VectorEdit {
            id: None,
            new: None,
            draft,
            base: Props::defaults(NodeKind::Vector),
            world: Affine::IDENTITY,
            work: Arc::default(),
            version: 0,
            sel: VectorSelection::default(),
            tool: VectorTool::Pen,
            from_pen_tool: true,
            pen: PenPath::default(),
            drag: None,
            pointer: None,
            mods: Modifiers::NONE,
            snap_lines: Vec::new(),
        });
    }

    /// Выход из правки (Esc, Enter, другой инструмент): незаконченный путь пера
    /// записывается, узел остаётся выделенным.
    pub fn end_vector_edit(&mut self) {
        let Some(id) = self.finish_vector_edit() else {
            self.emit();
            return;
        };
        if self.scene.slot(id).is_some() {
            self.selection.set(vec![id]);
        }
        self.emit();
    }

    /// Заканчивает сеанс: путь пера записывается, жест отменяется. Возвращает
    /// узел правки (после записи — и новый вектор).
    pub(crate) fn finish_vector_edit(&mut self) -> Option<NodeId> {
        self.vector_edit.as_ref()?;
        if let Some(v) = &mut self.vector_edit {
            v.drag = None;
        }
        self.finish_pen_path();
        let v = self.vector_edit.take()?;
        self.scene.clear_preview();
        v.id
    }

    /// Матрица формы и сеть узла правки — заново из сцены (после записи,
    /// отмены, правки из панели). Узел пропал или стал не тем — правка кончается.
    pub(crate) fn resync_vector_edit(&mut self) {
        let Some(v) = &self.vector_edit else {
            return;
        };
        if v.drag.is_some() || v.pen.dirty {
            return;
        }
        let Some(id) = v.id else {
            return;
        };
        let Some(n) = self.scene.node(id) else {
            self.vector_edit = None;
            return;
        };
        if n.version == v.version && n.world == v.world {
            return;
        }
        let Some(net) = shape_network(&n.props) else {
            self.vector_edit = None;
            return;
        };
        let (props, world, version) = (n.props.clone(), n.world, n.version);
        let v = self.vector_edit.as_mut().expect("проверено выше");
        v.base = props;
        v.world = world;
        v.version = version;
        v.work = Arc::new(net);
        v.sel.clamp(&v.work);
        if v.pen
            .last
            .is_some_and(|l| l as usize >= v.work.points.len())
        {
            v.pen = PenPath::default();
        }
    }

    /// Показывает рабочую сеть: превью узла или черновик нового вектора.
    pub(crate) fn show_vector_work(&mut self) {
        let Some(v) = &self.vector_edit else {
            return;
        };
        let patch = reframe(&v.base, (*v.work).clone());
        match (&v.id, &v.new) {
            (Some(id), _) if v.base.kind == NodeKind::Vector => {
                self.scene.set_preview(&[(*id, patch)]);
            }
            (Some(id), _) => {
                // Фигура ещё не вектор: превью — её сеть черновиком поверх, а
                // сама фигура на время превью скрыта.
                let mut props = v.base.clone();
                as_vector(&mut props);
                patch.apply_to(&mut props);
                let draft = v.draft;
                let parent = self
                    .scene
                    .node(*id)
                    .and_then(|n| n.parent)
                    .map(|s| self.scene.at(s).id);
                self.scene.set_preview(&[(
                    *id,
                    PropsPatch {
                        visible: Some(false),
                        ..Default::default()
                    },
                )]);
                self.scene.set_drafts(&[Draft {
                    id: draft,
                    parent,
                    props,
                }]);
            }
            (None, Some(n)) => {
                let mut props = v.base.clone();
                patch.apply_to(&mut props);
                let draft = Draft {
                    id: v.draft,
                    parent: n.parent,
                    props,
                };
                self.scene.set_drafts(&[draft]);
            }
            (None, None) => {}
        }
    }

    /// Записывает рабочую сеть одной транзакцией. Фигура при этом становится
    /// вектором (новый узел на её месте, та же рамка, имя, заливки и обводки),
    /// новый вектор пера — создаётся. `false` — отказ документа (сообщение ушло).
    pub(crate) fn commit_vector_work(&mut self) -> bool {
        let Some(v) = &self.vector_edit else {
            return false;
        };
        let patch = reframe(&v.base, (*v.work).clone());
        let (id, new, base) = (v.id, v.new.clone(), v.base.clone());
        self.scene.clear_preview();
        let page = self.page;
        let result = self.doc.transact(
            page,
            self.selection.to_vec(),
            |tx| -> DocResult<Vec<NodeId>> {
                match (id, &new) {
                    (Some(id), _) => Ok(vec![convert_to_vector(tx, id, &patch)?]),
                    (None, Some(n)) => {
                        let mut props = base.clone();
                        patch.apply_to(&mut props);
                        Ok(vec![tx.create_with_props(n.parent, None, &props)?])
                    }
                    (None, None) => Ok(vec![]),
                }
            },
        );
        let changed = sync(&mut self.doc, page, &mut self.scene, &mut self.out);
        match result {
            Ok(sel) => {
                if changed {
                    self.out.steps += 1;
                }
                let node = sel.first().copied();
                self.selection.set(sel);
                if let Some(v) = &mut self.vector_edit {
                    v.id = node.or(v.id);
                    v.new = None;
                    v.pen.dirty = false;
                    // Сверка с новой рамкой — заново.
                    v.version = u64::MAX;
                }
                self.resync_vector_edit();
                true
            }
            Err(e) => {
                self.out.events.push(EditorEvent::Error(e.to_string()));
                // Рабочая сеть — снова как в документе.
                if let Some(v) = &mut self.vector_edit {
                    v.pen = PenPath::default();
                    v.version = u64::MAX;
                    if v.id.is_none() {
                        v.work = Arc::default();
                        v.new = None;
                    }
                }
                self.resync_vector_edit();
                false
            }
        }
    }

    /// Точка сети под экранной точкой (верхняя — ближайшая).
    pub(crate) fn vector_point_at(&self, screen: Point) -> Option<u32> {
        let v = self.vector_edit.as_ref()?;
        let to_screen = self.camera.world_to_screen_transform() * v.world;
        let mut best: Option<(u32, f64)> = None;
        for (i, p) in v.work.points.iter().enumerate() {
            let d = (to_screen * Point::new(p.x, p.y) - screen).hypot();
            if d <= POINT_HIT_PX && best.is_none_or(|b| d < b.1) {
                best = Some((i as u32, d));
            }
        }
        best.map(|b| b.0)
    }

    /// Сегмент под экранной точкой и параметр на нём.
    pub(crate) fn vector_segment_at(&self, screen: Point) -> Option<(usize, f64)> {
        let v = self.vector_edit.as_ref()?;
        let world = self.camera.screen_to_world(screen);
        let local = v.to_shape(world);
        // Допуск — в координатах формы (масштаба у узлов нет, только камера).
        let tol = SEGMENT_HIT_PX / self.camera.zoom();
        ferrum_scene::vector::nearest_segment(&v.work, local, tol).map(|(s, t, _)| (s, t))
    }

    /// Нажатие пера.
    pub(crate) fn pen_pointer_down(&mut self, e: &PointerEvent) {
        let world_raw = self.camera.screen_to_world(e.point);
        let hit_point = self.vector_point_at(e.point);
        let hit_segment = self.vector_segment_at(e.point);
        // Новый вектор: родитель — самый глубокий фрейм под точкой, поза — как
        // у фигур (поворот и отражение родителя компенсируются).
        if self
            .vector_edit
            .as_ref()
            .is_some_and(|v| v.id.is_none() && v.new.is_none())
        {
            let mut world = world_raw;
            if self.settings.pixel_snap {
                world = Point::new(world.x.round(), world.y.round());
            }
            let tol = crate::tools::HIT_TOLERANCE_PX / self.camera.zoom();
            let parent = drop_frame_at(&self.scene, world, tol, &HashSet::new());
            let parent_world = parent
                .and_then(|p| self.scene.node(p))
                .map_or(Affine::IDENTITY, |n| n.world);
            let pose = decompose_with(
                parent_world.inverse() * Affine::translate(world.to_vec2()),
                0.0,
                0.0,
                0.0,
            );
            let mut base = Props::defaults(NodeKind::Vector);
            base.x = pose.x;
            base.y = pose.y;
            base.rotation = pose.rotation;
            (base.flip_x, base.flip_y) = (pose.flip_x, pose.flip_y);
            let v = self.vector_edit.as_mut().expect("сеанс пера");
            v.world = parent_world * local_transform(&base);
            v.base = base;
            v.new = Some(NewVector { parent });
        }
        let pixel_snap = self.settings.pixel_snap;
        let v = self.vector_edit.as_mut().expect("сеанс пера");
        v.mods = e.modifiers;
        let mut net = (*v.work).clone();
        let from_last = v.pen.last;
        // Щелчок по точке сети: замкнуть, соединить или продолжить от неё.
        if let Some(p) = hit_point {
            match from_last {
                Some(last) if last == p => {}
                Some(last) => {
                    let seg = net.segments.len() as u32;
                    let mut s = VectorSegment::line(last, p);
                    if let Some(t) = v.pen.pending_out.take() {
                        s.tangent_start = [t.x, t.y];
                    }
                    net.segments.push(s);
                    v.pen.chain.push(seg);
                    let closes = v.pen.first == Some(p);
                    // Второй сегмент точки: у замыкания — первый сегмент пути.
                    let others: Vec<usize> = net
                        .segments
                        .iter()
                        .enumerate()
                        .filter(|(i, s)| *i != seg as usize && (s.start == p || s.end == p))
                        .map(|(i, _)| i)
                        .collect();
                    let outgoing = if others.len() == 1 {
                        others.first().copied()
                    } else {
                        None
                    };
                    let mirror = others.len() <= 1;
                    if closes && v.pen.chain.len() >= 2 {
                        net.regions.push(VectorRegion {
                            rule: FillRule::NonZero,
                            loops: vec![v.pen.chain.clone()],
                            fills: None,
                        });
                    }
                    v.work = Arc::new(net);
                    v.pen.dirty = true;
                    v.sel = VectorSelection {
                        points: BTreeSet::from([p]),
                        ..Default::default()
                    };
                    // Протяжка с замыкающего щелчка тянет его входящую ручку.
                    v.drag = Some(Drag::PenHandle {
                        point: p,
                        incoming: Some(seg as usize),
                        outgoing,
                        mirror,
                        origin: (*v.work).clone(),
                        start_screen: e.point,
                        broken: e.modifiers.alt,
                    });
                    v.pen.last = None;
                    v.pen.first = None;
                    v.pen.chain.clear();
                    self.show_vector_work();
                    return;
                }
                None => {
                    // Продолжить путь от существующей точки.
                    v.pen.last = Some(p);
                    v.pen.first = Some(p);
                    v.pen.chain.clear();
                    v.sel = VectorSelection {
                        points: BTreeSet::from([p]),
                        ..Default::default()
                    };
                    return;
                }
            }
            return;
        }
        // Щелчок пером по сегменту без пути — новая точка на нём.
        if let (None, Some((seg, t))) = (from_last, hit_segment) {
            let (n2, p) = crate::vector_ops::add_point(&net, seg, t);
            v.work = Arc::new(n2);
            v.pen.dirty = true;
            v.sel = VectorSelection {
                points: BTreeSet::from([p]),
                ..Default::default()
            };
            self.show_vector_work();
            return;
        }
        let mut world = world_raw;
        if let (Some(l), true) = (from_last, e.modifiers.shift) {
            world = snap45(v.to_world(point_of(&net, l)), world);
        }
        if pixel_snap {
            world = Point::new(world.x.round(), world.y.round());
        }
        let local = v.to_shape(world);
        let p = net.points.len() as u32;
        net.points.push(VectorPoint::new(local.x, local.y));
        let mut incoming = None;
        if let Some(last) = from_last {
            let mut s = VectorSegment::line(last, p);
            if let Some(t) = v.pen.pending_out.take() {
                s.tangent_start = [t.x, t.y];
            }
            incoming = Some(net.segments.len());
            v.pen.chain.push(net.segments.len() as u32);
            net.segments.push(s);
        } else {
            v.pen.first = Some(p);
            v.pen.chain.clear();
        }
        v.pen.last = Some(p);
        v.pen.pending_out = None;
        v.pen.dirty = true;
        v.work = Arc::new(net);
        v.sel = VectorSelection {
            points: BTreeSet::from([p]),
            ..Default::default()
        };
        v.drag = Some(Drag::PenHandle {
            point: p,
            incoming,
            outgoing: None,
            mirror: true,
            origin: (*v.work).clone(),
            start_screen: e.point,
            broken: e.modifiers.alt,
        });
        self.show_vector_work();
    }

    /// Протяжка пера: ручки точки — к курсору (исходящая) и зеркально
    /// (входящая); Alt разрывает зеркальность.
    pub(crate) fn pen_pointer_move(&mut self, e: &PointerEvent) {
        let world = self.camera.screen_to_world(e.point);
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        v.pointer = Some(world);
        v.mods = e.modifiers;
        let Some(Drag::PenHandle {
            point,
            incoming,
            outgoing,
            mirror,
            origin,
            start_screen,
            broken,
        }) = v.drag.clone()
        else {
            return;
        };
        if (e.point - start_screen).hypot() < DRAG_THRESHOLD_PX {
            return;
        }
        let broken = broken || e.modifiers.alt;
        let anchor = point_of(&origin, point);
        let d = v.to_shape(world) - anchor;
        let mut net = origin.clone();
        net.points[point as usize].handle_mode = if broken || !mirror {
            HandleMode::Free
        } else {
            HandleMode::MirrorAll
        };
        if let Some(seg) = incoming {
            if broken {
                // После Alt входящая ручка остаётся той, что была на момент Alt.
                net.segments[seg] = v.work.segments[seg];
            } else {
                let s = &mut net.segments[seg];
                if s.end == point {
                    s.tangent_end = [-d.x, -d.y];
                } else {
                    s.tangent_start = [-d.x, -d.y];
                }
            }
        }
        // Замыкание и соединение: исходящая ручка — отражение входящей.
        if let Some(o) = outgoing {
            if broken {
                net.segments[o] = v.work.segments[o];
            } else {
                let s = &mut net.segments[o];
                if s.start == point {
                    s.tangent_start = [d.x, d.y];
                } else {
                    s.tangent_end = [d.x, d.y];
                }
            }
        }
        // Замыкающий щелчок не продолжает путь: исходящей ручки нет.
        if v.pen.last == Some(point) {
            v.pen.pending_out = Some(d);
        }
        v.work = Arc::new(net);
        v.drag = Some(Drag::PenHandle {
            point,
            incoming,
            outgoing,
            mirror,
            origin,
            start_screen,
            broken,
        });
        self.show_vector_work();
    }

    /// Отпускание пера: протяжка ручек кончается, путь продолжается.
    pub(crate) fn pen_pointer_up(&mut self) {
        let closed = self.vector_edit.as_ref().is_some_and(|v| {
            v.pen.last.is_none() && v.pen.dirty && matches!(v.drag, Some(Drag::PenHandle { .. }))
        });
        if let Some(v) = self.vector_edit.as_mut() {
            v.drag = None;
        }
        // Замкнули или соединили — путь закончен.
        if closed {
            self.finish_pen_path();
            if self.vector_edit.as_ref().is_some_and(|v| v.from_pen_tool) {
                self.exit_pen_tool();
            }
        }
    }

    /// Записывает незаписанный путь пера одним шагом. Путь из одной точки без
    /// сегментов не записывается (у нового вектора — исчезает).
    pub(crate) fn finish_pen_path(&mut self) {
        let Some(v) = &mut self.vector_edit else {
            return;
        };
        let (last, no_chain) = (v.pen.last, v.pen.chain.is_empty());
        v.pen.last = None;
        v.pen.first = None;
        v.pen.chain.clear();
        v.pen.pending_out = None;
        if !v.pen.dirty {
            return;
        }
        // Одинокая свежая точка без сегментов пропадает, а не пишется.
        if let Some(l) = last.filter(|_| no_chain) {
            let lone = l as usize + 1 == v.work.points.len()
                && !v.work.segments.iter().any(|s| s.start == l || s.end == l);
            if lone {
                Arc::make_mut(&mut v.work).points.pop();
            }
        }
        // Выделение не ссылается на убранные точки.
        v.sel.clamp(&v.work);
        if v.id.is_none() && v.work.segments.is_empty() {
            v.pen.dirty = false;
            v.work = Arc::default();
            v.new = None;
            self.scene.clear_preview();
            return;
        }
        // Сеть не изменилась (одна точка или всё отменено): ни шага, ни превращения.
        let unchanged = shape_network(&v.base).is_some_and(|n| n == *v.work);
        if unchanged {
            v.pen.dirty = false;
            self.scene.clear_preview();
            return;
        }
        self.commit_vector_work();
    }

    /// Перо вне правки закончило путь: «Перемещение», новый вектор выделен.
    pub(crate) fn exit_pen_tool(&mut self) {
        let id = self.finish_vector_edit();
        self.set_tool(ToolKind::Move);
        if let Some(id) = id.filter(|id| self.scene.slot(*id).is_some()) {
            self.selection.set(vec![id]);
        }
    }

    /// Ctrl/Cmd+Z во время пути пера: убрать последнюю точку пути. `false` —
    /// путь пуст, отмена — обычная.
    pub(crate) fn pen_undo_point(&mut self) -> bool {
        let Some(v) = &mut self.vector_edit else {
            return false;
        };
        let Some(last) = v.pen.last.filter(|_| v.pen.dirty) else {
            return false;
        };
        // Новая точка пути — последняя в сети, её сегмент — последний.
        if last as usize + 1 != v.work.points.len() {
            return false;
        }
        let seg = v.pen.chain.pop();
        // Предыдущая точка пути — другой конец убранного сегмента.
        let prev = seg.map(|s| {
            let s = v.work.segments[s as usize];
            if s.end == last {
                s.start
            } else {
                s.end
            }
        });
        if let Some(s) = seg {
            Arc::make_mut(&mut v.work).segments.truncate(s as usize);
        }
        Arc::make_mut(&mut v.work).points.pop();
        v.pen.last = prev;
        v.pen.pending_out = None;
        v.sel = VectorSelection {
            points: prev.into_iter().collect(),
            ..Default::default()
        };
        if prev.is_none() {
            v.pen.first = None;
        }
        if v.id.is_none() && v.work.points.is_empty() {
            v.pen = PenPath::default();
            v.new = None;
            self.scene.clear_preview();
            return true;
        }
        self.show_vector_work();
        true
    }

    /// Нажатие в правке. `false` — нажатие мимо: правка кончилась, его
    /// обработает инструмент.
    pub(crate) fn vector_pointer_down(&mut self, e: &PointerEvent) -> bool {
        // Нажатие без отпускания предыдущего: прежний жест отменяется, а не
        // записывается следующей правкой (превью не остаётся в рабочей сети).
        self.cancel_stale_vector_drag();
        match self.vector_edit.as_ref().map(|v| v.tool) {
            Some(VectorTool::Pen) => {
                self.pen_pointer_down(e);
                true
            }
            Some(VectorTool::Bucket) => {
                // Щелчок по другому узлу — выход из правки, как в выделении.
                if self.vector_press_leaves(e) {
                    return false;
                }
                self.bucket_click(e);
                true
            }
            Some(VectorTool::Select) => self.vector_select_down(e),
            None => false,
        }
    }

    /// Движение указателя в правке (с кнопкой и без).
    pub(crate) fn vector_pointer_move(&mut self, e: &PointerEvent) {
        match self.vector_tool() {
            Some(VectorTool::Pen) => self.pen_pointer_move(e),
            Some(VectorTool::Select) => self.vector_select_move(e),
            _ => {}
        }
    }

    /// Отпускание кнопки в правке.
    pub(crate) fn vector_pointer_up(&mut self, e: &PointerEvent) {
        // Жест кончает только основная кнопка.
        if e.button != PointerButton::Primary {
            return;
        }
        match self.vector_tool() {
            Some(VectorTool::Pen) => self.pen_pointer_up(),
            Some(VectorTool::Select) => self.vector_select_up(e),
            _ => {}
        }
    }

    /// Клавиша в правке. `false` — не её: дальше по раскладке.
    pub(crate) fn vector_key(&mut self, e: &KeyEvent) -> bool {
        let plain = e.modifiers == Modifiers::NONE || e.modifiers == Modifiers::SHIFT;
        // Во время жеста — только модификаторы; Esc отменяет жест.
        if self.vector_edit.as_ref().is_some_and(|v| v.drag.is_some()) {
            if e.code == "Escape" {
                self.cancel_vector_drag();
            }
            return true;
        }
        match e.code.as_str() {
            // Автоповтор Esc и Enter не должен выходить из режима целиком.
            "Escape" | "Enter" | "NumpadEnter" if e.repeat => true,
            "Escape" | "Enter" | "NumpadEnter" if plain => {
                self.vector_escape();
                true
            }
            // Shift+Delete — удаление с разрывом (без Shift — команда раскладки).
            "Delete" | "Backspace" if e.modifiers == Modifiers::SHIFT => {
                self.vector_delete(true);
                true
            }
            // «Заливка области»: B включает и выключает её.
            "KeyB" if e.modifiers == Modifiers::NONE && !e.repeat => {
                // У нового пути пера областей ещё нет.
                if self.vector_edit_node().is_some() {
                    self.finish_pen_path();
                    if let Some(v) = &mut self.vector_edit {
                        v.tool = if v.tool == VectorTool::Bucket {
                            VectorTool::Select
                        } else {
                            VectorTool::Bucket
                        };
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Esc и Enter в правке: путь пера закончить (перо из инструмента — выйти
    /// к «Перемещению»), иначе — выйти из правки.
    pub(crate) fn vector_escape(&mut self) {
        let Some(v) = &self.vector_edit else {
            return;
        };
        if v.from_pen_tool {
            self.finish_pen_path();
            self.exit_pen_tool();
            return;
        }
        if v.tool != VectorTool::Select {
            // Перо и заливка области — к выделению точек (путь пера записан).
            self.finish_pen_path();
            if let Some(v) = &mut self.vector_edit {
                v.tool = VectorTool::Select;
            }
            return;
        }
        self.end_vector_edit();
    }

    /// Жест выделения, оставшийся без отпускания, отменяется (протяжка пера
    /// живёт по своим правилам).
    pub(crate) fn cancel_stale_vector_drag(&mut self) {
        if self.vector_edit.as_ref().is_some_and(|v| {
            v.drag
                .as_ref()
                .is_some_and(|d| !matches!(d, Drag::PenHandle { .. }))
        }) {
            self.cancel_vector_drag();
        }
    }

    /// Отменяет жест правки (Esc при нажатой кнопке, потеря фокуса): рабочая
    /// сеть — снова как до жеста. Протяжка пера оставляет поставленную точку.
    pub(crate) fn cancel_vector_drag(&mut self) {
        let Some(v) = &mut self.vector_edit else {
            return;
        };
        match v.drag.take() {
            Some(Drag::Points { origin, .. })
            | Some(Drag::Handle { origin, .. })
            | Some(Drag::Bend { origin, .. }) => {
                v.work = Arc::new(origin);
                v.snap_lines.clear();
                self.scene.clear_preview();
            }
            Some(Drag::Marquee { base, .. }) => v.sel = base,
            Some(Drag::PenHandle { .. }) | None => {}
        }
    }

    /// Курсор над холстом в правке.
    pub(crate) fn vector_cursor(&self) -> Option<Cursor> {
        let v = self.vector_edit.as_ref()?;
        Some(match v.tool {
            VectorTool::Pen => Cursor::Pen,
            VectorTool::Bucket => Cursor::Bucket,
            VectorTool::Select => Cursor::Default,
        })
    }

    /// Оверлей правки: сеть, точки, ручки выделенного, резинка пера.
    pub(crate) fn vector_overlay(&self) -> Option<VectorEditOverlay> {
        let v = self.vector_edit.as_ref()?;
        if v.work.points.is_empty() {
            return None;
        }
        // Ручки — у выделенных точек, концов выделенных сегментов и соседей
        // выделенных точек (как в Figma).
        // Маской, а не множеством: Ctrl/Cmd+A на сети в 100 000 точек — кадр.
        let mut around = vec![false; v.work.points.len()];
        for i in v.sel.moving_points(&v.work) {
            around[i as usize] = true;
        }
        let mut handles = Vec::new();
        for (i, s) in v.work.segments.iter().enumerate() {
            if around[s.start as usize] && s.tangent_start != [0.0; 2] {
                handles.push((i as u32, true));
            }
            if around[s.end as usize] && s.tangent_end != [0.0; 2] {
                handles.push((i as u32, false));
            }
        }
        let rubber = match (v.tool, v.pen.last, v.pointer, &v.drag) {
            (VectorTool::Pen, Some(last), Some(world), None) => {
                let from = point_of(&v.work, last);
                let mut to = world;
                if v.mods.shift {
                    to = snap45(v.to_world(from), to);
                }
                Some((from, v.to_shape(to)))
            }
            _ => None,
        };
        Some(VectorEditOverlay {
            transform: v.world,
            net: v.work.clone(),
            points: v.sel.points.iter().copied().collect(),
            segments: v.sel.segments.iter().copied().collect(),
            handles,
            selected_handles: v
                .sel
                .handles
                .iter()
                .map(|&(s, e)| (s, e == HandleEnd::Start))
                .collect(),
            rubber,
        })
    }
}

/// Свойства фигуры как свойства вектора той же рамки: тип, сеть, скругление
/// уже в сети.
pub(crate) fn as_vector(p: &mut Props) {
    p.kind = NodeKind::Vector;
    p.corner_radius = 0.0;
    p.clip_content = false;
    p.text = None;
    p.vector = Some(Arc::new(VectorNetwork::default()));
}

/// Превращает фигуру `id` в вектор на месте (тот же узел, ID, позиция среди
/// соседей, имя, заливки, обводки, поза) и применяет правку `patch` с сетью.
/// Для вектора только применяет правку. Годится для «Сплющить».
pub(crate) fn convert_to_vector(tx: &Tx, id: NodeId, patch: &PropsPatch) -> DocResult<NodeId> {
    let net = patch.vector.clone().unwrap_or_default();
    tx.convert_to_vector(id, net)?;
    tx.set_props(id, patch)?;
    Ok(id)
}
