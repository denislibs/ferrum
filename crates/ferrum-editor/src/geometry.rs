//! Геометрия жестов: рамка выделения, ручки, ресайз и поворот, перевод мировых
//! трансформаций обратно в свойства узлов.
//!
//! Модель хранит у узла только сдвиг, поворот вокруг левого верхнего угла, размер и
//! отражение по осям узла — без масштаба и скоса. Поэтому любая правка считается
//! так: новая локальная матрица узла (с масштабом жеста) раскладывается обратно на
//! положение, поворот, размер и отражение (`decompose`). Для поворота, отражения и
//! неповёрнутого ресайза это точно; ресайз повёрнутого узла внутри общей рамки
//! неравномерно теряет скос (как и в Figma, где такой узел остаётся прямоугольником).

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
    /// Отражение по осям узла (см. `shape::flip_transform`).
    pub flip_x: bool,
    pub flip_y: bool,
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

/// Разложение без отражения: матрица `m` считается сдвигом, поворотом и
/// масштабом по осям узла. Без поворота в матрице (оба направления схлопнуты
/// масштабом 0) берётся `fallback_deg`: схлопнутый узел не должен терять свой угол.
fn decompose_plain(m: Affine, w: f64, h: f64, fallback_deg: f64) -> Pose {
    let c = m.as_coeffs();
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
        flip_x: false,
        flip_y: false,
    }
}

/// Угловое расстояние в градусах, `[0, 180]`.
fn angle_between(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Раскладывает локальную матрицу узла (сдвиг, поворот, отражение и, возможно,
/// масштаб жеста) на положение, размер и отражение узла с исходным размером `w × h`.
///
/// Локальная матрица — `T(x, y) · R(θ) · S · F(w, h)`: `F` отражает исходную рамку
/// относительно её центра, `S` — масштаб жеста по осям узла. Число отражений
/// задаёт знак определителя (нечётное — отрицательный), но не их оси: отражение по
/// x с поворотом θ — та же матрица, что отражение по y с поворотом θ + 180°, а оба
/// отражения — то же, что поворот на 180°. Поэтому выбор подсказывает `hint` —
/// отражения, которых ждёт жест: если их чётность сходится с определителем, они и
/// берутся; иначе меняется ровно одна ось — та, при которой поворот ближе к
/// `fallback_deg`, при равенстве — x. После выбора `m · F` уже без отражения и
/// раскладывается на сдвиг, поворот и масштаб; `F` переводит рамку саму в себя,
/// поэтому `S · F(w, h) = F(w′, h′) · S`, и узел с новым размером рисуется той же
/// матрицей. Без поворота в матрице берётся `fallback_deg`, нечисловая матрица
/// даёт нулевое положение: NaN не должен стать документом.
pub fn decompose_hinted(m: Affine, w: f64, h: f64, fallback_deg: f64, hint: (bool, bool)) -> Pose {
    if !finite_affine(m) {
        return Pose {
            x: 0.0,
            y: 0.0,
            rotation: normalize_deg(fallback_deg),
            w,
            h,
            flip_x: hint.0,
            flip_y: hint.1,
        };
    }
    let flips = choose_flips(m, w, h, fallback_deg, hint);
    let mut pose = decompose_plain(
        m * shape::flip_transform(w, h, flips.0, flips.1),
        w,
        h,
        fallback_deg,
    );
    (pose.flip_x, pose.flip_y) = flips;
    pose
}

/// Отражения для разложения `m` (см. `decompose_hinted`).
fn choose_flips(m: Affine, w: f64, h: f64, fallback_deg: f64, hint: (bool, bool)) -> (bool, bool) {
    let c = m.as_coeffs();
    let (su, sv) = (Vec2::new(c[0], c[1]).hypot(), Vec2::new(c[2], c[3]).hypot());
    // Схлопнутая ось: знака у определителя нет, отражение остаётся, каким ждали.
    if su <= 1e-12 || sv <= 1e-12 {
        return hint;
    }
    let mirrored = c[0] * c[3] - c[1] * c[2] < 0.0;
    if mirrored == (hint.0 != hint.1) {
        return hint;
    }
    let rotation_with = |f: (bool, bool)| {
        decompose_plain(
            m * shape::flip_transform(w, h, f.0, f.1),
            w,
            h,
            fallback_deg,
        )
        .rotation
    };
    let by_x = (!hint.0, hint.1);
    let by_y = (hint.0, !hint.1);
    // Допуск — чтобы равные расстояния (поворот ровно на 45° от осей) не решались
    // ошибкой округления.
    if angle_between(rotation_with(by_x), fallback_deg)
        <= angle_between(rotation_with(by_y), fallback_deg) + 1e-9
    {
        by_x
    } else {
        by_y
    }
}

/// Как `decompose_hinted` без подсказки отражений: при отрицательном определителе
/// отражается ось, при которой поворот ближе к `fallback_deg`.
pub fn decompose_with(m: Affine, w: f64, h: f64, fallback_deg: f64) -> Pose {
    decompose_hinted(m, w, h, fallback_deg, (false, false))
}

/// `decompose_with` без запасного угла: для схлопнутой матрицы поворот — нулевой.
pub fn decompose(m: Affine, w: f64, h: f64) -> Pose {
    decompose_with(m, w, h, 0.0)
}

/// Разложение мировой матрицы узла в координатах нового родителя — с прежними
/// размером, поворотом и отражением узла как подсказкой. Так узел, перенесённый
/// к другому родителю (в том числе отражённому), остаётся на экране тем же.
pub fn decompose_props(m: Affine, props: &Props) -> Pose {
    decompose_hinted(
        m,
        props.w,
        props.h,
        props.rotation,
        (props.flip_x, props.flip_y),
    )
}

/// Как `decompose_with`, но для произвольного жеста (с неравномерным масштабом и
/// скосом), и центр узла попадает точно в мировую точку `m · (w/2, h/2)`.
///
/// Матрица `m` представляется как поворот (θ — по первому столбцу `u`) и
/// размеры: ширина `w′ = w·|u|` — длина первого столбца, высота `h′ = h·|det| / |u|` —
/// из сохранения площади (QR-разложение: `m = R(θ) · [[w′/w, k], [0, h′/h]]`). Скос
/// `k` отбрасывается; ширина по первой оси и площадь сохраняются. Для осевого
/// масштаба и для равномерного результат точен: `h′ = h·sy` при повороте 0.
///
/// Центр переносится отдельно: верхний левый угол берётся так, чтобы центр лёг в
/// отображённую точку. Иначе у повёрнутого узла центр уезжает: квадрат 10×10 под
/// 45° при растяжении ×2 по x без этого давал бы центр (10.61, 3.54) вместо
/// (14.14, 0). Линии сюда не относятся: у них точен `decompose_with`.
fn decompose_centred(m: Affine, w: f64, h: f64, fallback_deg: f64, hint: (bool, bool)) -> Pose {
    // Отражение `F` переставляет только знаки столбцов и центр рамки оставляет на
    // месте, поэтому длина первого столбца, |det| и центр ниже считаются по `m`.
    let mut pose = decompose_hinted(m, w, h, fallback_deg, hint);
    if !finite_affine(m) {
        return pose;
    }
    let c = m.as_coeffs();
    let su = Vec2::new(c[0], c[1]).hypot();
    // Схлопнутый по первой оси узел: высоту берём из второго столбца (как в
    // `decompose_with`), иначе площадь делит на ноль.
    if su > 1e-12 {
        let det = (c[0] * c[3] - c[1] * c[2]).abs();
        pose.h = h * det / su;
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

    /// Оси рамки совпадают с осями мира (нет поворота): тогда к ней применимы
    /// привязки по краям. Отражённая рамка тоже выровнена — у неё лишь другой знак
    /// на диагонали, края лежат на тех же линиях.
    pub fn axis_aligned(&self) -> bool {
        let c = self.to_world.as_coeffs();
        c[1].abs() < 1e-12 && c[2].abs() < 1e-12 && c[0] != 0.0 && c[3] != 0.0
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
/// `keep_aspect` сохраняет пропорции.
///
/// Результат ориентирован: `(x0, y0)` — куда переходит прежний угол `(x0, y0)`,
/// `(x1, y1)` — куда прежний `(x1, y1)`. Если ручку утащили за противоположную
/// сторону, прямоугольник вывернут (`x1 < x0` или `y1 < y0`), и `rect_map` в него —
/// отражение, как в Figma: сторона, за которую тянут, остаётся под курсором, а
/// фигура отражается. Обратный переход выворачивает её назад.
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
    // Одна ось: новые положения прежних сторон `lo` и `hi`, со знаком.
    let axis = |f: f64, lo: f64, hi: f64, center: f64, m: f64| -> (f64, f64) {
        if f == 0.5 {
            (lo, hi)
        } else if from_center {
            // Половина со знаком: курсор за центром выворачивает обе стороны.
            let half = if f == 1.0 { m - center } else { center - m };
            (center - half, center + half)
        } else if f == 1.0 {
            (lo, m)
        } else {
            (m, hi)
        }
    };
    let (mut x0, mut x1) = axis(fx, rect.x0, rect.x1, c.x, p.x);
    let (mut y0, mut y1) = axis(fy, rect.y0, rect.y1, c.y, p.y);
    let (w0, h0) = (rect.width(), rect.height());
    if keep_aspect && w0 > 0.0 && h0 > 0.0 {
        // Масштабы со знаком: знак — вывернута ли ось.
        let (sx, sy) = ((x1 - x0) / w0, (y1 - y0) / h0);
        let s = if fx == 0.5 {
            sy.abs()
        } else if fy == 0.5 {
            sx.abs()
        } else {
            sx.abs().max(sy.abs())
        };
        // Ось без ручки не отражается: она растёт симметрично от центра.
        let sign = |f: f64, v: f64| if f != 0.5 && v < 0.0 { -1.0 } else { 1.0 };
        let (w, h) = (w0 * s * sign(fx, sx), h0 * s * sign(fy, sy));
        let place = |f: f64, lo: f64, hi: f64, center: f64, len: f64| -> (f64, f64) {
            if f == 0.5 || from_center {
                (center - len / 2.0, center + len / 2.0)
            } else if f == 1.0 {
                (lo, lo + len)
            } else {
                (hi - len, hi)
            }
        };
        (x0, x1) = place(fx, rect.x0, rect.x1, c.x, w);
        (y0, y1) = place(fy, rect.y0, rect.y1, c.y, h);
    }
    Rect::new(x0, y0, x1, y1)
}

/// Матрица, переводящая прямоугольник `from` в `to` (сдвиг и масштаб по осям).
/// Нулевая сторона не масштабируется: вертикальную линию нельзя растянуть вширь.
/// Ресайз фигуры до нулевой стороны не доводит (`MIN_SIDE`), поэтому нулевая
/// сторона рамки — только у линий.
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
        if !with_group_children || !n.props.kind.is_group_like() {
            continue;
        }
        // Потомки групп и булевых групп (не фреймов: дети фрейма не
        // масштабируются).
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
                if cn.props.kind.is_group_like() {
                    stack.push((c, out.len() - 1));
                }
            }
        }
    }
    out
}

/// Базы для инструмента «Масштаб» (K): узлы верхнего уровня и всё их поддерево —
/// у групп, булевых групп и фреймов (масштаб меняет и содержимое фреймов, в
/// отличие от ресайза). Обход без рекурсии: глубина дерева из файла не ограничена.
pub fn bases_deep(scene: &Scene, ids: &[NodeId]) -> Vec<Base> {
    let mut out = Vec::new();
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        let n = scene.at(slot);
        out.push(Base {
            id: n.id,
            props: n.props.clone(),
            local: n.local,
            parent: None,
            parent_world: parent_world(scene, slot),
        });
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
                stack.push((c, out.len() - 1));
            }
        }
    }
    out
}

/// Правки положения и размера для равномерного масштаба `t` в мире (инструмент
/// «Масштаб»): как `scale_patches_in`, но поддерево фреймов тоже масштабируется —
/// ребёнок фрейма получает масштаб в осях фрейма после его правки. Без привязки к
/// пикселям: округление нарушило бы пропорции содержимого. `bases` — из
/// `bases_deep` (родитель всегда раньше ребёнка).
pub fn scale_patches_deep(bases: &[Base], t: Affine, frame: Affine) -> Vec<(NodeId, PropsPatch)> {
    let t = if finite_affine(t) {
        t
    } else {
        Affine::IDENTITY
    };
    let axes = mirror_axes(t, frame);
    let off = PixelSnap { x: false, y: false };
    // Масштаб в координатах родителя каждого узла и его новая локальная матрица.
    let mut in_parent: Vec<Affine> = Vec::with_capacity(bases.len());
    let mut new_local: Vec<Affine> = Vec::with_capacity(bases.len());
    let mut out = Vec::with_capacity(bases.len());
    for b in bases {
        let tp = match b.parent {
            None => b.parent_world.inverse() * t * b.parent_world,
            // Мир ребёнка после правки — новая матрица родителя × новая своя; она
            // должна совпасть с масштабом родителя поверх прежних матриц.
            Some(i) => new_local[i].inverse() * in_parent[i] * bases[i].local,
        };
        in_parent.push(tp);
        if b.props.kind.is_group_like() {
            new_local.push(b.local);
            continue;
        }
        let m = tp * b.local;
        let hint = match axes {
            Some((ax, ay)) => (b.props.flip_x ^ ax, b.props.flip_y ^ ay),
            None => mirror_hint(&b.props, b.local, m),
        };
        let pose = if b.props.kind == NodeKind::Line {
            decompose_hinted(m, b.props.w, b.props.h, b.props.rotation, hint)
        } else {
            decompose_centred(m, b.props.w, b.props.h, b.props.rotation, hint)
        };
        let patch = pose_patch(&b.props, pose, true, off);
        new_local.push(local_with(&b.props, &patch));
        out.push((b.id, patch));
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
/// округления `atan2`, а не правка пользователя. Тот же допуск решает, кратен ли
/// поворот 90° (`right_angle`, `quarter_turn`): иначе узел мог бы считаться
/// повёрнутым на четверть для размеров и не кратным 90° для положения.
const ANGLE_EPS: f64 = 1e-9;

/// Отличается ли `rotation` от ближайшего кратного `step` градусам меньше `ANGLE_EPS`.
fn multiple_of(rotation: f64, step: f64) -> bool {
    let r = rotation.rem_euclid(step);
    r < ANGLE_EPS || step - r < ANGLE_EPS
}

/// Поворот кратен 90°: стороны рамки идут вдоль осей родителя, положение можно
/// округлять до пикселя.
fn right_angle(rotation: f64) -> bool {
    multiple_of(rotation, 90.0)
}

/// Поворот на четверть (90° или 270°): ширина идёт вдоль оси y родителя, высота — вдоль x.
fn quarter_turn(rotation: f64) -> bool {
    right_angle(rotation) && !multiple_of(rotation, 180.0)
}

/// Наименьшая сторона узла после ресайза, как 0.01 в Figma. Ручку можно довести
/// ровно до противоположной стороны, но сторона не схлопывается в ноль: размер
/// после жеста — исходный, умноженный на масштаб, и у нулевой стороны любой
/// следующий масштаб снова дал бы ноль — узел уже нельзя было бы растянуть.
pub const MIN_SIDE: f64 = 0.01;

fn pose_patch(base: &Props, pose: Pose, size: bool, pixel: PixelSnap) -> PropsPatch {
    let mut pose = pose;
    if size {
        // Ширина округляется по той оси, вдоль которой она лежит в родителе: при
        // повороте на четверть это y, иначе край, притянутый соседом по x, сдвинулся бы
        // на высоту, округлённую по y.
        let (w_axis, h_axis) = if quarter_turn(pose.rotation) {
            (pixel.y, pixel.x)
        } else {
            (pixel.x, pixel.y)
        };
        if w_axis {
            pose.w = pose.w.round();
        }
        if h_axis {
            pose.h = pose.h.round();
        }
        // После округления: иначе сторона в доли пикселя округлилась бы в ноль.
        // `max` заодно заменяет NaN минимумом.
        pose.w = pose.w.max(MIN_SIDE);
        pose.h = pose.h.max(MIN_SIDE);
    }
    if right_angle(pose.rotation) {
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
    if pose.flip_x != base.flip_x {
        patch.flip_x = Some(pose.flip_x);
    }
    if pose.flip_y != base.flip_y {
        patch.flip_y = Some(pose.flip_y);
    }
    if size && !base.kind.is_group_like() {
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
            // Жёсткое движение не отражает: поворот на 180° остаётся поворотом,
            // а не двумя отражениями.
            let pose = decompose_props(m, &b.props);
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
/// округляется вместе с осью x, высота — с осью y. Нечисловой `t` заменяется
/// тождественным. Отражение жеста считается по осям мира (см. `scale_patches_in`).
pub fn scale_patches_snapped(
    bases: &[Base],
    t: Affine,
    pixel: PixelSnap,
) -> Vec<(NodeId, PropsPatch)> {
    scale_patches_in(bases, t, Affine::IDENTITY, pixel)
}

/// Как `scale_patches_snapped`, но жест задан в рамке `frame` (её матрица в мир):
/// у ресайза одного узла это сам узел, у группы — группа, у нескольких узлов и у
/// команд отражения — мир.
///
/// Отражение — одно правило для любого поворота, как в Figma: если `t` в осях
/// рамки отрицателен по оси x, у каждого узла переключается `flip_x`, по оси y —
/// `flip_y` (по обеим — оба). Поворот получается из разложения: при отражении по
/// оси рамки узел с поворотом θ относительно неё получает −θ. Так у одного узла
/// ручка за противоположной стороной отражает именно эту ось, угол за обеими
/// сторонами — обе без поворота на 180°, а Shift+H дважды возвращает узел как был.
pub fn scale_patches_in(
    bases: &[Base],
    t: Affine,
    frame: Affine,
    pixel: PixelSnap,
) -> Vec<(NodeId, PropsPatch)> {
    let t = if finite_affine(t) {
        t
    } else {
        Affine::IDENTITY
    };
    let axes = mirror_axes(t, frame);
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
        if b.props.kind.is_group_like() {
            continue;
        }
        let m = tp * b.local;
        let hint = match axes {
            Some((ax, ay)) => (b.props.flip_x ^ ax, b.props.flip_y ^ ay),
            None => mirror_hint(&b.props, b.local, m),
        };
        // Линии: у них точен сдвиг по u, а центр лежит на линии и уехать не может.
        let pose = if b.props.kind == NodeKind::Line {
            decompose_hinted(m, b.props.w, b.props.h, b.props.rotation, hint)
        } else {
            decompose_centred(m, b.props.w, b.props.h, b.props.rotation, hint)
        };
        out.push((b.id, pose_patch(&b.props, pose, true, pixel)));
    }
    out
}

/// Оси рамки, по которым `t` отражает: `frame⁻¹ · t · frame` — масштаб по осям
/// рамки (так устроены ресайз и команды отражения). `None` — жест не диагонален в
/// рамке (не из ресайза), отражение тогда ищет `mirror_hint`.
fn mirror_axes(t: Affine, frame: Affine) -> Option<(bool, bool)> {
    if !finite_affine(frame) || frame.determinant().abs() < 1e-24 {
        return None;
    }
    let g = (frame.inverse() * t * frame).as_coeffs();
    if !g.iter().all(|v| v.is_finite()) {
        return None;
    }
    let scale = g[0].abs().max(g[3].abs());
    if g[1].abs() > 1e-9 * scale || g[2].abs() > 1e-9 * scale {
        return None;
    }
    Some((g[0] < 0.0, g[3] < 0.0))
}

/// Запасная подсказка для жеста, не диагонального в своей рамке: жест в осях узла
/// — `local⁻¹ · m`. Если он диагональный, ось с отрицательным масштабом
/// переключает своё отражение; иначе подсказка — прежние отражения, а ось при
/// несовпадении чётности выбирает `decompose_hinted` по ближайшему повороту.
fn mirror_hint(props: &Props, local: Affine, m: Affine) -> (bool, bool) {
    let base = (props.flip_x, props.flip_y);
    let g = (local.inverse() * m).as_coeffs();
    if !g.iter().all(|v| v.is_finite()) {
        return base;
    }
    let scale = g[0].abs().max(g[3].abs());
    if g[1].abs() > 1e-9 * scale || g[2].abs() > 1e-9 * scale {
        return base;
    }
    (base.0 ^ (g[0] < 0.0), base.1 ^ (g[3] < 0.0))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Округление положения (`right_angle`) и размеров по осям (`quarter_turn`)
    /// решают кратность 90° одним допуском: раньше положение требовало точного
    /// `rotation % 90 == 0`, и узел под 90° + 5·10⁻¹⁰ округлял размеры по осям
    /// четверти, а положение — нет.
    #[test]
    fn right_angle_and_quarter_turn_share_one_tolerance() {
        for base in [0.0f64, 90.0, 180.0, 270.0] {
            let quarter = base == 90.0 || base == 270.0;
            for d in [0.0, 5e-10, -5e-10] {
                let r = (base + d).rem_euclid(360.0);
                assert!(right_angle(r), "{r}");
                assert_eq!(quarter_turn(r), quarter, "{r}");
            }
            for d in [2e-9, -2e-9, 0.25, 45.0] {
                let r = (base + d).rem_euclid(360.0);
                assert!(!right_angle(r), "{r}");
                assert!(!quarter_turn(r), "{r}");
            }
        }
    }
}
