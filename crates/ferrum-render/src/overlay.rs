//! Оверлеи поверх содержимого в экранных пикселях: пиксельная сетка, названия
//! фреймов, подсветка наведения, рамка выделения мышью, линии привязки, рамка
//! выделения с ручками, бейдж «W × H» и измерения расстояний с Alt. Толщины и
//! размеры задаются в CSS-пикселях и умножаются на `devicePixelRatio`.

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::shape::Geometry;
use ferrum_scene::{Scene, SceneNode};
use kurbo::{Affine, BezPath, Line, Point, Rect, RoundedRect, Shape, Stroke};
use vello_cpu::peniko::Fill;

use crate::backend::{RenderBackend, Rgba};
use crate::camera::Camera;
use crate::text::{Label, TextEngine};

/// Сторона квадратной ручки выделения.
pub const HANDLE_SIZE: f64 = 8.0;
/// Пиксельная сетка видна с зума 800%.
pub const PIXEL_GRID_MIN_ZOOM: f64 = 8.0;
/// Наибольшее число линий пиксельной сетки по одной оси.
const MAX_GRID_LINES: usize = 4096;
/// Кегль подписей фреймов и бейджа.
pub const LABEL_SIZE: f64 = 11.0;
/// Штрих и промежуток пунктира направляющей измерения.
pub const MEASURE_DASH: f64 = 4.0;
pub const MEASURE_GAP: f64 = 3.0;

/// Цвета оверлеев из дизайн-системы Ferrum.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayTheme {
    /// Рамки выделения и наведения, ручки, фон бейджа.
    pub select: Rgba,
    /// Текст на фоне `select`.
    pub on_select: Rgba,
    /// Выделение компонента (подсистема 5).
    pub component: Rgba,
    pub handle_fill: Rgba,
    /// Названия фреймов (`ink-muted`).
    pub frame_label: Rgba,
    /// Линии привязки.
    pub snap: Rgba,
    pub pixel_grid: Rgba,
    /// Линии и плашки измерений с Alt (`danger`); текст плашки — `on_select`.
    pub measure: Rgba,
}

fn hex(rgb: u32) -> Rgba {
    Rgba::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

impl OverlayTheme {
    pub fn dark() -> OverlayTheme {
        OverlayTheme {
            select: hex(0x4aa3ff),
            on_select: hex(0x1a1918),
            component: hex(0xb49cff),
            handle_fill: hex(0xffffff),
            frame_label: hex(0xa9a39c),
            snap: hex(0xf24822),
            pixel_grid: Rgba::from_rgba8(255, 255, 255, 26),
            measure: hex(0xff7a7a),
        }
    }

    pub fn light() -> OverlayTheme {
        OverlayTheme {
            select: hex(0x1769e0),
            on_select: hex(0xffffff),
            component: hex(0x6d44d9),
            handle_fill: hex(0xffffff),
            frame_label: hex(0x655f58),
            snap: hex(0xf24822),
            pixel_grid: Rgba::from_rgba8(0, 0, 0, 20),
            measure: hex(0xc22c2c),
        }
    }
}

/// Вид отрезка измерения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureKind {
    /// Расстояние: сплошная линия и плашка со значением посередине.
    Distance,
    /// Направляющая: пунктир вдоль продолжения края, до которого мерили (у объекта,
    /// который не перекрывается с выделением ни по одной оси). Без плашки.
    Guide,
}

/// Отрезок измерения в мировых координатах. Значение — в единицах документа, уже
/// округлённое до сотых (у направляющей не показывается).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    pub line: Line,
    pub value: f64,
    pub kind: MeasureKind,
}

/// Правка текста на холсте (подсистема 3, этап 2): рамка текста, выделение,
/// курсор и подчёркивание составляемого текста IME. Всё — в координатах узла.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEditOverlay {
    /// Координаты узла → мир.
    pub transform: Affine,
    /// Рамка узла: тонкий контур цвета выделения вместо ручек (как в Figma).
    pub frame: Rect,
    /// Выделенные участки строк: заливка `select` 25 %.
    pub selection: Vec<Rect>,
    /// Курсор: x, верх и низ строки; `None` — погашен миганием или есть выделение.
    pub caret: Option<(f64, f64, f64)>,
    /// Подчёркивание составляемого IME текста: `(x0, x1, y)` по строкам.
    pub composition: Vec<(f64, f64, f64)>,
}

/// Прозрачность заливки выделения текста (спека подсистемы 3, раздел 7).
pub const TEXT_SELECTION_ALPHA: f32 = 0.25;

/// Радиус точки сети в правке вектора и ручки — CSS-пиксели.
pub const VECTOR_POINT_RADIUS: f64 = 3.5;
pub const VECTOR_HANDLE_RADIUS: f64 = 2.5;
/// Больше стольких точек на экране не рисуются невыделенные: сеть в 100 000
/// точек при отдалении была бы сплошной кашей кружков (и дорогим кадром). Больше
/// стольких выделенных или ручек на экране — не рисуются и они (выделение видно
/// по сети, обведённой толще).
pub const VECTOR_MAX_POINTS_DRAWN: usize = 5000;

/// Правка вектора (подсистема 3, этап 3): сеть, точки, ручки и резинка пера.
/// Всё — в координатах формы узла.
#[derive(Debug, Clone)]
pub struct VectorEditOverlay {
    /// Координаты формы → мир.
    pub transform: Affine,
    pub net: std::sync::Arc<ferrum_doc::VectorNetwork>,
    /// Выделенные точки и сегменты.
    pub points: Vec<u32>,
    pub segments: Vec<u32>,
    /// Видимые ручки: сегмент и `true` — у его начала.
    pub handles: Vec<(u32, bool)>,
    pub selected_handles: Vec<(u32, bool)>,
    /// Резинка пера: от последней точки пути к курсору.
    pub rubber: Option<(Point, Point)>,
}

impl PartialEq for VectorEditOverlay {
    /// Сеть сравнивается по указателю (одна и та же — равна, без обхода 100 000
    /// точек), иначе по значению.
    fn eq(&self, o: &Self) -> bool {
        self.transform == o.transform
            && (std::sync::Arc::ptr_eq(&self.net, &o.net) || self.net == o.net)
            && self.points == o.points
            && self.segments == o.segments
            && self.handles == o.handles
            && self.selected_handles == o.selected_handles
            && self.rubber == o.rubber
    }
}

/// Что показать поверх содержимого. Заполняет редактор (этап 3).
#[derive(Debug, Clone)]
pub struct Overlay {
    pub theme: OverlayTheme,
    pub selection: Vec<NodeId>,
    pub hover: Option<NodeId>,
    /// Рамка выделения мышью в мировых координатах.
    pub marquee: Option<Rect>,
    /// Линии привязки в мировых координатах.
    pub snap_lines: Vec<Line>,
    /// Названия фреймов верхнего уровня.
    pub frame_labels: bool,
    /// Пиксельная сетка (рисуется только с `PIXEL_GRID_MIN_ZOOM`).
    pub pixel_grid: bool,
    /// Измерения расстояний с Alt (раздел 8.6 спеки).
    pub measurements: Vec<Measure>,
    /// Правка текста: курсор и выделение внутри текстового узла.
    pub text_edit: Option<TextEditOverlay>,
    /// Правка вектора: сеть, точки и ручки.
    pub vector_edit: Option<VectorEditOverlay>,
}

impl Default for Overlay {
    fn default() -> Self {
        Overlay {
            theme: OverlayTheme::dark(),
            selection: Vec::new(),
            hover: None,
            marquee: None,
            snap_lines: Vec::new(),
            frame_labels: true,
            pixel_grid: true,
            measurements: Vec::new(),
            text_edit: None,
            vector_edit: None,
        }
    }
}

/// Координата тонкой линии, выровненная по пикселям: линия нечётной толщины
/// проходит через центры пикселей, чётной — по их границам. Иначе линия в 1 px
/// размывается на две полупрозрачные строки.
fn crisp(v: f64, width: f64) -> f64 {
    if (width.round() as i64) % 2 == 1 {
        v.floor() + 0.5
    } else {
        v.round()
    }
}

fn crisp_point(p: Point, width: f64) -> Point {
    Point::new(crisp(p.x, width), crisp(p.y, width))
}

/// Размер для бейджа: целые без дробной части, остальное — до сотых.
pub fn format_size(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r}")
    }
}

pub(crate) fn draw(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    scene: &Scene,
    camera: &Camera,
    overlay: &Overlay,
) {
    let dpr = camera.effective_dpr();
    // Мир → пиксели устройства.
    let view = camera.view_transform();
    let theme = &overlay.theme;
    let hairline = Stroke::new(dpr);

    if overlay.pixel_grid && camera.zoom() >= PIXEL_GRID_MIN_ZOOM {
        pixel_grid(backend, camera, theme.pixel_grid);
    }
    if overlay.frame_labels {
        frame_labels(backend, text, scene, camera, overlay);
    }
    // Скрытый узел (или узел внутри скрытого) на холсте не виден — и обводки
    // наведения у него нет, даже если редактор передал его из панели слоёв.
    let hover = overlay.hover.filter(|h| {
        !overlay.selection.contains(h) && scene.slot(*h).is_some_and(|s| scene.is_visible(s))
    });
    if let Some(id) = hover {
        let (w, h) = backend.size();
        let screen = Rect::new(0.0, 0.0, w as f64, h as f64).inflate(8.0 * dpr, 8.0 * dpr);
        if let Some(outline) = node_outline(scene, id, view, screen) {
            backend.set_transform(Affine::IDENTITY);
            backend.set_color(theme.select);
            backend.stroke_path(&outline, &Stroke::new(1.5 * dpr));
        }
    }
    if let Some(m) = overlay.marquee {
        let r = view.transform_rect_bbox(m);
        let r = Rect::from_points(
            crisp_point(r.origin(), dpr),
            crisp_point(Point::new(r.x1, r.y1), dpr),
        );
        backend.set_transform(Affine::IDENTITY);
        backend.set_color(theme.select.multiply_alpha(0.12));
        backend.fill_rect(&r);
        backend.set_color(theme.select);
        backend.stroke_path(&r.to_path(0.1), &hairline);
    }
    for line in &overlay.snap_lines {
        let mut path = BezPath::new();
        path.move_to(crisp_point(view * line.p0, dpr));
        path.line_to(crisp_point(view * line.p1, dpr));
        backend.set_transform(Affine::IDENTITY);
        backend.set_color(theme.snap);
        backend.stroke_path(&path, &hairline);
    }
    if let Some(t) = &overlay.text_edit {
        text_edit(backend, view, dpr, theme, t);
    }
    if let Some(v) = &overlay.vector_edit {
        vector_edit(backend, view, dpr, theme, v);
    }
    selection(backend, text, scene, camera, overlay);
    measurements(backend, text, camera, overlay);
}

/// Правка текста: контур рамки, выделение, подчёркивание IME и курсор в 1 CSS px.
fn text_edit(
    backend: &mut impl RenderBackend,
    view: Affine,
    dpr: f64,
    theme: &OverlayTheme,
    t: &TextEditOverlay,
) {
    let to_device = view * t.transform;
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    let mut frame = BezPath::new();
    let corners = [
        Point::new(t.frame.x0, t.frame.y0),
        Point::new(t.frame.x1, t.frame.y0),
        Point::new(t.frame.x1, t.frame.y1),
        Point::new(t.frame.x0, t.frame.y1),
    ];
    for (i, c) in corners.iter().enumerate() {
        let p = crisp_point(to_device * *c, dpr);
        if i == 0 {
            frame.move_to(p);
        } else {
            frame.line_to(p);
        }
    }
    frame.close_path();
    backend.stroke_path(&frame, &Stroke::new(dpr));
    if !t.selection.is_empty() {
        backend.set_transform(to_device);
        backend.set_color(theme.select.multiply_alpha(TEXT_SELECTION_ALPHA));
        for r in &t.selection {
            backend.fill_rect(r);
        }
    }
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    for &(x0, x1, y) in &t.composition {
        let a = crisp_point(to_device * Point::new(x0, y), dpr);
        let b = crisp_point(to_device * Point::new(x1, y), dpr);
        let mut path = BezPath::new();
        path.move_to(a);
        path.line_to(b);
        backend.stroke_path(&path, &Stroke::new(dpr));
    }
    if let Some((x, top, bottom)) = t.caret {
        let a = crisp_point(to_device * Point::new(x, top), dpr);
        let b = crisp_point(to_device * Point::new(x, bottom), dpr);
        let mut path = BezPath::new();
        path.move_to(a);
        path.line_to(b);
        backend.stroke_path(&path, &Stroke::new(dpr));
    }
}

/// Правка вектора: все сегменты тонкой линией, выделенные — толще, ручки
/// выделенного — линией с кружком, точки — кружками (выделенные залиты).
/// Рисуется только видимое на экране.
fn vector_edit(
    backend: &mut impl RenderBackend,
    view: Affine,
    dpr: f64,
    theme: &OverlayTheme,
    v: &VectorEditOverlay,
) {
    let to_device = view * v.transform;
    let (w, h) = backend.size();
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64).inflate(8.0 * dpr, 8.0 * dpr);
    let net = &*v.net;
    let pt = |i: u32| {
        let p = &net.points[i as usize];
        Point::new(p.x, p.y)
    };
    // Сегменты по порядку: если сегмент начинается там, где кончился прежний, путь
    // продолжается без нового подпути (так большую сеть можно упростить целиком).
    let seg_path = |i: usize, path: &mut BezPath, last: &mut Option<Point>| {
        let s = &net.segments[i];
        let (a, b) = (to_device * pt(s.start), to_device * pt(s.end));
        let ta =
            to_device * (pt(s.start) + kurbo::Vec2::new(s.tangent_start[0], s.tangent_start[1]));
        let tb = to_device * (pt(s.end) + kurbo::Vec2::new(s.tangent_end[0], s.tangent_end[1]));
        let bb = Rect::from_points(a, b).union_pt(ta).union_pt(tb);
        if bb.x1 < screen.x0 || bb.x0 > screen.x1 || bb.y1 < screen.y0 || bb.y0 > screen.y1 {
            *last = None;
            return;
        }
        if *last != Some(a) {
            path.move_to(a);
        }
        *last = Some(b);
        if s.is_straight() {
            path.line_to(b);
        } else {
            path.curve_to(ta, tb, b);
        }
    };
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    let mut all = BezPath::new();
    let mut last = None;
    for i in 0..net.segments.len() {
        seg_path(i, &mut all, &mut last);
    }
    // Большая сеть — ломаной с точностью в четверть пикселя устройства (как списки
    // отображения): тысячи сегментов меньше пикселя обводить по одному незачем.
    if all.elements().len() > ferrum_scene::vector::SIMPLIFY_MIN_ELEMENTS {
        all = ferrum_scene::vector::simplify_path(&all, crate::display::LOD_TOLERANCE_PX);
    }
    backend.stroke_path(&all, &Stroke::new(dpr));
    if !v.segments.is_empty() {
        let mut sel = BezPath::new();
        let mut last = None;
        for &i in &v.segments {
            if (i as usize) < net.segments.len() {
                seg_path(i as usize, &mut sel, &mut last);
            }
        }
        backend.stroke_path(&sel, &Stroke::new(2.0 * dpr));
    }
    if let Some((a, b)) = v.rubber {
        let mut r = BezPath::new();
        r.move_to(to_device * a);
        r.line_to(to_device * b);
        backend.stroke_path(&r, &Stroke::new(dpr));
    }
    // Точки на экране; при толпе — только выделенные.
    let visible: Vec<u32> = (0..net.points.len() as u32)
        .filter(|&i| screen.contains(to_device * pt(i)))
        .collect();
    let selected_visible = visible
        .iter()
        .filter(|i| v.points.binary_search(i).is_ok())
        .count();
    // Выделенных на экране больше предела (Ctrl/Cmd+A на большой сети): кружки и
    // ручки по одному не рисуются — выделение видно по сети, обведённой толще.
    if selected_visible > VECTOR_MAX_POINTS_DRAWN {
        backend.set_color(theme.select);
        backend.stroke_path(&all, &Stroke::new(2.0 * dpr));
        return;
    }
    // Ручки на экране: линия от точки и кружок на конце — тремя путями на все.
    let handle_r = VECTOR_HANDLE_RADIUS * dpr;
    let mut chosen = v.selected_handles.clone();
    chosen.sort_unstable();
    let mut lines = BezPath::new();
    let mut plain = BezPath::new();
    let mut picked = BezPath::new();
    let mut drawn = 0;
    for &(i, at_start) in &v.handles {
        let Some(s) = net.segments.get(i as usize) else {
            continue;
        };
        let (p, t) = if at_start {
            (pt(s.start), s.tangent_start)
        } else {
            (pt(s.end), s.tangent_end)
        };
        let a = to_device * p;
        let b = to_device * (p + kurbo::Vec2::new(t[0], t[1]));
        if !Rect::from_points(a, b).overlaps(screen) {
            continue;
        }
        drawn += 1;
        if drawn > VECTOR_MAX_POINTS_DRAWN {
            // Ручек на экране больше предела — не рисуется ни одна.
            lines = BezPath::new();
            plain = BezPath::new();
            picked = BezPath::new();
            break;
        }
        lines.move_to(a);
        lines.line_to(b);
        let circle = kurbo::Circle::new(b, handle_r).path_elements(0.1);
        if chosen.binary_search(&(i, at_start)).is_ok() {
            picked.extend(circle);
        } else {
            plain.extend(circle);
        }
    }
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    backend.stroke_path(&lines, &Stroke::new(dpr));
    backend.set_color(theme.handle_fill);
    backend.fill_path(&plain, Fill::NonZero);
    backend.set_color(theme.select);
    backend.fill_path(&picked, Fill::NonZero);
    backend.stroke_path(&plain, &Stroke::new(dpr));
    backend.stroke_path(&picked, &Stroke::new(dpr));
    let crowd = visible.len() > VECTOR_MAX_POINTS_DRAWN;
    let r = VECTOR_POINT_RADIUS * dpr;
    let mut filled = BezPath::new();
    let mut hollow = BezPath::new();
    for i in visible {
        let selected = v.points.binary_search(&i).is_ok();
        if crowd && !selected {
            continue;
        }
        let c = kurbo::Circle::new(to_device * pt(i), r);
        if selected {
            filled.extend(c.path_elements(0.1));
        } else {
            hollow.extend(c.path_elements(0.1));
        }
    }
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.handle_fill);
    backend.fill_path(&hollow, Fill::NonZero);
    backend.set_color(theme.select);
    backend.stroke_path(&hollow, &Stroke::new(dpr));
    backend.fill_path(&filled, Fill::NonZero);
    backend.set_color(theme.handle_fill);
    backend.stroke_path(&filled, &Stroke::new(dpr));
}

/// Контур формы узла в пикселях устройства; у группы — рамка её границ.
fn node_outline(scene: &Scene, id: NodeId, view: Affine, screen: Rect) -> Option<BezPath> {
    let n = scene.node(id)?;
    let to_device = view * n.world;
    let mut path = BezPath::new();
    match n.geometry() {
        Geometry::Line(len) => {
            path.move_to(to_device * Point::ZERO);
            path.line_to(to_device * Point::new(len, 0.0));
        }
        Geometry::None => path = to_device * n.local_bounds?.to_path(0.1),
        // Вектор подсвечивается своей сетью (контуры из кэша узла), а не рамкой.
        Geometry::Vector(_) => path = device_path(&n.path_shape()?.stroke, to_device, screen),
        geom => {
            geom.fill_path_into(&mut path);
            path.apply_affine(to_device);
        }
    }
    Some(path)
}

/// Путь в пикселях устройства для подсветки: сегменты, чья рамка ручек не
/// задевает экран, отброшены; большой путь — с точностью в четверть пикселя
/// (как списки отображения). Сеть в 100 000 точек иначе обводилась бы целиком
/// на каждом кадре, пока указатель над ней.
fn device_path(path: &BezPath, to_device: Affine, screen: Rect) -> BezPath {
    use kurbo::PathSeg;
    let mut out = BezPath::new();
    let mut last: Option<Point> = None;
    for seg in path.segments() {
        let seg = to_device * seg;
        let (start, end, bb) = match seg {
            PathSeg::Line(l) => (l.p0, l.p1, Rect::from_points(l.p0, l.p1)),
            PathSeg::Quad(q) => (q.p0, q.p2, Rect::from_points(q.p0, q.p2).union_pt(q.p1)),
            PathSeg::Cubic(c) => (
                c.p0,
                c.p3,
                Rect::from_points(c.p0, c.p3).union_pt(c.p1).union_pt(c.p2),
            ),
        };
        if !bb.overlaps(screen) {
            last = None;
            continue;
        }
        if last != Some(start) {
            out.move_to(start);
        }
        last = Some(end);
        match seg {
            PathSeg::Line(l) => out.line_to(l.p1),
            PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
            PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
        }
    }
    if out.elements().len() > ferrum_scene::vector::SIMPLIFY_MIN_ELEMENTS {
        out = ferrum_scene::vector::simplify_path(&out, crate::display::LOD_TOLERANCE_PX);
    }
    out
}

/// Целые мировые координаты линий сетки на отрезке `lo..=hi`. Число линий задаётся
/// счётчиком, а не накоплением `x += 1.0`: при |x| ≥ 2^53 прибавление единицы
/// ничего не меняет, и такой цикл не заканчивался бы. Предел защищает и от
/// чрезмерного числа заливок на огромных вьюпортах.
fn grid_lines(lo: f64, hi: f64) -> impl Iterator<Item = f64> {
    let base = lo.floor();
    let count = ((hi - base).floor() + 1.0).clamp(0.0, MAX_GRID_LINES as f64) as usize;
    (0..count).map(move |i| base + i as f64)
}

fn pixel_grid(backend: &mut impl RenderBackend, camera: &Camera, color: Rgba) {
    let visible = camera.visible_world_rect();
    let view = camera.view_transform();
    let (w, h) = backend.size();
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(color);
    for x in grid_lines(visible.x0, visible.x1) {
        let dx = (view * Point::new(x, 0.0)).x.round();
        backend.fill_rect(&Rect::new(dx, 0.0, dx + 1.0, h as f64));
    }
    for y in grid_lines(visible.y0, visible.y1) {
        let dy = (view * Point::new(0.0, y)).y.round();
        backend.fill_rect(&Rect::new(0.0, dy, w as f64, dy + 1.0));
    }
}

/// Названия фреймов верхнего уровня над их левым верхним углом, усечённые до
/// ширины фрейма. Выделенный фрейм подписан цветом выделения.
fn frame_labels(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    scene: &Scene,
    camera: &Camera,
    overlay: &Overlay,
) {
    let dpr = camera.effective_dpr();
    let view = camera.view_transform();
    let (w, h) = backend.size();
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64);
    for &slot in scene.roots() {
        let n = scene.at(slot);
        if n.props.kind != NodeKind::Frame || !n.props.visible {
            continue;
        }
        let Some(b) = n.world_bounds.map(|b| view.transform_rect_bbox(b)) else {
            continue;
        };
        let gap = 4.0 * dpr;
        if b.x1 < 0.0
            || b.x0 > screen.x1
            || b.y0 < 0.0
            || b.y0 - gap - LABEL_SIZE * dpr * 1.5 > screen.y1
        {
            continue;
        }
        // Фрейм уже многоточия: подписи не будет, раскладывать название незачем
        // (на малом зуме так отсекаются почти все фреймы).
        if b.width() < text.ellipsis_width(LABEL_SIZE * dpr, 400) {
            continue;
        }
        let Some(label) = text.label(&n.props.name, LABEL_SIZE * dpr, 400, b.width()) else {
            continue;
        };
        let color = if overlay.selection.contains(&n.id) {
            overlay.theme.select
        } else {
            overlay.theme.frame_label
        };
        label.draw(backend, Point::new(b.x0, b.y0 - gap - label.descent), color);
    }
}

/// Размер для бейджа: у одного узла — ширина и высота его формы (у повёрнутого —
/// собственные, как в Figma), у нескольких — общая рамка их форм в мире. Обводки
/// не входят ни там, ни там: иначе один и тот же узел показывал бы разный размер
/// в одиночном и в общем выделении. Узлы — с `local_bounds`.
fn selection_size(nodes: &[&SceneNode]) -> (f64, f64) {
    if let [n] = nodes {
        let b = n.local_bounds.expect("узел выделения с границами");
        return (b.width(), b.height());
    }
    let world = nodes
        .iter()
        .filter_map(|n| n.local_bounds.map(|b| n.world.transform_rect_bbox(b)))
        .reduce(|a, b| a.union(b))
        .expect("непустое выделение");
    (world.width(), world.height())
}

fn selection(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    scene: &Scene,
    camera: &Camera,
    overlay: &Overlay,
) {
    let dpr = camera.effective_dpr();
    let view = camera.view_transform();
    let theme = &overlay.theme;
    let nodes: Vec<_> = overlay
        .selection
        .iter()
        .filter_map(|id| scene.node(*id))
        .filter(|n| n.local_bounds.is_some())
        .collect();
    if nodes.is_empty() {
        return;
    }
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    let hairline = Stroke::new(dpr);
    // Рамка каждого узла — его повёрнутый прямоугольник.
    let mut boxes = Vec::new();
    for n in &nodes {
        let to_device = view * n.world;
        let b = n.local_bounds.expect("отфильтровано выше");
        let c = to_device.as_coeffs();
        // Рамка неповёрнутого узла выравнивается по пикселям, повёрнутого — нет.
        let axis_aligned = c[1] == 0.0 && c[2] == 0.0;
        let quad = [
            Point::new(b.x0, b.y0),
            Point::new(b.x1, b.y0),
            Point::new(b.x1, b.y1),
            Point::new(b.x0, b.y1),
        ]
        .map(|p| {
            let d = to_device * p;
            if axis_aligned {
                crisp_point(d, dpr)
            } else {
                d
            }
        });
        let mut path = BezPath::new();
        path.move_to(quad[0]);
        for p in &quad[1..] {
            path.line_to(*p);
        }
        path.close_path();
        backend.stroke_path(&path, &hairline);
        boxes.push(quad);
    }

    // Ручки и бейдж: у одного узла — по его повёрнутой рамке, у нескольких — по
    // общей рамке на экране.
    let size = selection_size(&nodes);
    let (quad, is_line) = if let [n] = nodes[..] {
        (boxes[0], n.props.kind == NodeKind::Line)
    } else {
        let r = boxes
            .iter()
            .flat_map(|q| q.iter())
            .fold(Rect::from_points(boxes[0][0], boxes[0][0]), |r, p| {
                r.union_pt(*p)
            });
        let r = Rect::from_points(
            crisp_point(r.origin(), dpr),
            crisp_point(Point::new(r.x1, r.y1), dpr),
        );
        backend.stroke_path(&r.to_path(0.1), &hairline);
        let quad = [
            Point::new(r.x0, r.y0),
            Point::new(r.x1, r.y0),
            Point::new(r.x1, r.y1),
            Point::new(r.x0, r.y1),
        ];
        (quad, false)
    };
    let handles: Vec<Point> = if is_line {
        vec![quad[0], quad[1]]
    } else {
        (0..4)
            .flat_map(|i| [quad[i], quad[i].midpoint(quad[(i + 1) % 4])])
            .collect()
    };
    let angle = (quad[1] - quad[0]).atan2();
    let half = HANDLE_SIZE * dpr / 2.0;
    let square = Rect::new(-half, -half, half, half).to_path(0.1);
    for h in handles {
        backend.set_transform(Affine::translate(h.to_vec2()) * Affine::rotate(angle));
        backend.set_color(theme.handle_fill);
        backend.fill_path(&square, Fill::NonZero);
        backend.set_color(theme.select);
        backend.stroke_path(&square, &hairline);
    }

    let caption = if is_line {
        format_size(size.0)
    } else {
        format!("{} × {}", format_size(size.0), format_size(size.1))
    };
    let Some(label) = text.label(&caption, LABEL_SIZE * dpr, 500, f64::MAX) else {
        return;
    };
    let bottom = quad.iter().fold(f64::MIN, |m, p| m.max(p.y));
    let center_x = quad.iter().map(|p| p.x).sum::<f64>() / 4.0;
    badge(
        backend,
        &label,
        dpr,
        center_x,
        bottom + 6.0 * dpr,
        theme.select,
        theme.on_select,
    );
}

/// Высота плашки с подписью `label` (поля 2 px сверху и снизу).
fn badge_height(label: &Label, dpr: f64) -> f64 {
    label.ascent + label.descent + 4.0 * dpr
}

/// Плашка с подписью: по центру `center_x`, верхний край — `top` (пиксели
/// устройства). Та же у бейджа «W × H» и у значений измерений.
fn badge(
    backend: &mut impl RenderBackend,
    label: &Label,
    dpr: f64,
    center_x: f64,
    top: f64,
    fill: Rgba,
    ink: Rgba,
) {
    let (pad_x, pad_y) = (4.0 * dpr, 2.0 * dpr);
    let pill = Rect::new(
        center_x - label.width / 2.0 - pad_x,
        top,
        center_x + label.width / 2.0 + pad_x,
        top + badge_height(label, dpr),
    );
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(fill);
    backend.fill_path(
        &RoundedRect::from_rect(pill, 2.0 * dpr).to_path(0.1),
        Fill::NonZero,
    );
    label.draw(
        backend,
        Point::new(pill.x0 + pad_x, pill.y0 + pad_y + label.ascent),
        ink,
    );
}

/// Отрезок измерения в пикселях устройства. У горизонтального и вертикального
/// по пикселям выравнивается поперечная координата: линия в 1 px не размывается,
/// а концы остаются на краях объектов.
fn device_segment(view: Affine, line: Line, width: f64) -> (Point, Point) {
    let (mut a, mut b) = (view * line.p0, view * line.p1);
    if a.y == b.y {
        a.y = crisp(a.y, width);
        b.y = a.y;
    } else if a.x == b.x {
        a.x = crisp(a.x, width);
        b.x = a.x;
    }
    (a, b)
}

/// Наибольшее число штрихов пунктира одной направляющей (защита от огромных
/// холстов; видимая часть отрезка короче).
const MAX_DASHES: usize = 4096;

/// Часть отрезка `a + dir·t`, `t ∈ [0, len]`, внутри `clip` (Лянг — Барски).
fn clip_span(a: Point, dir: kurbo::Vec2, len: f64, clip: Rect) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0_f64, len);
    for (p, d, lo, hi) in [
        (a.x, dir.x, clip.x0, clip.x1),
        (a.y, dir.y, clip.y0, clip.y1),
    ] {
        if d == 0.0 {
            if p < lo || p > hi {
                return None;
            }
        } else {
            let (u, v) = ((lo - p) / d, (hi - p) / d);
            t0 = t0.max(u.min(v));
            t1 = t1.min(u.max(v));
        }
    }
    (t0 <= t1).then_some((t0, t1))
}

/// Пунктир от `a` к `b`: штрихи `MEASURE_DASH` через `MEASURE_GAP` (CSS-пиксели),
/// первый — от `a`; строятся только штрихи внутри `clip`, поэтому их число зависит
/// от экрана, а не от длины направляющей в мире. Штрихи строит сам оверлей, а не
/// узор обводки: так пунктир одинаков в `vello_cpu` и `vello_gpu`.
fn dashes(a: Point, b: Point, dpr: f64, clip: Rect) -> BezPath {
    let mut path = BezPath::new();
    let v = b - a;
    let len = v.hypot();
    if !len.is_finite() || len <= 0.0 {
        return path;
    }
    let dir = v / len;
    let Some((lo, hi)) = clip_span(a, dir, len, clip) else {
        return path;
    };
    let (dash, period) = (MEASURE_DASH * dpr, (MEASURE_DASH + MEASURE_GAP) * dpr);
    let first = (lo / period).floor();
    let count = (((hi / period).ceil() - first).max(0.0) as usize).min(MAX_DASHES);
    for i in 0..count {
        let t0 = (first + i as f64) * period;
        let t1 = (t0 + dash).min(len);
        if t1 <= t0 {
            break;
        }
        path.move_to(a + dir * t0);
        path.line_to(a + dir * t1);
    }
    path
}

/// Измерения расстояний с Alt: сплошные линии с плашкой значения посередине и
/// пунктирные направляющие. Толщины и плашки — в экранных пикселях, от зума не
/// зависят; значения — в единицах документа.
fn measurements(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    camera: &Camera,
    overlay: &Overlay,
) {
    if overlay.measurements.is_empty() {
        return;
    }
    let dpr = camera.effective_dpr();
    let view = camera.view_transform();
    let theme = &overlay.theme;
    let hairline = Stroke::new(dpr);
    let (w, h) = backend.size();
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64);
    let clip = screen.inflate(8.0 * dpr, 8.0 * dpr);
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.measure);
    // Сначала все линии, потом плашки: плашка не перекрывается чужой линией.
    for m in &overlay.measurements {
        let (a, b) = device_segment(view, m.line, dpr);
        let path = match m.kind {
            // Сплошная линия — только видимая часть: при большом зуме линия до
            // дальнего края фрейма уходит за экран на тысячи пикселей.
            MeasureKind::Distance => match visible(a, b, clip) {
                Some((p, q)) => {
                    let mut path = BezPath::new();
                    path.move_to(p);
                    path.line_to(q);
                    path
                }
                None => continue,
            },
            MeasureKind::Guide => dashes(a, b, dpr, clip),
        };
        backend.stroke_path(&path, &hairline);
    }
    for m in &overlay.measurements {
        if m.kind != MeasureKind::Distance {
            continue;
        }
        let (a, b) = device_segment(view, m.line, dpr);
        // Плашка — посередине видимой части линии и целиком на экране.
        let Some((p, q)) = visible(a, b, screen) else {
            continue;
        };
        let Some(label) = text.label(&format_size(m.value), LABEL_SIZE * dpr, 500, f64::MAX) else {
            continue;
        };
        let (half_w, half_h) = (
            label.width / 2.0 + 4.0 * dpr,
            badge_height(&label, dpr) / 2.0,
        );
        let keep = |v: f64, half: f64, side: f64| {
            if side >= 2.0 * half {
                v.clamp(half, side - half)
            } else {
                v
            }
        };
        let mid = p.midpoint(q);
        let (cx, cy) = (
            keep(mid.x, half_w, screen.x1),
            keep(mid.y, half_h, screen.y1),
        );
        badge(
            backend,
            &label,
            dpr,
            cx,
            cy - half_h,
            theme.measure,
            theme.on_select,
        );
    }
}

/// Видимая в `clip` часть отрезка `a → b` (пиксели устройства).
fn visible(a: Point, b: Point, clip: Rect) -> Option<(Point, Point)> {
    let v = b - a;
    let len = v.hypot();
    if !len.is_finite() || len <= 0.0 {
        return None;
    }
    let dir = v / len;
    let (t0, t1) = clip_span(a, dir, len, clip)?;
    Some((a + dir * t0, a + dir * t1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_doc::{Document, PropsPatch, Stroke as DocStroke, StrokeAlign};

    #[test]
    fn multi_selection_size_ignores_strokes_like_single() {
        // Размер в бейдже — по формам узлов, без обводок: у одного узла это его
        // ширина и высота, у нескольких — общая рамка их форм в мире.
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        let ids = doc
            .transact(page, vec![], |p| {
                let mut ids = Vec::new();
                for (x, y) in [(0.0, 0.0), (100.0, 50.0)] {
                    let id = p.create_node(None, None, NodeKind::Rect)?;
                    p.set_props(
                        id,
                        &PropsPatch {
                            x: Some(x),
                            y: Some(y),
                            w: Some(40.0),
                            h: Some(30.0),
                            strokes: Some(vec![DocStroke {
                                color: ferrum_doc::Color::rgba(0, 0, 0, 255),
                                width: 10.0,
                                align: StrokeAlign::Outside,
                            }]),
                            ..Default::default()
                        },
                    )?;
                    ids.push(id);
                }
                Ok(ids)
            })
            .unwrap();
        let scene = Scene::from_page(doc.page(page).unwrap());
        let nodes: Vec<_> = ids.iter().map(|id| scene.node(*id).unwrap()).collect();
        assert_eq!(selection_size(&nodes[..1]), (40.0, 30.0));
        assert_eq!(selection_size(&nodes), (140.0, 80.0));
    }

    /// Штрихи пунктира: (начало, конец) каждого отрезка пути.
    fn dash_spans(path: &BezPath) -> Vec<(Point, Point)> {
        use kurbo::PathEl;
        let mut out = Vec::new();
        let mut start = Point::ZERO;
        for el in path.elements() {
            match el {
                PathEl::MoveTo(p) => start = *p,
                PathEl::LineTo(p) => out.push((start, *p)),
                _ => panic!("в пунктире только отрезки"),
            }
        }
        out
    }

    #[test]
    fn dashes_follow_pattern_and_stay_on_screen() {
        let screen = Rect::new(0.0, 0.0, 100.0, 100.0);
        // Штрихи 4 через 3 от начала; последний усечён концом отрезка.
        let d = dash_spans(&dashes(
            Point::new(10.0, 5.0),
            Point::new(26.0, 5.0),
            1.0,
            screen,
        ));
        assert_eq!(
            d,
            [
                (Point::new(10.0, 5.0), Point::new(14.0, 5.0)),
                (Point::new(17.0, 5.0), Point::new(21.0, 5.0)),
                (Point::new(24.0, 5.0), Point::new(26.0, 5.0)),
            ]
        );
        // dpr 2: штрих и промежуток вдвое длиннее.
        let d = dash_spans(&dashes(
            Point::new(0.0, 5.0),
            Point::new(0.0, 20.0),
            2.0,
            screen,
        ));
        assert_eq!(d[0], (Point::new(0.0, 5.0), Point::new(0.0, 13.0)));
        assert_eq!(d[1].0, Point::new(0.0, 19.0));
        // Направляющая в миллиард пикселей при большом зуме: строятся только штрихи у
        // экрана, в той же фазе, что от начала, и видимая часть покрыта.
        let d = dash_spans(&dashes(
            Point::new(-1e9, 50.0),
            Point::new(1e9, 50.0),
            1.0,
            screen,
        ));
        assert!(d.len() <= 20, "штрихов: {}", d.len());
        assert!(d.first().unwrap().0.x <= 0.0 && d.last().unwrap().1.x >= 100.0);
        for (a, _) in &d {
            let phase = (a.x + 1e9).rem_euclid(7.0);
            assert!(
                phase.abs() < 1e-3 || (phase - 7.0).abs() < 1e-3,
                "фаза {phase}"
            );
        }
        // Вне экрана — ни одного штриха; вырожденный и нечисловой отрезок — тоже.
        assert!(dashes(Point::new(0.0, 500.0), Point::new(90.0, 500.0), 1.0, screen).is_empty());
        assert!(dashes(Point::new(5.0, 5.0), Point::new(5.0, 5.0), 1.0, screen).is_empty());
        assert!(dashes(Point::new(f64::NAN, 5.0), Point::new(5.0, 5.0), 1.0, screen).is_empty());
    }

    #[test]
    fn grid_lines_are_bounded_and_exact() {
        assert_eq!(
            grid_lines(1.5, 4.2).collect::<Vec<_>>(),
            [1.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(grid_lines(1e16, 1e16 + 50.0).count(), 51);
        assert_eq!(grid_lines(-1e300, 1e300).count(), MAX_GRID_LINES);
        assert_eq!(grid_lines(5.0, 4.0).count(), 0);
    }
}
