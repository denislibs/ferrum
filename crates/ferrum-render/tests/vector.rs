//! Рендер векторов и обводки по контуру (подсистема 3, этап 3): области с
//! правилами заливки и своими заливками, каждое окончание, каждое соединение и
//! предел острого угла, обводка внутри, по центру и снаружи замкнутого пути,
//! стрелка у линии, полупрозрачная обводка с окончаниями. Каждый вариант —
//! пиксели, затем эталон. Пересоздать эталоны:
//! `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test vector`.

mod common;

use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_scene::vector::network_bounds;

/// Вектор по точкам в мировых координатах: рамка узла — границы сети.
fn add_vector(
    p: &mut Page,
    points: &[(f64, f64)],
    segs: &[(u32, u32)],
    regions: Vec<VectorRegion>,
    patch: PropsPatch,
) -> NodeId {
    let net = VectorNetwork {
        points: points
            .iter()
            .map(|&(x, y)| VectorPoint::new(x, y))
            .collect(),
        segments: segs
            .iter()
            .map(|&(a, b)| VectorSegment::line(a, b))
            .collect(),
        regions,
    };
    let b = network_bounds(&net).unwrap();
    p.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            x: Some(b.x0),
            y: Some(b.y0),
            w: Some(b.width()),
            h: Some(b.height()),
            vector: Some(Arc::new(net)),
            ..patch
        },
    )
}

fn black(width: f64, align: StrokeAlign) -> Option<Vec<Stroke>> {
    Some(vec![Stroke {
        color: Color::BLACK,
        width,
        align,
    }])
}

fn solid(r: u8, g: u8, b: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, 255))])
}

/// Точки и сегменты сети.
type Net = (Vec<(f64, f64)>, Vec<(u32, u32)>);

/// Квадрат 80 × 80 от `(x, y)` и квадрат-дырка внутри; `same_dir` — дырка
/// обходится в ту же сторону (иначе — в обратную).
fn square_with_hole(x: f64, y: f64, same_dir: bool) -> Net {
    let pts = vec![
        (x, y),
        (x + 80.0, y),
        (x + 80.0, y + 80.0),
        (x, y + 80.0),
        (x + 30.0, y + 30.0),
        (x + 50.0, y + 30.0),
        (x + 50.0, y + 50.0),
        (x + 30.0, y + 50.0),
    ];
    let hole = if same_dir {
        [(4, 5), (5, 6), (6, 7), (7, 4)]
    } else {
        [(4, 7), (7, 6), (6, 5), (5, 4)]
    };
    let mut segs = vec![(0, 1), (1, 2), (2, 3), (3, 0)];
    segs.extend(hole);
    (pts, segs)
}

#[test]
fn vector_regions() {
    let mut p = Page::new();
    // Even-odd: дырка пустая.
    let (pts, segs) = square_with_hole(10.0, 10.0, true);
    add_vector(
        &mut p,
        &pts,
        &segs,
        vec![VectorRegion {
            rule: FillRule::EvenOdd,
            loops: vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
            fills: None,
        }],
        PropsPatch {
            fills: solid(255, 0, 0),
            strokes: Some(vec![]),
            ..Default::default()
        },
    );
    // Non-zero, петли в одну сторону: дырка залита.
    let (pts, segs) = square_with_hole(100.0, 10.0, true);
    add_vector(
        &mut p,
        &pts,
        &segs,
        vec![VectorRegion {
            rule: FillRule::NonZero,
            loops: vec![vec![0, 1, 2, 3], vec![4, 5, 6, 7]],
            fills: None,
        }],
        PropsPatch {
            fills: solid(255, 0, 0),
            strokes: Some(vec![]),
            ..Default::default()
        },
    );
    // Своя заливка области (синяя) у узла без заливок, обводка по центру.
    add_vector(
        &mut p,
        &[(190.0, 90.0), (250.0, 90.0), (220.0, 10.0)],
        &[(0, 1), (1, 2), (2, 0)],
        vec![VectorRegion {
            rule: FillRule::NonZero,
            loops: vec![vec![0, 1, 2]],
            fills: solid(0, 0, 255),
        }],
        PropsPatch {
            strokes: black(2.0, StrokeAlign::Center),
            ..Default::default()
        },
    );
    let img = render(&p.scene(), &camera(260.0, 100.0));
    assert_px(&img, 20, 20, [255, 0, 0], 0);
    assert_px(&img, 50, 50, [255, 255, 255], 0);
    assert_px(&img, 110, 20, [255, 0, 0], 0);
    assert_px(&img, 140, 50, [255, 0, 0], 0);
    assert_px(&img, 220, 70, [0, 0, 255], 0);
    assert_px(&img, 220, 90, [0, 0, 0], 40);
    assert_px(&img, 220, 94, [255, 255, 255], 0);
    golden("vector_regions", &img);
}

#[test]
fn stroke_caps() {
    let mut p = Page::new();
    // Окончание на обоих концах отрезка (40, y)–(200, y) толщиной 4.
    for (i, cap) in StrokeCap::ALL.into_iter().enumerate() {
        let y = 20.0 + 30.0 * i as f64;
        add_vector(
            &mut p,
            &[(40.0, y), (200.0, y)],
            &[(0, 1)],
            vec![],
            PropsPatch {
                strokes: black(4.0, StrokeAlign::Center),
                stroke_cap_start: Some(cap),
                stroke_cap_end: Some(cap),
                ..Default::default()
            },
        );
    }
    // Полупрозрачная обводка с кругами на концах: перекрытие не темнее.
    add_vector(
        &mut p,
        &[(40.0, 230.0), (200.0, 230.0)],
        &[(0, 1)],
        vec![],
        PropsPatch {
            strokes: Some(vec![Stroke {
                color: Color::rgba(0, 0, 0, 128),
                width: 4.0,
                align: StrokeAlign::Center,
            }]),
            stroke_cap_end: Some(StrokeCap::Circle),
            ..Default::default()
        },
    );
    let img = render(&p.scene(), &camera(240.0, 250.0));
    let row = |i: usize| 20 + 30 * i as u16;
    // None: плоский конец.
    assert_px(&img, 198, row(0), [0, 0, 0], 0);
    assert_px(&img, 202, row(0), [255, 255, 255], 0);
    assert_px(&img, 38, row(0), [255, 255, 255], 0);
    // Round: полукруг радиуса 2 за концом, угол квадрата пуст.
    assert_px(&img, 201, row(1), [0, 0, 0], 60);
    assert_px(&img, 203, row(1), [255, 255, 255], 0);
    assert_px(&img, 201, row(1) + 1, [255, 255, 255], 120);
    // Square: квадрат на 2 за концом, угол залит.
    assert_px(&img, 201, row(2) + 1, [0, 0, 0], 0);
    assert_px(&img, 203, row(2), [255, 255, 255], 0);
    // ArrowLines: плечо длиной 18 под 30° назад.
    assert_px(&img, 192, row(3) + 4, [0, 0, 0], 60);
    assert_px(&img, 38, row(3) + 4, [255, 255, 255], 0);
    assert_px(&img, 48, row(3) + 4, [0, 0, 0], 60);
    // ArrowTriangle: остриё на 204, у основания шире линии.
    assert_px(&img, 195, row(4) + 3, [0, 0, 0], 0);
    assert_px(&img, 206, row(4), [255, 255, 255], 0);
    assert_px(&img, 34, row(4), [255, 255, 255], 0);
    assert_px(&img, 38, row(4), [0, 0, 0], 60);
    // Circle: радиус 7,5; Diamond: тот же радиус ромбом — угол пуст.
    assert_px(&img, 200, row(5) + 6, [0, 0, 0], 0);
    assert_px(&img, 204, row(5) + 4, [0, 0, 0], 0);
    assert_px(&img, 200, row(5) + 9, [255, 255, 255], 0);
    assert_px(&img, 200, row(6) + 5, [0, 0, 0], 0);
    assert_px(&img, 204, row(6) + 4, [255, 255, 255], 0);
    // Полупрозрачная: линия и круг одного тона, перекрытия нет.
    let line = px(&img, 150, 230);
    let joint = px(&img, 198, 230);
    let dot = px(&img, 200, 236);
    assert!(line[0].abs_diff(127) <= 2, "{line:?}");
    assert!(line[0].abs_diff(joint[0]) <= 2, "{line:?} {joint:?}");
    assert!(line[0].abs_diff(dot[0]) <= 2, "{line:?} {dot:?}");
    golden("stroke_caps", &img);
}

#[test]
fn stroke_joins_and_closed_alignment() {
    let mut p = Page::new();
    // «V» толщиной 10: острый, круглый, скошенный и острый с пределом 1,5.
    let joins = [
        (10.0, StrokeJoin::Miter, DEFAULT_MITER_LIMIT),
        (80.0, StrokeJoin::Round, DEFAULT_MITER_LIMIT),
        (150.0, StrokeJoin::Bevel, DEFAULT_MITER_LIMIT),
        (220.0, StrokeJoin::Miter, 1.5),
    ];
    for (x, join, limit) in joins {
        add_vector(
            &mut p,
            &[(x, 20.0), (x + 30.0, 90.0), (x + 60.0, 20.0)],
            &[(0, 1), (1, 2)],
            vec![],
            PropsPatch {
                strokes: black(10.0, StrokeAlign::Center),
                stroke_join: Some(join),
                stroke_miter_limit: Some(limit),
                ..Default::default()
            },
        );
    }
    // Замкнутый квадрат 30 × 30 с обводкой 6 внутри, по центру и снаружи — те же
    // полосы, что у прямоугольника (`golden::stroke_alignment`).
    for (x, align) in [
        (15.0, StrokeAlign::Inside),
        (65.0, StrokeAlign::Center),
        (115.0, StrokeAlign::Outside),
    ] {
        add_vector(
            &mut p,
            &[(x, 130.0), (x + 30.0, 130.0), (x + 30.0, 160.0), (x, 160.0)],
            &[(0, 1), (1, 2), (2, 3), (3, 0)],
            vec![VectorRegion {
                rule: FillRule::NonZero,
                loops: vec![vec![0, 1, 2, 3]],
                fills: None,
            }],
            PropsPatch {
                fills: solid(200, 200, 200),
                strokes: black(6.0, align),
                ..Default::default()
            },
        );
    }
    // Прямоугольник с круглым соединением: угол скруглён.
    p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![]),
            strokes: black(10.0, StrokeAlign::Center),
            stroke_join: Some(StrokeJoin::Round),
            ..rect(180.0, 130.0, 40.0, 30.0)
        },
    );
    // Линия со стрелкой на конце (инструмент «Стрелка»).
    p.add(
        None,
        NodeKind::Line,
        PropsPatch {
            strokes: black(2.0, StrokeAlign::Center),
            stroke_cap_end: Some(StrokeCap::ArrowLines),
            ..rect(240.0, 145.0, 50.0, 0.0)
        },
    );
    let img = render(&p.scene(), &camera(300.0, 180.0));
    // Остриё «V»: острый стык уходит до 102,7, круглый — до 95, скошенный — до 92.
    assert_px(&img, 40, 99, [0, 0, 0], 0);
    assert_px(&img, 110, 99, [255, 255, 255], 0);
    assert_px(&img, 110, 93, [0, 0, 0], 0);
    assert_px(&img, 180, 93, [255, 255, 255], 0);
    assert_px(&img, 180, 90, [0, 0, 0], 0);
    // Предел 1,5 < 2,54: острый стык становится скошенным.
    assert_px(&img, 250, 99, [255, 255, 255], 0);
    assert_px(&img, 250, 93, [255, 255, 255], 0);
    // Квадрат: внутри — пиксели 15..=20, по центру — 62..=67, снаружи — 109..=114.
    assert_px(&img, 14, 145, [255, 255, 255], 0);
    assert_px(&img, 15, 145, [0, 0, 0], 0);
    assert_px(&img, 20, 145, [0, 0, 0], 0);
    assert_px(&img, 21, 145, [200, 200, 200], 0);
    assert_px(&img, 61, 145, [255, 255, 255], 0);
    assert_px(&img, 62, 145, [0, 0, 0], 0);
    assert_px(&img, 67, 145, [0, 0, 0], 0);
    assert_px(&img, 68, 145, [200, 200, 200], 0);
    assert_px(&img, 108, 145, [255, 255, 255], 0);
    assert_px(&img, 109, 145, [0, 0, 0], 0);
    assert_px(&img, 114, 145, [0, 0, 0], 0);
    assert_px(&img, 115, 145, [200, 200, 200], 0);
    // Круглый угол прямоугольника: у острого (175, 125) было бы чёрным.
    assert_px(&img, 175, 125, [255, 255, 255], 0);
    assert_px(&img, 200, 126, [0, 0, 0], 0);
    // Стрелка: плечо под концом линии (290, 145).
    assert_px(&img, 284, 148, [0, 0, 0], 90);
    assert_px(&img, 284, 152, [255, 255, 255], 0);
    golden("stroke_joins", &img);
}

/// Острый угол замкнутого треугольника с обводкой снаружи и острым соединением
/// (предел по умолчанию 4): кончик острия дальше `2w + 1` от пути, но внутри
/// предела — обрезка внешности его не режет.
#[test]
fn outside_stroke_keeps_the_miter_tip_of_a_sharp_corner() {
    let mut p = Page::new();
    // Угол при (40, 20) около 30 градусов: стороны уходят вниз к y = 140.
    add_vector(
        &mut p,
        &[(40.0, 20.0), (72.0, 140.0), (8.0, 140.0)],
        &[(0, 1), (1, 2), (2, 0)],
        vec![VectorRegion {
            rule: FillRule::NonZero,
            loops: vec![vec![0, 1, 2]],
            fills: None,
        }],
        PropsPatch {
            fills: solid(200, 200, 200),
            strokes: black(6.0, StrokeAlign::Outside),
            stroke_join: Some(StrokeJoin::Miter),
            stroke_miter_limit: Some(DEFAULT_MITER_LIMIT),
            ..Default::default()
        },
    );
    let img = render(&p.scene(), &camera(80.0, 160.0));
    // Обводка снаружи вдвое шире (12), острие вытянуто на 6 / sin 15 градусов
    // ≈ 23 от вершины, до y ≈ -3 (отношение 3,9 < предела 4). Пиксель y = 4
    // лежит выше рамки пути на 16 > `2w + 1 = 13`.
    assert_px(&img, 40, 4, [0, 0, 0], 8);
}

#[test]
fn vector_edit_overlay() {
    use ferrum_render::{Overlay, VectorEditOverlay};
    use kurbo::{Affine, Point};
    let scene = Page::new().scene();
    // Треугольник: сегмент 1 — кривая; выделена точка 1, её ручка видна и выделена.
    let mut net = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(50.0, 80.0),
        ],
        segments: vec![
            VectorSegment::line(0, 1),
            VectorSegment::line(1, 2),
            VectorSegment::line(2, 0),
        ],
        regions: vec![],
    };
    net.segments[1].tangent_start = [30.0, 30.0];
    let overlay = Overlay {
        frame_labels: false,
        pixel_grid: false,
        vector_edit: Some(VectorEditOverlay {
            transform: Affine::translate((20.0, 20.0)),
            net: Arc::new(net),
            points: vec![1],
            segments: vec![0],
            handles: vec![(1, true)],
            selected_handles: vec![],
            rubber: Some((Point::new(50.0, 80.0), Point::new(130.0, 90.0))),
        }),
        ..Default::default()
    };
    let img = render_overlay(&scene, &camera(180.0, 120.0), &overlay, WHITE);
    let select = [0x4a, 0xa3, 0xff];
    // Невыделенная точка (20, 20) — белая середина, выделенная (120, 20) — залита.
    assert_px(&img, 20, 20, [255, 255, 255], 30);
    assert_px(&img, 120, 20, select, 30);
    // Выделенный сегмент — 2 px по верхней стороне, середина его видна.
    assert_px(&img, 70, 20, select, 30);
    // Ручка точки 1: кружок в (150, 50) — белая середина, линия к нему.
    assert_px(&img, 150, 50, [255, 255, 255], 40);
    assert_px(&img, 135, 35, select, 90);
    // Резинка пера от (70, 100) к (150, 110).
    assert_px(&img, 110, 105, select, 90);
    // Внутри треугольника — фон.
    assert_px(&img, 70, 50, [255, 255, 255], 0);
    golden("vector_edit", &img);
}

/// Бэкенд-счётчик: вызовы рисования и элементы путей, без растеризации.
struct Count {
    size: (u16, u16),
    calls: usize,
    elements: usize,
}

impl ferrum_render::RenderBackend for Count {
    fn begin_frame(&mut self, width: u16, height: u16) {
        self.size = (width, height);
        self.calls = 0;
        self.elements = 0;
    }
    fn size(&self) -> (u16, u16) {
        self.size
    }
    fn set_transform(&mut self, _: kurbo::Affine) {}
    fn set_color(&mut self, _: ferrum_render::Rgba) {}
    fn set_image(&mut self, _: &ferrum_render::ImagePaint) {}
    fn fill_path(&mut self, path: &kurbo::BezPath, _: vello_cpu::peniko::Fill) {
        self.calls += 1;
        self.elements += path.elements().len();
    }
    fn fill_rect(&mut self, _: &kurbo::Rect) {
        self.calls += 1;
    }
    fn stroke_path(&mut self, path: &kurbo::BezPath, _: &kurbo::Stroke) {
        self.calls += 1;
        self.elements += path.elements().len();
    }
    fn push_clip(&mut self, _: &kurbo::BezPath) {}
    fn pop_clip(&mut self) {}
    fn push_opacity_layer(&mut self, _: f32) {}
    fn pop_layer(&mut self) {}
}

/// Оверлей правки кривой сети из `n` точек, вписанной в 1200 × 800: выделены
/// все точки, видны все ручки.
fn huge_selection(n: usize, transform: kurbo::Affine) -> (Count, ferrum_render::Overlay) {
    use ferrum_render::{Overlay, VectorEditOverlay};
    let net = ferrum_doc::fixture::curvy_network(n, 500.0);
    let handles = (0..net.segments.len() as u32)
        .flat_map(|i| [(i, true), (i, false)])
        .collect();
    let overlay = Overlay {
        frame_labels: false,
        pixel_grid: false,
        vector_edit: Some(VectorEditOverlay {
            transform,
            points: (0..n as u32).collect(),
            net: Arc::new(net),
            segments: vec![],
            handles,
            selected_handles: vec![(0, true)],
            rubber: None,
        }),
        ..Default::default()
    };
    let count = Count {
        size: (0, 0),
        calls: 0,
        elements: 0,
    };
    (count, overlay)
}

#[test]
fn edit_overlay_of_a_huge_selection_is_bounded() {
    // Ctrl/Cmd+A на сети в 20 000 точек: точки и ручки не рисуются по одной, а
    // число вызовов и элементов не растёт с размером выделения.
    let scene = Page::new().scene();
    let fit = kurbo::Affine::translate((100.0, 20.0)) * kurbo::Affine::scale(0.75);
    let (mut count, overlay) = huge_selection(20_000, fit);
    let mut r = ferrum_render::Renderer::new();
    r.render(
        &mut count,
        &scene,
        &camera(1200.0, 800.0),
        &overlay,
        &ferrum_render::NoImages,
        None,
    );
    assert!(count.calls < 40, "вызовов {}", count.calls);
    assert!(count.elements < 25_000, "элементов {}", count.elements);
    // Приближение: на экране — сотня точек; они и их ручки видны (не больше
    // предела), невидимые отброшены.
    let near = kurbo::Affine::scale(40.0) * kurbo::Affine::translate((-990.0, -500.0));
    let (mut count, overlay) = huge_selection(20_000, near);
    r.render(
        &mut count,
        &scene,
        &camera(1200.0, 800.0),
        &overlay,
        &ferrum_render::NoImages,
        None,
    );
    assert!(count.calls < 40, "вызовов {}", count.calls);
    assert!(
        count.elements > 100,
        "точки и ручки видны: {}",
        count.elements
    );
    assert!(count.elements < 25_000, "элементов {}", count.elements);
}
