//! Геометрия жестов: рамка выделения, ручки, ресайз и поворот, перевод мировых
//! трансформаций обратно в свойства узлов.
//!
//! Модель хранит у узла только сдвиг, поворот вокруг левого верхнего угла и размер —
//! без масштаба и скоса. Поэтому любая правка считается так: новая локальная
//! матрица узла (с масштабом жеста) раскладывается обратно на положение, поворот и
//! размер (`decompose`). Для поворота и неповёрнутого ресайза это точно; ресайз
//! повёрнутого узла внутри общей рамки неравномерно теряет скос (как и в Figma,
//! где такой узел остаётся прямоугольником).

use std::f64::consts::FRAC_PI_2;

use ferrum_doc::{NodeId, NodeKind, Props, PropsPatch};
use ferrum_scene::{shape, Scene, Slot};
use kurbo::{Affine, Point, Rect, Vec2};

use crate::snap::Snap;

/// Положение узла в координатах родителя — то, что хранит модель.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    /// Градусы против часовой стрелки на экране, в `[0, 360)`.
    pub rotation: f64,
    pub w: f64,
    pub h: f64,
}

/// Угол в `[0, 360)`; почти целые значения округляются, чтобы поворот на 90° не
/// оставлял 89.99999999999999 в документе.
pub fn normalize_deg(deg: f64) -> f64 {
    // Нечисловой угол не может стать документом: ноль, а не NaN в свойствах.
    if !deg.is_finite() {
        return 0.0;
    }
    let r = deg.rem_euclid(360.0);
    let rounded = r.round();
    let r = if (r - rounded).abs() < 1e-9 {
        rounded
    } else {
        r
    };
    if r >= 360.0 {
        0.0
    } else {
        r
    }
}

/// Конечны ли все коэффициенты матрицы. Нечисловая матрица не даёт положения, поэтому
/// жесты подменяют её тождественной, а не пишут NaN в документ.
fn finite_affine(m: Affine) -> bool {
    m.as_coeffs().iter().all(|v| v.is_finite())
}

fn rect_finite(r: Rect) -> bool {
    [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite())
}

/// Раскладывает локальную матрицу узла (сдвиг, поворот и, возможно, масштаб жеста)
/// на положение и размер узла с исходным размером `w × h`. Без поворота в матрице
/// (оба направления схлопнуты масштабом 0) берётся `fallback_deg`: схлопнутый узел
/// не должен терять свой угол.
pub fn decompose_with(m: Affine, w: f64, h: f64, fallback_deg: f64) -> Pose {
    let c = m.as_coeffs();
    if !c.iter().all(|v| v.is_finite()) {
        return Pose {
            x: 0.0,
            y: 0.0,
            rotation: normalize_deg(fallback_deg),
            w,
            h,
        };
    }
    let u = Vec2::new(c[0], c[1]);
    let v = Vec2::new(c[2], c[3]);
    let (su, sv) = (u.hypot(), v.hypot());
    let angle = if su > 1e-12 {
        u.y.atan2(u.x)
    } else if sv > 1e-12 {
        v.y.atan2(v.x) - FRAC_PI_2
    } else {
        // Угла в матрице нет: возвращаем прежний поворот (с обратным знаком, как в angle).
        -fallback_deg.to_radians()
    };
    Pose {
        x: c[4],
        y: c[5],
        rotation: normalize_deg(-angle.to_degrees()),
        w: w * su,
        h: h * sv,
    }
}

/// `decompose_with` без запасного угла: для схлопнутой матрицы поворот — нулевой.
pub fn decompose(m: Affine, w: f64, h: f64) -> Pose {
    decompose_with(m, w, h, 0.0)
}

/// Как `decompose_with`, но центр узла попадает точно в мировую точку центра `m · (w/2, h/2)`.
/// Угол и размер из `decompose_with` верны, а верхний левый угол — нет: у повёрнутого
/// квадрата 10×10 под 45° при растяжении ×2 по x центр уехал бы с (14.14, 0) на
/// (10.61, 3.54). Линии сюда не относятся: у них точен `decompose_with`.
fn decompose_centred(m: Affine, w: f64, h: f64, fallback_deg: f64) -> Pose {
    let mut pose = decompose_with(m, w, h, fallback_deg);
    if !finite_affine(m) {
        return pose;
    }
    let centre = m * Point::new(w / 2.0, h / 2.0);
    // Смещение центра от верхнего левого угла в новой позе: тот же поворот, что в
    // `shape::local_transform`, применённый к половинам нового размера.
    let off = Affine::rotate(-pose.rotation.to_radians()) * Point::new(pose.w / 2.0, pose.h / 2.0);
    pose.x = centre.x - off.x;
    pose.y = centre.y - off.y;
    pose
}

/// Мировая матрица родителя узла (у корневого — единичная).
pub fn parent_world(scene: &Scene, slot: Slot) -> Affine {
    scene
        .at(slot)
        .parent
        .map_or(Affine::IDENTITY, |p| scene.at(p).world)
}

/// Границы формы узла в мире без обводки (у группы — по видимым детям).
pub fn world_box(scene: &Scene, slot: Slot) -> Option<Rect> {
    let n = scene.at(slot);
    n.local_bounds.map(|b| n.world.transform_rect_bbox(b))
}

/// Узлы выделения без тех, у кого выделен предок: предок двигает их сам.
pub fn top_level(scene: &Scene, ids: &[NodeId]) -> Vec<NodeId> {
    let selected: std::collections::HashSet<NodeId> = ids.iter().copied().collect();
    ids.iter()
        .copied()
        .filter(|id| {
            let Some(slot) = scene.slot(*id) else {
                return false;
            };
            let mut cur = scene.at(slot).parent;
            while let Some(p) = cur {
                if selected.contains(&scene.at(p).id) {
                    return false;
                }
                cur = scene.at(p).parent;
            }
            true
        })
        .collect()
}

/// Рамка выделения: прямоугольник в своих координатах и матрица в мир. У одного
/// узла — его повёрнутая рамка, у нескольких — общая рамка в мире.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionBox {
    pub rect: Rect,
    pub to_world: Affine,
}

impl SelectionBox {
    pub fn of(scene: &Scene, ids: &[NodeId]) -> Option<SelectionBox> {
        let slots: Vec<Slot> = ids
            .iter()
            .filter_map(|id| scene.slot(*id))
            .filter(|s| scene.at(*s).local_bounds.is_some())
            .collect();
        if let [one] = slots[..] {
            let n = scene.at(one);
            return Some(SelectionBox {
                rect: n.local_bounds?,
                to_world: n.world,
            });
        }
        let rect = slots
            .iter()
            .filter_map(|s| world_box(scene, *s))
            .reduce(|a, b| a.union(b))?;
        Some(SelectionBox {
            rect,
            to_world: Affine::IDENTITY,
        })
    }

    pub fn center_world(&self) -> Point {
        self.to_world * self.rect.center()
    }

    /// Ось рамки совпадает с осями мира (нет поворота): тогда к ней применимы
    /// привязки по краям.
    pub fn axis_aligned(&self) -> bool {
        let c = self.to_world.as_coeffs();
        c[1].abs() < 1e-12 && c[2].abs() < 1e-12 && c[0] > 0.0 && c[3] > 0.0
    }

    /// Точка рамки по долям ширины и высоты, в мире.
    pub fn point(&self, fx: f64, fy: f64) -> Point {
        let r = self.rect;
        self.to_world * Point::new(r.x0 + fx * r.width(), r.y0 + fy * r.height())
    }
}

/// Ручка рамки выделения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    /// Концы линии: у выделенной линии вместо восьми ручек две.
    LineStart,
    LineEnd,
}

impl Handle {
    pub const BOX: [Handle; 8] = [
        Handle::TopLeft,
        Handle::Top,
        Handle::TopRight,
        Handle::Right,
        Handle::BottomRight,
        Handle::Bottom,
        Handle::BottomLeft,
        Handle::Left,
    ];

    /// Положение ручки в долях рамки.
    pub fn frac(self) -> (f64, f64) {
        match self {
            Handle::TopLeft | Handle::LineStart => (0.0, 0.0),
            Handle::Top => (0.5, 0.0),
            Handle::TopRight => (1.0, 0.0),
            Handle::Right => (1.0, 0.5),
            Handle::BottomRight => (1.0, 1.0),
            Handle::Bottom => (0.5, 1.0),
            Handle::BottomLeft => (0.0, 1.0),
            Handle::Left => (0.0, 0.5),
            Handle::LineEnd => (1.0, 0.0),
        }
    }

    pub fn is_corner(self) -> bool {
        let (fx, fy) = self.frac();
        fx != 0.5 && fy != 0.5
    }
}

/// Новый прямоугольник рамки при перетаскивании ручки в точку `p` (в координатах
/// рамки). Противоположная сторона стоит на месте; с `from_center` — центр.
/// `keep_aspect` сохраняет пропорции. Перетаскивание за противоположную сторону
/// не отражает фигуру (у модели нет отражения), а меняет сторону рамки.
pub fn resize_rect(
    rect: Rect,
    handle: Handle,
    p: Point,
    keep_aspect: bool,
    from_center: bool,
) -> Rect {
    // Нечисловая точка не двигает рамку: возвращаем её как была.
    if !p.x.is_finite() || !p.y.is_finite() || !rect_finite(rect) {
        return rect;
    }
    let (fx, fy) = handle.frac();
    let c = rect.center();
    // Одна ось: неподвижная точка и новая протяжённость.
    let axis = |f: f64, lo: f64, hi: f64, center: f64, m: f64| -> (f64, f64) {
        if f == 0.5 {
            (lo, hi)
        } else if from_center {
            let half = (m - center).abs();
            (center - half, center + half)
        } else {
            let anchor = if f == 1.0 { lo } else { hi };
            (anchor.min(m), anchor.max(m))
        }
    };
    let (mut x0, mut x1) = axis(fx, rect.x0, rect.x1, c.x, p.x);
    let (mut y0, mut y1) = axis(fy, rect.y0, rect.y1, c.y, p.y);
    let (w0, h0) = (rect.width(), rect.height());
    if keep_aspect && w0 > 0.0 && h0 > 0.0 {
        let (sx, sy) = ((x1 - x0) / w0, (y1 - y0) / h0);
        let s = if fx == 0.5 {
            sy
        } else if fy == 0.5 {
            sx
        } else {
            sx.max(sy)
        };
        let (w, h) = (w0 * s, h0 * s);
        // Сторона, которая двигается, остаётся с той стороны от неподвижной точки,
        // где курсор; ось без ручки растёт симметрично от центра.
        let place = |f: f64, lo: f64, hi: f64, center: f64, m: f64, len: f64| -> (f64, f64) {
            if f == 0.5 || from_center {
                (center - len / 2.0, center + len / 2.0)
            } else {
                let anchor = if f == 1.0 { lo } else { hi };
                if m >= anchor {
                    (anchor, anchor + len)
                } else {
                    (anchor - len, anchor)
                }
            }
        };
        (x0, x1) = place(fx, rect.x0, rect.x1, c.x, p.x, w);
        (y0, y1) = place(fy, rect.y0, rect.y1, c.y, p.y, h);
    }
    Rect::new(x0, y0, x1, y1)
}

/// Матрица, переводящая прямоугольник `from` в `to` (сдвиг и масштаб по осям).
/// Нулевая сторона не масштабируется: вертикальную линию нельзя растянуть вширь.
pub fn rect_map(from: Rect, to: Rect) -> Affine {
    if !rect_finite(from) || !rect_finite(to) {
        return Affine::IDENTITY;
    }
    let sx = if from.width() > 0.0 {
        to.width() / from.width()
    } else {
        1.0
    };
    let sy = if from.height() > 0.0 {
        to.height() / from.height()
    } else {
        1.0
    };
    Affine::translate(to.origin().to_vec2())
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate(-from.origin().to_vec2())
}

/// Исходное состояние узла, который меняет жест: свойства документа на момент
/// начала жеста. Каждое движение считается от него, а не от превью.
#[derive(Debug, Clone)]
pub struct Base {
    pub id: NodeId,
    pub props: Props,
    pub local: Affine,
    /// Индекс родителя в списке баз — у потомков масштабируемой группы.
    pub parent: Option<usize>,
    /// Мировая матрица родителя — у узлов верхнего уровня жеста.
    pub parent_world: Affine,
}

/// Базы для узлов верхнего уровня; с `with_group_children` — и всё поддерево групп:
/// ресайз группы масштабирует её детей (раздел 5.2 спеки).
pub fn bases(scene: &Scene, ids: &[NodeId], with_group_children: bool) -> Vec<Base> {
    let mut out = Vec::new();
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        let pw = parent_world(scene, slot);
        let n = scene.at(slot);
        out.push(Base {
            id: n.id,
            props: n.props.clone(),
            local: n.local,
            parent: None,
            parent_world: pw,
        });
        if !with_group_children || n.props.kind != NodeKind::Group {
            continue;
        }
        // Потомки групп (не фреймов: дети фрейма не масштабируются).
        let mut stack = vec![(slot, out.len() - 1)];
        while let Some((s, at)) = stack.pop() {
            for &c in &scene.at(s).children {
                let cn = scene.at(c);
                out.push(Base {
                    id: cn.id,
                    props: cn.props.clone(),
                    local: cn.local,
                    parent: Some(at),
                    parent_world: Affine::IDENTITY,
                });
                if cn.props.kind == NodeKind::Group {
                    stack.push((c, out.len() - 1));
                }
            }
        }
    }
    out
}

/// Какие оси округлять до пикселя. Ось, которую притянул сосед (раздел 7.3 спеки),
/// не округляется: привязка к соседу важнее сетки пикселей.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelSnap {
    pub x: bool,
    pub y: bool,
}

impl PixelSnap {
    /// Пиксельная привязка включена: округляются оси, которые сосед не притянул.
    /// Редактор передаёт сюда результат `Snapper::snap`.
    pub fn unless_snapped(snap: &Snap) -> PixelSnap {
        PixelSnap {
            x: snap.dx.is_none(),
            y: snap.dy.is_none(),
        }
    }
}

impl From<bool> for PixelSnap {
    /// Прежнее `pixel_snap: bool`: обе оси или ни одной.
    fn from(on: bool) -> PixelSnap {
        PixelSnap { x: on, y: on }
    }
}

/// Порог, с которого поворот считается изменившимся. Меньшие отличия — ошибки
/// округления `atan2`, а не правка пользователя.
const ANGLE_EPS: f64 = 1e-9;

fn pose_patch(base: &Props, pose: Pose, size: bool, pixel: PixelSnap) -> PropsPatch {
    let mut pose = pose;
    if size {
        if pixel.x {
            pose.w = pose.w.round();
        }
        if pixel.y {
            pose.h = pose.h.round();
        }
    }
    if pose.rotation % 90.0 == 0.0 {
        if pixel.x {
            pose.x = pose.x.round();
        }
        if pixel.y {
            pose.y = pose.y.round();
        }
    }
    let mut patch = PropsPatch {
        x: Some(pose.x),
        y: Some(pose.y),
        ..Default::default()
    };
    // Поворот по модулю 360: 359.9999 и 0 — один угол, правки не нужно.
    let diff = (pose.rotation - base.rotation).rem_euclid(360.0);
    if diff.min(360.0 - diff) > ANGLE_EPS {
        patch.rotation = Some(pose.rotation);
    }
    if size && base.kind != NodeKind::Group {
        patch.w = Some(pose.w);
        if base.kind != NodeKind::Line {
            patch.h = Some(pose.h);
        }
    }
    patch
}

/// Правки свойств для жёсткого движения (сдвиг и поворот) `t` в мире.
pub fn rigid_patches(bases: &[Base], t: Affine, pixel_snap: bool) -> Vec<(NodeId, PropsPatch)> {
    rigid_patches_snapped(bases, t, PixelSnap::from(pixel_snap))
}

/// Как `rigid_patches`, но округление — по осям (см. `PixelSnap`). Нечисловой `t`
/// заменяется тождественным: узлы остаются как были.
pub fn rigid_patches_snapped(
    bases: &[Base],
    t: Affine,
    pixel: PixelSnap,
) -> Vec<(NodeId, PropsPatch)> {
    let t = if finite_affine(t) {
        t
    } else {
        Affine::IDENTITY
    };
    bases
        .iter()
        .filter(|b| b.parent.is_none())
        .map(|b| {
            let m = b.parent_world.inverse() * t * b.parent_world * b.local;
            let pose = decompose_with(m, b.props.w, b.props.h, b.props.rotation);
            (b.id, pose_patch(&b.props, pose, false, pixel))
        })
        .collect()
}

/// Правки свойств для масштаба `t` в мире: узлы меняют положение и размер, у групп
/// масштабируются дети в координатах группы.
pub fn scale_patches(bases: &[Base], t: Affine, pixel_snap: bool) -> Vec<(NodeId, PropsPatch)> {
    scale_patches_snapped(bases, t, PixelSnap::from(pixel_snap))
}

/// Как `scale_patches`, но округление — по осям (см. `PixelSnap`). Ширина
/// округляется вместе с осью x, высота — с осью y. Нечисловой `t` заменяется тождественным.
pub fn scale_patches_snapped(
    bases: &[Base],
    t: Affine,
    pixel: PixelSnap,
) -> Vec<(NodeId, PropsPatch)> {
    let t = if finite_affine(t) {
        t
    } else {
        Affine::IDENTITY
    };
    // Масштаб в координатах родителя каждого узла; для детей группы — в
    // координатах группы, посчитанный при обработке самой группы.
    let mut in_parent: Vec<Affine> = Vec::with_capacity(bases.len());
    let mut out = Vec::with_capacity(bases.len());
    for b in bases {
        let tp = match b.parent {
            None => b.parent_world.inverse() * t * b.parent_world,
            Some(i) => {
                let g = &bases[i];
                g.local.inverse() * in_parent[i] * g.local
            }
        };
        in_parent.push(tp);
        if b.props.kind == NodeKind::Group {
            continue;
        }
        let m = tp * b.local;
        // Линии: у них точен сдвиг по u, а центр лежит на линии и уехать не может.
        let pose = if b.props.kind == NodeKind::Line {
            decompose_with(m, b.props.w, b.props.h, b.props.rotation)
        } else {
            decompose_centred(m, b.props.w, b.props.h, b.props.rotation)
        };
        out.push((b.id, pose_patch(&b.props, pose, true, pixel)));
    }
    out
}

/// Локальная матрица узла по свойствам (с правкой поверх).
pub fn local_with(props: &Props, patch: &PropsPatch) -> Affine {
    let mut p = props.clone();
    patch.apply_to(&mut p);
    shape::local_transform(&p)
}

/// Округление угла до шага `step` градусов.
pub fn snap_angle(deg: f64, step: f64) -> f64 {
    // Нечисловой угол или шаг округлять нечего: возвращаем угол как был.
    if !deg.is_finite() || !step.is_finite() || step <= 0.0 {
        return deg;
    }
    (deg / step).round() * step
}
