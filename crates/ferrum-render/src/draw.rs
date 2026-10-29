//! Сборка кадра: обход сцены с отсечением невидимого и LOD, растровый кэш
//! мелких фреймов верхнего уровня, воспроизведение списков отрисовки в бэкенд.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ferrum_doc::{NodeId, NodeKind, ScaleMode};
use ferrum_scene::shape::Geometry;
use ferrum_scene::{Scene, SceneNode, Slot};
use ferrum_text::{Fonts, LayoutRequest, TextLayout};
use kurbo::{Affine, BezPath, Point, Rect};
use vello_cpu::peniko::{Extend, Fill};
use vello_cpu::Pixmap;

use crate::backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend};
use crate::clock::{default_clock, Clock};
use crate::display::{self, DisplayList, Item, Paint, IMAGE_PLACEHOLDER};
use crate::image::ImageProvider;
use crate::text_node::{self, TEXT_LAYOUT_TIME_PER_FRAME, TEXT_LOD_PX};

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
/// Сколько растровых кэшей строить за кадр — страховка поверх предела по времени
/// (`RASTER_BUILD_TIME_PER_FRAME`). Фреймы без готового растра этого кадра рисуются
/// прежним растром в новом масштабе (если он есть) или узлами, а кэш догоняет в
/// следующих кадрах.
pub const RASTER_BUILDS_PER_FRAME: usize = 32;
/// Сколько времени за кадр тратить на построение растров. Один растр фрейма из сотни
/// узлов строится около 0,5 мс, поэтому счётный предел в 32 растра сам по себе
/// давал кадры по 16+ мс; 4 мс оставляют в кадре 60 Гц место остальной работе.
/// Хотя бы один растр за кадр строится всегда — иначе кэш не догнал бы никогда.
pub const RASTER_BUILD_TIME_PER_FRAME: Duration = Duration::from_millis(4);
/// Сколько байт занимают растры фреймов всего: сверх этого вытесняются давно не
/// рисовавшиеся. Возраст (`EVICT_AFTER_FRAMES`) сам память не ограничивает: при
/// быстром панорамировании по огромной странице за 300 кадров растров накопится
/// на гигабайты.
pub const RASTER_CACHE_BYTES: usize = 64 * 1024 * 1024;
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
    /// Текстовые узлы: разложено за кадр, нарисовано глифами, полосами (LOD).
    pub text_layouts: usize,
    pub text_glyphs: usize,
    pub text_greeked: usize,
    /// Кадр неполный: часть растров не построена (бюджет кадра исчерпан, показаны
    /// прежние растры или узлы) или вместо картинки нарисована заглушка. Холст без
    /// правок и движения камеры должен запросить ещё кадр, иначе так и останется
    /// размытым или с заглушками.
    pub needs_another_frame: bool,
}

/// Параметры обхода.
pub struct DrawContext<'a> {
    pub scene: &'a Scene,
    pub images: &'a dyn ImageProvider,
    /// Реестр шрифтов и раскладка текста.
    pub fonts: &'a RefCell<Fonts>,
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
    /// Конец поддерева. У фрейма с детьми обводка рисуется после детей (в Figma
    /// обводка фрейма лежит поверх содержимого); `stroke` хранит его слот и, если
    /// списки не кэшируются, уже построенный список, чтобы не строить его заново.
    Exit {
        clip: bool,
        layer: bool,
        stroke: Option<(Slot, Option<DisplayList>)>,
    },
}

/// Какие элементы списка воспроизводить.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    All,
    /// Заливки фрейма — под детьми.
    Fills,
    /// Обводки фрейма — над детьми.
    Strokes,
}

/// Растр поддерева контейнера в мировых координатах.
struct Raster {
    id: NodeId,
    subtree_version: u64,
    /// Пикселей растра на единицу мира — степень двойки.
    scale: f64,
    origin: Point,
    pixmap: Arc<Pixmap>,
    /// Ключ этого построения для кэшей бэкенда (атлас GPU).
    key: ImageKey,
    last_used: u64,
    /// В растр попала заглушка вместо ещё не загруженной картинки. Такой растр не
    /// переиспользуется: картинка появится без изменения сцены, и устаревший
    /// растр показывал бы заглушку вечно.
    placeholder: bool,
}

impl Raster {
    fn bytes(&self) -> usize {
        self.pixmap.width() as usize * self.pixmap.height() as usize * 4
    }
}

/// Раскладка текстового узла: годна, пока не изменились вид узла (`version`)
/// и реестр шрифтов (`generation`).
struct CachedText {
    id: NodeId,
    version: u64,
    generation: u64,
    layout: Arc<TextLayout>,
    last_used: u64,
}

/// Кэши между кадрами: списки отрисовки по слотам сцены, растры фреймов, общий
/// буфер для контуров обрезки.
pub struct Painter {
    lists: Vec<Option<DisplayList>>,
    /// Раскладки текстовых узлов по слотам (ленивые: при первом рисовании).
    texts: HashMap<Slot, CachedText>,
    text_time_spent: Duration,
    rasters: HashMap<Slot, Raster>,
    path: BezPath,
    frame: u64,
    raster_budget: usize,
    /// Предел времени на растры за кадр и сколько уже потрачено (`None` — только
    /// счётный предел).
    raster_time: Option<Duration>,
    raster_time_spent: Duration,
    /// Часы для предела по времени; без них — только счётный предел.
    clock: Option<Box<dyn Clock>>,
    /// Один бэкенд на все построения растров: создание `RenderContext` дороже
    /// самой отрисовки мелкого фрейма.
    raster_backend: Option<CpuBackend>,
    /// Занято растрами и предел.
    raster_bytes: usize,
    raster_limit: usize,
    /// В этом кадре растр уже не поместился в предел памяти: остальные фреймы
    /// кадра — узлами.
    raster_full: bool,
    /// В текущем рисовании встретилась заглушка вместо картинки.
    missing_image: bool,
    pub stats: FrameStats,
}

impl Default for Painter {
    fn default() -> Painter {
        Painter::with_raster_limit(RASTER_CACHE_BYTES)
    }
}

impl Painter {
    pub fn with_raster_limit(bytes: usize) -> Painter {
        Painter {
            lists: Vec::new(),
            texts: HashMap::new(),
            text_time_spent: Duration::ZERO,
            rasters: HashMap::new(),
            path: BezPath::new(),
            frame: 0,
            raster_budget: 0,
            raster_time: Some(RASTER_BUILD_TIME_PER_FRAME),
            raster_time_spent: Duration::ZERO,
            clock: default_clock(),
            raster_backend: None,
            raster_bytes: 0,
            raster_limit: bytes,
            missing_image: false,
            raster_full: false,
            stats: FrameStats::default(),
        }
    }

    /// Предел времени на построение растров за кадр; `None` — только счётный предел
    /// `RASTER_BUILDS_PER_FRAME` (детерминированно: тесты и платформы без часов).
    pub fn set_raster_time(&mut self, budget: Option<Duration>) {
        self.raster_time = budget;
    }

    /// Часы для предела времени на растры (`None` — только счётный предел).
    pub fn set_clock(&mut self, clock: Option<Box<dyn Clock>>) {
        self.clock = clock;
    }

    /// Сколько байт занимают растры в кэше.
    pub fn raster_bytes(&self) -> usize {
        self.raster_bytes
    }

    /// Начало кадра: сброс статистики, бюджет кэша, вытеснение старого.
    pub fn begin_frame(&mut self) {
        self.stats = FrameStats::default();
        self.frame += 1;
        self.raster_budget = RASTER_BUILDS_PER_FRAME;
        self.raster_time_spent = Duration::ZERO;
        self.text_time_spent = Duration::ZERO;
        self.missing_image = false;
        self.raster_full = false;
        if self.frame.is_multiple_of(64) {
            self.evict();
        }
    }

    /// Конец кадра на экране: статистика с признаком неполного кадра (заглушки
    /// картинок, встреченные при рисовании, тоже его делают неполным).
    pub fn finish_frame(&mut self) -> FrameStats {
        self.stats.needs_another_frame |= self.missing_image;
        self.stats
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
        self.texts
            .retain(|_, t| t.last_used + EVICT_AFTER_FRAMES >= frame);
        self.raster_bytes = self.rasters.values().map(Raster::bytes).sum();
    }

    /// Вытесняет давно не рисовавшиеся растры, пока кэш не уложится в предел.
    /// `keep` — только что построенный растр: он нужен прямо сейчас.
    fn enforce_raster_limit(&mut self, keep: Slot) {
        while self.raster_bytes > self.raster_limit {
            let frame = self.frame;
            let oldest = self
                .rasters
                .iter()
                .filter(|(&s, r)| s != keep && evictable(r, frame))
                .min_by_key(|(_, r)| r.last_used)
                .map(|(&s, _)| s);
            let Some(slot) = oldest else { break };
            if let Some(r) = self.rasters.remove(&slot) {
                self.raster_bytes -= r.bytes();
            }
        }
    }

    /// Поместится ли растр `bytes` байт для `slot` в предел памяти, если вытеснить
    /// всё, что можно (`evictable`); старый растр этого же слота он заменит.
    fn raster_fits(&self, slot: Slot, bytes: usize) -> bool {
        // Быстрый путь: место есть и без вытеснения (старый растр слота освободит
        // ещё больше). Полный перебор кэша — только у предела: иначе он шёл бы для
        // каждого фрейма без готового растра, O(фреймов × растров) за кадр.
        if self.raster_bytes + bytes <= self.raster_limit {
            return true;
        }
        let frame = self.frame;
        let freeable: usize = self
            .rasters
            .iter()
            .filter(|(&s, r)| s == slot || evictable(r, frame))
            .map(|(_, r)| r.bytes())
            .sum();
        self.raster_bytes - freeable + bytes <= self.raster_limit
    }

    /// Рисует поддеревья `roots` снизу вверх. Обход без рекурсии: глубина дерева из
    /// файла не ограничена.
    pub fn draw(&mut self, backend: &mut impl RenderBackend, ctx: &DrawContext, roots: &[Slot]) {
        let scene = ctx.scene;
        let mut stack: Vec<Visit> = roots.iter().rev().map(|&s| Visit::Enter(s)).collect();
        while let Some(visit) = stack.pop() {
            let slot = match visit {
                Visit::Enter(slot) => slot,
                Visit::Exit {
                    clip,
                    layer,
                    stroke,
                } => {
                    if clip {
                        backend.pop_clip();
                    }
                    if let Some((slot, held)) = stroke {
                        let n = scene.at(slot);
                        self.draw_own(backend, ctx, slot, n, 1.0, Part::Strokes, held);
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
            // Обводка фрейма с детьми — над детьми, заливка — под ними.
            let split = has_children && n.props.kind == NodeKind::Frame;
            let held = self.draw_own(
                backend,
                ctx,
                slot,
                n,
                if layer { 1.0 } else { opacity },
                if split { Part::Fills } else { Part::All },
                None,
            );
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
                let stroke = (split && !n.props.strokes.is_empty()).then_some((slot, held));
                stack.push(Visit::Exit {
                    clip,
                    layer,
                    stroke,
                });
                stack.extend(n.children.iter().rev().map(|&c| Visit::Enter(c)));
            }
        }
    }

    /// Рисует собственное содержимое узла (без детей). Возвращает построенный
    /// список, если он нужен для `Part::Strokes` этого же узла, а кэшировать его
    /// нельзя.
    #[allow(clippy::too_many_arguments)]
    fn draw_own(
        &mut self,
        backend: &mut impl RenderBackend,
        ctx: &DrawContext,
        slot: Slot,
        n: &SceneNode,
        alpha: f32,
        part: Part,
        held: Option<DisplayList>,
    ) -> Option<DisplayList> {
        if n.props.kind == NodeKind::Group {
            return None;
        }
        let bounds = n.world_bounds?;
        if ctx.cull.is_some_and(|c| !overlaps(bounds, c)) {
            return None;
        }
        let px = max_side(bounds) * ctx.scale;
        if ctx.lod && px < LOD_SKIP_PX {
            return None;
        }
        if ctx.lod && px < LOD_RECT_PX {
            // Мелкому узлу список отрисовки не нужен — только цвет. Прямоугольник
            // рисуется одним разом вместе с заливкой, обводка поверх не нужна.
            if part != Part::Strokes {
                if let Some(c) = display::lod_color(&n.props) {
                    backend.set_transform(ctx.transform);
                    backend.set_color(c.multiply_alpha(alpha));
                    backend.fill_rect(&bounds);
                    self.stats.lod_rects += 1;
                }
            }
            return None;
        }
        if n.props.kind == NodeKind::Text {
            if part != Part::Strokes {
                self.draw_text(backend, ctx, slot, n, alpha);
            }
            return None;
        }
        let i = slot as usize;
        let split = part != Part::All;
        backend.set_transform(ctx.transform * n.world);
        if part != Part::Strokes {
            self.stats.drawn += 1;
        }
        if let Some(mut list) = held {
            list.paths();
            self.play(backend, &list, alpha, ctx, part);
            return None;
        }
        let cached = self.lists.get(i).and_then(Option::as_ref).is_some_and(|l| {
            l.version == n.version && l.split == split && (ctx.retain || l.paths.is_some())
        });
        if !cached {
            if part != Part::Strokes {
                self.stats.lists_built += 1;
            }
            let mut list = DisplayList::build(n, split);
            list.paths();
            self.play(backend, &list, alpha, ctx, part);
            if ctx.retain {
                list.last_used = self.frame;
                if self.lists.len() <= i {
                    self.lists.resize(i + 1, None);
                }
                self.lists[i] = Some(list);
                return None;
            }
            return (part == Part::Fills).then_some(list);
        }
        let mut list = self.lists[i].take().expect("проверено выше");
        list.last_used = self.frame;
        list.paths();
        self.play(backend, &list, alpha, ctx, part);
        self.lists[i] = Some(list);
        None
    }

    /// Текстовый узел: раскладка из кэша или новая (в пределах времени кадра на
    /// экране; при экспорте — всегда), затем глифы или полосы строк (LOD).
    /// Раскладка ещё грузящимися шрифтами не рисуется: кадр перерисуют, когда
    /// придут байты (поколение реестра).
    fn draw_text(
        &mut self,
        backend: &mut impl RenderBackend,
        ctx: &DrawContext,
        slot: Slot,
        n: &SceneNode,
        alpha: f32,
    ) {
        let Some(text) = &n.props.text else { return };
        let to_device = ctx.transform * n.world;
        let generation = ctx.fonts.borrow().generation();
        let frame = self.frame;
        let cached = self
            .texts
            .get_mut(&slot)
            .filter(|c| c.id == n.id && c.version == n.version && c.generation == generation)
            .map(|c| {
                c.last_used = frame;
                c.layout.clone()
            });
        let layout = match cached {
            Some(l) => Some(l),
            None if self.can_layout_text(ctx) => {
                let started = self.clock.as_ref().map(|c| c.now_ms());
                let req = LayoutRequest::for_node(text, &n.props.fills, n.props.w, n.props.h);
                let layout = Arc::new(ctx.fonts.borrow_mut().layout(&req));
                if let (Some(t), Some(c)) = (started, self.clock.as_ref()) {
                    let ms = (c.now_ms() - t).max(0.0);
                    self.text_time_spent += Duration::from_secs_f64(ms / 1000.0);
                }
                self.stats.text_layouts += 1;
                self.texts.insert(
                    slot,
                    CachedText {
                        id: n.id,
                        version: n.version,
                        generation,
                        layout: layout.clone(),
                        last_used: self.frame,
                    },
                );
                Some(layout)
            }
            None => {
                // Время кадра на раскладку вышло: пока — полосы по оценке строк.
                self.stats.needs_another_frame = true;
                None
            }
        };
        let Some(layout) = layout else {
            text_node::paint_greek(
                backend,
                &text_node::estimate_lines(&n.props),
                to_device,
                alpha,
            );
            self.stats.text_greeked += 1;
            return;
        };
        if layout.loading {
            return;
        }
        let line_px = layout
            .lines
            .iter()
            .map(|l| l.rect.height())
            .fold(0.0, f64::max)
            * ctx.scale;
        if ctx.lod && line_px < TEXT_LOD_PX {
            text_node::paint_greek(backend, &layout.lines, to_device, alpha);
            self.stats.text_greeked += 1;
        } else {
            text_node::paint_glyphs(
                backend,
                &mut ctx.fonts.borrow_mut(),
                &layout,
                to_device,
                alpha,
            );
            self.stats.text_glyphs += 1;
        }
    }

    /// Можно ли разложить ещё один текст в этом кадре: экспорт и превью (без LOD)
    /// раскладывают всё; на экране — первый всегда, дальше в пределах времени.
    fn can_layout_text(&self, ctx: &DrawContext) -> bool {
        if !ctx.lod || self.stats.text_layouts == 0 {
            return true;
        }
        match self.clock.as_ref() {
            Some(_) => self.text_time_spent < TEXT_LAYOUT_TIME_PER_FRAME,
            None => true,
        }
    }

    fn play(
        &mut self,
        backend: &mut impl RenderBackend,
        list: &DisplayList,
        alpha: f32,
        ctx: &DrawContext,
        part: Part,
    ) {
        let info = replay(backend, list, alpha, ctx.images, part);
        self.missing_image |= info.missing_image;
        self.stats.layers += info.layered as usize;
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
        // Растр того же содержимого, возможно, другой плотности.
        let same = self
            .rasters
            .get(&slot)
            .is_some_and(|r| r.id == n.id && r.subtree_version == n.subtree_version);
        let fresh = same
            && self.rasters.get(&slot).is_some_and(|r| {
                !r.placeholder && r.scale >= ctx.scale && r.scale < 2.0 * ctx.scale
            });
        if !fresh {
            let scale = 2f64.powf(ctx.scale.log2().ceil());
            let (w, h) = (
                (area.width() * scale).ceil(),
                (area.height() * scale).ceil(),
            );
            if w > 2.0 * RASTER_MAX_PX || h > 2.0 * RASTER_MAX_PX {
                return false;
            }
            // Растры видимых фреймов не помещаются в предел памяти: до конца кадра
            // новые не строятся, а оставшиеся фреймы рисуются узлами — полностью.
            // Это устойчивое состояние, а не догоняние: кадр не помечается неполным
            // (иначе холст перерисовывался бы вечно), и заглушка-заливка здесь не
            // годится — она осталась бы на экране навсегда.
            if self.raster_full || !self.raster_fits(slot, w as usize * h as usize * 4) {
                self.raster_full = true;
                return false;
            }
            if !self.can_build_raster() {
                // Бюджет кадра исчерпан: кадр неполный, растр догонит в следующих.
                self.stats.needs_another_frame = true;
                // После зума через степень двойки у фрейма есть растр прежней
                // плотности с тем же содержимым — он и рисуется, растянутый в новый
                // масштаб (матрица в `blit_raster` учитывает его плотность): чуть
                // размыто несколько кадров, зато без поштучной отрисовки десятков
                // тысяч узлов.
                if same {
                    return self.blit_raster(backend, ctx, slot);
                }
                // Растра этого узла ещё не было (первый показ страницы издалека), а
                // в предел памяти он поместится (проверено выше):
                // пока — только собственная заливка фрейма, без детей. Поштучная
                // отрисовка сотни тысяч LOD-прямоугольников стоила бы ~10 мс на
                // каждом кадре прогрева.
                let cold = self.rasters.get(&slot).is_none_or(|r| r.id != n.id);
                if cold {
                    let opacity = n.props.opacity as f32;
                    if opacity > 0.0 {
                        self.draw_own(backend, ctx, slot, n, opacity, Part::Fills, None);
                    }
                    return true;
                }
                // Содержимое изменилось (правка): точный вид важнее — узлами.
                return false;
            }
            self.raster_budget -= 1;
            let started = self.clock.as_ref().map(|c| c.now_ms());
            let (w, h) = (w.max(1.0) as u16, h.max(1.0) as u16);
            let mut cpu = match self.raster_backend.take() {
                Some(mut b) => {
                    b.begin_frame(w, h);
                    b
                }
                None => CpuBackend::new(w, h),
            };
            let inner = DrawContext {
                scene: ctx.scene,
                images: ctx.images,
                fonts: ctx.fonts,
                transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
                scale,
                cull: None,
                lod: true,
                raster: false,
                retain: false,
            };
            let outer = self.stats;
            let outer_missing = std::mem::take(&mut self.missing_image);
            self.draw(&mut cpu, &inner, &[slot]);
            let placeholder = std::mem::replace(&mut self.missing_image, outer_missing);
            // Растр с заглушкой на экране — та же заглушка: кадр неполный.
            self.missing_image |= placeholder;
            self.stats = FrameStats {
                raster_builds: outer.raster_builds + 1,
                lists_built: self.stats.lists_built,
                ..outer
            };
            if let Some(old) = self.rasters.remove(&slot) {
                self.raster_bytes -= old.bytes();
            }
            let raster = Raster {
                id: n.id,
                subtree_version: n.subtree_version,
                scale,
                origin: area.origin(),
                pixmap: Arc::new(cpu.render()),
                key: ImageKey::next_raster(),
                last_used: self.frame,
                placeholder,
            };
            self.raster_bytes += raster.bytes();
            self.rasters.insert(slot, raster);
            self.enforce_raster_limit(slot);
            self.raster_backend = Some(cpu);
            if let (Some(t), Some(c)) = (started, self.clock.as_ref()) {
                let ms = (c.now_ms() - t).max(0.0);
                self.raster_time_spent += Duration::from_secs_f64(ms / 1000.0);
            }
        }
        self.blit_raster(backend, ctx, slot)
    }

    /// Можно ли в этом кадре построить ещё один растр: счётный предел и предел по
    /// времени. Первый растр кадра строится всегда — так кэш догоняет при любом
    /// пределе времени.
    fn can_build_raster(&self) -> bool {
        if self.raster_budget == 0 {
            return false;
        }
        let first = self.stats.raster_builds == 0;
        // Без часов время не измерить: остаётся счётный предел.
        let timed = self.raster_time.filter(|_| self.clock.is_some());
        first || timed.is_none_or(|t| self.raster_time_spent < t)
    }

    /// Рисует растр фрейма из кэша (он там есть) в текущий кадр.
    fn blit_raster(
        &mut self,
        backend: &mut impl RenderBackend,
        ctx: &DrawContext,
        slot: Slot,
    ) -> bool {
        let r = self.rasters.get_mut(&slot).expect("растр есть в кэше");
        r.last_used = self.frame;
        backend.set_transform(ctx.transform);
        backend.set_image(&ImagePaint {
            key: r.key,
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

/// Можно ли вытеснить растр ради нового. Нельзя — нарисованный в этом кадре или в
/// прошлом: первый уже на экране, второй почти наверняка понадобится ниже в этом
/// же кадре (порядок обхода тот же). Иначе при нехватке памяти растры видимых
/// фреймов вытесняли бы друг друга каждый кадр, и часть фреймов не дорисовывалась
/// бы никогда.
fn evictable(r: &Raster, frame: u64) -> bool {
    r.last_used + 1 < frame
}

/// Что случилось при воспроизведении списка.
struct ReplayInfo {
    /// Вместо картинки нарисована заглушка: картинки ещё нет.
    missing_image: bool,
    /// Узел нарисован слоем прозрачности.
    layered: bool,
}

/// Воспроизводит список отрисовки в локальных координатах узла.
/// Контуры списка уже построены (`DisplayList::paths`).
///
/// Прозрачность листа (`alpha < 1`) умножается в краску, только когда краска одна:
/// иначе перекрывающиеся краски (заливка и обводка, несколько заливок) просвечивали
/// бы друг через друга, и результат отличался бы от прозрачности всего узла. Тогда
/// узел рисуется слоем.
fn replay(
    backend: &mut impl RenderBackend,
    list: &DisplayList,
    alpha: f32,
    images: &dyn ImageProvider,
    part: Part,
) -> ReplayInfo {
    let mut info = ReplayInfo {
        missing_image: false,
        layered: false,
    };
    let single_paint =
        list.items.len() == 1 && !matches!(list.items[0], Item::FilledInsideStroke { .. });
    if alpha < 1.0 && !single_paint && !list.items.is_empty() {
        backend.push_opacity_layer(alpha);
        info.layered = true;
        replay_items(backend, list, 1.0, images, part, &mut info);
        backend.pop_layer();
    } else {
        replay_items(backend, list, alpha, images, part, &mut info);
    }
    info
}

fn replay_items(
    backend: &mut impl RenderBackend,
    list: &DisplayList,
    alpha: f32,
    images: &dyn ImageProvider,
    part: Part,
    info: &mut ReplayInfo,
) {
    let geom = list.geometry;
    let paths = list
        .paths
        .as_ref()
        .expect("контуры построены до воспроизведения");
    for (item, item_path) in list.items.iter().zip(&paths.items) {
        let is_fill = matches!(item, Item::Fill(_));
        match part {
            Part::Fills if !is_fill => continue,
            Part::Strokes if is_fill => continue,
            _ => {}
        }
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
                // Вырожденная область: матрица размещения необратима.
                if !(area.width() > 0.0 && area.height() > 0.0) {
                    continue;
                }
                let Some(pixmap) = images.image(hash) else {
                    info.missing_image = true;
                    backend.set_color(IMAGE_PLACEHOLDER.multiply_alpha(a * alpha));
                    fill_shape(backend, geom, &paths.shape);
                    continue;
                };
                let (iw, ih) = (pixmap.width() as f64, pixmap.height() as f64);
                let (transform, extend, fit) = place_image(*mode, area, iw, ih);
                backend.set_image(&ImagePaint {
                    key: ImageKey::for_hash(hash),
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
