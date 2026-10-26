//! Ядро Ferrum в Web Worker: обмен данными с основным потоком и цикл кадров.
//!
//! Нативная часть (тесты обычным `cargo test`): раскладка общей памяти
//! (`layout`), записи кольцевого буфера ввода (`input`), блок живого состояния
//! (`live`), сообщения (`protocol`), картинки документа (`images`) и ядро кадра
//! (`core`). Основной поток пишет ввод в кольцевой буфер SharedArrayBuffer (или
//! в буфер сообщения, если страница не изолирована), worker разбирает его в начале
//! кадра и отдаёт редактору по порядку.
//!
//! Браузерная часть (`web`, только wasm32): `WasmCore` для worker'а —
//! поверхность WebGPU, WebGL2 или CPU на OffscreenCanvas, часы `performance.now()`
//! и привязки wasm-bindgen.

pub mod core;
pub mod images;
pub mod input;
pub mod layers;
pub mod layout;
pub mod live;
pub mod present;
pub mod protocol;
pub mod recover;
pub mod summary;
#[cfg(target_arch = "wasm32")]
pub mod web;
