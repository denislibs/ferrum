//! Булевы операции (спека подсистемы 3, раздел 10): форма заливки узла как набор
//! контуров-ломаных и операция над формами детей булевой группы.
//!
//! Библиотека — `i_overlay` по раздроблённым кривым (выбор замером в плане
//! этапа 4): на 1000 перекрывающихся кругах она укладывается в ~20 мс, а
//! `linesweeper` (операции над кривыми без дробления) — 70–90 мс, больше
//! предела 50 мс; на касаниях и совпадающих рёбрах обе дают одно и то же.
//! Кривые дробятся с точностью `BOOLEAN_TOLERANCE` в координатах узла, поэтому
//! результат — ломаные (кривые сохраняются только у «Контура обводки»).
//!
//! Каждая форма сначала приводится к простым контурам своим правилом заливки
//! (внешние — по одну сторону, дыры — по другую), поэтому число перекрытий в
//! точке равно числу форм, её покрывающих: объединение — хотя бы одна,
//! пересечение — все, исключение — нечётное число, вычитание — нижняя без
//! остальных (как в Figma).

use ferrum_doc::{BooleanOp, FillRule};
use i_overlay::core::fill_rule::FillRule as IFillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::overlay::FloatOverlay;
use kurbo::{Affine, BezPath, PathEl, Point, Rect};

/// Точность дробления кривых в координатах узла: 0,005 — четверть пикселя
/// при приближении ×50; круг радиуса 100 — около 300 отрезков.
pub const BOOLEAN_TOLERANCE: f64 = 0.005;

/// Контур — замкнутая ломаная (последняя точка не повторяет первую).
pub type Contour = Vec<[f64; 2]>;

/// Форма заливки: простые контуры без самопересечений, внешние и дыры с
/// противоположным обходом (правило заливки для них не важно).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Contours(pub Vec<Contour>);

impl Contours {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Число точек всех контуров.
    pub fn points(&self) -> usize {
        self.0.iter().map(Vec::len).sum()
    }

    /// Форма под аффинным преобразованием. Отражение обратило бы обход
    /// контуров, и форма в объединении с другими вычиталась бы из них: обход
    /// тогда возвращается разворотом каждого контура.
    pub fn transformed(&self, a: Affine) -> Contours {
        if a == Affine::IDENTITY {
            return self.clone();
        }
        let flip = a.determinant() < 0.0;
        Contours(
            self.0
                .iter()
                .map(|c| {
                    let mut out: Contour = c
                        .iter()
                        .map(|p| {
                            let q = a * Point::new(p[0], p[1]);
                            [q.x, q.y]
                        })
                        .collect();
                    if flip {
                        out.reverse();
                    }
                    out
                })
                .collect(),
        )
    }

    /// Контуры как путь (подпути замкнуты).
    pub fn to_path(&self) -> BezPath {
        let mut out = BezPath::new();
        for c in &self.0 {
            let Some((first, rest)) = c.split_first() else {
                continue;
            };
            out.move_to((first[0], first[1]));
            for p in rest {
                out.line_to((p[0], p[1]));
            }
            out.close_path();
        }
        out
    }

    /// Рамка всех точек; `None` — форма пуста.
    pub fn bounds(&self) -> Option<Rect> {
        self.0
            .iter()
            .flatten()
            .map(|p| Rect::from_points((p[0], p[1]), (p[0], p[1])))
            .reduce(|a, b| a.union(b))
    }
}

/// Контуры пути: кривые дробятся с точностью `tolerance`, открытые подпути
/// замыкаются (как заливка в рендере), вырожденные (меньше трёх точек и
/// нечисловые) пропускаются.
pub fn flatten_path(path: &BezPath, tolerance: f64) -> Vec<Contour> {
    let mut out: Vec<Contour> = Vec::new();
    let mut cur: Contour = Vec::new();
    let finish = |cur: &mut Contour, out: &mut Vec<Contour>| {
        if cur.len() > 1 && cur.first() == cur.last() {
            cur.pop();
        }
        let ok = cur.len() >= 3 && cur.iter().all(|p| p[0].is_finite() && p[1].is_finite());
        if ok {
            out.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    kurbo::flatten(path.iter(), tolerance, |el| match el {
        PathEl::MoveTo(p) => {
            finish(&mut cur, &mut out);
            cur.push([p.x, p.y]);
        }
        PathEl::LineTo(p) => cur.push([p.x, p.y]),
        PathEl::ClosePath => finish(&mut cur, &mut out),
        // `flatten` отдаёт только отрезки.
        _ => {}
    });
    finish(&mut cur, &mut out);
    out
}

fn ifill(rule: FillRule) -> IFillRule {
    match rule {
        FillRule::NonZero => IFillRule::NonZero,
        FillRule::EvenOdd => IFillRule::EvenOdd,
    }
}

fn collect(shapes: Vec<Vec<Contour>>) -> Contours {
    Contours(shapes.into_iter().flatten().collect())
}

/// Приводит контуры с правилом `rule` к простым (`Contours`). Один контур
/// без самопересечений (прямоугольник, эллипс) возвращается как есть, только с
/// обходом, общим у всех простых форм, — без запуска операции.
pub fn normalize(contours: Vec<Contour>, rule: FillRule) -> Contours {
    if contours.is_empty() {
        return Contours::default();
    }
    if let [one] = &contours[..] {
        if is_simple_convex(one) {
            let mut c = one.clone();
            if signed_area(&c) < 0.0 {
                c.reverse();
            }
            return Contours(vec![c]);
        }
    }
    collect(FloatOverlay::with_subj(&contours).overlay(OverlayRule::Subject, ifill(rule)))
}

/// Удвоенная площадь со знаком (положительная — обход по часовой на экране с
/// осью Y вниз).
fn signed_area(c: &[[f64; 2]]) -> f64 {
    let n = c.len();
    (0..n)
        .map(|i| {
            let (p, q) = (c[i], c[(i + 1) % n]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum()
}

/// Выпуклый контур без самопересечений: все повороты в одну сторону и обход
/// — один оборот. Такой форме операция приведения не нужна.
fn is_simple_convex(c: &[[f64; 2]]) -> bool {
    let n = c.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0f64;
    let mut turn = 0.0f64;
    for i in 0..n {
        let (a, b, d) = (c[i], c[(i + 1) % n], c[(i + 2) % n]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1]], [d[0] - b[0], d[1] - b[1]]);
        let cross = u[0] * v[1] - u[1] * v[0];
        if cross.abs() > 1e-12 {
            if sign != 0.0 && cross.signum() != sign {
                return false;
            }
            sign = cross.signum();
        }
        turn += cross.atan2(u[0] * v[0] + u[1] * v[1]);
    }
    (turn.abs() - std::f64::consts::TAU).abs() < 1e-6
}

/// Операция над формами в порядке слоёв снизу вверх. Пустые формы (линии,
/// открытые пути, пустые группы) уже отброшены вызывающим: основа вычитания —
/// нижняя непустая.
pub fn combine(op: BooleanOp, inputs: &[Contours]) -> Contours {
    let all = |from: usize| -> Vec<Contour> {
        inputs[from..]
            .iter()
            .flat_map(|c| c.0.iter().cloned())
            .collect()
    };
    match (op, inputs) {
        (_, []) => Contours::default(),
        // Одна форма: объединение, исключение и пересечение — она сама.
        (BooleanOp::Subtract, [one]) | (_, [one]) => one.clone(),
        (BooleanOp::Union, _) => collect(
            FloatOverlay::with_subj(&all(0)).overlay(OverlayRule::Subject, IFillRule::NonZero),
        ),
        (BooleanOp::Exclude, _) => collect(
            FloatOverlay::with_subj(&all(0)).overlay(OverlayRule::Subject, IFillRule::EvenOdd),
        ),
        (BooleanOp::Subtract, [base, ..]) => collect(
            FloatOverlay::with_subj_and_clip(&base.0, &all(1))
                .overlay(OverlayRule::Difference, IFillRule::NonZero),
        ),
        (BooleanOp::Intersect, [first, rest @ ..]) => {
            let mut acc = first.clone();
            for next in rest {
                if acc.is_empty() {
                    break;
                }
                acc = collect(
                    FloatOverlay::with_subj_and_clip(&acc.0, &next.0)
                        .overlay(OverlayRule::Intersect, IFillRule::NonZero),
                );
            }
            acc
        }
    }
}

/// Форма заливки узла-листа в его локальных координатах (без детей): фигура —
/// её контур (у фрейма и изображения — прямоугольник), вектор — его области
/// каждая своим правилом (`path` — контуры узла, `vector::path_shape`), текст —
/// контуры глифов `text` (правило ненулевое). У линии, открытого пути без
/// областей и группы формы нет. Обводки детей в булеву операцию не входят (как
/// в Figma).
pub fn leaf_fill(
    p: &ferrum_doc::Props,
    path: Option<&crate::vector::PathShape>,
    text: Option<&BezPath>,
) -> Contours {
    use crate::shape::{geometry, Geometry};
    use ferrum_doc::NodeKind;
    match p.kind {
        NodeKind::Vector => {
            let Some(shape) = path else {
                return Contours::default();
            };
            let parts: Vec<Contours> = shape
                .regions
                .iter()
                .map(|r| normalize(flatten_path(&r.path, BOOLEAN_TOLERANCE), r.rule))
                .filter(|c| !c.is_empty())
                .collect();
            combine(BooleanOp::Union, &parts)
        }
        NodeKind::Text => match text {
            Some(t) => normalize(flatten_path(t, BOOLEAN_TOLERANCE), FillRule::NonZero),
            None => Contours::default(),
        },
        _ => match geometry(p) {
            g @ (Geometry::Rect { .. } | Geometry::Ellipse(_)) => {
                let path = g.fill_path().unwrap_or_default();
                normalize(flatten_path(&path, BOOLEAN_TOLERANCE), FillRule::NonZero)
            }
            _ => Contours::default(),
        },
    }
}

/// Результат булевой группы для рендера и попадания: контуры и форма для
/// обводки по контуру (одна область — сам результат, путь замкнут).
pub fn outline_shape(c: &Contours) -> crate::vector::PathShape {
    let path = c.to_path();
    crate::vector::PathShape {
        stroke: path.clone(),
        regions: vec![crate::vector::RegionPath {
            path,
            rule: FillRule::NonZero,
            fills: None,
        }],
        ends: vec![],
        junctions: vec![],
        closed: true,
    }
}
