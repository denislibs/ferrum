//! Часы рендера: предел времени на растры фреймов за кадр считается по часам,
//! которые дал вызывающий; без часов остаётся счётный предел.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch};
use ferrum_render::{
    default_clock, Camera, Clock, CpuBackend, NoImages, Renderer, StdClock, RASTER_BUILDS_PER_FRAME,
};
use kurbo::Size;

/// Часы, которые сдвигаются на `step` миллисекунд при каждом чтении: между
/// чтениями до и после построения растра проходит ровно `step`.
struct FakeClock {
    now: Rc<Cell<f64>>,
    step: f64,
}

impl Clock for FakeClock {
    fn now_ms(&self) -> f64 {
        let t = self.now.get();
        self.now.set(t + self.step);
        t
    }
}

/// 64 фрейма по 10 прямоугольников при сильном отдалении: все рисуются из растров.
fn zoomed_out_grid() -> (Page, Camera) {
    let mut p = Page::new();
    for i in 0..64 {
        let f = p.add(
            None,
            NodeKind::Frame,
            PropsPatch {
                fills: Some(vec![Fill::solid(Color::rgba(240, 240, 240, 255))]),
                ..rect((i % 8) as f64 * 220.0, (i / 8) as f64 * 220.0, 200.0, 200.0)
            },
        );
        for k in 0..10 {
            p.add(
                Some(f),
                NodeKind::Rect,
                PropsPatch {
                    fills: Some(vec![Fill::solid(Color::rgba(20, 120, 200, 255))]),
                    ..rect(10.0 + k as f64 * 18.0, 10.0, 12.0, 12.0)
                },
            );
        }
    }
    let mut cam = Camera::new(Size::new(400.0, 400.0), 1.0);
    let bounds = p.scene().bounds().unwrap();
    cam.fit(bounds, 10.0);
    (p, cam)
}

fn builds_per_frame(r: &mut Renderer, p: &mut Page, cam: &Camera, frames: usize) -> Vec<usize> {
    let scene = p.scene();
    let (w, h) = cam.device_size();
    (0..frames)
        .map(|_| {
            let mut b = CpuBackend::new(w, h);
            r.draw_scene(&mut b, &scene, cam, &NoImages).raster_builds
        })
        .collect()
}

#[test]
fn raster_budget_follows_the_given_clock() {
    let (mut p, cam) = zoomed_out_grid();
    let now = Rc::new(Cell::new(0.0));
    let mut r = Renderer::new();
    r.set_raster_build_time(Some(Duration::from_millis(4)));
    // Каждое построение «стоит» 1 мс: в кадр с пределом 4 мс входит ровно 4.
    r.set_clock(Some(Box::new(FakeClock {
        now: now.clone(),
        step: 1.0,
    })));
    let builds = builds_per_frame(&mut r, &mut p, &cam, 3);
    assert_eq!(builds, [4, 4, 4]);
    assert!(now.get() >= 12.0, "часы читались: {}", now.get());
}

#[test]
fn slow_clock_still_builds_one_raster_per_frame() {
    let (mut p, cam) = zoomed_out_grid();
    let mut r = Renderer::new();
    // Одно построение «стоит» 100 мс — больше предела, но кэш всё равно догоняет.
    r.set_clock(Some(Box::new(FakeClock {
        now: Rc::new(Cell::new(0.0)),
        step: 100.0,
    })));
    assert_eq!(builds_per_frame(&mut r, &mut p, &cam, 2), [1, 1]);
}

#[test]
fn without_clock_only_the_count_limit_applies() {
    let (mut p, cam) = zoomed_out_grid();
    let mut r = Renderer::new();
    r.set_raster_build_time(Some(Duration::from_millis(4)));
    r.set_clock(None);
    // Как в wasm32 без часов: 64 растра — по 32 за кадр, затем всё готово.
    let builds = builds_per_frame(&mut r, &mut p, &cam, 3);
    assert_eq!(
        builds,
        [RASTER_BUILDS_PER_FRAME, RASTER_BUILDS_PER_FRAME, 0]
    );
}

#[test]
fn std_clock_is_monotonic_and_default_on_native() {
    let c = StdClock::new();
    let a = c.now_ms();
    std::thread::sleep(Duration::from_millis(2));
    let b = c.now_ms();
    assert!(b - a >= 1.5, "{a} → {b}");
    assert!(default_clock().is_some());
}
