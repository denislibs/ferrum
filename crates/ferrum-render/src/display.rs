//! Список отрисовки узла: что рисовать в его локальных координатах. Строится из
//! свойств один раз и живёт, пока не изменилась версия узла в сцене; перемещение
//! узла или камеры его не трогает — меняется только матрица при воспроизведении.
//!
//! Контуры (`BezPath`) строятся при первом полном рисовании узла: мелкому узлу
//! (LOD) они не нужны, а скруглённый прямоугольник с кольцом обводки — около
//! 1.7 КБ, на 100 000 узлов это 170 МБ. Поэтому контуры давно не рисовавшихся
//! узлов выбрасываются (`Painter::evict`), а описание остаётся.

use ferrum_doc::{Color, FillKind, NodeKind, Props, ScaleMode, Stroke, StrokeAlign};
use ferrum_scene::shape::{self, Geometry};
use ferrum_scene::SceneNode;
use kurbo::BezPath;

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
}

/// Контуры в локальных координатах: форма и по одному на элемент списка.
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    pub shape: BezPath,
    /// Кольцо обводки или сжатая форма; `None` — элементу хватает `shape`.
    pub items: Vec<Option<BezPath>>,
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
    if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
        if let Some(f) = p.fills.iter().find(|f| f.visible && f.opacity > 0.0) {
            return Some(match f.kind {
                FillKind::Solid => color(f.color).multiply_alpha(f.opacity as f32),
                FillKind::Image => IMAGE_PLACEHOLDER.multiply_alpha(f.opacity as f32),
            });
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

impl DisplayList {
    pub fn build(node: &SceneNode, split: bool) -> DisplayList {
        let p: &Props = &node.props;
        let geometry = shape::geometry(p);
        let mut items = Vec::new();
        // У линии и группы своей заливки нет.
        let fillable = !matches!(p.kind, NodeKind::Line | NodeKind::Group);
        if fillable {
            for f in p.fills.iter().filter(|f| f.visible && f.opacity > 0.0) {
                let paint = match (f.kind, &f.image_hash) {
                    (FillKind::Solid, _) => {
                        Paint::Solid(color(f.color).multiply_alpha(f.opacity as f32))
                    }
                    (FillKind::Image, Some(hash)) => Paint::Image {
                        hash: hash.clone(),
                        mode: f.scale_mode,
                        alpha: f.opacity as f32,
                    },
                    (FillKind::Image, None) => continue,
                };
                items.push(Item::Fill(paint));
            }
        }
        if p.kind != NodeKind::Group {
            for s in p.strokes.iter().filter(|s| s.width > 0.0 && s.color.a > 0) {
                items.push(Item::Stroke(s.clone(), color(s.color)));
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
        }
    }

    /// Контуры, построенные при первом обращении.
    pub fn paths(&mut self) -> &Paths {
        let geom = self.geometry;
        let items = &self.items;
        self.paths.get_or_insert_with(|| {
            let mut shape = BezPath::new();
            geom.fill_path_into(&mut shape);
            let items = items
                .iter()
                .map(|item| match item {
                    Item::Fill(_) => None,
                    Item::Stroke(s, _) => geom.stroke_ring(s),
                    Item::FilledInsideStroke { width, .. } => geom.inset_path(*width),
                })
                .collect();
            Paths { shape, items }
        })
    }
}
