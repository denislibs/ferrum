//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod binary;
mod boolean;
mod changes;
mod codec;
pub mod container;
mod document;
mod error;
mod guides;
mod history;
mod images;
pub mod journal;
mod loro_ext;
mod page;
mod scene_cache;
mod sync;
mod text;
mod text_codec;
mod text_ot;
mod types;
mod vector;

pub use boolean::BooleanOp;
pub use changes::Changes;
pub use document::{Document, GuideOwner, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use guides::{check_guides, dedup_guides, Guide, GuideAxis, MAX_GUIDES, MAX_GUIDE_OFFSET};
pub use history::{TextMerge, MERGE_PAUSE_MS};
pub use images::ImageStore;
pub use journal::Replayed;
pub use page::{PageDoc, Tx};
pub use scene_cache::SceneRow;
pub use sync::{
    DocKey, RemoteOutcome, SyncBatch, SyncVersion, TextShift, MAX_SYNC_BATCH_BYTES,
    MAX_VERSION_PEERS,
};
pub use text::*;
pub use types::*;
pub use vector::*;

#[doc(hidden)]
pub mod fixture;
