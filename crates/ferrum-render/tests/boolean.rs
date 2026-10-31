//! Рендер булевых групп (подсистема 3, этап 4): результат каждой операции
//! заливками и обводкой самой группы, дети не рисуются, вложенная группа,
//! правка ребёнка перестраивает картинку, подсветка — по контуру результата.
//! Пересоздать эталоны: `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test boolean`.

mod common;

use common::*;
use ferrum_doc::*;

fn solid(r: u8, g: u8, b: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, 255))])
}

/// Булева группа `op` в `(x, y)`: красный квадрат 0..60 и синий эллипс 30..90
/// по x (дети своих цветов не показывают). Группа — зелёная с чёрной обводкой 2.
fn add_boolean(p: &mut Page, op: BooleanOp, x: f64, y: f64) -> (NodeId, NodeId) {
    let b = p.add(
        None,
        NodeKind::Boolean,
        PropsPatch {
            x: Some(x),
            y: Some(y),
            boolean_op: Some(op),
            fills: solid(0, 160, 0),
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 2.0,
                align: StrokeAlign::Center,
            }]),
            ..Default::default()
        },
    );
    p.add(
        Some(b),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0),
            ..rect(0.0, 0.0, 60.0, 60.0)
        },
    );
    let e = p.add(
        Some(b),
        NodeKind::Ellipse,
        PropsPatch {
            fills: solid(0, 0, 255),
            ..rect(30.0, 0.0, 60.0, 60.0)
        },
    );
    (b, e)
}

const GREEN: [u8; 3] = [0, 160, 0];
const WHITE: [u8; 3] = [255, 255, 255];

#[test]
fn every_op_is_drawn_with_the_group_paint() {
    let mut p = Page::new();
    for (i, op) in BooleanOp::ALL.into_iter().enumerate() {
        add_boolean(&mut p, op, 10.0 + 110.0 * i as f64, 10.0);
    }
    let img = render(&p.scene(), &camera(450.0, 80.0));
    // Объединение: квадрат и правая половина эллипса — зелёные.
    assert_px(&img, 15, 40, GREEN, 0);
    assert_px(&img, 95, 40, GREEN, 0);
    // Вычитание: левая часть квадрата есть, на месте эллипса — пусто.
    assert_px(&img, 125, 40, GREEN, 0);
    assert_px(&img, 175, 40, WHITE, 0);
    assert_px(&img, 205, 40, WHITE, 0);
    // Пересечение: только линза посередине.
    assert_px(&img, 235, 40, WHITE, 0);
    assert_px(&img, 285, 40, GREEN, 0);
    assert_px(&img, 315, 40, WHITE, 0);
    // Исключение: края есть, середина пуста.
    assert_px(&img, 345, 40, GREEN, 0);
    assert_px(&img, 395, 40, WHITE, 0);
    assert_px(&img, 425, 40, GREEN, 0);
    // Обводка — по контуру результата: граница вычитания посередине квадрата.
    assert_px(&img, 150, 40, [0, 0, 0], 90);
    // Ни красного, ни синего: дети не рисуются.
    for x in (0..450).step_by(3) {
        for y in (0..80).step_by(3) {
            let c = px(&img, x, y);
            assert!(!(c[0] > 200 && c[1] < 60), "красный в ({x}, {y})");
            assert!(!(c[2] > 200 && c[0] < 60), "синий в ({x}, {y})");
        }
    }
    golden("boolean_ops", &img);
}

#[test]
fn nested_boolean_and_live_child_edit() {
    let mut p = Page::new();
    let (outer, e) = add_boolean(&mut p, BooleanOp::Union, 10.0, 10.0);
    // Вложенная «Вычитание» внизу справа: квадрат без левой половины.
    let inner = p.add(
        Some(outer),
        NodeKind::Boolean,
        PropsPatch {
            x: Some(100.0),
            boolean_op: Some(BooleanOp::Subtract),
            ..Default::default()
        },
    );
    p.add(Some(inner), NodeKind::Rect, rect(0.0, 0.0, 60.0, 60.0));
    p.add(Some(inner), NodeKind::Rect, rect(-10.0, 0.0, 40.0, 60.0));
    let mut scene = p.scene();
    let img = render(&scene, &camera(200.0, 80.0));
    assert_px(&img, 115, 40, WHITE, 0);
    assert_px(&img, 160, 40, GREEN, 0);
    assert_px(&img, 95, 40, GREEN, 0);
    golden("boolean_nested", &img);
    // Ребёнок ушёл вправо: кадр сразу по превью, без записи в документ.
    scene.set_preview(&[(
        e,
        PropsPatch {
            x: Some(60.0),
            ..Default::default()
        },
    )]);
    let img = render(&scene, &camera(200.0, 80.0));
    assert_px(&img, 115, 40, GREEN, 0);
    assert_px(&img, 95, 40, GREEN, 0);
    // Правка документа после превью — та же картинка.
    scene.clear_preview();
    p.edit(
        e,
        PropsPatch {
            x: Some(60.0),
            ..Default::default()
        },
    );
    p.update(&mut scene);
    let img = render(&scene, &camera(200.0, 80.0));
    assert_px(&img, 115, 40, GREEN, 0);
}

#[test]
fn hover_outline_follows_the_result() {
    let mut p = Page::new();
    let (b, _) = add_boolean(&mut p, BooleanOp::Subtract, 10.0, 10.0);
    let scene = p.scene();
    let overlay = ferrum_render::Overlay {
        hover: Some(b),
        ..Default::default()
    };
    let img = render_overlay(
        &scene,
        &camera(120.0, 80.0),
        &overlay,
        ferrum_render::Rgba::from_rgba8(255, 255, 255, 255),
    );
    // Контур вычитания проходит по дуге эллипса внутри квадрата (x ≈ 40), а не
    // по рамке детей справа (x = 100).
    let edge = px(&img, 40, 40);
    assert!(edge[2] > edge[0] + 40, "подсветка на дуге: {edge:?}");
    assert_px(&img, 100, 40, WHITE, 0);
}
