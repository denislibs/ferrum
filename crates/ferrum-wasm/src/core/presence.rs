//! Присутствие в ядре (спека подсистемы 4, раздел 9): записи пачек сервера от `sync.worker` —
//! редактору, своё состояние — событием `Presence` не чаще 20 раз в секунду и только при
//! изменении. Повтор того же состояния раз в 10 с, пока соединение живо, шлёт `sync.worker`:
//! в скрытой вкладке кадров нет, а участник не должен пропадать у других.
//!
//! Записи приходят из сети через `sync.worker`: разбор кадра там строгий, но ядро проверяет и само
//! — числа конечны и в пределах мира, частей и узлов не больше пределов, участников не больше
//! `PRESENCE_PEERS_MAX`.

use std::str::FromStr;

use ferrum_doc::{NodeId, PageId};
use ferrum_editor::{peer_color, Gesture, GestureKind, PeerSelection, PeerState};
use kurbo::{Affine, Point, Rect};
use serde_bytes::ByteBuf;

use super::Core;
use crate::protocol::{
    Command, CoreEvent, GestureKindDto, PresenceGestureDto, PresenceItemDto, PresenceSelectionDto,
    PRESENCE_ITEMS_MAX, PRESENCE_PART_IDS_MAX, PRESENCE_PEERS_MAX,
};

/// Своё состояние уходит не чаще (20 раз в секунду, спека раздел 9).
pub const PRESENCE_SEND_MS: f64 = 50.0;
/// Части выделения: байты узлов в кадре 2 КБ и их число — как в `packages/protocol`.
pub const PRESENCE_PART_BYTES: usize = 2040;
pub const PRESENCE_PARTS_MAX: usize = 16;
pub const PRESENCE_SELECTION_MAX: usize = 100_000;
const WORLD_MAX: f64 = 1e9;
/// Коэффициент своего «Масштаба» для других — не меньше: рамку, схлопнутую ручкой, `k = 0`
/// (протокол такой не примет), они видят почти схлопнутой, а не теряют жест посреди него.
const SCALE_K_MIN: f64 = 1e-6;

#[derive(Default)]
pub(crate) struct PresenceOut {
    last_ms: Option<f64>,
    last: Option<CoreEvent>,
    rev: u32,
    /// Своё выделение: версия, ключ сцены для рамки, заголовок, части версии и ушли ли они.
    /// Части считаются при новой версии, а отправленными — только когда кадр с ними ушёл.
    sel_version: Option<u64>,
    bounds_key: Option<(u64, u64)>,
    selection: Option<PresenceSelectionDto>,
    parts: Option<Vec<ByteBuf>>,
    parts_sent: bool,
}

/// Команды, которые сцену не правят и чужое превью снимать не должны: присутствие, синхронизация,
/// чтение пикселей, выбор контекстного меню (цель — по тому, что видно, как у нажатия; меняется
/// только выделение). Остальные идут с `presence_hold`: правка не прочтёт чужое превью.
pub(crate) fn holds(cmd: &Command) -> bool {
    !matches!(
        cmd,
        Command::Presence { .. }
            | Command::PresenceReset
            | Command::Follow { .. }
            | Command::SyncStart
            | Command::SyncReset
            | Command::SyncSubscribed { .. }
            | Command::SyncAcked { .. }
            | Command::SyncFlush
            | Command::ApplyRemote { .. }
            | Command::AddRemoteImage { .. }
            | Command::ReadPixels { .. }
            | Command::ReadMeasurements
            | Command::ReadPresence
            | Command::ContextMenu { .. }
    )
}

fn coord(v: f64) -> bool {
    v.is_finite() && v.abs() <= WORLD_MAX
}

fn rect(r: [f64; 4], strict: bool) -> Option<Rect> {
    if !r.iter().all(|v| coord(*v)) {
        return None;
    }
    let ok = if strict {
        r[2] > r[0] && r[3] > r[1]
    } else {
        r[2] >= r[0] && r[3] >= r[1]
    };
    ok.then(|| Rect::new(r[0], r[1], r[2], r[3]))
}

fn affine(a: [f64; 6]) -> Option<Affine> {
    a.iter().all(|v| coord(*v)).then(|| Affine::new(a))
}

fn gesture(g: PresenceGestureDto) -> Option<Gesture> {
    Some(Gesture {
        kind: match g.kind {
            GestureKindDto::Move => GestureKind::Move,
            GestureKindDto::Resize => GestureKind::Resize,
            GestureKindDto::Rotate => GestureKind::Rotate,
            GestureKindDto::Scale => GestureKind::Scale,
        },
        transform: affine(g.transform)?,
        frame: affine(g.frame)?,
        // Коэффициент «Масштаба»: ноль или минус схлопнули бы и отразили превью.
        k: (coord(g.k) && g.k > 0.0).then_some(g.k)?,
    })
}

fn gesture_dto(g: Gesture) -> Option<PresenceGestureDto> {
    let out = PresenceGestureDto {
        kind: match g.kind {
            GestureKind::Move => GestureKindDto::Move,
            GestureKind::Resize => GestureKindDto::Resize,
            GestureKind::Rotate => GestureKindDto::Rotate,
            GestureKind::Scale => GestureKindDto::Scale,
        },
        transform: g.transform.as_coeffs(),
        frame: g.frame.as_coeffs(),
        k: g.k.max(SCALE_K_MIN),
    };
    // Нечисловая матрица (вырожденная рамка) не уходит: превью у других было бы мусором.
    (out.transform.iter().chain(&out.frame).all(|v| coord(*v)) && coord(out.k) && out.k > 0.0)
        .then_some(out)
}

/// Узел как пара (пир, счётчик) `TreeID`.
fn id_parts(id: NodeId) -> Option<(u64, u32)> {
    let s = id.to_string();
    let (counter, peer) = s.split_once('@')?;
    Some((peer.parse().ok()?, counter.parse().ok()?))
}

fn varint_len(v: u32) -> usize {
    match v {
        0..=0x7f => 1,
        0x80..=0x3fff => 2,
        0x4000..=0x1f_ffff => 3,
        0x20_0000..=0xfff_ffff => 4,
        _ => 5,
    }
}

fn put_varint(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Узлы частями по `PRESENCE_PART_BYTES`, не больше `PRESENCE_PARTS_MAX` частей; запись — как
/// `encodeSelectionIds` протокола: группы по пиру (по возрастанию), счётчики разностями.
pub fn encode_selection(ids: &[NodeId]) -> Vec<Vec<u8>> {
    let mut all: Vec<(u64, u32)> = ids.iter().filter_map(|id| id_parts(*id)).collect();
    all.sort_unstable();
    all.dedup();
    let mut parts: Vec<Vec<(u64, Vec<u32>)>> = Vec::new();
    let mut groups: Vec<(u64, Vec<u32>)> = Vec::new();
    let mut size = 0;
    for (peer, counter) in all {
        let grow = match groups.last() {
            Some((p, list)) if *p == peer => {
                let n = list.len() as u32;
                varint_len(n + 1) - varint_len(n)
                    + varint_len(counter - list.last().expect("группа не пуста"))
            }
            _ => 8 + 1 + varint_len(counter),
        };
        if size + grow > PRESENCE_PART_BYTES {
            parts.push(std::mem::take(&mut groups));
            size = 0;
            if parts.len() == PRESENCE_PARTS_MAX {
                break;
            }
        }
        match groups.last_mut() {
            Some((p, list)) if *p == peer => {
                size += varint_len(list.len() as u32 + 1) - varint_len(list.len() as u32)
                    + varint_len(counter - list.last().expect("группа не пуста"));
                list.push(counter);
            }
            _ => {
                size += 8 + 1 + varint_len(counter);
                groups.push((peer, vec![counter]));
            }
        }
    }
    if !groups.is_empty() && parts.len() < PRESENCE_PARTS_MAX {
        parts.push(groups);
    }
    parts
        .into_iter()
        .map(|groups| {
            let mut out = Vec::new();
            for (peer, list) in groups {
                out.extend_from_slice(&peer.to_le_bytes());
                put_varint(&mut out, list.len() as u32);
                put_varint(&mut out, list[0]);
                for w in list.windows(2) {
                    put_varint(&mut out, w[1] - w[0]);
                }
            }
            out
        })
        .collect()
}

impl Core {
    /// Записи пачки сервера — редактору. Неверная запись пропускается одна, остальные идут.
    pub(super) fn presence_items(&mut self, items: Vec<PresenceItemDto>) {
        if items.len() > PRESENCE_ITEMS_MAX {
            self.error("Слишком много записей присутствия за раз");
            return;
        }
        for item in items {
            match item {
                PresenceItemDto::Join {
                    participant,
                    user,
                    name,
                    color,
                } => {
                    let known = self.editor.peers().iter().any(|p| p.id == participant);
                    if !known && self.editor.peers().len() >= PRESENCE_PEERS_MAX {
                        continue;
                    }
                    if user.is_empty() || user.len() > 128 || name.len() > 256 || color > 0xff_ffff
                    {
                        continue;
                    }
                    self.editor
                        .presence_join(participant, &user, &name, peer_color(color));
                }
                PresenceItemDto::State {
                    participant,
                    page,
                    view,
                    cursor,
                    selection,
                    gesture: g,
                } => {
                    let Some(view) = rect(view, true) else {
                        continue;
                    };
                    let cursor = match cursor {
                        Some([x, y]) if coord(x) && coord(y) => Some(Point::new(x, y)),
                        Some(_) => continue,
                        None => None,
                    };
                    let selection = match selection {
                        Some(s) => {
                            let ok = s.count >= 1
                                && s.count as usize <= PRESENCE_SELECTION_MAX
                                && s.parts >= 1
                                && s.parts as usize <= PRESENCE_PARTS_MAX;
                            match rect(s.bounds, false).filter(|_| ok) {
                                Some(bounds) => Some(PeerSelection {
                                    rev: s.rev,
                                    count: s.count,
                                    parts: s.parts,
                                    bounds,
                                }),
                                None => continue,
                            }
                        }
                        None => None,
                    };
                    let gesture = match g {
                        Some(g) => match gesture(g) {
                            Some(g) => Some(g),
                            None => continue,
                        },
                        None => None,
                    };
                    // Страница, которой здесь нет, — участник не на нашей странице.
                    let page = PageId::from_str(&page)
                        .ok()
                        .filter(|p| self.editor.document().pages().iter().any(|(id, _)| id == p));
                    self.editor.presence_state(
                        participant,
                        PeerState {
                            page,
                            view,
                            cursor,
                            selection,
                            gesture,
                        },
                    );
                }
                PresenceItemDto::Part {
                    participant,
                    rev,
                    index,
                    parts,
                    ids,
                } => {
                    if ids.is_empty()
                        || ids.len() > PRESENCE_PART_IDS_MAX
                        || parts == 0
                        || parts as usize > PRESENCE_PARTS_MAX
                        || index >= parts
                    {
                        continue;
                    }
                    let Ok(ids) = ids.iter().map(|s| NodeId::from_str(s)).collect() else {
                        continue;
                    };
                    self.editor
                        .presence_part(participant, rev, index, parts, ids);
                }
                PresenceItemDto::Left { participant } => self.editor.presence_left(participant),
            }
        }
    }

    /// Своё состояние для других: не чаще `PRESENCE_SEND_MS` и только при изменении.
    pub(super) fn presence_flush(&mut self) {
        if !self.sync.on {
            return;
        }
        let now = self.now_ms;
        if self
            .presence
            .last_ms
            .is_some_and(|t| now - t < PRESENCE_SEND_MS && now >= t)
        {
            return;
        }
        let own = self.editor.own_presence();
        let version = own.selection_version;
        // Новое выделение — новая версия и её части, есть у него рамка или нет.
        if self.presence.sel_version != Some(version) {
            self.presence.sel_version = Some(version);
            self.presence.bounds_key = None;
            let ids = self.editor.selection();
            if ids.is_empty() {
                self.presence.parts = None;
            } else {
                self.presence.rev = self.presence.rev.wrapping_add(1);
                let encoded = encode_selection(&ids[..ids.len().min(PRESENCE_SELECTION_MAX)]);
                self.presence.parts = Some(encoded.into_iter().map(ByteBuf::from).collect());
            }
            self.presence.parts_sent = false;
        }
        // Рамка — заново, если менялись документ или своё превью; слой чужого превью не в счёт.
        let scene = self.editor.scene();
        let key = (scene.generation(), scene.own_change_counter());
        if self.presence.bounds_key != Some(key) {
            let count = self.editor.selection().len().min(PRESENCE_SELECTION_MAX) as u32;
            // По документу: чужое превью моих узлов — не моя рамка.
            let bounds = self.editor.own_selection_bounds();
            self.presence.bounds_key = Some(key);
            let n = self.presence.parts.as_ref().map_or(0, Vec::len);
            self.presence.selection = bounds
                .filter(|b| n > 0 && [b.x0, b.y0, b.x1, b.y1].iter().all(|v| coord(*v)))
                .map(|b| PresenceSelectionDto {
                    rev: self.presence.rev,
                    count,
                    parts: n as u8,
                    bounds: [b.x0, b.y0, b.x1, b.y1],
                });
            // Без рамки выделения нет и у получателей: на `None` они сбрасывают части. Рамка
            // вернётся той же версией — части уйдут с ней снова.
            if self.presence.selection.is_none() {
                self.presence.parts_sent = false;
            }
        }
        // Части уходят с первым кадром, где о выделении сказано.
        let parts = if self.presence.selection.is_some() && !self.presence.parts_sent {
            self.presence.parts.clone()
        } else {
            None
        };
        let view = own.view;
        if ![view.x0, view.y0, view.x1, view.y1]
            .iter()
            .all(|v| coord(*v))
            || view.x1 <= view.x0
            || view.y1 <= view.y0
        {
            return;
        }
        let event = CoreEvent::Presence {
            page: own.page.to_string(),
            view: [view.x0, view.y0, view.x1, view.y1],
            cursor: own
                .cursor
                .filter(|p| coord(p.x) && coord(p.y))
                .map(|p| [p.x, p.y]),
            selection: self.presence.selection,
            parts: None,
            gesture: own.gesture.and_then(gesture_dto),
        };
        if parts.is_none() && self.presence.last.as_ref() == Some(&event) {
            return;
        }
        self.presence.last = Some(event.clone());
        self.presence.last_ms = Some(now);
        if parts.is_some() {
            self.presence.parts_sent = true;
        }
        let event = match event {
            CoreEvent::Presence {
                page,
                view,
                cursor,
                selection,
                gesture,
                ..
            } => CoreEvent::Presence {
                page,
                view,
                cursor,
                selection,
                parts,
                gesture,
            },
            e => e,
        };
        self.events.push(event);
    }

    /// Тестовая `ReadPresence`: что видно от других участников.
    pub(super) fn read_presence(&mut self) {
        let hex = |c: ferrum_render::Rgba| {
            let [r, g, b, _] = c.to_rgba8().to_u8_array();
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        let marks = self
            .editor
            .overlay()
            .remote
            .iter()
            .map(|m| crate::protocol::PresenceMarkDto {
                name: m.name.to_string(),
                color: hex(m.color),
                cursor: m.cursor.map(|p| [p.x, p.y]),
                frame: m.frame.map(|r| [r.x0, r.y0, r.x1, r.y1]),
                boxes: m.boxes.len() as u32,
            })
            .collect();
        let scene = self.editor.scene();
        let preview = scene
            .remote_patches()
            .into_iter()
            .filter_map(|(id, _)| {
                let n = scene.node(id)?;
                Some(crate::protocol::PreviewedNodeDto {
                    id: id.to_string(),
                    x: n.props.x,
                    y: n.props.y,
                })
            })
            .collect();
        self.events.push(CoreEvent::PresenceMarks {
            marks,
            following: self.editor.following(),
            preview,
        });
    }

    /// Соединение заново: о себе сказать всё снова (сервер не помнит прежнее соединение).
    pub(super) fn presence_reset(&mut self) {
        self.editor.presence_reset();
        self.presence = PresenceOut::default();
    }
}
