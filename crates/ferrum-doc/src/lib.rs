//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod binary;
mod changes;
mod codec;
pub mod container;
mod document;
mod error;
mod history;
mod images;
pub mod journal;
mod loro_ext;
mod page;
mod scene_cache;
mod types;

pub use changes::Changes;
pub use document::{Document, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use images::ImageStore;
pub use journal::Replayed;
pub use page::{PageDoc, Tx};
pub use scene_cache::SceneRow;
pub use types::*;

#[doc(hidden)]
pub mod fixture;
