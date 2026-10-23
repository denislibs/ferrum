//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod binary;
mod codec;
mod error;
mod page;
mod scene_cache;
mod types;

pub use error::{DocError, DocResult};
pub use page::PageDoc;
pub use scene_cache::SceneRow;
pub use types::*;
