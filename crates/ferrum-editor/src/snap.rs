//! Привязки к краям и центрам соседей и родительского фрейма (раздел 7.3 спеки).
//!
//! Кандидаты ищутся не перебором страницы, а двумя запросами к R-tree сцены — по
//! полосам вокруг двигающейся рамки в пределах видимой части холста. Вертикальная
//! полоса `[min x − порог, max x + порог] × видимая высота` содержит каждый узел,
//! у которого есть край или центр ближе порога к одному из двигающихся краёв по x:
//! такая координата лежит внутри полосы, значит и сам узел её пересекает. Поэтому
//! полосы находят ровно все привязки в видимой части, а работа зависит от числа
//! узлов рядом, а не от размера страницы.

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Rect};

use crate::geometry::world_box;

/// Порог привязки в экранных пикселях.
pub const SNAP_THRESHOLD_PX: f64 = 4.0;
const EPS: f64 = 1e-6;

/// Результат привязки: сдвиги по осям (если привязка сработала) и линии-подсказки.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snap {
    pub dx: Option<f64>,
    pub dy: Option<f64>,
    pub lines: Vec<Line>,
}

/// Кого и к чему привязывать в одном жесте.
#[derive(Debug, Clone)]
pub struct Snapper {
    /// Узлы жеста и черновики: к себе не привязываются.
    excluded: HashSet<NodeId>,
    /// Родители двигающихся узлов: соседи — их дети (`None` — корень страницы).
    parents: HashSet<Option<NodeId>>,
    /// Рамки родительских фреймов в мире: к их краям и центру тоже привязываемся.
    frames: Vec<Rect>,
}

impl Snapper {
    /// Для узлов `moving`: соседи — дети их родителей.
    pub fn new(scene: &Scene, moving: &[NodeId]) -> Snapper {
        let mut parents = HashSet::new();
        for id in moving {
            if let Some(s) = scene.slot(*id) {
                parents.insert(scene.at(s).parent.map(|p| scene.at(p).id));
            }
        }
        Snapper::with_parents(scene, parents, moving)
    }

    /// Для новой фигуры у родителя `parent`.
    pub fn for_parent(scene: &Scene, parent: Option<NodeId>, exclude: &[NodeId]) -> Snapper {
        Snapper::with_parents(scene, HashSet::from([parent]), exclude)
    }

    fn with_parents(
        scene: &Scene,
        parents: HashSet<Option<NodeId>>,
        exclude: &[NodeId],
    ) -> Snapper {
        let mut frames: Vec<Rect> = parents
            .iter()
            .flatten()
            .filter_map(|p| scene.slot(*p))
            .filter(|s| scene.at(*s).props.kind == NodeKind::Frame)
            .filter_map(|s| world_box(scene, s))
            .collect();
        frames.sort_by(|a, b| {
            (a.x0, a.y0, a.x1, a.y1)
                .partial_cmp(&(b.x0, b.y0, b.x1, b.y1))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Snapper {
            excluded: exclude.iter().copied().collect(),
            parents,
            frames,
        }
    }

    /// Можно ли привязываться к узлу: сосед, видим, не участвует в жесте.
    fn eligible(&self, scene: &Scene, slot: Slot) -> bool {
        let n = scene.at(slot);
        if self.excluded.contains(&n.id) || scene.is_draft(n.id) {
            return false;
        }
        let parent = n.parent.map(|p| scene.at(p).id);
        if !self.parents.contains(&parent) || !scene.is_visible(slot) {
            return false;
        }
        // Потомок двигающегося узла едет вместе с ним.
        let mut cur = n.parent;
        while let Some(p) = cur {
            if self.excluded.contains(&scene.at(p).id) {
                return false;
            }
            cur = scene.at(p).parent;
        }
        true
    }

    /// Рамки кандидатов, пересекающих полосу.
    fn candidates(&self, scene: &Scene, band: Rect) -> Vec<Rect> {
        let mut out: Vec<Rect> = scene
            .query_rect(band)
            .into_iter()
            .filter_map(|id| scene.slot(id))
            .filter(|s| self.eligible(scene, *s))
            .filter_map(|s| world_box(scene, s))
            .collect();
        out.extend(self.frames.iter().copied());
        out
    }

    /// Привязывает двигающиеся координаты `xs` и `ys` (края и центр рамки или точка
    /// ручки) к ближайшим краям и центрам соседей не дальше `threshold` (в мире).
    /// `moving` — рамка жеста до привязки (для длины линий), `view` — видимая
    /// часть мира. Пустой список координат выключает ось.
    pub fn snap(
        &self,
        scene: &Scene,
        xs: &[f64],
        ys: &[f64],
        moving: Rect,
        threshold: f64,
        view: Rect,
    ) -> Snap {
        let span = |v: &[f64]| {
            v.iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                    (lo.min(*x), hi.max(*x))
                })
        };
        let x_cands = if xs.is_empty() {
            Vec::new()
        } else {
            let (lo, hi) = span(xs);
            self.candidates(
                scene,
                Rect::new(lo - threshold, view.y0, hi + threshold, view.y1),
            )
        };
        let y_cands = if ys.is_empty() {
            Vec::new()
        } else {
            let (lo, hi) = span(ys);
            self.candidates(
                scene,
                Rect::new(view.x0, lo - threshold, view.x1, hi + threshold),
            )
        };
        let xt = |b: &Rect| [b.x0, (b.x0 + b.x1) / 2.0, b.x1];
        let yt = |b: &Rect| [b.y0, (b.y0 + b.y1) / 2.0, b.y1];
        let best = |vals: &[f64], cands: &[Rect], targets: &dyn Fn(&Rect) -> [f64; 3]| {
            let mut best: Option<f64> = None;
            for b in cands {
                for t in targets(b) {
                    for m in vals {
                        let d = t - m;
                        if d.abs() <= threshold && best.is_none_or(|x| d.abs() < x.abs()) {
                            best = Some(d);
                        }
                    }
                }
            }
            best
        };
        let dx = best(xs, &x_cands, &xt);
        let dy = best(ys, &y_cands, &yt);
        let moved = moving + kurbo::Vec2::new(dx.unwrap_or(0.0), dy.unwrap_or(0.0));
        let mut lines = Vec::new();
        if let Some(d) = dx {
            let hits: Vec<f64> = xs.iter().map(|m| m + d).collect();
            lines.extend(guide_lines(&hits, &x_cands, moved, true));
        }
        if let Some(d) = dy {
            let hits: Vec<f64> = ys.iter().map(|m| m + d).collect();
            lines.extend(guide_lines(&hits, &y_cands, moved, false));
        }
        Snap { dx, dy, lines }
    }
}

/// Линии привязки: по одной на каждую совпавшую координату, от двигающейся рамки
/// до самого дальнего совпавшего соседа.
fn guide_lines(values: &[f64], cands: &[Rect], moving: Rect, vertical: bool) -> Vec<Line> {
    let mut out: Vec<(f64, f64, f64)> = Vec::new();
    for b in cands {
        let targets = if vertical {
            [b.x0, (b.x0 + b.x1) / 2.0, b.x1]
        } else {
            [b.y0, (b.y0 + b.y1) / 2.0, b.y1]
        };
        for t in targets {
            if !values.iter().any(|v| (v - t).abs() < EPS) {
                continue;
            }
            let (lo, hi) = if vertical {
                (moving.y0.min(b.y0), moving.y1.max(b.y1))
            } else {
                (moving.x0.min(b.x0), moving.x1.max(b.x1))
            };
            match out.iter_mut().find(|(v, _, _)| (v - t).abs() < EPS) {
                Some(l) => {
                    l.1 = l.1.min(lo);
                    l.2 = l.2.max(hi);
                }
                None => out.push((t, lo, hi)),
            }
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out.into_iter()
        .map(|(v, lo, hi)| {
            if vertical {
                Line::new((v, lo), (v, hi))
            } else {
                Line::new((lo, v), (hi, v))
            }
        })
        .collect()
}
