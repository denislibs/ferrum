//! Сцена Ferrum: производный кэш страницы для рендера и hit-testing — мировые
//! трансформации, границы в мире, R-tree и превью текущего жеста. Документ сцена
//! не меняет и видит только через публичный API `ferrum-doc`.

pub mod boolean;
mod scene;
pub mod shape;
pub mod vector;

pub use scene::{testing, BooleanOutline, Draft, Scene, SceneNode, Slot, TextOutlines};
