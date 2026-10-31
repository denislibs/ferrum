//! Правки векторной сети без документа (спека подсистемы 3, раздел 9): фигура →
//! сеть, сеть → рамка узла, добавление точки на сегменте, удаление с сохранением
//! формы и с разрывом, ручки с зеркальностью, изгиб сегмента, грани для
//! «Заливки области», сплющивание нескольких узлов. Все функции чистые: на входе
//! сеть и параметры, на выходе новая сеть — режим правки и перо записывают её
//! одной транзакцией.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use ferrum_doc::{
    Color, Fill, FillRule, HandleMode, NodeKind, Props, PropsPatch, VectorNetwork, VectorPoint,
    VectorRegion, VectorSegment,
};
use ferrum_scene::vector::{fit, network_bounds, segment_curve};
use kurbo::{
    Affine, BezPath, CubicBez, ParamCurve, ParamCurveNearest, PathEl, PathSeg, Point, Rect, Shape,
    Vec2,
};

/// Доля радиуса для ручек дуги в четверть окружности.
const KAPPA: f64 = 0.552_284_749_830_793_4;

fn v2(t: [f64; 2]) -> Vec2 {
    Vec2::new(t[0], t[1])
}

fn arr(v: Vec2) -> [f64; 2] {
    [v.x, v.y]
}

fn pt(net: &VectorNetwork, i: u32) -> Point {
    let p = &net.points[i as usize];
    Point::new(p.x, p.y)
}

/// Сеть узла в координатах его формы (`0..w × 0..h`, до отражения): у вектора —
/// растянутая под рамку, у прямоугольника, эллипса и линии — их контур (как в
/// Figma при входе в правку фигуры). `None` — у узла нет контура (группа, текст,
/// изображение, фрейм).
pub fn shape_network(p: &Props) -> Option<VectorNetwork> {
    let (w, h) = (p.w.max(0.0), p.h.max(0.0));
    let closed = |points: Vec<VectorPoint>, segments: Vec<VectorSegment>| {
        let loop_ = (0..segments.len() as u32).collect();
        VectorNetwork {
            points,
            segments,
            regions: vec![VectorRegion {
                rule: FillRule::NonZero,
                loops: vec![loop_],
                fills: None,
            }],
        }
    };
    match p.kind {
        NodeKind::Vector => {
            let net = p.vector.as_deref()?;
            Some(transform_network(net, fit(p)))
        }
        NodeKind::Line => Some(VectorNetwork {
            points: vec![VectorPoint::new(0.0, 0.0), VectorPoint::new(w, 0.0)],
            segments: vec![VectorSegment::line(0, 1)],
            regions: vec![],
        }),
        NodeKind::Rect => {
            let r = p.corner_radius.max(0.0).min(w / 2.0).min(h / 2.0);
            if r <= 0.0 {
                let pts = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
                return Some(closed(
                    pts.iter().map(|&(x, y)| VectorPoint::new(x, y)).collect(),
                    (0..4)
                        .map(|i| VectorSegment::line(i, (i + 1) % 4))
                        .collect(),
                ));
            }
            // Скругление — дугами: по две точки на угол, между ними кривая.
            let k = r * KAPPA;
            let pts = [
                (r, 0.0),
                (w - r, 0.0),
                (w, r),
                (w, h - r),
                (w - r, h),
                (r, h),
                (0.0, h - r),
                (0.0, r),
            ];
            let arcs = [
                ([k, 0.0], [0.0, -k]),
                ([0.0, k], [k, 0.0]),
                ([-k, 0.0], [0.0, k]),
                ([0.0, -k], [-k, 0.0]),
            ];
            let mut segments = Vec::new();
            for c in 0..4u32 {
                segments.push(VectorSegment::line(2 * c, 2 * c + 1));
                let (ts, te) = arcs[c as usize];
                segments.push(VectorSegment {
                    start: 2 * c + 1,
                    end: (2 * c + 2) % 8,
                    tangent_start: ts,
                    tangent_end: te,
                });
            }
            Some(closed(
                pts.iter().map(|&(x, y)| VectorPoint::new(x, y)).collect(),
                segments,
            ))
        }
        NodeKind::Ellipse => {
            let (rx, ry) = (w / 2.0, h / 2.0);
            let (kx, ky) = (rx * KAPPA, ry * KAPPA);
            let point = |x, y| VectorPoint {
                handle_mode: HandleMode::MirrorAll,
                ..VectorPoint::new(x, y)
            };
            // Сверху по часовой: верх, право, низ, лево.
            let points = vec![point(rx, 0.0), point(w, ry), point(rx, h), point(0.0, ry)];
            let t = [
                ([kx, 0.0], [0.0, -ky]),
                ([0.0, ky], [kx, 0.0]),
                ([-kx, 0.0], [0.0, ky]),
                ([0.0, -ky], [-kx, 0.0]),
            ];
            let segments = (0..4u32)
                .map(|i| VectorSegment {
                    start: i,
                    end: (i + 1) % 4,
                    tangent_start: t[i as usize].0,
                    tangent_end: t[i as usize].1,
                })
                .collect();
            Some(closed(points, segments))
        }
        _ => None,
    }
}

/// Сеть под аффинным преобразованием: точки — целиком, ручки — линейной частью.
pub fn transform_network(net: &VectorNetwork, a: Affine) -> VectorNetwork {
    let c = a.as_coeffs();
    let lin = |t: [f64; 2]| [c[0] * t[0] + c[2] * t[1], c[1] * t[0] + c[3] * t[1]];
    VectorNetwork {
        points: net
            .points
            .iter()
            .map(|p| {
                let q = a * Point::new(p.x, p.y);
                VectorPoint {
                    x: q.x,
                    y: q.y,
                    ..*p
                }
            })
            .collect(),
        segments: net
            .segments
            .iter()
            .map(|s| VectorSegment {
                tangent_start: lin(s.tangent_start),
                tangent_end: lin(s.tangent_end),
                ..*s
            })
            .collect(),
        regions: net.regions.clone(),
    }
}

/// Новая сеть узла `p` в координатах его формы → правка узла: сеть сдвигается к
/// началу своих границ, рамка узла — по ним, положение пересчитывается с учётом
/// поворота и отражения так, что на экране ничего не сдвигается.
pub fn reframe(p: &Props, net: VectorNetwork) -> PropsPatch {
    let b = network_bounds(&net).unwrap_or(kurbo::Rect::ZERO);
    let m = b.origin();
    let (nw, nh) = (b.width(), b.height());
    let d = Vec2::new(
        if p.flip_x { p.w - m.x - nw } else { m.x },
        if p.flip_y { p.h - m.y - nh } else { m.y },
    );
    let r = Affine::rotate(-p.rotation.to_radians()) * d.to_point();
    let shifted = transform_network(&net, Affine::translate(-m.to_vec2()));
    PropsPatch {
        x: Some(p.x + r.x),
        y: Some(p.y + r.y),
        w: Some(nw),
        h: Some(nh),
        vector: Some(Arc::new(shifted)),
        ..Default::default()
    }
}

/// Убирает точки `drop_points` и сегменты `drop_segs`, перенумеровывая остальное.
/// Петли, в которых был удалённый сегмент, разорваны — уходят (область без
/// петель — тоже).
pub fn compact(
    net: &VectorNetwork,
    drop_points: &HashSet<u32>,
    drop_segs: &HashSet<u32>,
) -> VectorNetwork {
    let mut point_map = vec![u32::MAX; net.points.len()];
    let mut points = Vec::new();
    for (i, p) in net.points.iter().enumerate() {
        if !drop_points.contains(&(i as u32)) {
            point_map[i] = points.len() as u32;
            points.push(*p);
        }
    }
    let mut seg_map = vec![u32::MAX; net.segments.len()];
    let mut segments = Vec::new();
    for (i, s) in net.segments.iter().enumerate() {
        let (a, b) = (point_map[s.start as usize], point_map[s.end as usize]);
        if drop_segs.contains(&(i as u32)) || a == u32::MAX || b == u32::MAX {
            continue;
        }
        seg_map[i] = segments.len() as u32;
        segments.push(VectorSegment {
            start: a,
            end: b,
            ..*s
        });
    }
    let regions = net
        .regions
        .iter()
        .filter_map(|r| {
            let loops: Vec<Vec<u32>> = r
                .loops
                .iter()
                .filter(|l| l.iter().all(|&s| seg_map[s as usize] != u32::MAX))
                .map(|l| l.iter().map(|&s| seg_map[s as usize]).collect())
                .collect();
            (!loops.is_empty()).then(|| VectorRegion { loops, ..r.clone() })
        })
        .collect();
    VectorNetwork {
        points,
        segments,
        regions,
    }
}

/// Точка на сегменте `seg` при параметре `t`: сегмент делится в этой точке
/// (кривая — по де Кастельжо, форма та же), петли областей получают оба
/// куска в порядке обхода. Возвращает сеть и номер новой точки.
pub fn add_point(net: &VectorNetwork, seg: usize, t: f64) -> (VectorNetwork, u32) {
    let mut out = net.clone();
    let s = net.segments[seg];
    let t = t.clamp(0.0, 1.0);
    let curve = segment_curve(net, &s);
    let at = curve.eval(t);
    let new = out.points.len() as u32;
    let (first, second, mode) = match curve {
        PathSeg::Cubic(c) => {
            let (a, b) = (c.subsegment(0.0..t), c.subsegment(t..1.0));
            (
                VectorSegment {
                    start: s.start,
                    end: new,
                    tangent_start: arr(a.p1 - a.p0),
                    tangent_end: arr(a.p2 - a.p3),
                },
                VectorSegment {
                    start: new,
                    end: s.end,
                    tangent_start: arr(b.p1 - b.p0),
                    tangent_end: arr(b.p2 - b.p3),
                },
                HandleMode::MirrorAngle,
            )
        }
        _ => (
            VectorSegment::line(s.start, new),
            VectorSegment::line(new, s.end),
            HandleMode::None,
        ),
    };
    out.points.push(VectorPoint {
        handle_mode: mode,
        ..VectorPoint::new(at.x, at.y)
    });
    out.segments[seg] = first;
    let second_i = out.segments.len() as u32;
    out.segments.push(second);
    for r in &mut out.regions {
        for l in &mut r.loops {
            let Some(k) = l.iter().position(|&x| x as usize == seg) else {
                continue;
            };
            let Some(order) = net.loop_points(l) else {
                continue;
            };
            // Петля проходит сегмент от его начала — второй кусок после первого.
            if order[k] == s.start {
                l.insert(k + 1, second_i);
            } else {
                l.insert(k, second_i);
            }
        }
    }
    (out, new)
}

/// Сегменты точки.
fn incident(net: &VectorNetwork, p: u32) -> Vec<usize> {
    net.segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.start == p || s.end == p)
        .map(|(i, _)| i)
        .collect()
}

/// Сегмент `seg` как кривая от точки `from` (развёрнутая, если сегмент идёт к ней).
fn curve_from(net: &VectorNetwork, seg: usize, from: u32) -> CubicBez {
    let c = match segment_curve(net, &net.segments[seg]) {
        PathSeg::Cubic(c) => c,
        PathSeg::Line(l) => CubicBez::new(l.p0, l.p0, l.p1, l.p1),
        PathSeg::Quad(q) => q.raise(),
    };
    if net.segments[seg].start == from {
        c
    } else {
        CubicBez::new(c.p3, c.p2, c.p1, c.p0)
    }
}

/// Одна кубическая кривая вместо двух `a` (от x к p) и `b` (от p к y): концы и
/// направления касательных в x и y те же, длины ручек — наименьшими квадратами
/// по точкам обеих кривых (параметр — по длине хорды, как у Шнайдера).
fn merge_curves(a: CubicBez, b: CubicBez) -> CubicBez {
    let (x, y) = (a.p0, b.p3);
    let dir = |v: Vec2, fallback: Vec2| {
        let v = if v.hypot2() > 1e-18 { v } else { fallback };
        let n = v.hypot();
        if n > 0.0 {
            v / n
        } else {
            Vec2::ZERO
        }
    };
    let t1 = dir(a.p1 - a.p0, a.p3 - a.p0);
    let t2 = dir(b.p2 - b.p3, b.p0 - b.p3);
    // Точки обеих кривых и их параметры по накопленной длине хорды.
    let mut samples = Vec::new();
    for (c, k) in [(a, 0), (b, 1)] {
        for i in 0..=16 {
            if k == 1 && i == 0 {
                continue;
            }
            samples.push(c.eval(i as f64 / 16.0));
        }
    }
    let mut len = vec![0.0];
    for w in samples.windows(2) {
        len.push(len.last().unwrap() + (w[1] - w[0]).hypot());
    }
    let total = *len.last().unwrap();
    if total <= 0.0 {
        return CubicBez::new(x, x, y, y);
    }
    let (mut c11, mut c12, mut c22, mut x1, mut x2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (q, l) in samples.iter().zip(&len) {
        let u = l / total;
        let (b0, b1, b2, b3) = (
            (1.0 - u).powi(3),
            3.0 * u * (1.0 - u).powi(2),
            3.0 * u * u * (1.0 - u),
            u.powi(3),
        );
        let a1 = t1 * b1;
        let a2 = t2 * b2;
        let rest = q.to_vec2() - (x.to_vec2() * (b0 + b1) + y.to_vec2() * (b2 + b3));
        c11 += a1.dot(a1);
        c12 += a1.dot(a2);
        c22 += a2.dot(a2);
        x1 += a1.dot(rest);
        x2 += a2.dot(rest);
    }
    let det = c11 * c22 - c12 * c12;
    let chord = (y - x).hypot();
    let (mut l1, mut l2) = if det.abs() > 1e-12 {
        ((x1 * c22 - x2 * c12) / det, (c11 * x2 - c12 * x1) / det)
    } else {
        (chord / 3.0, chord / 3.0)
    };
    // Отрицательная длина — петля: тогда треть хорды, как у Шнайдера.
    if l1 < 1e-9 || l2 < 1e-9 {
        l1 = chord / 3.0;
        l2 = chord / 3.0;
    }
    CubicBez::new(x, x + t1 * l1, y + t2 * l2, y)
}

/// Удаление точек с сохранением формы (Delete): у точки с двумя сегментами они
/// сливаются в один, приближающий оба (два отрезка — отрезок); конец пути
/// уходит со своим сегментом; у остальных точек уходят и их сегменты.
pub fn delete_points_keep_shape(net: &VectorNetwork, points: &[u32]) -> VectorNetwork {
    let mut cur = net.clone();
    // По одной, от больших номеров к меньшим: удаление точки сдвигает номера
    // только после неё, поэтому оставшиеся в очереди номера верны.
    let mut queue: Vec<u32> = points.to_vec();
    queue.sort_unstable();
    queue.dedup();
    while let Some(p) = queue.pop() {
        if p as usize >= cur.points.len() {
            continue;
        }
        let segs = incident(&cur, p);
        if let [a, b] = segs[..] {
            let ca = curve_from(&cur, a, p);
            let cb = curve_from(&cur, b, p);
            let other = |s: usize| {
                let s = &cur.segments[s];
                if s.start == p {
                    s.end
                } else {
                    s.start
                }
            };
            let (xi, yi) = (other(a), other(b));
            if xi != yi {
                // Обе прямые — прямая; иначе — приближение двух кривых одной.
                let both_straight = cur.segments[a].is_straight() && cur.segments[b].is_straight();
                let merged = if both_straight {
                    VectorSegment::line(xi, yi)
                } else {
                    // Кривая от x через p к y.
                    let a_rev = CubicBez::new(ca.p3, ca.p2, ca.p1, ca.p0);
                    let m = merge_curves(a_rev, cb);
                    VectorSegment {
                        start: xi,
                        end: yi,
                        tangent_start: arr(m.p1 - m.p0),
                        tangent_end: arr(m.p2 - m.p3),
                    }
                };
                cur.segments[a] = merged;
                // Петли: сегмент `b` уходит, `a` его заменяет.
                for r in &mut cur.regions {
                    for l in &mut r.loops {
                        l.retain(|&s| s as usize != b);
                    }
                }
                let drop_p = HashSet::from([p]);
                let drop_s = HashSet::from([b as u32]);
                cur = compact(&cur, &drop_p, &drop_s);
                continue;
            }
        }
        // Конец пути, ветвление или одиночная точка: точка уходит с сегментами.
        let drop_s: HashSet<u32> = segs.iter().map(|&s| s as u32).collect();
        cur = compact(&cur, &HashSet::from([p]), &drop_s);
    }
    cur
}

/// Удаление с разрывом (Shift+Delete): точки уходят вместе со своими сегментами.
pub fn delete_points_break(net: &VectorNetwork, points: &[u32]) -> VectorNetwork {
    let drop_p: HashSet<u32> = points.iter().copied().collect();
    let drop_s: HashSet<u32> = net
        .segments
        .iter()
        .enumerate()
        .filter(|(_, s)| drop_p.contains(&s.start) || drop_p.contains(&s.end))
        .map(|(i, _)| i as u32)
        .collect();
    compact(net, &drop_p, &drop_s)
}

/// Удаление сегментов: точки остаются, кроме тех, у которых не осталось ни
/// одного сегмента (как в Figma — висящих точек не бывает).
pub fn delete_segments(net: &VectorNetwork, segs: &[u32]) -> VectorNetwork {
    let drop_s: HashSet<u32> = segs.iter().copied().collect();
    let mut degree = vec![0u32; net.points.len()];
    let mut touched = HashSet::new();
    for (i, s) in net.segments.iter().enumerate() {
        if drop_s.contains(&(i as u32)) {
            touched.insert(s.start);
            touched.insert(s.end);
        } else {
            degree[s.start as usize] += 1;
            degree[s.end as usize] += 1;
        }
    }
    let drop_p: HashSet<u32> = touched
        .into_iter()
        .filter(|&p| degree[p as usize] == 0)
        .collect();
    compact(net, &drop_p, &drop_s)
}

/// Сдвиг точек (ручки — смещения от точек — едут вместе с ними).
pub fn move_points(net: &VectorNetwork, points: &[u32], d: Vec2) -> VectorNetwork {
    let mut out = net.clone();
    for &i in points {
        let p = &mut out.points[i as usize];
        p.x += d.x;
        p.y += d.y;
    }
    out
}

/// Какой конец сегмента: ручка у начала или у конца.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HandleEnd {
    Start,
    End,
}

/// Ручка сегмента `seg` у его конца `end` становится `tangent` (смещение от
/// точки). Зеркальность точки (`MirrorAngle`, `MirrorAll`) поворачивает ручку
/// второго сегмента точки в противоположную сторону (у `MirrorAll` — и той же
/// длины); `break_mirror` (Alt) делает точку независимой (`Free`).
pub fn set_handle(
    net: &VectorNetwork,
    seg: usize,
    end: HandleEnd,
    tangent: Vec2,
    break_mirror: bool,
) -> VectorNetwork {
    let mut out = net.clone();
    let s = &mut out.segments[seg];
    let p = match end {
        HandleEnd::Start => {
            s.tangent_start = arr(tangent);
            s.start
        }
        HandleEnd::End => {
            s.tangent_end = arr(tangent);
            s.end
        }
    };
    let mode = out.points[p as usize].handle_mode;
    if break_mirror {
        out.points[p as usize].handle_mode = HandleMode::Free;
        return out;
    }
    if mode == HandleMode::None {
        // Точка без ручек получила ручку: дальше — зеркально, как у пера Figma.
        out.points[p as usize].handle_mode = HandleMode::MirrorAll;
    }
    let mode = out.points[p as usize].handle_mode;
    if !matches!(mode, HandleMode::MirrorAngle | HandleMode::MirrorAll) {
        return out;
    }
    let segs = incident(&out, p);
    let Some(&other) = segs.iter().find(|&&i| i != seg) else {
        return out;
    };
    if segs.len() != 2 {
        return out;
    }
    let o = &mut out.segments[other];
    let slot = if o.start == p {
        &mut o.tangent_start
    } else {
        &mut o.tangent_end
    };
    let len = tangent.hypot();
    let own = v2(*slot).hypot();
    if len > 0.0 {
        let l = if mode == HandleMode::MirrorAll || own == 0.0 {
            len
        } else {
            own
        };
        *slot = arr(-tangent / len * l);
    }
    out
}

/// Изгиб (Ctrl/Cmd + протяжка сегмента): точка сегмента при параметре `t`
/// уходит на `d`, обе ручки сдвигаются на `d / (3t(1 − t))` — так точка кривой
/// при этом параметре попадает ровно под курсор. Концы не двигаются.
pub fn bend(net: &VectorNetwork, seg: usize, t: f64, d: Vec2) -> VectorNetwork {
    let mut out = net.clone();
    let t = t.clamp(0.1, 0.9);
    let k = d / (3.0 * t * (1.0 - t));
    let s = &mut out.segments[seg];
    if s.is_straight() {
        // Отрезок — кубической кривой с ручками в трети: параметр тот же, что у
        // отрезка (`t` пришёл из попадания по отрезку).
        let third = (pt(net, s.end) - pt(net, s.start)) / 3.0;
        s.tangent_start = arr(third);
        s.tangent_end = arr(-third);
    }
    s.tangent_start = arr(v2(s.tangent_start) + k);
    s.tangent_end = arr(v2(s.tangent_end) + k);
    let (a, b) = (s.start as usize, s.end as usize);
    // Концы без ручек остаются «углами»: ручка изгиба не должна зеркально
    // искривлять соседние прямые стороны при последующей протяжке.
    for i in [a, b] {
        if out.points[i].handle_mode == HandleMode::None {
            out.points[i].handle_mode = HandleMode::Free;
        }
    }
    out
}

/// Мосты сети: сегменты, после удаления которых сеть распадается (висячие
/// цепочки и перемычки между циклами). Они не ограничивают ни одну грань —
/// с обеих сторон у них одна и та же грань. Тарьян по рёбрам (кратные сегменты
/// между двумя точками мостами не считаются), без рекурсии.
fn bridges(net: &VectorNetwork) -> Vec<bool> {
    let adj = Adjacency::new(
        net.points.len(),
        net.segments
            .iter()
            .enumerate()
            .filter(|(_, s)| s.start != s.end)
            .flat_map(|(i, s)| {
                [
                    (s.start as usize, (s.end as usize, i)),
                    (s.end as usize, (s.start as usize, i)),
                ]
            }),
    );
    let mut out = vec![false; net.segments.len()];
    let mut disc = vec![usize::MAX; net.points.len()];
    let mut low = vec![0usize; net.points.len()];
    let mut time = 0;
    for root in 0..net.points.len() {
        if disc[root] != usize::MAX {
            continue;
        }
        disc[root] = time;
        low[root] = time;
        time += 1;
        // (точка, сегмент, которым в неё пришли, следующий номер в списке)
        let mut stack = vec![(root, usize::MAX, 0usize)];
        while let Some(top) = stack.last_mut() {
            let (v, via, k) = *top;
            if let Some(&(w, e)) = adj.of(v).get(k) {
                top.2 += 1;
                if e == via {
                    continue;
                }
                if disc[w] == usize::MAX {
                    disc[w] = time;
                    low[w] = time;
                    time += 1;
                    stack.push((w, e, 0));
                } else {
                    low[v] = low[v].min(disc[w]);
                }
            } else {
                stack.pop();
                if let Some(&(u, _, _)) = stack.last() {
                    low[u] = low[u].min(low[v]);
                    if low[v] > disc[u] {
                        out[via] = true;
                    }
                }
            }
        }
    }
    out
}

/// Списки смежности одним массивом (без вектора на каждую из 100 000 точек):
/// элементы точки `v` — `of(v)`, в порядке добавления.
struct Adjacency<T> {
    start: Vec<usize>,
    items: Vec<T>,
}

impl<T: Copy + Default> Adjacency<T> {
    fn new(points: usize, pairs: impl Iterator<Item = (usize, T)> + Clone) -> Adjacency<T> {
        let mut start = vec![0usize; points + 1];
        for (v, _) in pairs.clone() {
            start[v + 1] += 1;
        }
        for v in 0..points {
            start[v + 1] += start[v];
        }
        let mut fill = start.clone();
        let mut items = vec![T::default(); start[points]];
        for (v, t) in pairs {
            items[fill[v]] = t;
            fill[v] += 1;
        }
        Adjacency { start, items }
    }

    fn of(&self, v: usize) -> &[T] {
        &self.items[self.start[v]..self.start[v + 1]]
    }

    fn of_mut(&mut self, v: usize) -> &mut [T] {
        &mut self.items[self.start[v]..self.start[v + 1]]
    }
}

/// Обход одной грани плоского графа: сегменты по ходу, контур, знаковая площадь
/// (kurbo: у внутренних граней < 0, у внешней грани связной части > 0) и
/// связная часть.
struct Walk {
    segs: Vec<u32>,
    path: BezPath,
    /// Рамка ручек контура: вне неё точка не внутри (без обхода контура).
    bbox: Rect,
    area: f64,
    part: usize,
    /// Точка сети на контуре — представитель связной части.
    at: Point,
}

impl Walk {
    fn inside(&self, p: Point) -> bool {
        self.bbox.contains(p) && self.path.winding(p) != 0
    }
}

/// Сетка рамок: какие рамки задевают клетку с точкой. «Заливка области» на
/// десятках тысяч контуров ищет грани и петли у точки по ней, а не перебором
/// всех (было квадратично: 25 000 контуров — минута).
struct Grid {
    origin: Point,
    cell: f64,
    side: usize,
    cells: Vec<Vec<u32>>,
}

impl Grid {
    /// Сетка `√n × √n` (не больше 1024 × 1024) по объединению рамок; номера
    /// `items` — их номера в `boxes`.
    fn new(boxes: &[Rect], items: impl Iterator<Item = usize> + Clone) -> Grid {
        let bounds = items
            .clone()
            .map(|i| boxes[i])
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::ZERO);
        let count = items.clone().count().max(1);
        let side = ((count as f64).sqrt().ceil() as usize).clamp(1, 1024);
        let cell = (bounds.width().max(bounds.height()) / side as f64).max(f64::MIN_POSITIVE);
        let mut grid = Grid {
            origin: bounds.origin(),
            cell,
            side,
            cells: vec![Vec::new(); side * side],
        };
        for i in items {
            let b = boxes[i];
            let (x0, y0) = grid.coords(Point::new(b.x0, b.y0));
            let (x1, y1) = grid.coords(Point::new(b.x1, b.y1));
            for y in y0..=y1 {
                for x in x0..=x1 {
                    grid.cells[y * side + x].push(i as u32);
                }
            }
        }
        grid
    }

    fn coords(&self, p: Point) -> (usize, usize) {
        let k = |v: f64| ((v / self.cell).floor().max(0.0) as usize).min(self.side - 1);
        (k(p.x - self.origin.x), k(p.y - self.origin.y))
    }

    /// Рамки, которые могут содержать `p` (проверка рамок — за вызывающим).
    fn at(&self, p: Point) -> &[u32] {
        let (x, y) = self.coords(p);
        &self.cells[y * self.side + x]
    }
}

/// Все грани сети без мостов (из полуребра u→v к следующему по углу
/// касательной в v). Без мостов каждый сегмент разделяет две разные грани, так
/// что ни одна грань не проходит сегмент дважды. Вырожденные (нулевой площади)
/// не входят.
fn face_walks(net: &VectorNetwork) -> Vec<Walk> {
    let n = net.segments.len();
    // Полуребро 2i — сегмент i от начала, 2i+1 — от конца.
    let from = |h: usize| {
        let s = &net.segments[h / 2];
        if h.is_multiple_of(2) {
            s.start
        } else {
            s.end
        }
    };
    let angle = |h: usize| {
        let s = &net.segments[h / 2];
        let c = segment_curve(net, s);
        let (p, q) = match c {
            PathSeg::Cubic(c) if h.is_multiple_of(2) => {
                (c.p0, if c.p1 != c.p0 { c.p1 } else { c.p2 })
            }
            PathSeg::Cubic(c) => (c.p3, if c.p2 != c.p3 { c.p2 } else { c.p1 }),
            PathSeg::Line(l) if h.is_multiple_of(2) => (l.p0, l.p1),
            PathSeg::Line(l) => (l.p1, l.p0),
            PathSeg::Quad(q) => (q.p0, q.p1),
        };
        (q - p).atan2()
    };
    let bridge = bridges(net);
    let active = |i: usize| !bridge[i] && net.segments[i].start != net.segments[i].end;
    // Связные части по оставшимся сегментам.
    let mut parent: Vec<usize> = (0..net.points.len()).collect();
    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for (i, s) in net.segments.iter().enumerate() {
        if active(i) {
            let (a, b) = (
                root(&mut parent, s.start as usize),
                root(&mut parent, s.end as usize),
            );
            parent[a] = b;
        }
    }
    // Полурёбра из каждой точки по углу (углы — один раз, не на каждое сравнение).
    let mut out_of = Adjacency::new(
        net.points.len(),
        (0..2 * n)
            .filter(|h| active(h / 2))
            .map(|h| (from(h) as usize, h)),
    );
    for v in 0..net.points.len() {
        let list = out_of.of_mut(v);
        if list.len() > 2 {
            let mut keyed: Vec<(f64, usize)> = list.iter().map(|&h| (angle(h), h)).collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (slot, (_, h)) in list.iter_mut().zip(keyed) {
                *slot = h;
            }
        }
    }
    let mut seen = vec![false; 2 * n];
    let mut result = Vec::new();
    for start in (0..2 * n).filter(|h| active(h / 2)) {
        if seen[start] {
            continue;
        }
        let mut face = Vec::new();
        let mut h = start;
        loop {
            seen[h] = true;
            face.push(h);
            let twin = h ^ 1;
            let list = out_of.of(from(twin) as usize);
            let k = list.iter().position(|&x| x == twin).unwrap_or(0);
            h = list[(k + 1) % list.len()];
            if h == start || seen[h] || face.len() > 2 * n {
                break;
            }
        }
        if h != start {
            continue;
        }
        let path = face_path(net, &face);
        let area = path.area();
        if area.abs() <= 1e-9 {
            continue;
        }
        let p = from(start);
        result.push(Walk {
            segs: face.iter().map(|h| (h / 2) as u32).collect(),
            bbox: path.control_box(),
            path,
            area,
            part: root(&mut parent, p as usize),
            at: pt(net, p),
        });
    }
    result
}

/// Грани сети как плоского графа (сегменты без пересечений): петли сегментов,
/// ограничивающие внутренние области; внешняя грань каждой связной части не
/// входит, мосты и висячие цепочки — тоже (сегмент в грани не повторяется).
pub fn faces(net: &VectorNetwork) -> Vec<Vec<u32>> {
    face_walks(net)
        .into_iter()
        .filter(|w| w.area < 0.0)
        .map(|w| w.segs)
        .collect()
}

/// Контур грани по полурёбрам.
fn face_path(net: &VectorNetwork, face: &[usize]) -> BezPath {
    let mut path = BezPath::new();
    for (k, &h) in face.iter().enumerate() {
        let c = curve_from(
            net,
            h / 2,
            if h.is_multiple_of(2) {
                net.segments[h / 2].start
            } else {
                net.segments[h / 2].end
            },
        );
        if k == 0 {
            path.move_to(c.p0);
        }
        push_curve(&mut path, c);
    }
    path.close_path();
    path
}

/// Прямой сегмент — отрезком: поворот и расстояние у него в разы дешевле, чем у
/// кривой (у «Заливки области» на тысячах контуров это заметно).
fn push_curve(path: &mut BezPath, c: CubicBez) {
    if c.p1 == c.p0 && c.p2 == c.p3 {
        path.line_to(c.p3);
    } else {
        path.curve_to(c.p1, c.p2, c.p3);
    }
}

/// Контур петли сегментов (в порядке обхода) — для попадания в грань.
pub fn loop_path(net: &VectorNetwork, l: &[u32]) -> Option<BezPath> {
    let order = net.loop_points(l)?;
    let mut path = BezPath::new();
    path.move_to(pt(net, order[0]));
    for (k, &s) in l.iter().enumerate() {
        push_curve(&mut path, curve_from(net, s as usize, order[k]));
    }
    path.close_path();
    Some(path)
}

/// Неделимый участок, который видит «Заливка области»: внутренняя грань минус
/// внешние контуры связных частей, лежащих прямо в ней (дыры). Участки не
/// перекрываются и вместе покрывают всё, что могут залить петли сети.
struct Atom {
    /// Номер внутренней грани в обходах.
    outer: usize,
    /// Номера внешних контуров вложенных частей.
    holes: Vec<usize>,
}

impl Atom {
    fn contains(&self, walks: &[Walk], p: Point) -> bool {
        walks[self.outer].inside(p) && self.holes.iter().all(|&h| !walks[h].inside(p))
    }

    /// Петли участка: его грань и дыры (по правилу `EvenOdd` — ровно участок).
    fn loops(&self, walks: &[Walk]) -> Vec<Vec<u32>> {
        std::iter::once(self.outer)
            .chain(self.holes.iter().copied())
            .map(|i| walks[i].segs.clone())
            .collect()
    }

    /// Точка внутри участка подальше от его границ: сетка по рамке, всё гуще;
    /// из точек участка — с наибольшим расстоянием до контура.
    fn inner_point(&self, walks: &[Walk]) -> Option<Point> {
        let outer = &walks[self.outer].path;
        let b = outer.bounding_box();
        let edges: Vec<PathSeg> = std::iter::once(self.outer)
            .chain(self.holes.iter().copied())
            .flat_map(|i| walks[i].path.segments())
            .collect();
        let boxes: Vec<Rect> = edges.iter().map(|e| control_box(*e)).collect();
        let tiny = 1e-9 * (b.width() + b.height()).max(1.0);
        for n in [8u32, 32, 128] {
            let mut best: Option<(f64, Point)> = None;
            for iy in 0..n {
                for ix in 0..n {
                    let p = Point::new(
                        b.x0 + b.width() * (ix as f64 + 0.5) / n as f64,
                        b.y0 + b.height() * (iy as f64 + 0.5) / n as f64,
                    );
                    if !self.contains(walks, p) {
                        continue;
                    }
                    // Рёбра, чья рамка дальше найденного минимума, не ближе
                    // его: у рамки с тысячами дыр точный поиск — у немногих.
                    let mut clear = f64::INFINITY;
                    for (e, r) in edges.iter().zip(&boxes) {
                        let dx = (r.x0 - p.x).max(p.x - r.x1).max(0.0);
                        let dy = (r.y0 - p.y).max(p.y - r.y1).max(0.0);
                        if dx * dx + dy * dy < clear {
                            clear = clear.min(e.nearest(p, 1e-9).distance_sq);
                        }
                    }
                    // Не ближе полуклетки к контуру — достаточно далеко: дальше
                    // не ищем (тысячи участков проверяются по одной точке).
                    let half = (b.width().min(b.height()) / (2 * n) as f64).powi(2);
                    if clear >= half && clear.sqrt() > tiny {
                        return Some(p);
                    }
                    if best.is_none_or(|(c, _)| clear > c) {
                        best = Some((clear, p));
                    }
                }
            }
            if let Some((c, p)) = best {
                if c.sqrt() > tiny {
                    return Some(p);
                }
            }
        }
        None
    }
}

/// Рамка ручек сегмента (шире точной, без поиска экстремумов).
fn control_box(e: PathSeg) -> Rect {
    match e {
        PathSeg::Line(l) => Rect::from_points(l.p0, l.p1),
        PathSeg::Quad(q) => Rect::from_points(q.p0, q.p2).union_pt(q.p1),
        PathSeg::Cubic(c) => Rect::from_points(c.p0, c.p3).union_pt(c.p1).union_pt(c.p2),
    }
}

/// Неделимые участки: каждой внутренней грани — дыры из внешних контуров тех
/// связных частей, для которых она самая маленькая охватывающая грань чужой
/// части.
/// Возвращает участки и сетку их граней (номера в ней — номера обходов).
fn atoms(walks: &[Walk]) -> (Vec<Atom>, Grid, HashMap<usize, usize>) {
    let inner = (0..walks.len()).filter(|&i| walks[i].area < 0.0);
    let boxes: Vec<Rect> = walks.iter().map(|w| w.bbox).collect();
    let grid = Grid::new(&boxes, inner.clone());
    let mut out: Vec<Atom> = inner
        .map(|outer| Atom {
            outer,
            holes: Vec::new(),
        })
        .collect();
    let of: HashMap<usize, usize> = out.iter().enumerate().map(|(k, a)| (a.outer, k)).collect();
    for (o, w) in walks.iter().enumerate().filter(|(_, w)| w.area > 0.0) {
        // Охватывающие грани — только из клетки точки части.
        let host = grid
            .at(w.at)
            .iter()
            .map(|&f| f as usize)
            .filter(|&f| walks[f].part != w.part && walks[f].inside(w.at))
            .min_by(|&a, &b| walks[a].area.abs().total_cmp(&walks[b].area.abs()));
        if let Some(f) = host {
            out[of[&f]].holes.push(o);
        }
    }
    (out, grid, of)
}

/// Петли области из набора участков (правило `EvenOdd`): одинаковые петли
/// попарно сокращаются — при чётно-нечётном правиле пара ничего не меняет.
fn atom_loops<'a>(walks: &[Walk], set: impl Iterator<Item = &'a Atom>) -> Vec<Vec<u32>> {
    // Петля по набору сегментов → её место в списке (`None` — сократилась).
    let mut at: HashMap<Vec<u32>, usize> = HashMap::new();
    let mut loops: Vec<Option<Vec<u32>>> = Vec::new();
    for l in set.flat_map(|a| a.loops(walks)) {
        let mut key = l.clone();
        key.sort_unstable();
        match at.remove(&key) {
            Some(k) => loops[k] = None,
            None => {
                at.insert(key, loops.len());
                loops.push(Some(l));
            }
        }
    }
    loops.into_iter().flatten().collect()
}

/// Петли области с рамками и сеткой: закрашена ли точка — по петлям, чьи рамки
/// её содержат (вне рамки поворот петли — ноль).
struct Coverage<'a> {
    region: &'a VectorRegion,
    paths: Vec<Option<BezPath>>,
    boxes: Vec<Rect>,
    bounds: Option<Rect>,
    grid: Grid,
}

impl<'a> Coverage<'a> {
    fn new(net: &VectorNetwork, region: &'a VectorRegion) -> Coverage<'a> {
        let paths: Vec<Option<BezPath>> = region.loops.iter().map(|l| loop_path(net, l)).collect();
        let boxes: Vec<Rect> = paths
            .iter()
            .map(|p| p.as_ref().map_or(Rect::ZERO, |p| p.control_box()))
            .collect();
        let live = (0..paths.len()).filter(|&i| paths[i].is_some());
        let bounds = live.clone().map(|i| boxes[i]).reduce(|a, b| a.union(b));
        let grid = Grid::new(&boxes, live);
        Coverage {
            region,
            paths,
            boxes,
            bounds,
            grid,
        }
    }

    /// Закрашена ли точка областью: сумма поворотов её петель по её правилу.
    fn covers(&self, p: Point) -> bool {
        let w: i32 = self
            .grid
            .at(p)
            .iter()
            .map(|&i| i as usize)
            .filter(|&i| self.boxes[i].contains(p))
            .filter_map(|i| self.paths[i].as_ref())
            .map(|path| path.winding(p))
            .sum();
        match self.region.rule {
            FillRule::NonZero => w != 0,
            FillRule::EvenOdd => w.rem_euclid(2) == 1,
        }
    }
}

/// «Заливка области» (B) в точке `at` (координаты сети) переключает ровно тот
/// неделимый участок, по которому щёлкнули (грань минус вложенные в неё части),
/// остальное выглядит как прежде. Участок залит (его закрашивает какая-то
/// область) — каждая такая область переписывается участками, которые она
/// закрашивала, кроме этого (те же заливки, правило `EvenOdd`, петли — грани
/// участков и их дыры). Не залит — новая область из него с заливками узла (у
/// узла без заливок — серая, как у новой фигуры). `None` — под точкой нет
/// замкнутой грани или результат не прошёл бы проверку сети (тогда сеть не
/// меняется).
pub fn toggle_fill(net: &VectorNetwork, at: Point, node_has_fills: bool) -> Option<VectorNetwork> {
    let walks = face_walks(net);
    let (atoms, grid, of) = atoms(&walks);
    // Участки не перекрываются: щёлкнутый — среди граней из клетки точки.
    let clicked = grid
        .at(at)
        .iter()
        .map(|&f| of[&(f as usize)])
        .find(|&a| atoms[a].contains(&walks, at))?;
    let coverage: Vec<Coverage> = net.regions.iter().map(|r| Coverage::new(net, r)).collect();
    let covering: Vec<usize> = (0..net.regions.len())
        .filter(|&i| coverage[i].covers(at))
        .collect();
    let mut out = net.clone();
    if covering.is_empty() {
        out.regions.push(VectorRegion {
            rule: FillRule::EvenOdd,
            loops: atom_loops(&walks, std::iter::once(&atoms[clicked])),
            fills: (!node_has_fills).then(|| vec![Fill::solid(Color::SHAPE_GRAY)]),
        });
    } else {
        // Внутренние точки остальных участков — по ним решаем, что залито; только
        // у участков в рамке закрашивающей области и один раз на участок.
        let mut inner: Vec<Option<Option<Point>>> = vec![None; atoms.len()];
        for &ri in &covering {
            let cover = &coverage[ri];
            let Some(bounds) = cover.bounds else {
                continue;
            };
            let mut kept = Vec::new();
            for (i, a) in atoms.iter().enumerate() {
                if i == clicked || !walks[a.outer].bbox.overlaps(bounds) {
                    continue;
                }
                let p = *inner[i].get_or_insert_with(|| a.inner_point(&walks));
                if p.is_some_and(|p| cover.covers(p)) {
                    kept.push(a);
                }
            }
            let kept = kept.into_iter();
            out.regions[ri].rule = FillRule::EvenOdd;
            out.regions[ri].loops = atom_loops(&walks, kept);
        }
        out.regions.retain(|r| !r.loops.is_empty());
    }
    out.validate().is_ok().then_some(out)
}

/// Сплющивание: сети узлов (каждая — в координатах формы своего узла, с
/// матрицей формы → общие координаты) в одну сеть общих координат.
pub fn merge_networks(parts: &[(VectorNetwork, Affine)]) -> VectorNetwork {
    let mut out = VectorNetwork::default();
    for (net, a) in parts {
        let t = transform_network(net, *a);
        let (po, so) = (out.points.len() as u32, out.segments.len() as u32);
        out.points.extend(t.points);
        out.segments
            .extend(t.segments.into_iter().map(|s| VectorSegment {
                start: s.start + po,
                end: s.end + po,
                ..s
            }));
        out.regions.extend(t.regions.into_iter().map(|r| {
            VectorRegion {
                loops: r
                    .loops
                    .into_iter()
                    .map(|l| l.into_iter().map(|i| i + so).collect())
                    .collect(),
                ..r
            }
        }));
    }
    out
}

/// Путь → сеть (результат булевой группы, контуры глифов, контур обводки):
/// каждый подпуть — цепочка точек и сегментов (квадратичные кривые — кубическими),
/// замкнутые подпути — петли одной области с правилом `rule` и заливками узла.
/// Отрезки нулевой длины пропускаются; подпуть меньше двух точек — тоже.
pub fn path_network(path: &BezPath, rule: FillRule) -> VectorNetwork {
    let mut net = VectorNetwork::default();
    let mut loops = Vec::new();
    // Точки и сегменты текущего подпути: (конечная точка, ручки).
    let mut start: Option<Point> = None;
    let mut sub: Vec<(Point, [f64; 2], [f64; 2])> = Vec::new();
    let mut flush = |start: Point,
                     sub: &mut Vec<(Point, [f64; 2], [f64; 2])>,
                     closed: bool,
                     net: &mut VectorNetwork| {
        let mut items = std::mem::take(sub);
        // Замыкающий отрезок — тот же, что `close_path` дорисует сам.
        if closed {
            match items.last() {
                Some((p, _, _)) if (*p - start).hypot() < 1e-9 => {}
                _ => items.push((start, [0.0; 2], [0.0; 2])),
            }
        }
        // Замкнутый подпуть из одного сегмента (в том числе петля одной кривой)
        // сеть не принимает: сегмент с совпадающими концами недопустим.
        if items.is_empty() || (closed && items.len() < 2) {
            return;
        }
        let first = net.points.len() as u32;
        net.points.push(VectorPoint::new(start.x, start.y));
        let n = items.len();
        let mut seg_ids = Vec::with_capacity(n);
        for (k, (p, ts, te)) in items.into_iter().enumerate() {
            let from = net.points.len() as u32 - 1;
            let to = if closed && k == n - 1 {
                first
            } else {
                net.points.push(VectorPoint::new(p.x, p.y));
                net.points.len() as u32 - 1
            };
            seg_ids.push(net.segments.len() as u32);
            net.segments.push(VectorSegment {
                start: from,
                end: to,
                tangent_start: ts,
                tangent_end: te,
            });
        }
        if closed {
            loops.push(seg_ids);
        }
    };
    let mut last = Point::ZERO;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                if let Some(s) = start.take() {
                    flush(s, &mut sub, false, &mut net);
                }
                start = Some(p);
                last = p;
            }
            PathEl::LineTo(p) => {
                if (p - last).hypot() > 1e-9 {
                    sub.push((p, [0.0; 2], [0.0; 2]));
                    last = p;
                }
            }
            PathEl::QuadTo(c, p) => {
                if (p - last).hypot() > 1e-9 || (c - last).hypot() > 1e-9 {
                    let c = kurbo::QuadBez::new(last, c, p).raise();
                    sub.push((p, arr(c.p1 - c.p0), arr(c.p2 - c.p3)));
                    last = p;
                }
            }
            PathEl::CurveTo(a, b, p) => {
                if [p, a, b].iter().any(|q| (*q - last).hypot() > 1e-9) {
                    sub.push((p, arr(a - last), arr(b - p)));
                    last = p;
                }
            }
            PathEl::ClosePath => {
                if let Some(s) = start.take() {
                    flush(s, &mut sub, true, &mut net);
                    last = s;
                }
            }
        }
    }
    if let Some(s) = start.take() {
        flush(s, &mut sub, false, &mut net);
    }
    if !loops.is_empty() {
        net.regions.push(VectorRegion {
            rule,
            loops,
            fills: None,
        });
    }
    net
}
