//! Геометрия векторной сети и обводки по контуру (спека подсистемы 3, раздел 9):
//! контуры для заливки областей и для обводки, открытые концы с их окончаниями,
//! границы, попадание. Рендер и hit-testing берут форму отсюда, поэтому то, что
//! нарисовано, и то, во что попадает клик, совпадает.
//!
//! Сеть хранится в своих координатах; рамка узла `w × h` — её границы, поэтому
//! сеть растягивается под рамку (`fit`): ресайз вектора, как в Figma, меняет его
//! геометрию без переписывания точек.
//!
//! Обводка по контуру — у вектора всегда, у линии с окончаниями и у фигур с
//! углами при соединении не «острым» (`uses_path_stroke`): контур обводится
//! `kurbo::stroke` с плоскими концами, а окончания открытых концов дорисовываются
//! фигурами (`cap_shapes`). Стыки в точках, где сходится больше двух сегментов,
//! закрываются кругом (как в Figma, ветви сети не обрываются зарубкой).

use ferrum_doc::{
    Fill, FillRule, NodeKind, Props, Stroke, StrokeAlign, StrokeCap, StrokeJoin, StrokeStyle,
    VectorNetwork, VectorSegment,
};
use kurbo::{
    Affine, BezPath, Cap, Circle, CubicBez, Join, Line, ParamCurveNearest, PathEl, PathSeg, Point,
    Rect, Shape, StrokeOpts, Vec2,
};

use crate::shape::PATH_TOLERANCE;

/// Кривая сегмента в координатах сети.
pub fn segment_curve(net: &VectorNetwork, s: &VectorSegment) -> PathSeg {
    let p = |i: u32| {
        let v = &net.points[i as usize];
        Point::new(v.x, v.y)
    };
    let (a, b) = (p(s.start), p(s.end));
    if s.is_straight() {
        PathSeg::Line(Line::new(a, b))
    } else {
        PathSeg::Cubic(CubicBez::new(
            a,
            a + Vec2::new(s.tangent_start[0], s.tangent_start[1]),
            b + Vec2::new(s.tangent_end[0], s.tangent_end[1]),
            b,
        ))
    }
}

/// Точные границы сети в её координатах: кривые сегментов (с экстремумами) и
/// отдельные точки. `None` — точек нет.
pub fn network_bounds(net: &VectorNetwork) -> Option<Rect> {
    let mut r: Option<Rect> = None;
    let mut add = |b: Rect| r = Some(r.map_or(b, |a| a.union(b)));
    for p in &net.points {
        add(Rect::from_points((p.x, p.y), (p.x, p.y)));
    }
    for s in &net.segments {
        if !s.is_straight() {
            add(segment_curve(net, s).bounding_box());
        }
    }
    r
}

/// Сеть → координаты формы узла: границы сети растягиваются в `(0, 0, w, h)`.
/// Вырожденная по оси сеть (отрезок по горизонтали) по этой оси не растягивается.
pub fn fit(p: &Props) -> Affine {
    let Some(net) = &p.vector else {
        return Affine::IDENTITY;
    };
    let Some(b) = network_bounds(net) else {
        return Affine::IDENTITY;
    };
    // Крошечный размах в большой рамке даёт бесконечный масштаб — тогда 1.0.
    let scale = |frame: f64, extent: f64| {
        let k = frame / extent;
        if extent > 0.0 && k.is_finite() {
            k
        } else {
            1.0
        }
    };
    let (sx, sy) = (scale(p.w, b.width()), scale(p.h, b.height()));
    Affine::scale_non_uniform(sx, sy) * Affine::translate((-b.x0, -b.y0))
}

/// Открытый конец пути: точка, направление наружу и какое окончание у него.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct End {
    pub point: Point,
    /// Единичный вектор от пути наружу.
    pub dir: Vec2,
    pub cap: StrokeCap,
}

/// Область для заливки: контур петель, правило и свои заливки (`None` — узла).
#[derive(Debug, Clone, PartialEq)]
pub struct RegionPath {
    pub path: BezPath,
    pub rule: FillRule,
    pub fills: Option<Vec<Fill>>,
}

/// Контуры формы для обводки по контуру — в координатах формы узла.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathShape {
    /// Цепочки сегментов: через точки с двумя сегментами путь идёт дальше (там —
    /// соединение), цикл таких точек — замкнутый подпуть.
    pub stroke: BezPath,
    pub regions: Vec<RegionPath>,
    /// Концы в точках с одним сегментом.
    pub ends: Vec<End>,
    /// Точки, где сходится больше двух сегментов.
    pub junctions: Vec<Point>,
    /// Путь замкнут (нет концов): обводка внутри и снаружи — по нему.
    pub closed: bool,
}

/// Обводится ли узел по контуру (иначе — кольцами `shape::Geometry`).
pub fn uses_path_stroke(p: &Props) -> bool {
    let s = &p.stroke_style;
    match p.kind {
        NodeKind::Vector => true,
        NodeKind::Line => s.cap_start != StrokeCap::None || s.cap_end != StrokeCap::None,
        // Углы есть только у прямоугольника без скругления.
        NodeKind::Rect | NodeKind::Frame | NodeKind::Image => {
            s.join != StrokeJoin::Miter && p.corner_radius <= 0.0
        }
        _ => false,
    }
}

/// Направление кривой в начале (`at_end = false`) или в конце, единичное; у
/// вырожденной кривой — по хорде.
fn tangent(seg: PathSeg, at_end: bool) -> Vec2 {
    let v = match seg {
        PathSeg::Cubic(c) => {
            let d = if at_end { c.p3 - c.p2 } else { c.p1 - c.p0 };
            if d.hypot2() > 1e-18 {
                d
            } else {
                // Ближней ручки нет — идём к дальней, потом по хорде.
                let far = if at_end { c.p3 - c.p1 } else { c.p2 - c.p0 };
                if far.hypot2() > 1e-18 {
                    far
                } else {
                    c.p3 - c.p0
                }
            }
        }
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.p2 - q.p0,
    };
    let n = v.hypot();
    if n > 0.0 {
        v / n
    } else {
        Vec2::new(1.0, 0.0)
    }
}

fn reversed(seg: PathSeg) -> PathSeg {
    match seg {
        PathSeg::Cubic(c) => PathSeg::Cubic(CubicBez::new(c.p3, c.p2, c.p1, c.p0)),
        PathSeg::Line(l) => PathSeg::Line(Line::new(l.p1, l.p0)),
        PathSeg::Quad(q) => PathSeg::Quad(kurbo::QuadBez::new(q.p2, q.p1, q.p0)),
    }
}

fn push_seg(path: &mut BezPath, seg: PathSeg) {
    match seg {
        PathSeg::Line(l) => path.line_to(l.p1),
        PathSeg::Cubic(c) => path.curve_to(c.p1, c.p2, c.p3),
        PathSeg::Quad(q) => path.quad_to(q.p1, q.p2),
    }
}

/// Контуры сети в координатах формы узла (`fit` применён).
pub fn network_shape(p: &Props) -> PathShape {
    let Some(net) = &p.vector else {
        return PathShape::default();
    };
    let fit = fit(p);
    let curve = |i: usize| fit_seg(fit, segment_curve(net, &net.segments[i]));
    let n = net.points.len();
    let incident = Incidence::new(net);
    let mut used = vec![false; net.segments.len()];
    let mut stroke = BezPath::new();
    // Цепочка от точки `from` по сегменту `first` до точки, где путь ветвится,
    // кончается или замыкается.
    let walk = |from: usize, first: usize, used: &mut Vec<bool>, stroke: &mut BezPath| {
        let mut at = from;
        let mut seg = first;
        let start = fit * point(net, from);
        stroke.move_to(start);
        loop {
            used[seg] = true;
            let s = &net.segments[seg];
            let forward = s.start as usize == at;
            let c = if forward {
                curve(seg)
            } else {
                reversed(curve(seg))
            };
            push_seg(stroke, c);
            at = if forward { s.end } else { s.start } as usize;
            if at == from && incident.of(at).len() == 2 {
                stroke.close_path();
                return;
            }
            if incident.of(at).len() != 2 {
                return;
            }
            match incident
                .of(at)
                .iter()
                .map(|&i| i as usize)
                .find(|&i| !used[i])
            {
                Some(next) => seg = next,
                None => return,
            }
        }
    };
    for v in 0..n {
        let segs = incident.of(v);
        if segs.len() == 2 {
            continue;
        }
        for &s in segs {
            if !used[s as usize] {
                walk(v, s as usize, &mut used, &mut stroke);
            }
        }
    }
    // Остались только циклы из точек с двумя сегментами.
    for s in 0..net.segments.len() {
        if !used[s] {
            walk(net.segments[s].start as usize, s, &mut used, &mut stroke);
        }
    }
    let mut ends = Vec::new();
    let mut junctions = Vec::new();
    for v in 0..n {
        match *incident.of(v) {
            [s] => {
                let s = s as usize;
                let seg = &net.segments[s];
                let is_start = seg.start as usize == v;
                let c = curve(s);
                // Наружу: против хода кривой в её начале, по ходу — в конце.
                let dir = if is_start {
                    -tangent(c, false)
                } else {
                    tangent(c, true)
                };
                ends.push(End {
                    point: fit * point(net, v),
                    dir,
                    cap: if is_start {
                        p.stroke_style.cap_start
                    } else {
                        p.stroke_style.cap_end
                    },
                });
            }
            [_, _, _, ..] => junctions.push(fit * point(net, v)),
            _ => {}
        }
    }
    let regions = net
        .regions
        .iter()
        .map(|r| {
            let mut path = BezPath::new();
            for l in &r.loops {
                let Some(pts) = net.loop_points(l) else {
                    continue;
                };
                path.move_to(fit * point(net, pts[0] as usize));
                for (k, &si) in l.iter().enumerate() {
                    let s = &net.segments[si as usize];
                    let c = curve(si as usize);
                    let forward = s.start == pts[k];
                    push_seg(&mut path, if forward { c } else { reversed(c) });
                }
                path.close_path();
            }
            RegionPath {
                path,
                rule: r.rule,
                fills: r.fills.clone(),
            }
        })
        .collect();
    let closed = ends.is_empty() && !net.segments.is_empty();
    PathShape {
        stroke,
        regions,
        ends,
        junctions,
        closed,
    }
}

/// Сегменты каждой точки одним массивом (смещения по точкам): без вектора на
/// точку — на сети в 100 000 точек это миллисекунды выделений памяти.
struct Incidence {
    offsets: Vec<u32>,
    segs: Vec<u32>,
}

impl Incidence {
    fn new(net: &VectorNetwork) -> Incidence {
        let n = net.points.len();
        let mut offsets = vec![0u32; n + 1];
        for s in &net.segments {
            offsets[s.start as usize + 1] += 1;
            offsets[s.end as usize + 1] += 1;
        }
        for i in 0..n {
            offsets[i + 1] += offsets[i];
        }
        let mut fill = offsets.clone();
        let mut segs = vec![0u32; offsets[n] as usize];
        for (i, s) in net.segments.iter().enumerate() {
            for v in [s.start, s.end] {
                segs[fill[v as usize] as usize] = i as u32;
                fill[v as usize] += 1;
            }
        }
        Incidence { offsets, segs }
    }

    fn of(&self, v: usize) -> &[u32] {
        &self.segs[self.offsets[v] as usize..self.offsets[v + 1] as usize]
    }
}

fn point(net: &VectorNetwork, i: usize) -> Point {
    Point::new(net.points[i].x, net.points[i].y)
}

fn fit_seg(a: Affine, s: PathSeg) -> PathSeg {
    match s {
        PathSeg::Line(l) => PathSeg::Line(Line::new(a * l.p0, a * l.p1)),
        PathSeg::Cubic(c) => PathSeg::Cubic(CubicBez::new(a * c.p0, a * c.p1, a * c.p2, a * c.p3)),
        PathSeg::Quad(q) => PathSeg::Quad(kurbo::QuadBez::new(a * q.p0, a * q.p1, a * q.p2)),
    }
}

/// Контуры узла для обводки по контуру: сеть вектора, отрезок линии, рамка
/// прямоугольника. `None` — узел обводится кольцами.
pub fn path_shape(p: &Props) -> Option<PathShape> {
    if !uses_path_stroke(p) {
        return None;
    }
    let (w, h) = (p.w.max(0.0), p.h.max(0.0));
    Some(match p.kind {
        NodeKind::Vector => network_shape(p),
        NodeKind::Line => {
            let mut stroke = BezPath::new();
            stroke.move_to((0.0, 0.0));
            stroke.line_to((w, 0.0));
            PathShape {
                stroke,
                ends: vec![
                    End {
                        point: Point::ZERO,
                        dir: Vec2::new(-1.0, 0.0),
                        cap: p.stroke_style.cap_start,
                    },
                    End {
                        point: Point::new(w, 0.0),
                        dir: Vec2::new(1.0, 0.0),
                        cap: p.stroke_style.cap_end,
                    },
                ],
                ..Default::default()
            }
        }
        _ => PathShape {
            stroke: Rect::new(0.0, 0.0, w, h).to_path(PATH_TOLERANCE),
            closed: true,
            ..Default::default()
        },
    })
}

/// Длина плеча стрелки и треугольника вдоль пути для толщины `w`.
pub fn arrow_length(w: f64) -> f64 {
    3.0 * w + 6.0
}

/// Радиус круга и ромба на конце для толщины `w`.
pub fn dot_radius(w: f64) -> f64 {
    1.5 * w + 1.5
}

/// Насколько окончание выступает за точку конца (для границ узла).
pub fn cap_extent(cap: StrokeCap, w: f64) -> f64 {
    match cap {
        StrokeCap::None => w / 2.0,
        StrokeCap::Round | StrokeCap::Square => w / 2.0 * std::f64::consts::SQRT_2,
        StrokeCap::ArrowLines | StrokeCap::ArrowTriangle => arrow_length(w) + w,
        StrokeCap::Circle | StrokeCap::Diamond => dot_radius(w),
    }
}

/// Фигуры окончания конца: заливаемые контуры и обводимые линии (стрелка из
/// двух линий — ломаная через конец, обводится той же толщиной с острым стыком).
pub fn cap_shapes(end: &End, w: f64) -> (BezPath, Option<BezPath>) {
    let (p, d) = (end.point, end.dir);
    let nrm = Vec2::new(-d.y, d.x);
    let mut fill = BezPath::new();
    let mut lines = None;
    match end.cap {
        StrokeCap::None => {}
        StrokeCap::Round => fill.extend(Circle::new(p, w / 2.0).path_elements(PATH_TOLERANCE)),
        StrokeCap::Square => {
            let h = w / 2.0;
            fill.move_to(p + nrm * h);
            fill.line_to(p + nrm * h + d * h);
            fill.line_to(p - nrm * h + d * h);
            fill.line_to(p - nrm * h);
            fill.close_path();
        }
        StrokeCap::ArrowLines => {
            let len = arrow_length(w);
            let back = -d * len * (30f64.to_radians().cos());
            let side = nrm * len * (30f64.to_radians().sin());
            let mut l = BezPath::new();
            l.move_to(p + back + side);
            l.line_to(p);
            l.line_to(p + back - side);
            lines = Some(l);
        }
        StrokeCap::ArrowTriangle => {
            // Остриё выступает на толщину: плоский конец пути целиком под треугольником.
            let len = arrow_length(w);
            let tip = p + d * w;
            let base = tip - d * len;
            let half = len * 30f64.to_radians().tan();
            fill.move_to(tip);
            fill.line_to(base + nrm * half);
            fill.line_to(base - nrm * half);
            fill.close_path();
        }
        StrokeCap::Circle => {
            fill.extend(Circle::new(p, dot_radius(w)).path_elements(PATH_TOLERANCE));
        }
        StrokeCap::Diamond => {
            let r = dot_radius(w);
            fill.move_to(p + d * r);
            fill.line_to(p + nrm * r);
            fill.line_to(p - d * r);
            fill.line_to(p - nrm * r);
            fill.close_path();
        }
    }
    (fill, lines)
}

/// Толщина, которой обводится контур: внутри и снаружи замкнутого — двойная с
/// обрезкой (`StrokeClip`), иначе — как есть.
pub fn effective_width(shape: &PathShape, s: &Stroke) -> f64 {
    if shape.closed && s.align != StrokeAlign::Center {
        2.0 * s.width
    } else {
        s.width
    }
}

/// Обрезка обводки замкнутого контура.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeClip {
    None,
    /// Только внутри контура.
    Inside,
    /// Только снаружи контура.
    Outside,
}

pub fn stroke_clip(shape: &PathShape, s: &Stroke) -> StrokeClip {
    match (shape.closed, s.align) {
        (true, StrokeAlign::Inside) => StrokeClip::Inside,
        (true, StrokeAlign::Outside) => StrokeClip::Outside,
        _ => StrokeClip::None,
    }
}

fn kurbo_join(j: StrokeJoin) -> Join {
    match j {
        StrokeJoin::Miter => Join::Miter,
        StrokeJoin::Round => Join::Round,
        StrokeJoin::Bevel => Join::Bevel,
    }
}

/// Контур обводки как заливка (правило ненулевое): сам путь с соединением узла и
/// плоскими концами, круги в точках ветвления и фигуры окончаний открытых концов.
/// Кэшируется рендером по версии узла: на кадр — только заливка.
pub fn stroke_outline(shape: &PathShape, s: &Stroke, style: &StrokeStyle) -> BezPath {
    let w = effective_width(shape, s);
    let mut out = BezPath::new();
    if w <= 0.0 {
        return out;
    }
    let k = kurbo::Stroke::new(w)
        .with_join(kurbo_join(style.join))
        .with_miter_limit(style.miter_limit)
        .with_caps(Cap::Butt);
    out.extend(
        kurbo::stroke(
            shape.stroke.iter(),
            &k,
            &StrokeOpts::default(),
            PATH_TOLERANCE,
        )
        .iter(),
    );
    for j in &shape.junctions {
        out.extend(Circle::new(*j, w / 2.0).path_elements(PATH_TOLERANCE));
    }
    let arms = kurbo::Stroke::new(w)
        .with_join(Join::Miter)
        .with_caps(Cap::Butt);
    for e in &shape.ends {
        let (fill, lines) = cap_shapes(e, w);
        // Фигуры — по часовой, как и контур: при ненулевом правиле не вырезают друг друга.
        if fill.area() < 0.0 {
            out.extend(fill.reverse_subpaths().iter());
        } else {
            out.extend(fill.iter());
        }
        if let Some(l) = lines {
            out.extend(
                kurbo::stroke(l.iter(), &arms, &StrokeOpts::default(), PATH_TOLERANCE).iter(),
            );
        }
    }
    out
}

/// Окончания открытых концов узла и замкнут ли путь — без построения контуров
/// (границы узла пересчитываются на каждую правку и кадр жеста).
fn end_caps(p: &Props) -> (Vec<StrokeCap>, bool) {
    let s = &p.stroke_style;
    match p.kind {
        NodeKind::Vector => {
            let Some(net) = &p.vector else {
                return (vec![], false);
            };
            let deg = net.degrees();
            let (mut start, mut end) = (false, false);
            for seg in &net.segments {
                start |= deg[seg.start as usize] == 1;
                end |= deg[seg.end as usize] == 1;
            }
            let caps = [(start, s.cap_start), (end, s.cap_end)]
                .into_iter()
                .filter_map(|(on, c)| on.then_some(c))
                .collect::<Vec<_>>();
            let closed = caps.is_empty() && !net.segments.is_empty();
            (caps, closed)
        }
        NodeKind::Line => (vec![s.cap_start, s.cap_end], false),
        _ => (vec![], true),
    }
}

/// На сколько обводки узла по контуру выступают за рамку формы.
pub fn path_stroke_outset(p: &Props) -> f64 {
    let style = &p.stroke_style;
    let (caps, closed) = end_caps(p);
    p.strokes
        .iter()
        .filter(|s| s.width > 0.0)
        .map(|s| {
            let w = if closed && s.align != StrokeAlign::Center {
                2.0 * s.width
            } else {
                s.width
            };
            let join = if style.join == StrokeJoin::Miter {
                w / 2.0 * style.miter_limit.max(1.0)
            } else {
                w / 2.0
            };
            let cap = caps.iter().map(|c| cap_extent(*c, w)).fold(0.0, f64::max);
            join.max(cap)
        })
        .fold(0.0, f64::max)
}

/// Расстояние от точки до кривой — с отсевом сегментов по рамке: на сети в
/// 100 000 точек точный поиск идёт только у сегментов рядом с точкой.
pub fn distance_to(path: &BezPath, pt: Point, within: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for seg in path.segments() {
        let b = seg.bounding_box().inflate(within, within);
        if !b.contains(pt) {
            continue;
        }
        let d = seg.nearest(pt, 1e-6).distance_sq.sqrt();
        best = Some(best.map_or(d, |x: f64| x.min(d)));
    }
    best
}

/// Лежит ли точка в области по её правилу.
pub fn region_contains(r: &RegionPath, pt: Point) -> bool {
    let w = r.path.winding(pt);
    match r.rule {
        FillRule::NonZero => w != 0,
        FillRule::EvenOdd => w % 2 != 0,
    }
}

/// Попадание в узел с обводкой по контуру (`local` — в координатах формы):
/// залитая область, полоса обводки не уже допуска, фигуры окончаний.
pub fn hit_path(p: &Props, shape: &PathShape, local: Point, tolerance: f64) -> bool {
    let node_filled = p.fills.iter().any(|f| f.visible && f.opacity > 0.0);
    if p.kind == NodeKind::Vector {
        for r in &shape.regions {
            let filled = match &r.fills {
                Some(f) => f.iter().any(|f| f.visible && f.opacity > 0.0),
                None => node_filled,
            };
            if filled && region_contains(r, local) {
                return true;
            }
        }
    }
    for s in p.strokes.iter().filter(|s| s.width > 0.0) {
        let w = effective_width(shape, s);
        let half = (w / 2.0).max(tolerance);
        if distance_to(&shape.stroke, local, half).is_some_and(|d| d <= half) {
            return true;
        }
        for e in &shape.ends {
            if e.cap != StrokeCap::None
                && (local - e.point).hypot() <= cap_extent(e.cap, w).max(tolerance)
            {
                return true;
            }
        }
    }
    false
}

/// Сегмент сети под точкой (в координатах сети) и параметр на нём — для
/// добавления точки щелчком и изгиба: ближайший в пределах `within`.
pub fn nearest_segment(net: &VectorNetwork, pt: Point, within: f64) -> Option<(usize, f64, f64)> {
    let mut best: Option<(usize, f64, f64)> = None;
    for (i, s) in net.segments.iter().enumerate() {
        let c = segment_curve(net, s);
        if !c.bounding_box().inflate(within, within).contains(pt) {
            continue;
        }
        let n = c.nearest(pt, 1e-9);
        let d = n.distance_sq.sqrt();
        if d <= within && best.is_none_or(|b| d < b.2) {
            best = Some((i, n.t, d));
        }
    }
    best
}

/// С какого числа элементов контуры упрощаются под масштаб (`simplify`): меньшие
/// рисуются точно на любом зуме.
pub const SIMPLIFY_MIN_ELEMENTS: usize = 4096;

/// Контуры с точностью `tol` (в координатах формы): кривые, почти прямые в этом
/// допуске, становятся хордами, точки ближе `tol` к последней оставленной —
/// пропускаются (концы подпутей остаются). Сеть в 100 000 точек при отдалении —
/// тысячи отрезков меньше пикселя; рисовать и обводить их все — сотни миллисекунд
/// на кадр. Выгнутые кривые остаются кривыми, поэтому упрощённый путь не больше
/// точного на любом масштабе. Небольшие контуры возвращаются как есть.
pub fn simplify(shape: &PathShape, tol: f64) -> PathShape {
    let size: usize = shape.stroke.elements().len()
        + shape
            .regions
            .iter()
            .map(|r| r.path.elements().len())
            .sum::<usize>();
    if size < SIMPLIFY_MIN_ELEMENTS || tol.is_nan() || tol <= 0.0 {
        return shape.clone();
    }
    PathShape {
        stroke: simplify_path(&shape.stroke, tol),
        ends: shape.ends.clone(),
        junctions: shape.junctions.clone(),
        closed: shape.closed,
        regions: shape
            .regions
            .iter()
            .map(|r| RegionPath {
                path: simplify_path(&r.path, tol),
                ..r.clone()
            })
            .collect(),
    }
}

/// Ломаная подпути с отбросом точек, лежащих ближе `tol` к прямой от последней
/// оставленной точки (Реуманн — Виткам, за один проход); точки, где путь
/// поворачивает назад, остаются.
#[derive(Default)]
struct Thinner {
    anchor: Point,
    dir: Option<Vec2>,
    /// Последняя пропущенная точка: оставляется, когда следующая уходит от прямой.
    prev: Option<Point>,
    along: f64,
}

impl Thinner {
    fn start(&mut self, out: &mut BezPath, p: Point) {
        out.move_to(p);
        *self = Thinner {
            anchor: p,
            ..Default::default()
        };
    }

    fn push(&mut self, out: &mut BezPath, p: Point, tol: f64) {
        let v = p - self.anchor;
        let Some(dir) = self.dir else {
            if v.hypot() >= tol {
                self.dir = Some(v / v.hypot());
                self.along = v.hypot();
            }
            self.prev = Some(p);
            return;
        };
        let along = v.dot(dir);
        if v.cross(dir).abs() <= tol && along >= self.along - tol {
            self.prev = Some(p);
            self.along = self.along.max(along);
            return;
        }
        let q = self.prev.unwrap_or(p);
        out.line_to(q);
        self.anchor = q;
        let w = p - q;
        let n = w.hypot();
        self.dir = (n > 0.0).then(|| w / n);
        self.along = n;
        self.prev = Some(p);
    }

    /// Продолжение подпути с `p` без нового `MoveTo` (после оставленной кривой).
    fn resume(&mut self, p: Point) {
        *self = Thinner {
            anchor: p,
            ..Default::default()
        };
    }

    fn finish(&mut self, out: &mut BezPath) {
        if let Some(q) = self.prev.take() {
            out.line_to(q);
        }
    }
}

/// Путь с точностью `tol` (см. `simplify`) — и для оверлея правки.
///
/// Каждый элемент результата — свой элемент исходного пути (точка или кривая),
/// поэтому результат не длиннее исходного; не стал короче — возвращается
/// исходный. Отклонение не больше `tol`: кривая становится хордой, только
/// если сама она не дальше `tol / 4` от хорды (ручки не дальше `tol / 3`:
/// кубическая кривая отходит от хорды не больше чем на 3/4 отхода ручек,
/// квадратичная — на половину), а точки ломаной отбрасываются в полосе
/// `tol / 4` вокруг прямой (с возвратом назад не больше `tol / 4` — до отрезка
/// выходит не больше `3/4 · tol`).
pub fn simplify_path(path: &BezPath, tol: f64) -> BezPath {
    let tol = tol / 4.0;
    let mut out = BezPath::new();
    let mut t = Thinner::default();
    let mut last = Point::ZERO;
    let handle = tol * 4.0 / 3.0;
    let near = |from: Point, to: Point, q: Point| {
        Line::new(from, to).nearest(q, 1e-9).distance_sq <= handle * handle
    };
    for el in path.iter() {
        match el {
            PathEl::MoveTo(p) => {
                t.finish(&mut out);
                t.start(&mut out, p);
                last = p;
            }
            PathEl::LineTo(p) => {
                t.push(&mut out, p, tol);
                last = p;
            }
            // Почти прямая кривая — хорда (на сети в 100 000 точек таких —
            // десятки тысяч); выгнутая — остаётся кривой: дробить её в ломаную
            // значило бы раздуть путь на крупном масштабе.
            PathEl::CurveTo(a, b, p) if near(last, p, a) && near(last, p, b) => {
                t.push(&mut out, p, tol);
                last = p;
            }
            PathEl::QuadTo(a, p) if near(last, p, a) => {
                t.push(&mut out, p, tol);
                last = p;
            }
            PathEl::CurveTo(..) | PathEl::QuadTo(..) => {
                let p = el.end_point().expect("у кривой есть конец");
                t.finish(&mut out);
                out.push(el);
                t.resume(p);
                last = p;
            }
            PathEl::ClosePath => {
                t.finish(&mut out);
                out.close_path();
            }
        }
    }
    t.finish(&mut out);
    if out.elements().len() >= path.elements().len() {
        return path.clone();
    }
    out
}
