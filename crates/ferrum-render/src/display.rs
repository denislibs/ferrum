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
use kurbo::{BezPath, PathEl, Point, Rect, Shape};

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
    /// Контуры построены по точной форме, хотя просили точность `detail`:
    /// упрощение ничего не дало. Точнее они не станут — приближение их не
    /// перестраивает.
    pub exact: bool,
    /// Часть узла (локальные координаты), для которой построен контур обводки:
    /// у вектора намного больше экрана — только сегменты рядом с видимой частью
    /// (`None` — весь путь).
    pub view: Option<Rect>,
    /// Контуры для обводки по контуру и областей вектора (`None` — кольца).
    /// Держится вместе со списком: по нему контуры строятся заново после
    /// вытеснения, без свойств узла.
    pub path: Option<Arc<PathSource>>,
}

/// Контуры узла с обводкой по контуру и стиль обводки (соединение, предел угла).
#[derive(Debug, Clone, PartialEq)]
pub struct PathSource {
    /// Общие с узлом сцены (`SceneNode::path_shape`): попадание и подсветка
    /// наведения строят их не заново.
    pub shape: Arc<PathShape>,
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
        let path_shape = node.path_shape().cloned();
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
            exact: false,
            view: None,
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
    ///
    /// `view` — видимая часть узла в его локальных координатах (`None` — весь
    /// узел, экспорт). У большого вектора, который намного больше экрана, контур
    /// обводки строится только по сегментам у видимой части (с запасом в экран
    /// с каждой стороны и на протяжённость обводки): правка точки при
    /// приближении не обводит все 100 000 сегментов на каждом кадре. Обрезанные
    /// концы лежат за запасом и на экран не попадают.
    pub fn paths_at(&mut self, scale: f64, view: Option<Rect>) -> &Paths {
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
        // Точные контуры годятся для любой точности тоньше той, при которой
        // они построены: перестройка на приближении дала бы те же самые.
        let exact_enough = self.exact && self.paths.is_some() && detail <= self.detail;
        if self.detail != detail && !exact_enough {
            self.paths = None;
            self.detail = detail;
        }
        let cull = self.cull_for(view);
        if cull != self.view {
            self.paths = None;
            self.view = cull;
        }
        let outset = self.reach();
        let geom = self.geometry;
        let items = &self.items;
        let list = &self.items;
        let style = self.path.as_deref().map(|p| p.style).unwrap_or_default();
        let full = self.path.as_deref().map(|p| &*p.shape);
        let exact = &mut self.exact;
        let detail = self.detail;
        self.paths.get_or_insert_with(|| {
            // Упрощение — только при построении: кадры с готовыми контурами его не платят.
            // Не уменьшило контуры — рисуется точная форма (без копии).
            //
            // Контур обводки у видимой части (`cull`): сам путь для него не
            // упрощается целиком — упрощаются только его куски у экрана и области.
            let near = cull.map(|c| c.inflate(outset, outset));
            let regions = list.iter().any(|i| matches!(i, Item::Region { .. }));
            let simplified = full
                .filter(|_| detail > 0.0 && (near.is_none() || regions))
                .map(|shape| match near {
                    Some(_) => vector::simplify_regions(shape, detail),
                    None => vector::simplify(shape, detail),
                })
                .filter(|s| full.is_some_and(|f| !same_size(s, f)));
            *exact = simplified.is_none() && near.is_none();
            let ps = simplified.as_ref().or(full);
            // Обрезка внутри и снаружи — по всему пути (заливка, не обводка).
            let clip_path = match near {
                Some(_) => full.map(|f| &f.stroke),
                None => ps.map(|ps| &ps.stroke),
            };
            let outline_shape = near.zip(full).map(|(r, f)| {
                // Куски — с той же точностью, что и весь путь (у узла меньше
                // порога упрощения окна не бывает: `cull_for`).
                let mut c = cull_shape(f, r);
                c.stroke = vector::simplify_path(&c.stroke, detail);
                c
            });
            let mut shape = BezPath::new();
            geom.fill_path_into(&mut shape);
            let items: Vec<Option<BezPath>> = items
                .iter()
                .map(|item| match item {
                    Item::Fill(_) => None,
                    Item::Stroke(s, _) => geom.stroke_ring(s),
                    Item::FilledInsideStroke { width, .. } => geom.inset_path(*width),
                    Item::Region { region, .. } => ps.map(|ps| ps.regions[*region].path.clone()),
                    Item::PathStroke { stroke, .. } => outline_shape
                        .as_ref()
                        .or(ps)
                        .map(|ps| vector::stroke_outline(ps, stroke, &style)),
                })
                .collect();
            let wants = |c: StrokeClip| {
                list.iter()
                    .any(|i| matches!(i, Item::PathStroke { clip, .. } if *clip == c))
            };
            let inside = clip_path.filter(|_| wants(StrokeClip::Inside)).cloned();
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
            let outside = clip_path
                .filter(|_| wants(StrokeClip::Outside))
                .map(|path| exterior(path, margin));
            Paths {
                shape,
                items,
                inside,
                outside,
            }
        })
    }
}

impl DisplayList {
    /// Наибольшая протяжённость обводки по контуру от пути: стык под пределом
    /// угла, кольцо, окончания (стрелка длиннее остальных фигур).
    fn reach(&self) -> f64 {
        let style = self.path.as_deref().map(|p| p.style).unwrap_or_default();
        let miter = if style.join == StrokeJoin::Miter {
            style.miter_limit.max(1.0)
        } else {
            1.0
        };
        self.items
            .iter()
            .filter_map(|i| match i {
                Item::PathStroke { stroke, .. } => {
                    let w = 2.0 * stroke.width;
                    Some((miter * w).max(w) + vector::arrow_length(w) + vector::dot_radius(w) + 1.0)
                }
                _ => None,
            })
            .fold(1.0, f64::max)
    }

    /// Часть узла, для которой строить контур обводки. Только у большого
    /// вектора с обводкой по контуру, когда его рамка не помещается в видимую
    /// часть с запасом в экран с каждой стороны. Прежняя часть ещё покрывает
    /// видимое — она же (без перестройки при прокрутке на экран).
    fn cull_for(&self, view: Option<Rect>) -> Option<Rect> {
        let v = view?;
        let p = self.path.as_deref()?;
        if self.detail <= 0.0
            || !self
                .items
                .iter()
                .any(|i| matches!(i, Item::PathStroke { .. }))
            || !(v.width().is_finite() && v.height().is_finite())
        {
            return None;
        }
        if self.view.is_some_and(|c| contains(c, v)) {
            return self.view;
        }
        let c = v.inflate(v.width(), v.height());
        let b = p.shape.stroke.control_box();
        (!contains(c, b)).then_some(c)
    }
}

fn contains(outer: Rect, inner: Rect) -> bool {
    outer.x0 <= inner.x0 && outer.y0 <= inner.y0 && outer.x1 >= inner.x1 && outer.y1 >= inner.y1
}

/// Рамка ручек сегмента.
fn seg_box(from: Point, el: PathEl) -> Rect {
    let r = Rect::from_points(from, from);
    match el {
        PathEl::LineTo(p) => r.union_pt(p),
        PathEl::QuadTo(a, p) => r.union_pt(a).union_pt(p),
        PathEl::CurveTo(a, b, p) => r.union_pt(a).union_pt(b).union_pt(p),
        _ => r,
    }
}

/// Контуры для обводки только у прямоугольника `r`: сегменты пути, чья рамка
/// ручек задевает `r`, по подпутям (замкнутый подпуть целиком — снова
/// замкнутый, разрезанный — куски, а куски через его начало сливаются в один),
/// окончания и развилки — внутри `r`. Области не нужны.
fn cull_shape(ps: &PathShape, r: Rect) -> PathShape {
    /// Подпуть, разрезанный на куски (каждый — с `MoveTo`).
    #[derive(Default)]
    struct Sub {
        start: Point,
        last: Point,
        runs: Vec<Vec<PathEl>>,
        /// Следующий оставленный сегмент продолжает последний кусок.
        joined: bool,
        whole: bool,
    }
    impl Sub {
        fn begin(p: Point) -> Sub {
            Sub {
                start: p,
                last: p,
                whole: true,
                ..Default::default()
            }
        }

        fn push(&mut self, el: PathEl, r: Rect) {
            let from = self.last;
            self.last = el.end_point().unwrap_or(from);
            if !seg_box(from, el).overlaps(r) {
                self.whole = false;
                self.joined = false;
                return;
            }
            if !self.joined {
                self.runs.push(vec![PathEl::MoveTo(from)]);
                self.joined = true;
            }
            self.runs.last_mut().expect("кусок начат").push(el);
        }

        fn finish(mut self, closed: bool, out: &mut BezPath) {
            if closed && self.whole {
                out.extend(self.runs.concat());
                out.close_path();
                return;
            }
            // Замкнутый подпуть, разрезанный не у начала: последний кусок
            // доходит до начала, первый с него начинается — один кусок, со
            // стыком в начале.
            if closed && self.runs.len() >= 2 {
                let first = matches!(self.runs[0][0], PathEl::MoveTo(p) if p == self.start);
                let last = self
                    .runs
                    .last()
                    .and_then(|r| r.last())
                    .and_then(|e| e.end_point());
                if first && last == Some(self.start) {
                    let head = self.runs.remove(0);
                    self.runs
                        .last_mut()
                        .expect("кусков два и больше")
                        .extend(head.into_iter().skip(1));
                }
            }
            for run in self.runs {
                out.extend(run);
            }
        }
    }
    let mut stroke = BezPath::new();
    let mut sub: Option<Sub> = None;
    for &el in ps.stroke.elements() {
        match el {
            PathEl::MoveTo(p) => {
                if let Some(s) = sub.take() {
                    s.finish(false, &mut stroke);
                }
                sub = Some(Sub::begin(p));
            }
            PathEl::ClosePath => {
                if let Some(mut s) = sub.take() {
                    if s.last != s.start {
                        s.push(PathEl::LineTo(s.start), r);
                    }
                    s.finish(true, &mut stroke);
                }
            }
            e => {
                if let Some(s) = sub.as_mut() {
                    s.push(e, r);
                }
            }
        }
    }
    if let Some(s) = sub {
        s.finish(false, &mut stroke);
    }
    PathShape {
        stroke,
        regions: Vec::new(),
        ends: ps
            .ends
            .iter()
            .filter(|e| r.contains(e.point))
            .cloned()
            .collect(),
        junctions: ps
            .junctions
            .iter()
            .copied()
            .filter(|j| r.contains(*j))
            .collect(),
        closed: ps.closed,
    }
}

/// Упрощение не убрало ни одного элемента (`simplify` монотонно: не больше точного).
fn same_size(a: &PathShape, b: &PathShape) -> bool {
    a.stroke.elements().len() == b.stroke.elements().len()
        && a.regions
            .iter()
            .zip(&b.regions)
            .all(|(x, y)| x.path.elements().len() == y.path.elements().len())
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
        node_of(ferrum_doc::fixture::wavy_network(n))
    }

    fn node_of(net: ferrum_doc::VectorNetwork) -> SceneNode {
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
        let exact = size(list.paths_at(0.0, None));
        assert!(exact > 0);
        for scale in [0.001, 1.0, 64.0] {
            assert_eq!(size(list.paths_at(scale, None)), exact);
            assert_eq!(list.detail, 0.0);
        }
    }

    #[test]
    fn big_vectors_are_simplified_to_the_scale_and_rebuilt_across_a_power_of_two() {
        let mut list = DisplayList::build(&vector_node(6000), false);
        let exact = size(list.paths_at(0.0, None));
        assert_eq!(list.detail, 0.0, "масштаб 0 — точные контуры");
        let coarse = size(list.paths_at(1.0, None));
        assert_eq!(list.detail, LOD_TOLERANCE_PX);
        assert!(coarse * 2 < exact, "{exact} → {coarse}");
        // Внутри одной степени двойки — те же контуры, без перестройки.
        let at = first(list.paths_at(0.9, None));
        assert_eq!(list.detail, LOD_TOLERANCE_PX);
        assert_eq!(first(list.paths_at(0.9, None)), at);
        // Приблизили за степень двойки — точность вдвое выше, контуры перестроены.
        let fine = size(list.paths_at(1.5, None));
        assert_eq!(list.detail, LOD_TOLERANCE_PX / 2.0);
        assert!(fine > coarse, "{coarse} → {fine}");
        // Крайние масштабы не падают.
        list.paths_at(f64::MIN_POSITIVE, None);
        list.paths_at(1e12, None);
        list.paths_at(f64::NAN, None);
        list.paths_at(f64::INFINITY, None);
    }

    #[test]
    fn curved_vectors_are_never_bigger_than_exact_and_not_rebuilt_once_exact() {
        // Каждый сегмент — выгнутая кривая: на приближении упрощать нечего.
        let mut list = DisplayList::build(
            &node_of(ferrum_doc::fixture::curvy_network(20_000, 2000.0)),
            false,
        );
        let exact = size(list.paths_at(0.0, None));
        let far = size(list.paths_at(0.05, None));
        assert!(far * 4 < exact, "отдаление прореживает: {exact} → {far}");
        let mut built = None;
        for scale in [8.0, 64.0, 512.0, 1e6, 1e12] {
            let n = size(list.paths_at(scale, None));
            assert!(n <= exact, "масштаб {scale}: {n} > {exact}");
            if n == exact {
                let at = first(list.paths_at(scale, None));
                // Дальнейшее приближение не перестраивает точные контуры.
                assert!(built.is_none_or(|b| b == at), "масштаб {scale}");
                built = Some(at);
            }
        }
        assert!(built.is_some() && list.exact);
        // Отдаление — снова упрощённые.
        assert_eq!(size(list.paths_at(0.05, None)), far);
        assert!(!list.exact);
    }

    #[test]
    fn zoomed_in_big_vectors_outline_only_near_the_view_and_match_the_whole_outline_there() {
        use kurbo::{ParamCurve, Vec2};
        for net in [
            ferrum_doc::fixture::curvy_network(5000, 500.0),
            ferrum_doc::fixture::wavy_network(5000),
        ] {
            let node = node_of(net);
            let shape = node.path_shape().unwrap().clone();
            let b = shape.stroke.control_box();
            // Окно у правого края: контур пересекает его границы.
            let view = Rect::new(
                b.x1 - 40.0,
                b.center().y - 20.0,
                b.x1 + 10.0,
                b.center().y + 20.0,
            );
            let stroke_item = |l: &DisplayList| {
                l.items
                    .iter()
                    .position(|i| matches!(i, Item::PathStroke { .. }))
                    .unwrap()
            };
            let mut list = DisplayList::build(&node, false);
            let k = stroke_item(&list);
            let culled = list.paths_at(64.0, Some(view)).items[k].clone().unwrap();
            assert!(list.view.is_some(), "окно меньше узла — контур у окна");
            let mut full = DisplayList::build(&node, false);
            let whole = full.paths_at(64.0, None).items[k].clone().unwrap();
            assert!(full.view.is_none());
            assert!(
                culled.elements().len() * 10 < whole.elements().len(),
                "{} из {}",
                culled.elements().len(),
                whole.elements().len()
            );
            let culled_size = culled.elements().len();
            assert!(culled_size > 0);
            // В окне (и у его краёв) — те же пиксели: внутри обводки, на её
            // краю и снаружи. Поворот — по замкнутым подпутям контура, чья
            // рамка содержит точку (у остальных он ноль).
            let pieces = |path: &BezPath| -> Vec<(Rect, BezPath)> {
                let mut out: Vec<(Rect, BezPath)> = Vec::new();
                for el in path.elements() {
                    if matches!(el, PathEl::MoveTo(_)) {
                        out.push((Rect::ZERO, BezPath::new()));
                    }
                    out.last_mut().unwrap().1.push(*el);
                }
                out.into_iter().map(|(_, p)| (p.control_box(), p)).collect()
            };
            let inside = |pieces: &[(Rect, BezPath)], q: Point| {
                pieces
                    .iter()
                    .filter(|(b, _)| b.contains(q))
                    .map(|(_, p)| p.winding(q))
                    .sum::<i32>()
                    != 0
            };
            let (culled, whole) = (pieces(&culled), pieces(&whole));
            let (mut checked, mut seen) = (0, 0);
            for seg in shape.stroke.segments() {
                if !seg.bounding_box().overlaps(view.inflate(3.0, 3.0)) {
                    continue;
                }
                for t in [0.0, 0.5] {
                    let p = seg.eval(t);
                    let d = seg.eval(t + 1e-3) - p;
                    let n = if d.hypot() > 0.0 {
                        Vec2::new(-d.y, d.x) / d.hypot()
                    } else {
                        Vec2::new(0.0, 1.0)
                    };
                    for off in [0.0, 0.5, -0.5, 2.5, -2.5] {
                        let q = p + n * off;
                        if !view.contains(q) {
                            continue;
                        }
                        // Каждая седьмая точка: поворот по контуру всего пути
                        // в отладочной сборке дорог.
                        seen += 1;
                        if seen % 7 != 0 {
                            continue;
                        }
                        checked += 1;
                        assert_eq!(
                            inside(&culled, q),
                            inside(&whole, q),
                            "{q:?} (смещение {off})"
                        );
                    }
                }
            }
            assert!(checked > 50, "проверено {checked}");
            // Сдвиг окна в пределах запаса — те же контуры, без перестройки.
            let at = first(list.paths_at(64.0, Some(view)));
            let moved = view + Vec2::new(10.0, 5.0);
            assert_eq!(first(list.paths_at(64.0, Some(moved))), at);
            // Отдаление, где узел целиком в окне, — снова весь контур.
            list.paths_at(0.5, Some(b.inflate(10.0, 10.0)));
            assert!(list.view.is_none());
        }
    }
}
