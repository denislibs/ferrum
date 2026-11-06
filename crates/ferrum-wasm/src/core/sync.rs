//! Синхронизация в ядре (спека подсистемы 4, разделы 7 и 8): подписки и свои правки уходят событиями в
//! `sync.worker`, чужие приходят командами. Журнал правок снимается только после подтверждения сервера.

use std::collections::{HashMap, VecDeque};

use ferrum_doc::{DocKey, SyncVersion, MAX_SYNC_BATCH_BYTES};
use serde_bytes::ByteBuf;

use super::{panels, Core};
use crate::protocol::CoreEvent;

/// Сообщение: другой участник правит более новой версией, и файл стал только для чтения.
pub const NEWER_VERSION_EDITS: &str =
    "Другой участник правит файл более новой версией Ferrum. Файл открыт только для чтения — обновите страницу";

#[derive(Default)]
pub(crate) struct SyncCore {
    pub on: bool,
    next_seq: u32,
    /// Пачки без ответа сервера: номер — документ и версия, которую он узнает.
    inflight: HashMap<u32, (DocKey, SyncVersion)>,
    /// Пачка журнала и версии документов на момент её записи. 0 — журнал прошлых сеансов.
    pub marks: VecDeque<(u64, Vec<(DocKey, SyncVersion)>)>,
    unacked_sent: Option<bool>,
}

impl Core {
    pub(super) fn sync_start(&mut self) {
        self.editor.sync_start();
        self.sync.on = true;
        // Всё, что было открыто, вместе с журналом прошлых сеансов — отметка 0.
        self.sync.marks.push_back((0, self.editor.sync_versions()));
    }

    pub(super) fn sync_reset(&mut self) {
        self.sync.inflight.clear();
        self.editor.sync_reset();
    }

    pub(super) fn sync_subscribed(&mut self, doc: &str, version: &[u8]) {
        let (Some(key), Some(v)) = (DocKey::parse(doc), SyncVersion::decode(version)) else {
            self.error("Сервер прислал неверную версию документа");
            return;
        };
        self.editor.sync_subscribed(key, v);
        self.sync_note();
    }

    pub(super) fn sync_acked(&mut self, doc: &str, client_seq: u32) {
        // Кадр не доверенный: чужой документ не подтверждает эту пачку и не снимает её ожидание.
        match self.sync.inflight.get(&client_seq) {
            Some((key, _)) if key.to_string() == doc => {}
            _ => return,
        }
        let Some((key, end)) = self.sync.inflight.remove(&client_seq) else {
            return;
        };
        self.editor.sync_acked(key, &end);
        self.sync_note();
    }

    /// Новые подписки, картинки и правки. Картинки — раньше правок, которые на них ссылаются.
    pub(super) fn sync_flush(&mut self) {
        if !self.sync.on {
            return;
        }
        for (doc, version) in self.editor.take_sync_subscribes() {
            self.events.push(CoreEvent::SyncSubscribe {
                doc: doc.to_string(),
                version: ByteBuf::from(version.encode()),
            });
        }
        let batches = match self.editor.take_sync_updates(MAX_SYNC_BATCH_BYTES) {
            Ok(b) => b,
            Err(e) => {
                self.error(format!("Не удалось собрать правки для сервера: {e}"));
                return;
            }
        };
        // Документ, который не собрался, не держит остальные; сообщение — один раз до нового соединения.
        for (_, e) in self.editor.take_sync_failures() {
            self.error(format!("Не удалось собрать правки для сервера: {e}"));
        }
        if !batches.is_empty() {
            for (hash, bytes) in self.editor.take_sync_images() {
                self.events.push(CoreEvent::LocalImage {
                    hash,
                    bytes: ByteBuf::from(bytes),
                });
            }
        }
        for b in batches {
            self.sync.next_seq = self.sync.next_seq.wrapping_add(1);
            let client_seq = self.sync.next_seq;
            self.sync.inflight.insert(client_seq, (b.doc, b.end));
            self.events.push(CoreEvent::LocalUpdates {
                doc: b.doc.to_string(),
                client_seq,
                bytes: ByteBuf::from(b.bytes),
            });
        }
        self.sync_note();
    }

    /// Что изменилось от ответа сервера: журнал, который можно снять, и «всё подтверждено».
    pub(super) fn sync_note(&mut self) {
        if !self.sync.on {
            return;
        }
        let mut covered = None;
        while let Some((batch, marks)) = self.sync.marks.front() {
            if !self.editor.sync_covers(marks) {
                break;
            }
            covered = Some(*batch);
            self.sync.marks.pop_front();
        }
        if let Some(batch) = covered {
            self.events.push(CoreEvent::JournalCovered {
                batch: batch as f64,
            });
        }
        let unacked = self.editor.sync_unacked();
        if self.sync.unacked_sent != Some(unacked) {
            self.sync.unacked_sent = Some(unacked);
            self.events.push(CoreEvent::SyncState { unacked });
        }
        // «Изменён» значит «не на сервере».
        if !unacked && self.modified {
            self.modified = false;
        }
    }

    pub(super) fn apply_remote(&mut self, doc: &str, batches: &[ByteBuf]) {
        let Some(key) = DocKey::parse(doc) else {
            self.error("Сервер прислал правки неизвестного документа");
            return;
        };
        let refs: Vec<&[u8]> = batches.iter().map(|b| b.as_slice()).collect();
        let name = self.editor.document().name();
        match self.editor.apply_remote(key, &refs) {
            Ok(fx) => {
                if fx.failed {
                    self.error("Часть правок других участников не применилась — обновите страницу");
                }
                if fx.became_read_only {
                    self.error(NEWER_VERSION_EDITS);
                }
                if !fx.missing_images.is_empty() {
                    self.events.push(CoreEvent::ImagesNeeded {
                        hashes: fx.missing_images,
                    });
                }
                if fx.root_changed {
                    self.ui.pages = true;
                    let now = self.editor.document().name();
                    if now != name {
                        self.events.push(CoreEvent::DocumentName {
                            name: panels::event_name(&now),
                        });
                    }
                }
                self.dirty = true;
            }
            Err(e) => self.error(format!(
                "Не удалось применить правки других участников: {e}"
            )),
        }
        self.sync_note();
    }

    pub(super) fn add_remote_image(&mut self, hash: &str, bytes: Vec<u8>) {
        if self.editor.add_remote_image(hash, bytes) {
            self.images.forget(hash);
            self.dirty = true;
        }
    }

    pub(super) fn push_thumbnail(&mut self) {
        let (bytes, keep) = match self.thumbnail() {
            Ok(png) => (png.map(ByteBuf::from), false),
            Err(()) => (None, true),
        };
        self.events.push(CoreEvent::Thumbnail { bytes, keep });
    }
}
