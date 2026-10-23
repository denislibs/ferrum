//! Рендер Ferrum: камера, сборка кадра со списками отрисовки, отсечением и LOD,
//! оверлеи, экспорт PNG. Получает сцену, камеру и описание оверлеев; о Loro и
//! инструментах не знает.

mod backend;
mod camera;
mod image;

pub use backend::{CpuBackend, ImagePaint, RenderBackend, Rgba};
pub use camera::{Camera, MAX_ZOOM, MIN_ZOOM};
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use vello_cpu::Pixmap;
