//! Камера холста: центр в мировых координатах (`f64`), зум и размер вьюпорта.
//!
//! Точность: мировые координаты могут быть большими (1e7), а растеризатор работает в
//! `f32`. Поэтому итоговая матрица «мир → пиксели» собирается в `f64` относительно
//! центра камеры, и в `f32` переводятся уже экранные координаты (это делает vello при
//! разборе путей). Дрожания на больших координатах нет: в экранной системе числа малы.

use kurbo::{Affine, Point, Rect, Size, Vec2};

pub const MIN_ZOOM: f64 = 0.02;
pub const MAX_ZOOM: f64 = 256.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    /// Точка мира в центре вьюпорта.
    pub center: Point,
    zoom: f64,
    /// Размер вьюпорта в CSS-пикселях.
    pub viewport: Size,
    /// `devicePixelRatio`: пикселей устройства в одном CSS-пикселе.
    pub dpr: f64,
}

impl Camera {
    pub fn new(viewport: Size, dpr: f64) -> Camera {
        Camera {
            center: Point::ZERO,
            zoom: 1.0,
            viewport,
            dpr: if dpr.is_finite() && dpr > 0.0 {
                dpr
            } else {
                1.0
            },
        }
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    /// Зум ограничивается диапазоном 2%–25 600%.
    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = if zoom.is_finite() {
            zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.0
        };
    }

    /// Зум к курсору: точка мира под `screen` остаётся на месте.
    pub fn zoom_at(&mut self, screen: Point, factor: f64) {
        let anchor = self.screen_to_world(screen);
        self.set_zoom(self.zoom * factor);
        self.center = anchor - (screen - self.screen_center()) / self.zoom;
    }

    /// Панорамирование на сдвиг в CSS-пикселях (содержимое едет вместе с курсором).
    pub fn pan_by(&mut self, screen_delta: Vec2) {
        self.center -= screen_delta / self.zoom;
    }

    /// Показать прямоугольник мира целиком с отступом в CSS-пикселях.
    pub fn fit(&mut self, world: Rect, padding: f64) {
        let avail = Size::new(
            (self.viewport.width - 2.0 * padding).max(1.0),
            (self.viewport.height - 2.0 * padding).max(1.0),
        );
        let w = world.width().max(1e-9);
        let h = world.height().max(1e-9);
        self.set_zoom((avail.width / w).min(avail.height / h));
        self.center = world.center();
    }

    fn screen_center(&self) -> Point {
        Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0)
    }

    pub fn world_to_screen(&self, world: Point) -> Point {
        self.screen_center() + (world - self.center) * self.zoom
    }

    pub fn screen_to_world(&self, screen: Point) -> Point {
        self.center + (screen - self.screen_center()) / self.zoom
    }

    /// Мир → CSS-пиксели.
    pub fn world_to_screen_transform(&self) -> Affine {
        Affine::translate(self.screen_center().to_vec2())
            * Affine::scale(self.zoom)
            * Affine::translate(-self.center.to_vec2())
    }

    /// Мир → пиксели устройства: матрица, с которой рисуется содержимое.
    pub fn view_transform(&self) -> Affine {
        Affine::scale(self.dpr) * self.world_to_screen_transform()
    }

    /// Пикселей устройства на единицу мира.
    pub fn device_scale(&self) -> f64 {
        self.zoom * self.dpr
    }

    /// Размер холста в пикселях устройства.
    pub fn device_size(&self) -> (u16, u16) {
        let px = |v: f64| (v * self.dpr).ceil().clamp(1.0, u16::MAX as f64) as u16;
        (px(self.viewport.width), px(self.viewport.height))
    }

    /// Видимая часть мира.
    pub fn visible_world_rect(&self) -> Rect {
        Rect::from_points(
            self.screen_to_world(Point::ZERO),
            self.screen_to_world(Point::new(self.viewport.width, self.viewport.height)),
        )
    }
}
