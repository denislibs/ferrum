//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod error;
mod types;

pub use error::{DocError, DocResult};
pub use types::*;
