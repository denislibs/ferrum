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
    Color, Fill, FillKind, NodeKind, Props, ScaleMode, Stroke, StrokeAlign, StrokeStyle,
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
            path: path_shape.map(|shape| {
                Arc::new(PathSource {
                    shape,
                    style: p.stroke_style,
                })
            }),
        }
    }

    /// Контуры, построенные при первом обращении.
    pub fn paths(&mut self) -> &Paths {
        let geom = self.geometry;
        let items = &self.items;
        let list = &self.items;
        let ps = self.path.as_deref().map(|p| &p.shape);
        let style = self.path.as_deref().map(|p| p.style).unwrap_or_default();
        self.paths.get_or_insert_with(|| {
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
            // Запас рамки внешности — шире самой толстой обводки.
            let margin = list
                .iter()
                .filter_map(|i| match i {
                    Item::PathStroke { stroke, .. } => Some(2.0 * stroke.width + 1.0),
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
