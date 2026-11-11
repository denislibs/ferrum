//! Комментарии в ядре (спека подсистемы 4, раздел 10): метки и состояние от интерфейса —
//! редактору, события редактора — интерфейсу. Сами обсуждения живут на сервере; метки приходят
//! из `comment-event` и списка через основной поток и не доверены: ядро проверяет всё само —
//! числа конечны и в пределах мира, идентификаторы обсуждений не длиннее
//! `COMMENT_ID_MAX_BYTES` и из `[A-Za-z0-9_-]`, страница и узел — `counter@peer` (не черновик),
//! инициалы — до `COMMENT_INITIALS_MAX` графем, меток — не больше `COMMENT_PINS_MAX` (лишние не
//! принимаются). Неверная метка пропускается целиком, остальные команды с неверным полем ничего
//! не делают: интерфейс — свой, неверное значение — ошибка или подделка, а не ввод человека.
//!
//! Команды комментариев сцену не правят и чужое живое превью не снимают (`presence::holds`).

use std::str::FromStr;
use std::sync::Arc;

use ferrum_doc::{NodeId, PageId};
use ferrum_editor::{peer_color, CommentPin, EditorEvent, PinAnchor};
use kurbo::Point;
use unicode_segmentation::UnicodeSegmentation;

use super::fonts::Waiting;
use super::Core;
use crate::protocol::{
    Command, CommentDraftDto, CommentPinDto, CoreEvent, COMMENT_ID_MAX_BYTES, COMMENT_INITIALS_MAX,
    COMMENT_PINS_MAX,
};

const WORLD_MAX: f64 = 1e9;
/// Инициалы длиннее стольких байт не принимаются: одна «графема» из тысячи диакритик иначе
/// прошла бы предел графем.
const INITIALS_MAX_BYTES: usize = 32;

fn coord(v: f64) -> bool {
    v.is_finite() && v.abs() <= WORLD_MAX
}

/// Идентификатор обсуждения: UUID сервера или такой же короткий из `[A-Za-z0-9_-]`.
fn thread_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= COMMENT_ID_MAX_BYTES
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Узел или страница `counter@peer`: не длиннее 64 байт, и не черновик (пир `u64::MAX`).
fn tree_id(s: &str) -> bool {
    s.len() <= COMMENT_ID_MAX_BYTES
        && s.split_once('@')
            .is_some_and(|(_, peer)| peer.parse::<u64>().is_ok_and(|p| p != u64::MAX))
}

fn page(s: &str) -> Option<PageId> {
    tree_id(s).then(|| PageId::from_str(s).ok()).flatten()
}

fn node(s: &Option<String>) -> Option<Option<NodeId>> {
    match s {
        None => Some(None),
        Some(s) => tree_id(s)
            .then(|| NodeId::from_str(s).ok().map(Some))
            .flatten(),
    }
}

fn anchor(page_s: &str, node_s: &Option<String>, xy: [f64; 4]) -> Option<PinAnchor> {
    if !xy.iter().all(|v| coord(*v)) {
        return None;
    }
    Some(PinAnchor {
        page: page(page_s)?,
        node: node(node_s)?,
        local: Point::new(xy[0], xy[1]),
        world: Point::new(xy[2], xy[3]),
    })
}

/// Инициалы: не длиннее `INITIALS_MAX_BYTES`, первые `COMMENT_INITIALS_MAX` графем
/// **(поведение)**: длинное имя не прячет обсуждение, а укорачивается.
fn initials(s: &str) -> Option<Arc<str>> {
    if s.len() > INITIALS_MAX_BYTES {
        return None;
    }
    let short: String = s.graphemes(true).take(COMMENT_INITIALS_MAX).collect();
    Some(Arc::from(short))
}

/// Метка, если всё в ней верно.
pub fn comment_pin(p: &CommentPinDto) -> Option<CommentPin> {
    if !thread_id(&p.id) || p.color > 0xff_ffff {
        return None;
    }
    Some(CommentPin {
        id: Arc::from(p.id.as_str()),
        anchor: anchor(&p.page, &p.node, [p.x, p.y, p.wx, p.wy])?,
        initials: initials(&p.initials)?,
        color: peer_color(p.color),
        replies: p.replies,
        unread: p.unread,
        resolved: p.resolved,
        movable: p.movable,
    })
}

fn draft(d: &CommentDraftDto) -> Option<PinAnchor> {
    anchor(&d.page, &d.node, [d.x, d.y, d.wx, d.wy])
}

/// Команда комментариев: `true` — это она (и выполнена или отброшена проверкой).
pub(super) fn is_comment_command(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::CommentsAvailable { .. }
            | Command::CommentPins { .. }
            | Command::CommentPinsRemove { .. }
            | Command::CommentOpen { .. }
            | Command::CommentDraft { .. }
            | Command::CommentFocus { .. }
            | Command::CommentShowResolved { .. }
    )
}

impl Core {
    pub(super) fn comment_command(&mut self, cmd: Command) {
        match cmd {
            Command::CommentsAvailable { on } => self.editor.set_comments_available(on),
            Command::CommentPins { pins, replace } => {
                let pins: Vec<CommentPin> = pins
                    .iter()
                    .take(COMMENT_PINS_MAX)
                    .filter_map(comment_pin)
                    .collect();
                self.editor.set_comment_pins(pins, replace);
            }
            Command::CommentPinsRemove { ids } => {
                let ids: Vec<&str> = ids
                    .iter()
                    .take(COMMENT_PINS_MAX)
                    .map(String::as_str)
                    .filter(|id| thread_id(id))
                    .collect();
                self.editor.remove_comment_pins(&ids);
            }
            Command::CommentOpen { id } => match id {
                None => {
                    self.drop_waiting_focus();
                    self.editor.set_comment_open(None);
                }
                Some(id) if thread_id(&id) => {
                    self.drop_waiting_focus();
                    self.editor.set_comment_open(Some(&id));
                }
                Some(_) => {}
            },
            Command::CommentDraft { draft: d } => match d {
                None => self.editor.set_comment_draft(None),
                Some(d) => {
                    if let Some(a) = draft(&d) {
                        self.editor.set_comment_draft(Some(a));
                    }
                }
            },
            Command::CommentFocus { id } => {
                if thread_id(&id) {
                    self.editor.comment_focus(&id);
                }
            }
            Command::CommentShowResolved { on } => self.editor.set_comments_show_resolved(on),
            _ => unreachable!("is_comment_command"),
        }
    }
}

impl Core {
    /// `CommentFocus` ждёт шрифтов (меняет страницу), а `CommentOpen` идёт сразу: открыть другое
    /// обсуждение или закрыть отменяет ждущий переход, как `Editor::set_comment_open` отменяет
    /// ждущий метки, — иначе после загрузки шрифтов закрытое обсуждение открылось бы снова.
    fn drop_waiting_focus(&mut self) {
        let q = &mut self.font_state.waiting;
        q.retain(|w| !matches!(w, Waiting::Command(Command::CommentFocus { .. })));
        if q.is_empty() {
            self.font_state.waiting_since = None;
        }
    }
}

/// Событие редактора о комментариях — событием протокола; `None` — не о комментариях.
pub(super) fn comment_event(e: &EditorEvent) -> Option<CoreEvent> {
    let place = |a: &PinAnchor| {
        (
            a.page.to_string(),
            a.node.map(|n| n.to_string()),
            a.local,
            a.world,
        )
    };
    Some(match e {
        EditorEvent::CommentPlace { anchor, screen } => {
            let (page, node, l, w) = place(anchor);
            CoreEvent::CommentPlace {
                page,
                node,
                x: l.x,
                y: l.y,
                wx: w.x,
                wy: w.y,
                sx: screen.x,
                sy: screen.y,
            }
        }
        EditorEvent::CommentOpened { id, screen } => CoreEvent::CommentOpened {
            id: id.to_string(),
            sx: screen.x,
            sy: screen.y,
        },
        EditorEvent::CommentMoved { id, anchor } => {
            let (page, node, l, w) = place(anchor);
            CoreEvent::CommentMoved {
                id: id.to_string(),
                page,
                node,
                x: l.x,
                y: l.y,
                wx: w.x,
                wy: w.y,
            }
        }
        EditorEvent::CommentAnchor { screen, visible } => CoreEvent::CommentAnchor {
            sx: screen.x,
            sy: screen.y,
            visible: *visible,
        },
        EditorEvent::CommentsState {
            mode,
            visible,
            available,
        } => CoreEvent::CommentsState {
            mode: *mode,
            visible: *visible,
            available: *available,
        },
        _ => return None,
    })
}
