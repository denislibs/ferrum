//! Оверлеи поверх содержимого в экранных пикселях: пиксельная сетка, названия
//! фреймов, подсветка наведения, рамка выделения мышью, линии привязки, рамка
//! выделения с ручками и бейдж «W × H». Толщины и размеры задаются в CSS-пикселях
//! и умножаются на `devicePixelRatio`.

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::shape::Geometry;
use ferrum_scene::{Scene, SceneNode};
use kurbo::{Affine, BezPath, Line, Point, Rect, RoundedRect, Shape, Stroke};
use vello_cpu::peniko::Fill;

use crate::backend::{RenderBackend, Rgba};
use crate::camera::Camera;
use crate::text::TextEngine;

/// Сторона квадратной ручки выделения.
pub const HANDLE_SIZE: f64 = 8.0;
/// Пиксельная сетка видна с зума 800%.
pub const PIXEL_GRID_MIN_ZOOM: f64 = 8.0;
/// Наибольшее число линий пиксельной сетки по одной оси.
const MAX_GRID_LINES: usize = 4096;
/// Кегль подписей фреймов и бейджа.
pub const LABEL_SIZE: f64 = 11.0;

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
        }
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
        if let Some(outline) = node_outline(scene, id, view) {
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
    selection(backend, text, scene, camera, overlay);
}

/// Контур формы узла в пикселях устройства; у группы — рамка её границ.
fn node_outline(scene: &Scene, id: NodeId, view: Affine) -> Option<BezPath> {
    let n = scene.node(id)?;
    let to_device = view * n.world;
    let mut path = BezPath::new();
    match n.geometry() {
        Geometry::Line(len) => {
            path.move_to(to_device * Point::ZERO);
            path.line_to(to_device * Point::new(len, 0.0));
        }
        Geometry::None => path = to_device * n.local_bounds?.to_path(0.1),
        geom => {
            geom.fill_path_into(&mut path);
            path.apply_affine(to_device);
        }
    }
    Some(path)
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
    let (pad_x, pad_y) = (4.0 * dpr, 2.0 * dpr);
    let height = label.ascent + label.descent + 2.0 * pad_y;
    let pill = Rect::new(
        center_x - label.width / 2.0 - pad_x,
        bottom + 6.0 * dpr,
        center_x + label.width / 2.0 + pad_x,
        bottom + 6.0 * dpr + height,
    );
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    backend.fill_path(
        &RoundedRect::from_rect(pill, 2.0 * dpr).to_path(0.1),
        Fill::NonZero,
    );
    label.draw(
        backend,
        Point::new(pill.x0 + pad_x, pill.y0 + pad_y + label.ascent),
        theme.on_select,
    );
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
