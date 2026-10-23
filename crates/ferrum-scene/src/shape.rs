//! Геометрия узла в его локальных координатах: трансформация, контур заливки,
//! кольца обводок и точная проверка попадания. Рендер и hit-testing берут форму
//! отсюда, поэтому то, что нарисовано, и то, во что попадает клик, совпадает.

use ferrum_doc::{NodeKind, Props, Stroke, StrokeAlign};
use kurbo::{Affine, BezPath, Ellipse, Line, ParamCurveNearest, Point, Rect, RoundedRect, Shape};

/// Точность аппроксимации дуг кривыми Безье в локальных единицах. Контур строится
/// один раз на изменение узла и потом масштабируется камерой до 256×, поэтому
/// допуск берётся с запасом: 0.001 × 256 = 0.26 экранного пикселя.
pub const PATH_TOLERANCE: f64 = 0.001;

/// Предел координат и размеров. Значения из файла не проверяются при записи, а
/// бесконечность или 1e300 сломали бы перевод в `f32` при растеризации.
pub const COORD_LIMIT: f64 = 1.0e9;

/// Локальная трансформация узла: сдвиг к `(x, y)` и поворот вокруг левого верхнего
/// угла, как в Figma. Поворот в градусах; положительный — против часовой стрелки на
/// экране (ось Y направлена вниз, поэтому угол в `kurbo` берётся с минусом).
pub fn local_transform(p: &Props) -> Affine {
    Affine::translate((p.x, p.y)) * Affine::rotate(-p.rotation.to_radians())
}

/// Приводит свойства из файла или кэша к допустимым значениям: нечисловые — к
/// нейтральным, размеры — неотрицательные, прозрачность — в `[0, 1]`.
pub fn sanitize(p: &mut Props) {
    fn finite(v: f64, default: f64) -> f64 {
        if v.is_finite() {
            v
        } else {
            default
        }
    }
    p.x = finite(p.x, 0.0).clamp(-COORD_LIMIT, COORD_LIMIT);
    p.y = finite(p.y, 0.0).clamp(-COORD_LIMIT, COORD_LIMIT);
    p.w = finite(p.w, 0.0).clamp(0.0, COORD_LIMIT);
    p.h = finite(p.h, 0.0).clamp(0.0, COORD_LIMIT);
    p.rotation = finite(p.rotation, 0.0).rem_euclid(360.0);
    p.opacity = finite(p.opacity, 1.0).clamp(0.0, 1.0);
    p.corner_radius = finite(p.corner_radius, 0.0).clamp(0.0, COORD_LIMIT);
    for f in &mut p.fills {
        f.opacity = finite(f.opacity, 1.0).clamp(0.0, 1.0);
    }
    for s in &mut p.strokes {
        s.width = finite(s.width, 0.0).clamp(0.0, COORD_LIMIT);
    }
}

/// Собственная форма узла в локальных координатах.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Geometry {
    /// Группа: своей формы нет, границы — объединение детей.
    None,
    /// Фрейм, прямоугольник, изображение: прямоугольник `(0, 0, w, h)` со скруглением.
    Rect { rect: Rect, radius: f64 },
    /// Эллипс, вписанный в `(0, 0, w, h)`.
    Ellipse(Rect),
    /// Линия от `(0, 0)` до `(w, 0)`.
    Line(f64),
}

pub fn geometry(p: &Props) -> Geometry {
    // Отрицательные размеры и радиус здесь обнуляются, а не передаются дальше: иначе
    // `RoundedRect` получит отрицательный радиус, а `hit` будет считать его нулевым,
    // и рендер с попаданием разойдутся. Общая геометрия нужна, чтобы их не разводить.
    let w = p.w.max(0.0);
    let h = p.h.max(0.0);
    let rect = Rect::new(0.0, 0.0, w, h);
    match p.kind {
        NodeKind::Group => Geometry::None,
        NodeKind::Frame | NodeKind::Rect | NodeKind::Image => Geometry::Rect {
            rect,
            radius: p.corner_radius.max(0.0).min(w / 2.0).min(h / 2.0),
        },
        NodeKind::Ellipse => Geometry::Ellipse(rect),
        NodeKind::Line => Geometry::Line(p.w),
    }
}

impl Geometry {
    /// Границы формы без обводки; `None` у группы.
    pub fn bounds(&self) -> Option<Rect> {
        match *self {
            Geometry::None => None,
            Geometry::Rect { rect, .. } | Geometry::Ellipse(rect) => Some(rect),
            Geometry::Line(len) => Some(Rect::new(0.0, 0.0, len, 0.0)),
        }
    }

    /// Лежит ли точка внутри формы (без обводки и допуска). У линии и группы
    /// внутренности нет.
    pub fn contains(&self, p: Point) -> bool {
        self.offset(0.0).is_some_and(|s| s.contains(p))
    }

    /// Контур заливки; у линии и группы заливки нет.
    pub fn fill_path(&self) -> Option<BezPath> {
        let mut path = BezPath::new();
        self.fill_path_into(&mut path).then_some(path)
    }

    /// Дописывает контур заливки в `out` (рендер переиспользует один буфер на кадр);
    /// `false` — заливки нет.
    pub fn fill_path_into(&self, out: &mut BezPath) -> bool {
        match *self {
            Geometry::Rect { rect, radius } => {
                out.extend(rounded(rect, radius).path_elements(PATH_TOLERANCE));
                true
            }
            Geometry::Ellipse(rect) => {
                out.extend(Ellipse::from_rect(rect).path_elements(PATH_TOLERANCE));
                true
            }
            Geometry::None | Geometry::Line(_) => false,
        }
    }

    /// Форма, сдвинутая наружу на `d` (внутрь при `d < 0`); `None` — схлопнулась.
    /// Для прямоугольника смещение точное (радиус меняется на `d`, острые углы
    /// остаются острыми), для эллипса — эллипс с радиусами `r + d`: это не точная
    /// эквидистанта, но на глаз неотличимо и в рендере, и в попадании.
    fn offset(&self, d: f64) -> Option<Offset> {
        match *self {
            Geometry::Rect { rect, radius } => {
                let r = rect.inflate(d, d);
                (r.width() > 0.0 && r.height() > 0.0).then(|| {
                    let radius = if radius > 0.0 {
                        (radius + d).max(0.0)
                    } else {
                        0.0
                    };
                    Offset::Rect(rounded(r, radius))
                })
            }
            Geometry::Ellipse(rect) => {
                let r = rect.inflate(d, d);
                (r.width() > 0.0 && r.height() > 0.0)
                    .then(|| Offset::Ellipse(Ellipse::from_rect(r)))
            }
            Geometry::None | Geometry::Line(_) => None,
        }
    }

    /// Обводка как заливаемое кольцо (правило even-odd): внешний и внутренний
    /// контуры, сдвинутые по выравниванию обводки. Так обводка рисуется одной
    /// заливкой без расчёта штриха на каждом кадре, а выравнивание inside/outside
    /// не требует слоёв с маской. Линия обводится прямоугольником с плоскими концами.
    pub fn stroke_ring(&self, stroke: &Stroke) -> Option<BezPath> {
        let mut path = BezPath::new();
        self.stroke_ring_into(stroke, &mut path).then_some(path)
    }

    /// Дописывает кольцо обводки в `out`; `false` — обводки нет (нулевая ширина
    /// или форма схлопнулась).
    pub fn stroke_ring_into(&self, stroke: &Stroke, out: &mut BezPath) -> bool {
        let w = stroke.width;
        if w <= 0.0 {
            return false;
        }
        if let Geometry::Line(len) = *self {
            out.extend(Rect::new(0.0, -w / 2.0, len, w / 2.0).path_elements(PATH_TOLERANCE));
            return true;
        }
        let (outer, inner) = ring_offsets(stroke.align, w);
        let Some(outline) = self.offset(outer) else {
            return false;
        };
        outline.extend_path(out);
        if let Some(hole) = self.offset(inner) {
            hole.extend_path(out);
        }
        true
    }

    /// Форма, сжатая внутрь на `d`; `None` — схлопнулась (или это линия, группа).
    pub fn inset_path(&self, d: f64) -> Option<BezPath> {
        let mut out = BezPath::new();
        self.offset(-d)?.extend_path(&mut out);
        Some(out)
    }

    /// На сколько обводки выступают за форму наружу.
    pub fn stroke_outset(&self, strokes: &[Stroke]) -> f64 {
        strokes
            .iter()
            .map(|s| match self {
                Geometry::Line(_) => s.width / 2.0,
                _ => ring_offsets(s.align, s.width).0,
            })
            .fold(0.0, f64::max)
    }
}

/// Смещения внешнего и внутреннего контуров кольца обводки относительно формы.
fn ring_offsets(align: StrokeAlign, w: f64) -> (f64, f64) {
    match align {
        StrokeAlign::Inside => (0.0, -w),
        StrokeAlign::Center => (w / 2.0, -w / 2.0),
        StrokeAlign::Outside => (w, 0.0),
    }
}

fn rounded(rect: Rect, radius: f64) -> RoundedRect {
    RoundedRect::from_rect(
        rect,
        radius.min(rect.width() / 2.0).min(rect.height() / 2.0),
    )
}

enum Offset {
    Rect(RoundedRect),
    Ellipse(Ellipse),
}

impl Offset {
    fn extend_path(&self, out: &mut BezPath) {
        match self {
            Offset::Rect(r) => out.extend(r.path_elements(PATH_TOLERANCE)),
            Offset::Ellipse(e) => out.extend(e.path_elements(PATH_TOLERANCE)),
        }
    }

    fn contains(&self, p: Point) -> bool {
        match self {
            Offset::Rect(r) => r.contains(p),
            Offset::Ellipse(e) => e.contains(p),
        }
    }
}

/// Точное попадание точки `local` (в локальных координатах узла) в узел.
///
/// `tolerance` — допуск в мировых единицах для тонких элементов (4 экранных пикселя,
/// переведённые камерой в мир). Масштаба у узлов нет, поэтому локальные единицы
/// равны мировым. Заливка ловит клик внутри формы; фрейм ловит клик всегда, даже без
/// заливок (его можно выделить за пустую область). Обводка ловит клик в полосе
/// шириной не меньше допуска с каждой стороны от своей средней линии. Группа своей
/// формы не имеет — в неё попадают через детей.
pub fn hit(p: &Props, local: Point, tolerance: f64) -> bool {
    let geom = geometry(p);
    if let Geometry::Line(len) = geom {
        let half = p.strokes.iter().map(|s| s.width / 2.0).fold(0.0, f64::max);
        let seg = Line::new((0.0, 0.0), (len, 0.0));
        let nearest = seg.nearest(local, 1e-9);
        return nearest.distance_sq.sqrt() <= half.max(tolerance);
    }
    // Изображение рисуется на всём прямоугольнике, поэтому попадает в него, как фрейм,
    // даже без заливок: у свежесозданного изображения заливок нет, и без этого клик
    // по нему не попадал бы никуда.
    let filled = matches!(p.kind, NodeKind::Frame | NodeKind::Image)
        || p.fills.iter().any(|f| f.visible);
    if filled && geom.contains(local) {
        return true;
    }
    p.strokes.iter().any(|s| {
        if s.width <= 0.0 {
            return false;
        }
        let (outer, inner) = ring_offsets(s.align, s.width);
        let center = (outer + inner) / 2.0;
        let half = ((outer - inner) / 2.0).max(tolerance);
        let inside_outer = geom
            .offset(center + half)
            .is_some_and(|o| o.contains(local));
        let inside_inner = geom
            .offset(center - half)
            .is_some_and(|i| i.contains(local));
        inside_outer && !inside_inner
    })
}
