//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod binary;
mod codec;
pub mod container;
mod document;
mod error;
mod history;
mod images;
mod page;
mod scene_cache;
mod types;

pub use document::{Document, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use images::ImageStore;
pub use page::{PageDoc, Tx};
pub use scene_cache::SceneRow;
pub use types::*;

#[doc(hidden)]
pub mod fixture;
