//! Превью страницы для экрана файлов: содержимое вписано по центру с полем, фон —
//! фон страницы или прозрачный, пустая страница превью не даёт.

mod common;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch};
use ferrum_render::{decode_png, NoImages, Renderer, Rgba, THUMB_MAX_SCALE, THUMB_PADDING};

fn red(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        fills: Some(vec![Fill::solid(Color::rgba(255, 0, 0, 255))]),
        ..rect(x, y, w, h)
    }
}

#[test]
fn empty_page_has_no_thumbnail() {
    let scene = Page::new().scene();
    let out = Renderer::new()
        .thumbnail_png(&scene, 480, 320, None, &NoImages)
        .unwrap();
    assert_eq!(out, None);
}

#[test]
fn content_is_fitted_and_centred_with_padding() {
    let mut p = Page::new();
    // Широкая полоса 1000×100 далеко от начала координат: вписывается по ширине.
    p.add(None, NodeKind::Rect, red(5000.0, -3000.0, 1000.0, 100.0));
    let scene = p.scene();
    let png = Renderer::new()
        .thumbnail_png(&scene, 480, 320, None, &NoImages)
        .unwrap()
        .expect("есть содержимое");
    let img = decode_png(&png).unwrap();
    assert_eq!((img.width(), img.height()), (480, 320));
    // Масштаб (480 − 2·поле) / 1000: полоса высотой ~45 px по центру.
    let k = (480.0 - 2.0 * THUMB_PADDING) / 1000.0;
    let half = (100.0 * k / 2.0) as u16;
    assert_eq!(px(&img, 240, 160), [255, 0, 0, 255], "центр");
    assert_eq!(
        px(&img, THUMB_PADDING as u16 + 2, 160),
        [255, 0, 0, 255],
        "левый край"
    );
    assert_eq!(px(&img, THUMB_PADDING as u16 - 2, 160)[3], 0, "поле слева");
    assert_eq!(
        px(&img, 240, 160 - half - 3)[3],
        0,
        "над полосой — прозрачно"
    );
}

#[test]
fn background_fills_the_whole_thumbnail() {
    let mut p = Page::new();
    p.add(None, NodeKind::Rect, red(0.0, 0.0, 100.0, 100.0));
    let scene = p.scene();
    let bg = Rgba::new([0.0, 0.0, 1.0, 1.0]);
    let png = Renderer::new()
        .thumbnail_png(&scene, 480, 320, Some(bg), &NoImages)
        .unwrap()
        .unwrap();
    let img = decode_png(&png).unwrap();
    assert_eq!(px(&img, 1, 1), [0, 0, 255, 255]);
    assert_eq!(px(&img, 240, 160), [255, 0, 0, 255]);
}

#[test]
fn small_content_is_not_blown_up_past_max_scale() {
    let mut p = Page::new();
    p.add(None, NodeKind::Rect, red(0.0, 0.0, 10.0, 10.0));
    let scene = p.scene();
    let png = Renderer::new()
        .thumbnail_png(&scene, 480, 320, None, &NoImages)
        .unwrap()
        .unwrap();
    let img = decode_png(&png).unwrap();
    let side = (10.0 * THUMB_MAX_SCALE) as u16;
    assert_eq!(px(&img, 240, 160), [255, 0, 0, 255]);
    assert_eq!(
        px(&img, 240 + side / 2 + 2, 160)[3],
        0,
        "квадрат не шире {side} px"
    );
}

#[test]
fn hidden_content_gives_no_thumbnail() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            visible: Some(false),
            ..red(0.0, 0.0, 10.0, 10.0)
        },
    );
    let scene = p.scene();
    let out = Renderer::new()
        .thumbnail_png(&scene, 480, 320, None, &NoImages)
        .unwrap();
    assert_eq!(out, None);
}
