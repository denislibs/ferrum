//! Сборка кадра: обход сцены с отсечением невидимого и LOD, растровый кэш
//! мелких фреймов верхнего уровня, воспроизведение списков отрисовки в бэкенд.

use std::collections::HashMap;
use std::sync::Arc;

use ferrum_doc::{NodeId, NodeKind, ScaleMode};
use ferrum_scene::shape::Geometry;
use ferrum_scene::{Scene, SceneNode, Slot};
use kurbo::{Affine, BezPath, Point, Rect};
use vello_cpu::peniko::{Extend, Fill};
use vello_cpu::Pixmap;

use crate::backend::{CpuBackend, ImagePaint, RenderBackend};
use crate::display::{self, DisplayList, Item, Paint, IMAGE_PLACEHOLDER};
use crate::image::ImageProvider;

/// Поддерево меньше полупикселя не рисуется вовсе.
pub const LOD_SKIP_PX: f64 = 0.5;
/// Узел меньше трёх пикселей рисуется одним прямоугольником цвета первой заливки:
/// скругления и обводки на таком размере не различимы, а разбор пути дороже.
pub const LOD_RECT_PX: f64 = 3.0;
/// Контейнер верхнего уровня не больше этого размера на экране рисуется из
/// растрового кэша: при сильном отдалении на странице тысячи фреймов по сотне
/// мелких узлов, и даже прямоугольники LOD не укладываются в кадр.
pub const RASTER_MAX_PX: f64 = 256.0;
/// Растровый кэш — только при сильном отдалении (пикселей устройства на единицу
/// мира): вблизи узлы рисуются дёшево, а любая правка перестраивала бы растр.
pub const RASTER_MAX_SCALE: f64 = 0.25;
/// Сколько растровых кэшей строить за кадр; остальные фреймы этого кадра рисуются
/// узлами, а кэш догоняет в следующих кадрах.
pub const RASTER_BUILDS_PER_FRAME: usize = 32;
/// Через сколько кадров без отрисовки выбрасываются контуры узла и растр фрейма.
pub const EVICT_AFTER_FRAMES: u64 = 300;

/// Что произошло за кадр — для тестов, бенчмарка и статистики в интерфейсе.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    /// Узлы, до которых дошёл обход.
    pub visited: usize,
    /// Поддеревья, отброшенные как невидимые во вьюпорте.
    pub culled: usize,
    /// Поддеревья меньше `LOD_SKIP_PX`.
    pub too_small: usize,
    /// Узлы, нарисованные полностью.
    pub drawn: usize,
    /// Узлы, нарисованные прямоугольником LOD.
    pub lod_rects: usize,
    /// Списки отрисовки, построенные заново (новые или изменённые узлы).
    pub lists_built: usize,
    /// Контейнеры, нарисованные из растрового кэша, и построенные за кадр кэши.
    pub rasters: usize,
    pub raster_builds: usize,
    pub clips: usize,
    pub layers: usize,
}

/// Параметры обхода.
pub struct DrawContext<'a> {
    pub scene: &'a Scene,
    pub images: &'a dyn ImageProvider,
    /// Мир → пиксели устройства.
    pub transform: Affine,
    /// Пикселей устройства на единицу мира.
    pub scale: f64,
    /// Видимая часть мира; `None` — рисовать всё (экспорт).
    pub cull: Option<Rect>,
    pub lod: bool,
    /// Можно ли рисовать контейнеры из растрового кэша (бэкенд умеет изображения).
    pub raster: bool,
    /// Сохранять ли построенные списки отрисовки и контуры. При построении растра
    /// и экспорте узлы рисуются один раз: кэшировать их контуры значит держать
    /// в памяти всю страницу (сотни мегабайт), которую на экране никто не видит.
    pub retain: bool,
}

enum Visit {
    Enter(Slot),
    Exit { clip: bool, layer: bool },
}

/// Растр поддерева контейнера в мировых координатах.
struct Raster {
    id: NodeId,
    subtree_version: u64,
    /// Пикселей растра на единицу мира — степень двойки.
    scale: f64,
    origin: Point,
    pixmap: Arc<Pixmap>,
    last_used: u64,
}

/// Кэши между кадрами: списки отрисовки по слотам сцены, растры фреймов, общий
/// буфер для контуров обрезки.
#[derive(Default)]
pub struct Painter {
    lists: Vec<Option<DisplayList>>,
    rasters: HashMap<Slot, Raster>,
    path: BezPath,
    frame: u64,
    raster_budget: usize,
    /// Один бэкенд на все построения растров: создание `RenderContext` дороже
    /// самой отрисовки мелкого фрейма.
    raster_backend: Option<CpuBackend>,
    pub stats: FrameStats,
}

impl Painter {
    /// Начало кадра: сброс статистики, бюджет кэша, вытеснение старого.
    pub fn begin_frame(&mut self) {
        self.stats = FrameStats::default();
        self.frame += 1;
        self.raster_budget = RASTER_BUILDS_PER_FRAME;
        if self.frame.is_multiple_of(64) {
            self.evict();
        }
    }

    /// Выбрасывает контуры узлов и растры, не рисовавшиеся `EVICT_AFTER_FRAMES` кадров.
    fn evict(&mut self) {
        let frame = self.frame;
        for list in self.lists.iter_mut().flatten() {
            if list.paths.is_some() && list.last_used + EVICT_AFTER_FRAMES < frame {
                list.paths = None;
            }
        }
        self.rasters
            .retain(|_, r| r.last_used + EVICT_AFTER_FRAMES >= frame);
    }

    /// Рисует поддеревья `roots` снизу вверх. Обход без рекурсии: глубина дерева из
    /// файла не ограничена.
    pub fn draw(&mut self, backend: &mut impl RenderBackend, ctx: &DrawContext, roots: &[Slot]) {
        let scene = ctx.scene;
        let mut stack: Vec<Visit> = roots.iter().rev().map(|&s| Visit::Enter(s)).collect();
        while let Some(visit) = stack.pop() {
            let slot = match visit {
                Visit::Enter(slot) => slot,
                Visit::Exit { clip, layer } => {
                    if clip {
                        backend.pop_clip();
                    }
                    if layer {
                        backend.pop_layer();
                    }
                    continue;
                }
            };
            self.stats.visited += 1;
            let n = scene.at(slot);
            // Скрытый узел: границ поддерева нет.
            let Some(sub) = n.subtree_bounds else {
                continue;
            };
            if ctx.cull.is_some_and(|c| !overlaps(sub, c)) {
                self.stats.culled += 1;
                continue;
            }
            let px = max_side(sub) * ctx.scale;
            if ctx.lod && px < LOD_SKIP_PX {
                self.stats.too_small += 1;
                continue;
            }
            let has_children = !n.children.is_empty();
            if ctx.raster
                && ctx.scale <= RASTER_MAX_SCALE
                && n.parent.is_none()
                && has_children
                && px <= RASTER_MAX_PX
                && self.draw_raster(backend, ctx, slot, n, sub)
            {
                continue;
            }
            let opacity = n.props.opacity as f32;
            if opacity <= 0.0 {
                continue;
            }
            // Прозрачность контейнера — слоем: перекрытия детей не просвечивают друг
            // через друга. У листа прозрачность умножается в краску.
            let layer = has_children && opacity < 1.0;
            if layer {
                backend.push_opacity_layer(opacity);
                self.stats.layers += 1;
            }
            self.draw_own(backend, ctx, slot, n, if layer { 1.0 } else { opacity });
            if has_children {
                let clip =
                    n.props.kind == NodeKind::Frame && n.props.clip_content && needs_clip(scene, n);
                if clip {
                    backend.set_transform(ctx.transform * n.world);
                    self.path.truncate(0);
                    n.geometry().fill_path_into(&mut self.path);
                    backend.push_clip(&self.path);
                    self.stats.clips += 1;
                }
                stack.push(Visit::Exit { clip, layer });
                stack.extend(n.children.iter().rev().map(|&c| Visit::Enter(c)));
            }
        }
    }

    fn draw_own(
        &mut self,
        backend: &mut impl RenderBackend,
        ctx: &DrawContext,
        slot: Slot,
        n: &SceneNode,
        alpha: f32,
    ) {
        if n.props.kind == NodeKind::Group {
            return;
        }
        let Some(bounds) = n.world_bounds else { return };
        if ctx.cull.is_some_and(|c| !overlaps(bounds, c)) {
            return;
        }
        let px = max_side(bounds) * ctx.scale;
        if ctx.lod && px < LOD_SKIP_PX {
            return;
        }
        if ctx.lod && px < LOD_RECT_PX {
            // Мелкому узлу список отрисовки не нужен — только цвет.
            if let Some(c) = display::lod_color(&n.props) {
                backend.set_transform(ctx.transform);
                backend.set_color(c.multiply_alpha(alpha));
                backend.fill_rect(&bounds);
                self.stats.lod_rects += 1;
            }
            return;
        }
        let i = slot as usize;
        let cached = self
            .lists
            .get(i)
            .and_then(Option::as_ref)
            .is_some_and(|l| l.version == n.version && (ctx.retain || l.paths.is_some()));
        backend.set_transform(ctx.transform * n.world);
        self.stats.drawn += 1;
        if !cached {
            self.stats.lists_built += 1;
            let mut list = DisplayList::build(n);
            list.paths();
            replay(backend, &list, alpha, ctx.images);
            if ctx.retain {
                list.last_used = self.frame;
                if self.lists.len() <= i {
                    self.lists.resize(i + 1, None);
                }
                self.lists[i] = Some(list);
            }
            return;
        }
        let list = self.lists[i].as_mut().expect("проверено выше");
        list.last_used = self.frame;
        list.paths();
        replay(backend, list, alpha, ctx.images);
    }

    /// Рисует контейнер из растрового кэша, при необходимости строя кэш. `false` —
    /// кэша нет и бюджет кадра исчерпан (или растр был бы слишком большим): тогда
    /// контейнер рисуется узлами.
    ///
    /// Растр строится с плотностью — ближайшей степенью двойки не меньше текущего
    /// масштаба, и годится, пока масштаб не уменьшится вдвое: билинейная выборка с
    /// уменьшением до 2× не даёт заметной ряби, а плавный зум не перестраивает кэш
    /// на каждом кадре.
    fn draw_raster(
        &mut self,
        backend: &mut impl RenderBackend,
        ctx: &DrawContext,
        slot: Slot,
        n: &SceneNode,
        area: Rect,
    ) -> bool {
        let fresh = self.rasters.get(&slot).is_some_and(|r| {
            r.id == n.id
                && r.subtree_version == n.subtree_version
                && r.scale >= ctx.scale
                && r.scale < 2.0 * ctx.scale
        });
        if !fresh {
            if self.raster_budget == 0 {
                return false;
            }
            let scale = 2f64.powf(ctx.scale.log2().ceil());
            let (w, h) = (
                (area.width() * scale).ceil(),
                (area.height() * scale).ceil(),
            );
            if w > 2.0 * RASTER_MAX_PX || h > 2.0 * RASTER_MAX_PX {
                return false;
            }
            self.raster_budget -= 1;
            let (w, h) = (w.max(1.0) as u16, h.max(1.0) as u16);
            let mut cpu = match self.raster_backend.take() {
                Some(mut b) => {
                    b.reset(w, h);
                    b
                }
                None => CpuBackend::new(w, h),
            };
            let inner = DrawContext {
                scene: ctx.scene,
                images: ctx.images,
                transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
                scale,
                cull: None,
                lod: true,
                raster: false,
                retain: false,
            };
            let outer = self.stats;
            self.draw(&mut cpu, &inner, &[slot]);
            self.stats = FrameStats {
                raster_builds: outer.raster_builds + 1,
                lists_built: self.stats.lists_built,
                ..outer
            };
            self.rasters.insert(
                slot,
                Raster {
                    id: n.id,
                    subtree_version: n.subtree_version,
                    scale,
                    origin: area.origin(),
                    pixmap: Arc::new(cpu.render()),
                    last_used: 0,
                },
            );
            self.raster_backend = Some(cpu);
        }
        let r = self.rasters.get_mut(&slot).expect("есть или построен выше");
        r.last_used = self.frame;
        backend.set_transform(ctx.transform);
        backend.set_image(&ImagePaint {
            pixmap: r.pixmap.clone(),
            transform: Affine::translate(r.origin.to_vec2()) * Affine::scale(1.0 / r.scale),
            extend: Extend::Pad,
            alpha: 1.0,
        });
        backend.fill_rect(&Rect::from_origin_size(
            r.origin,
            (
                r.pixmap.width() as f64 / r.scale,
                r.pixmap.height() as f64 / r.scale,
            ),
        ));
        self.stats.rasters += 1;
        true
    }
}

/// Воспроизводит список отрисовки в локальных координатах узла.
/// Контуры списка уже построены (`DisplayList::paths`).
fn replay(
    backend: &mut impl RenderBackend,
    list: &DisplayList,
    alpha: f32,
    images: &dyn ImageProvider,
) {
    let geom = list.geometry;
    let paths = list
        .paths
        .as_ref()
        .expect("контуры построены до воспроизведения");
    for (item, item_path) in list.items.iter().zip(&paths.items) {
        match item {
            Item::Fill(Paint::Solid(c)) => {
                backend.set_color(c.multiply_alpha(alpha));
                fill_shape(backend, geom, &paths.shape);
            }
            Item::Fill(Paint::Image {
                hash,
                mode,
                alpha: a,
            }) => {
                let Some(area) = geom.bounds() else { continue };
                let Some(pixmap) = images.image(hash) else {
                    backend.set_color(IMAGE_PLACEHOLDER.multiply_alpha(a * alpha));
                    fill_shape(backend, geom, &paths.shape);
                    continue;
                };
                let (iw, ih) = (pixmap.width() as f64, pixmap.height() as f64);
                let (transform, extend, fit) = place_image(*mode, area, iw, ih);
                backend.set_image(&ImagePaint {
                    pixmap,
                    transform,
                    extend,
                    alpha: a * alpha,
                });
                match fit {
                    // «Вписать»: картинка целиком, поля прозрачные, форма обрезает углы.
                    Some(rect) => {
                        backend.push_clip(&paths.shape);
                        backend.fill_rect(&rect);
                        backend.pop_clip();
                    }
                    None => fill_shape(backend, geom, &paths.shape),
                }
            }
            Item::Stroke(_, c) => {
                if let Some(ring) = item_path {
                    backend.set_color(c.multiply_alpha(alpha));
                    backend.fill_path(ring, Fill::EvenOdd);
                }
            }
            Item::FilledInsideStroke {
                fill,
                stroke,
                width,
            } => {
                backend.set_color(stroke.multiply_alpha(alpha));
                fill_shape(backend, geom, &paths.shape);
                backend.set_color(fill.multiply_alpha(alpha));
                match geom {
                    Geometry::Rect { rect, radius } if radius <= 0.0 => {
                        let inner = rect.inset(-*width);
                        if inner.width() > 0.0 && inner.height() > 0.0 {
                            backend.fill_rect(&inner);
                        }
                    }
                    _ => {
                        if let Some(inner) = item_path {
                            backend.fill_path(inner, Fill::NonZero);
                        }
                    }
                }
            }
        }
    }
}

/// Заливка формы; прямоугольник без скругления — быстрым путём `fill_rect`.
fn fill_shape(backend: &mut impl RenderBackend, geom: Geometry, shape: &BezPath) {
    match geom {
        Geometry::Rect { rect, radius } if radius <= 0.0 => backend.fill_rect(&rect),
        _ if !shape.elements().is_empty() => backend.fill_path(shape, Fill::NonZero),
        _ => {}
    }
}

/// Размещение изображения `iw × ih` в области узла: матрица «пиксели картинки →
/// локальные координаты», режим краёв и, для «вписать», прямоугольник картинки.
pub fn place_image(
    mode: ScaleMode,
    area: Rect,
    iw: f64,
    ih: f64,
) -> (Affine, Extend, Option<Rect>) {
    let (aw, ah) = (area.width(), area.height());
    match mode {
        ScaleMode::Fill | ScaleMode::Fit => {
            let s = if mode == ScaleMode::Fill {
                (aw / iw).max(ah / ih)
            } else {
                (aw / iw).min(ah / ih)
            };
            let x = area.x0 + (aw - iw * s) / 2.0;
            let y = area.y0 + (ah - ih * s) / 2.0;
            let fit = (mode == ScaleMode::Fit).then(|| Rect::new(x, y, x + iw * s, y + ih * s));
            (
                Affine::translate((x, y)) * Affine::scale(s),
                Extend::Pad,
                fit,
            )
        }
        // Плитка в натуральном размере от левого верхнего угла.
        ScaleMode::Tile => (Affine::translate((area.x0, area.y0)), Extend::Repeat, None),
    }
}

/// Нужна ли фрейму обрезка детей. Обрезка не бесплатна, а в типичном макете дети
/// лежат внутри фрейма: если фрейм не повёрнут, без скругления и границы всех детей
/// внутри него, обрезать нечего.
fn needs_clip(scene: &Scene, n: &SceneNode) -> bool {
    let Geometry::Rect { rect, radius } = n.geometry() else {
        return true;
    };
    let c = n.world.as_coeffs();
    if radius > 0.0 || c[1] != 0.0 || c[2] != 0.0 {
        return true;
    }
    let frame = n.world.transform_rect_bbox(rect);
    n.children
        .iter()
        .filter_map(|&c| scene.at(c).subtree_bounds)
        .any(|b| b.x0 < frame.x0 || b.y0 < frame.y0 || b.x1 > frame.x1 || b.y1 > frame.y1)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && a.x1 >= b.x0 && a.y0 <= b.y1 && a.y1 >= b.y0
}

fn max_side(r: Rect) -> f64 {
    r.width().max(r.height())
}
