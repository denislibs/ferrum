//! Список отрисовки узла: что рисовать в его локальных координатах. Строится из
//! свойств один раз и живёт, пока не изменилась версия узла в сцене; перемещение
//! узла или камеры его не трогает — меняется только матрица при воспроизведении.
//!
//! Контуры (`BezPath`) строятся при первом полном рисовании узла: мелкому узлу
//! (LOD) они не нужны, а скруглённый прямоугольник с кольцом обводки — около
//! 1.7 КБ, на 100 000 узлов это 170 МБ. Поэтому контуры давно не рисовавшихся
//! узлов выбрасываются (`Painter::evict`), а описание остаётся.

use std::sync::Arc;

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, ScaleMode, Stroke, StrokeAlign, StrokeJoin, StrokeStyle,
};
use ferrum_scene::shape::{self, Geometry};
use ferrum_scene::vector::{self, PathShape, StrokeClip};
use ferrum_scene::SceneNode;
use kurbo::{BezPath, Shape};

use crate::backend::Rgba;

/// Цвет изображения для LOD и заглушки, пока картинка не загружена.
pub const IMAGE_PLACEHOLDER: Rgba = Rgba::new([0.85, 0.85, 0.85, 1.0]);

#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Solid(Rgba),
    Image {
        hash: String,
        mode: ScaleMode,
        alpha: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Заливка формы узла.
    Fill(Paint),
    /// Обводка — кольцо, заливаемое по even-odd.
    Stroke(Stroke, Rgba),
    /// Самый частый случай — непрозрачная заливка и внутренняя непрозрачная
    /// обводка: форма цветом обводки, поверх — форма, сжатая на ширину обводки,
    /// цветом заливки. Два контура вместо трёх (заливка + кольцо), результат тот же.
    FilledInsideStroke {
        fill: Rgba,
        stroke: Rgba,
        width: f64,
    },
    /// Заливка области вектора номер `region` (своей заливкой или заливкой узла).
    Region { region: usize, paint: Paint },
    /// Обводка по контуру (вектор, линия с окончаниями, фигура с соединением не
    /// «острым»): контур обводки заливается, внутри и снаружи замкнутого пути —
    /// с обрезкой по нему.
    PathStroke {
        stroke: Stroke,
        color: Rgba,
        clip: StrokeClip,
    },
}

/// Контуры в локальных координатах: форма и по одному на элемент списка.
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    pub shape: BezPath,
    /// Кольцо обводки или сжатая форма; `None` — элементу хватает `shape`.
    pub items: Vec<Option<BezPath>>,
    /// Обрезка обводок по контуру: внутренность замкнутого пути и его внешность.
    pub inside: Option<BezPath>,
    pub outside: Option<BezPath>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayList {
    pub version: u64,
    pub geometry: Geometry,
    /// Снизу вверх: заливки, затем обводки (как в Figma).
    pub items: Vec<Item>,
    /// Цвет, которым мелкий узел рисуется одним прямоугольником.
    pub lod_color: Option<Rgba>,
    pub paths: Option<Paths>,
    /// Список построен для фрейма с детьми: заливка и обводка воспроизводятся
    /// порознь (первая под детьми, вторая над), поэтому они не сливаются в
    /// `FilledInsideStroke`. Наличие детей версию узла не меняет, так что список
    /// помнит, для какого случая он строился.
    pub split: bool,
    /// Номер кадра, в котором узел последний раз рисовался полностью.
    pub last_used: u64,
    /// Точность, с которой построены контуры (`paths_at`): у больших векторов —
    /// по масштабу кадра, у остальных — 0 (точно).
    pub detail: f64,
    /// Контуры для обводки по контуру и областей вектора (`None` — кольца).
    /// Держится вместе со списком: по нему контуры строятся заново после
    /// вытеснения, без свойств узла.
    pub path: Option<Arc<PathSource>>,
}

/// Контуры узла с обводкой по контуру и стиль обводки (соединение, предел угла).
#[derive(Debug, Clone, PartialEq)]
pub struct PathSource {
    pub shape: PathShape,
    pub style: StrokeStyle,
}

/// Точность упрощённых контуров больших векторов — в пикселях устройства.
pub const LOD_TOLERANCE_PX: f64 = 0.25;

pub fn color(c: Color) -> Rgba {
    Rgba::from_rgba8(c.r, c.g, c.b, c.a)
}

/// Цвет узла для LOD-прямоугольника без построения списка: первая видимая заливка,
/// иначе первая обводка.
pub fn lod_color(p: &Props) -> Option<Rgba> {
    if p.kind == NodeKind::Text {
        return Some(crate::text_node::lod_color(p));
    }
    if p.kind == NodeKind::Vector {
        // Заливка первой области (своя или узла), иначе обводка.
        let net = p.vector.as_deref()?;
        let fill = net
            .regions
            .iter()
            .flat_map(|r| r.fills.as_deref().unwrap_or(&p.fills))
            .find(|f| f.visible && f.opacity > 0.0);
        if let Some(f) = fill {
            return Some(paint_lod(f));
        }
        return p
            .strokes
            .iter()
            .find(|s| s.width > 0.0 && s.color.a > 0)
            .map(|s| color(s.color));
    }
    if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
        if let Some(f) = p.fills.iter().find(|f| f.visible && f.opacity > 0.0) {
            return Some(paint_lod(f));
        }
    }
    if p.kind == NodeKind::Group {
        return None;
    }
    p.strokes
        .iter()
        .find(|s| s.width > 0.0 && s.color.a > 0)
        .map(|s| color(s.color))
}

fn paint_lod(f: &Fill) -> Rgba {
    match f.kind {
        FillKind::Solid => color(f.color).multiply_alpha(f.opacity as f32),
        FillKind::Image => IMAGE_PLACEHOLDER.multiply_alpha(f.opacity as f32),
    }
}

/// Краска заливки; `None` — невидимая или изображение без хэша.
fn paint_of(f: &Fill) -> Option<Paint> {
    if !f.visible || f.opacity <= 0.0 {
        return None;
    }
    match (f.kind, &f.image_hash) {
        (FillKind::Solid, _) => Some(Paint::Solid(
            color(f.color).multiply_alpha(f.opacity as f32),
        )),
        (FillKind::Image, Some(hash)) => Some(Paint::Image {
            hash: hash.clone(),
            mode: f.scale_mode,
            alpha: f.opacity as f32,
        }),
        (FillKind::Image, None) => None,
    }
}

impl DisplayList {
    pub fn build(node: &SceneNode, split: bool) -> DisplayList {
        let p: &Props = &node.props;
        let geometry = shape::geometry(p);
        let path_shape = vector::path_shape(p);
        let mut items = Vec::new();
        if let (NodeKind::Vector, Some(ps)) = (p.kind, &path_shape) {
            // Области снизу вверх, у каждой — её заливки (или заливки узла).
            for (region, r) in ps.regions.iter().enumerate() {
                for f in r.fills.as_deref().unwrap_or(&p.fills) {
                    if let Some(paint) = paint_of(f) {
                        items.push(Item::Region { region, paint });
                    }
                }
            }
        } else if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
            // У линии и группы своей заливки нет.
            items.extend(p.fills.iter().filter_map(paint_of).map(Item::Fill));
        }
        if p.kind != NodeKind::Group {
            for s in p.strokes.iter().filter(|s| s.width > 0.0 && s.color.a > 0) {
                items.push(match &path_shape {
                    Some(ps) => Item::PathStroke {
                        stroke: s.clone(),
                        color: color(s.color),
                        clip: vector::stroke_clip(ps, s),
                    },
                    None => Item::Stroke(s.clone(), color(s.color)),
                });
            }
        }
        let pair = if split { &[][..] } else { &items[..] };
        if let [Item::Fill(Paint::Solid(fill)), Item::Stroke(s, stroke)] = pair {
            if s.align == StrokeAlign::Inside && fill.components[3] >= 1.0 && s.color.a == 255 {
                items = vec![Item::FilledInsideStroke {
                    fill: *fill,
                    stroke: *stroke,
                    width: s.width,
                }];
            }
        }
        DisplayList {
            version: node.version,
            geometry,
            items,
            lod_color: lod_color(p),
            paths: None,
            split,
            last_used: 0,
            detail: 0.0,
            path: path_shape.map(|shape| {
                Arc::new(PathSource {
                    shape,
                    style: p.stroke_style,
                })
            }),
        }
    }

    /// Контуры для кадра с масштабом `scale` (пикселей устройства на единицу мира):
    /// у большого вектора — упрощённые до четверти пикселя ближайшего сверху
    /// масштаба-степени двойки (перестраиваются, когда масштаб уходит за неё), у
    /// остальных — точные и одни на все масштабы. `scale = 0` — точные.
    pub fn paths_at(&mut self, scale: f64) -> &Paths {
        let detail = match self.path.as_deref() {
            Some(p)
                if scale.is_finite()
                    && scale > 0.0
                    && p.shape.stroke.elements().len() >= vector::SIMPLIFY_MIN_ELEMENTS =>
            {
                // Предел масштаба — чтобы точность не ушла в бесконечность.
                LOD_TOLERANCE_PX / 2f64.powf(scale.clamp(1e-6, 1e9).log2().ceil())
            }
            _ => 0.0,
        };
        if self.detail != detail {
            self.paths = None;
            self.detail = detail;
        }
        let geom = self.geometry;
        let items = &self.items;
        let list = &self.items;
        let style = self.path.as_deref().map(|p| p.style).unwrap_or_default();
        let full = self.path.as_deref().map(|p| &p.shape);
        self.paths.get_or_insert_with(|| {
            // Упрощение — только при построении: кадры с готовыми контурами его не платят.
            let simplified = full
                .filter(|_| detail > 0.0)
                .map(|shape| vector::simplify(shape, detail));
            let ps = simplified.as_ref().or(full);
            let mut shape = BezPath::new();
            geom.fill_path_into(&mut shape);
            let items: Vec<Option<BezPath>> = items
                .iter()
                .map(|item| match item {
                    Item::Fill(_) => None,
                    Item::Stroke(s, _) => geom.stroke_ring(s),
                    Item::FilledInsideStroke { width, .. } => geom.inset_path(*width),
                    Item::Region { region, .. } => ps.map(|ps| ps.regions[*region].path.clone()),
                    Item::PathStroke { stroke, .. } => {
                        ps.map(|ps| vector::stroke_outline(ps, stroke, &style))
                    }
                })
                .collect();
            let wants = |c: StrokeClip| {
                list.iter()
                    .any(|i| matches!(i, Item::PathStroke { clip, .. } if *clip == c))
            };
            let inside = ps
                .filter(|_| wants(StrokeClip::Inside))
                .map(|ps| ps.stroke.clone());
            // Запас рамки внешности — вся протяжённость обводки: острие стыка
            // доходит до `w · предел` (как в границах узла), кольцо — до `2w`.
            let reach = if style.join == StrokeJoin::Miter {
                style.miter_limit.max(1.0)
            } else {
                1.0
            };
            let margin = list
                .iter()
                .filter_map(|i| match i {
                    Item::PathStroke { stroke, .. } => {
                        Some((reach * stroke.width).max(2.0 * stroke.width) + 1.0)
                    }
                    _ => None,
                })
                .fold(1.0, f64::max);
            let outside = ps
                .filter(|_| wants(StrokeClip::Outside))
                .map(|ps| exterior(&ps.stroke, margin));
            Paths {
                shape,
                items,
                inside,
                outside,
            }
        })
    }
}

/// Внешность замкнутого пути: рамка с запасом `margin` и сам путь с обходом
/// против неё — при ненулевом правиле обрезки путь вырезается (как у текста
/// снаружи).
pub fn exterior(path: &BezPath, margin: f64) -> BezPath {
    let r = path.bounding_box().inflate(margin, margin);
    let mut out = BezPath::new();
    out.move_to((r.x0, r.y0));
    out.line_to((r.x1, r.y0));
    out.line_to((r.x1, r.y1));
    out.line_to((r.x0, r.y1));
    out.close_path();
    // Рамка выше обходится с положительной площадью; путь — в обратную сторону.
    if path.area() > 0.0 {
        out.extend(path.reverse_subpaths().iter());
    } else {
        out.extend(path.iter());
    }
    out
}

#[cfg(test)]
mod tests {
    use ferrum_doc::{Document, NodeKind, PropsPatch};
    use ferrum_scene::Scene;
    use std::sync::Arc;

    use super::*;

    fn vector_node(n: usize) -> SceneNode {
        let net = ferrum_doc::fixture::wavy_network(n);
        let b = vector::network_bounds(&net).unwrap();
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        let id = doc
            .transact(page, vec![], |tx| {
                let id = tx.create_node(None, None, NodeKind::Vector)?;
                tx.set_props(
                    id,
                    &PropsPatch {
                        w: Some(b.width()),
                        h: Some(b.height()),
                        fills: Some(vec![ferrum_doc::Fill::solid(ferrum_doc::Color::SHAPE_GRAY)]),
                        strokes: Some(vec![ferrum_doc::Stroke {
                            color: ferrum_doc::Color::BLACK,
                            width: 2.0,
                            align: ferrum_doc::StrokeAlign::Center,
                        }]),
                        vector: Some(Arc::new(net.clone())),
                        ..Default::default()
                    },
                )?;
                Ok(vec![id])
            })
            .unwrap()[0];
        Scene::from_page(doc.page(page).unwrap())
            .node(id)
            .unwrap()
            .clone()
    }

    /// Всего элементов в контурах заливки и обводки.
    fn size(p: &Paths) -> usize {
        p.items.iter().flatten().map(|b| b.elements().len()).sum()
    }

    fn first(p: &Paths) -> *const kurbo::PathEl {
        p.items.iter().flatten().next().unwrap().elements().as_ptr()
    }

    #[test]
    fn small_vectors_are_exact_at_any_scale() {
        let mut list = DisplayList::build(&vector_node(50), false);
        let exact = size(list.paths_at(0.0));
        assert!(exact > 0);
        for scale in [0.001, 1.0, 64.0] {
            assert_eq!(size(list.paths_at(scale)), exact);
            assert_eq!(list.detail, 0.0);
        }
    }

    #[test]
    fn big_vectors_are_simplified_to_the_scale_and_rebuilt_across_a_power_of_two() {
        let mut list = DisplayList::build(&vector_node(6000), false);
        let exact = size(list.paths_at(0.0));
        assert_eq!(list.detail, 0.0, "масштаб 0 — точные контуры");
        let coarse = size(list.paths_at(1.0));
        assert_eq!(list.detail, LOD_TOLERANCE_PX);
        assert!(coarse * 2 < exact, "{exact} → {coarse}");
        // Внутри одной степени двойки — те же контуры, без перестройки.
        let at = first(list.paths_at(0.9));
        assert_eq!(list.detail, LOD_TOLERANCE_PX);
        assert_eq!(first(list.paths_at(0.9)), at);
        // Приблизили за степень двойки — точность вдвое выше, контуры перестроены.
        let fine = size(list.paths_at(1.5));
        assert_eq!(list.detail, LOD_TOLERANCE_PX / 2.0);
        assert!(fine > coarse, "{coarse} → {fine}");
        // Крайние масштабы не падают.
        list.paths_at(f64::MIN_POSITIVE);
        list.paths_at(1e12);
        list.paths_at(f64::NAN);
        list.paths_at(f64::INFINITY);
    }
}
