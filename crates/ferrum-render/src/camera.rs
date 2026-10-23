//! Камера холста: центр в мировых координатах (`f64`), зум и размер вьюпорта.
//!
//! Точность: мировые координаты могут быть большими (1e7), а растеризатор работает в
//! `f32`. Поэтому итоговая матрица «мир → пиксели» собирается в `f64` относительно
//! центра камеры, и в `f32` переводятся уже экранные координаты (это делает vello при
//! разборе путей). Дрожания на больших координатах нет: в экранной системе числа малы.
//!
//! Входные данные не доверенные (жесты, колесо мыши, размер окна), поэтому все
//! мутирующие операции игнорируют NaN и бесконечности и оставляют прежнее состояние.

use kurbo::{Affine, Point, Rect, Size, Vec2};

pub const MIN_ZOOM: f64 = 0.02;
pub const MAX_ZOOM: f64 = 256.0;
/// Наибольшая сторона поверхности в пикселях устройства.
pub const MAX_DEVICE_SIDE: u32 = 16_384;
/// Наибольшее число пикселей поверхности (64 Мпикс): ограничивает память кадра
/// (RGBA8 — 256 МБ на один `Pixmap`, без учёта кэша слоёв).
pub const MAX_DEVICE_PIXELS: u64 = 64 * 1024 * 1024;

/// Запас против ошибок округления, чтобы `ceil(размер · dpr)` не перескочил предел.
const CAP_SLACK: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    /// Точка мира в центре вьюпорта.
    center: Point,
    zoom: f64,
    /// Размер вьюпорта в CSS-пикселях.
    viewport: Size,
    /// `devicePixelRatio` как его задали. Для сборки кадра используется
    /// `effective_dpr`, которое может быть меньше из-за ограничения поверхности.
    dpr: f64,
}

fn finite_point(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn valid_size(s: Size) -> bool {
    s.width.is_finite() && s.height.is_finite() && s.width >= 0.0 && s.height >= 0.0
}

fn valid_dpr(dpr: f64) -> bool {
    dpr.is_finite() && dpr > 0.0
}

impl Camera {
    /// Некорректный вьюпорт становится пустым, некорректный `dpr` — 1.
    pub fn new(viewport: Size, dpr: f64) -> Camera {
        Camera {
            center: Point::ZERO,
            zoom: 1.0,
            viewport: if valid_size(viewport) {
                viewport
            } else {
                Size::ZERO
            },
            dpr: if valid_dpr(dpr) { dpr } else { 1.0 },
        }
    }

    pub fn center(&self) -> Point {
        self.center
    }

    /// Некорректная точка (NaN, бесконечность) игнорируется.
    pub fn set_center(&mut self, center: Point) {
        if finite_point(center) {
            self.center = center;
        }
    }

    pub fn viewport(&self) -> Size {
        self.viewport
    }

    /// Отрицательные и нечисловые размеры игнорируются.
    pub fn set_viewport(&mut self, viewport: Size) {
        if valid_size(viewport) {
            self.viewport = viewport;
        }
    }

    /// `devicePixelRatio`, как его задали (до ограничения поверхности).
    pub fn dpr(&self) -> f64 {
        self.dpr
    }

    /// Нулевой, отрицательный и нечисловой `dpr` игнорируется.
    pub fn set_dpr(&mut self, dpr: f64) {
        if valid_dpr(dpr) {
            self.dpr = dpr;
        }
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    /// Зум ограничивается диапазоном 2%–25 600%: `+inf` дает максимум, `-inf` и
    /// нули — минимум. NaN игнорируется, зум остаётся прежним.
    pub fn set_zoom(&mut self, zoom: f64) {
        if zoom.is_nan() {
            return;
        }
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// Зум к курсору: точка мира под `screen` остаётся на месте. Нечисловой курсор,
    /// множитель ≤ 0 или NaN и результат вне диапазона float ничего не меняют.
    pub fn zoom_at(&mut self, screen: Point, factor: f64) {
        if !finite_point(screen) || !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let anchor = self.screen_to_world(screen);
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let center = anchor - (screen - self.screen_center()) / zoom;
        if !finite_point(anchor) || !finite_point(center) {
            return;
        }
        self.zoom = zoom;
        self.center = center;
    }

    /// Панорамирование на сдвиг в CSS-пикселях (содержимое едет вместе с курсором).
    /// Нечисловой сдвиг или результат вне диапазона float ничего не меняют.
    pub fn pan_by(&mut self, screen_delta: Vec2) {
        if !screen_delta.x.is_finite() || !screen_delta.y.is_finite() {
            return;
        }
        let center = self.center - screen_delta / self.zoom;
        if finite_point(center) {
            self.center = center;
        }
    }

    /// Показать прямоугольник мира целиком с отступом в CSS-пикселях. Пустой или
    /// нечисловой прямоугольник и нечисловой отступ ничего не меняют.
    pub fn fit(&mut self, world: Rect, padding: f64) {
        let finite_rect = [world.x0, world.y0, world.x1, world.y1]
            .iter()
            .all(|v| v.is_finite());
        if !finite_rect || !padding.is_finite() {
            return;
        }
        let world = world.abs();
        let (w, h) = (world.width(), world.height());
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            return;
        }
        let center = world.center();
        if !finite_point(center) {
            return;
        }
        let avail_w = (self.viewport.width - 2.0 * padding).max(1.0);
        let avail_h = (self.viewport.height - 2.0 * padding).max(1.0);
        self.set_zoom((avail_w / w).min(avail_h / h));
        self.center = center;
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

    /// Мир → пиксели устройства: матрица, с которой рисуется содержимое. Совпадает
    /// с `device_size` и при ограничении поверхности: вьюпорт всегда целиком
    /// попадает на холст, без обрезки.
    pub fn view_transform(&self) -> Affine {
        Affine::scale(self.effective_dpr()) * self.world_to_screen_transform()
    }

    /// Пикселей устройства на единицу мира.
    pub fn device_scale(&self) -> f64 {
        self.zoom * self.effective_dpr()
    }

    /// Размер холста в пикселях устройства. Не превышает `MAX_DEVICE_SIDE` по стороне
    /// и `MAX_DEVICE_PIXELS` по площади (кроме вырожденных размеров в 1 пиксель).
    pub fn device_size(&self) -> (u16, u16) {
        let dpr = self.effective_dpr();
        let (w, h) = Self::device_dims(self.viewport, dpr);
        (w as u16, h as u16)
    }

    /// Видимая часть мира.
    pub fn visible_world_rect(&self) -> Rect {
        Rect::from_points(
            self.screen_to_world(Point::ZERO),
            self.screen_to_world(Point::new(self.viewport.width, self.viewport.height)),
        )
    }

    /// `dpr`, уменьшенный так, чтобы поверхность не превышала ограничения.
    fn effective_dpr(&self) -> f64 {
        let (vw, vh) = (self.viewport.width, self.viewport.height);
        let mut dpr = self.dpr;
        if vw > 0.0 && vh > 0.0 {
            let side = MAX_DEVICE_SIDE as f64;
            let capped = dpr
                .min(side / vw)
                .min(side / vh)
                .min((MAX_DEVICE_PIXELS as f64 / (vw * vh)).sqrt());
            // Запас только когда предел действительно сработал: без ограничения
            // dpr применяется как есть.
            if capped < dpr {
                dpr = capped * (1.0 - CAP_SLACK);
            }
            // Округление вверх по обеим осям может чуть превысить предел площади.
            // Шаг 1e-4 уменьшает площадь на несколько тысяч пикселей; лимит итераций
            // защищает от зацикливания при вырожденных входах.
            for _ in 0..64 {
                let (w, h) = Self::device_dims(self.viewport, dpr);
                if w * h <= MAX_DEVICE_PIXELS {
                    break;
                }
                dpr *= 1.0 - 1e-4;
            }
        }
        // Нулевой масштаб вырождает матрицу; минимальный положительный — безопасная
        // нижняя граница для экстремально больших вьюпортов.
        dpr.max(f64::MIN_POSITIVE)
    }

    /// Размер поверхности в целых пикселях для вьюпорта и масштаба устройства.
    fn device_dims(viewport: Size, dpr: f64) -> (u64, u64) {
        let px = |v: f64| (v * dpr).ceil().clamp(1.0, MAX_DEVICE_SIDE as f64) as u64;
        (px(viewport.width), px(viewport.height))
    }
}
