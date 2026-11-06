//! Другие участники файла (спека подсистемы 4, раздел 9): их курсоры с плавным движением,
//! рамки выделения, живое превью их жестов и следование за участником; своё присутствие для
//! них — `Editor::own_presence`.
//!
//! Данные из сети не доверены: числа проверил разбор кадра, здесь — смысл (страница, узлы,
//! сколько участников рисуется). Живое превью меняет только сцену — слой
//! `Scene::set_remote_preview` под своим превью — и никогда документ. Пока идёт свой жест,
//! правка текста или вектора и любая команда (`Editor::presence_hold`), слоя нет: иначе
//! базы своего жеста прочли бы чужое превью и записали бы его в документ.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use ferrum_doc::{NodeId, PageId, PropsPatch};
use ferrum_render::{Camera, RemoteMark, Rgba, REMOTE_BOXES_MAX};
use ferrum_scene::Scene;
use kurbo::{Affine, Point, Rect, Size};

use crate::editor::{Editor, EditorEvent};

/// Курсоры и рамки рисуются у стольких участников страницы, остальные — только в счётчике.
pub const PEERS_DRAWN: usize = 50;
/// Участник без кадров дольше этого пропадает (спека, раздел 9).
pub const PEER_STALE_MS: f64 = 30_000.0;
/// Плавное движение курсора: от прежнего положения к новому за промежуток между кадрами
/// участника (20 в секунду — 50 мс), не короче и не длиннее этих пределов.
pub const CURSOR_LERP_MIN_MS: f64 = 16.0;
pub const CURSOR_LERP_MAX_MS: f64 = 150.0;

/// Вид чужого жеста.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureKind {
    Move,
    Resize,
    Rotate,
    Scale,
}

/// Жест: матрица в мире, рамка выделения на начало (у ресайза и «Масштаба») и равномерный
/// коэффициент «Масштаба». Узлы жеста — выделение участника.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gesture {
    pub kind: GestureKind,
    pub transform: Affine,
    pub frame: Affine,
    pub k: f64,
}

/// Заголовок чужого выделения: узлы приходят частями, рамка — сразу.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeerSelection {
    pub rev: u32,
    pub count: u32,
    pub parts: u8,
    pub bounds: Rect,
}

/// Состояние участника из его последнего кадра. `page` — `None`, если такой страницы здесь
/// нет (удалена, ещё не пришла).
#[derive(Debug, Clone, PartialEq)]
pub struct PeerState {
    pub page: Option<PageId>,
    pub view: Rect,
    pub cursor: Option<Point>,
    pub selection: Option<PeerSelection>,
    pub gesture: Option<Gesture>,
}

#[derive(Debug, Clone, Copy)]
struct Lerp {
    from: Point,
    to: Point,
    start: f64,
    dur: f64,
}

impl Lerp {
    fn at(&self, now: f64) -> Point {
        let t = ((now - self.start) / self.dur).clamp(0.0, 1.0);
        self.from.lerp(self.to, t)
    }

    fn done(&self, now: f64) -> bool {
        now - self.start >= self.dur
    }
}

struct Peer {
    user: Arc<str>,
    name: Arc<str>,
    color: Rgba,
    state: Option<PeerState>,
    cursor: Option<Lerp>,
    /// Когда пришло прошлое положение курсора: длина плавного движения.
    cursor_at: f64,
    rev: u32,
    parts: Vec<Option<Vec<NodeId>>>,
    /// Выделение целиком, когда пришли все части его версии.
    ids: Option<Arc<[NodeId]>>,
    seen: f64,
}

impl Peer {
    fn on_page(&self, page: PageId) -> bool {
        self.state.as_ref().is_some_and(|s| s.page == Some(page))
    }

    /// Выделение целиком, если пришли все части нынешней версии.
    fn selection(&self) -> Option<&Arc<[NodeId]>> {
        let head = self.state.as_ref()?.selection?;
        (head.rev == self.rev).then_some(self.ids.as_ref()?)
    }
}

/// Участник для шапки и тестов.
#[derive(Debug, Clone, PartialEq)]
pub struct PeerInfo {
    pub id: u32,
    pub user: Arc<str>,
    pub name: Arc<str>,
    pub page: Option<PageId>,
}

type MarksCache = ((u64, u64, u64, PageId), Arc<[RemoteMark]>);

#[derive(Default)]
pub(crate) struct Presence {
    peers: BTreeMap<u32, Peer>,
    /// Растёт при всём, что видно на холсте: кадр нужен заново.
    pub(crate) revision: u64,
    /// Растёт, когда меняется живое превью: жест участника, его выделение, уход.
    preview_rev: u64,
    /// С чем посчитан слой чужого превью в сцене: версия превью, сцена, страница.
    applied: Option<(u64, u64, u64, PageId)>,
    /// Своё действие идёт (`Editor::presence_hold`): слоя нет.
    holds: u32,
    pub(crate) follow: Option<u32>,
    /// Страница, на которую следование переключает само: это не действие человека.
    following_switch: bool,
    marks: RefCell<Option<MarksCache>>,
    now: f64,
}

impl Presence {
    fn bump(&mut self, preview: bool) {
        self.revision += 1;
        if preview {
            self.preview_rev += 1;
        }
    }

    fn animating(&self) -> bool {
        let now = self.now;
        self.peers
            .values()
            .any(|p| p.cursor.is_some_and(|c| !c.done(now)))
    }
}

/// Цвет участника из `#rrggbb` палитры протокола.
pub fn peer_color(rgb: u32) -> Rgba {
    Rgba::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

impl Editor {
    /// Участник пришёл: его человек, имя и цвет — с сервера. Повторный приход того же
    /// номера (имя сменилось) состояние не теряет.
    pub fn presence_join(&mut self, id: u32, user: &str, name: &str, color: Rgba) {
        let now = self.presence.now;
        let p = self.presence.peers.entry(id).or_insert_with(|| Peer {
            user: Arc::from(user),
            name: Arc::from(name),
            color,
            state: None,
            cursor: None,
            cursor_at: now,
            rev: 0,
            parts: Vec::new(),
            ids: None,
            seen: now,
        });
        p.user = Arc::from(user);
        p.name = Arc::from(name);
        p.color = color;
        p.seen = now;
        self.presence.bump(false);
        self.emit();
    }

    /// Состояние участника. Участник без `join` пропускается: имя и цвет знает только сервер.
    pub fn presence_state(&mut self, id: u32, state: PeerState) {
        let now = self.presence.now;
        let Some(p) = self.presence.peers.get_mut(&id) else {
            return;
        };
        p.seen = now;
        // Курсор движется плавно от того места, где он виден сейчас.
        let same_page = p.state.as_ref().map(|s| s.page) == Some(state.page);
        p.cursor = match (state.cursor, p.cursor, same_page) {
            (Some(to), Some(c), true) => Some(Lerp {
                from: c.at(now),
                to,
                start: now,
                dur: (now - p.cursor_at).clamp(CURSOR_LERP_MIN_MS, CURSOR_LERP_MAX_MS),
            }),
            (Some(to), _, _) => Some(Lerp {
                from: to,
                to,
                start: now,
                dur: CURSOR_LERP_MIN_MS,
            }),
            (None, _, _) => None,
        };
        if state.cursor.is_some() {
            p.cursor_at = now;
        }
        // Новая версия выделения: прежние части не годятся.
        let head = state.selection;
        match head {
            Some(h) if h.rev != p.rev || h.parts as usize != p.parts.len() => {
                p.rev = h.rev;
                p.parts = vec![None; h.parts as usize];
                p.ids = None;
            }
            None => {
                p.parts.clear();
                p.ids = None;
            }
            _ => {}
        }
        let old = p.state.replace(state);
        let preview = {
            let new = p.state.as_ref().expect("только что поставлено");
            old.as_ref().map(|s| (s.gesture, s.selection, s.page))
                != Some((new.gesture, new.selection, new.page))
        };
        self.presence.bump(preview);
        if self.presence.follow == Some(id) {
            self.follow_peer();
        }
        self.emit();
    }

    /// Часть чужого выделения. Часть другой версии начинает её заново.
    pub fn presence_part(&mut self, id: u32, rev: u32, index: u8, parts: u8, ids: Vec<NodeId>) {
        let now = self.presence.now;
        let Some(p) = self.presence.peers.get_mut(&id) else {
            return;
        };
        if index >= parts {
            return;
        }
        p.seen = now;
        if rev != p.rev || parts as usize != p.parts.len() {
            p.rev = rev;
            p.parts = vec![None; parts as usize];
            p.ids = None;
        }
        p.parts[index as usize] = Some(ids);
        if p.parts.iter().all(Option::is_some) {
            let all: Vec<NodeId> = p.parts.iter().flatten().flatten().copied().collect();
            p.ids = Some(Arc::from(all));
        }
        self.presence.bump(true);
        self.emit();
    }

    /// Участник ушёл: его курсор, рамка и живое превью пропадают сразу; следование за ним
    /// кончается.
    pub fn presence_left(&mut self, id: u32) {
        if self.presence.peers.remove(&id).is_none() {
            return;
        }
        if self.presence.follow == Some(id) {
            self.set_follow(None);
        }
        self.presence.bump(true);
        self.emit();
    }

    /// Соединение заново: участники придут снова, прежние не остаются двойниками.
    pub fn presence_reset(&mut self) {
        if self.presence.peers.is_empty() {
            return;
        }
        self.presence.peers.clear();
        if self.presence.follow.is_some() {
            self.set_follow(None);
        }
        self.presence.bump(true);
        self.emit();
    }

    /// Время ядра: плавное движение курсоров и уход молчащих участников.
    pub(crate) fn presence_time(&mut self, now: f64) -> bool {
        let was = self.presence.animating();
        self.presence.now = now;
        let stale: Vec<u32> = self
            .presence
            .peers
            .iter()
            .filter(|(_, p)| now - p.seen > PEER_STALE_MS)
            .map(|(id, _)| *id)
            .collect();
        for id in &stale {
            self.presence.peers.remove(id);
            if self.presence.follow == Some(*id) {
                self.set_follow(None);
            }
        }
        if !stale.is_empty() {
            self.presence.bump(true);
            return true;
        }
        if was {
            self.presence.revision += 1;
            return true;
        }
        false
    }

    /// Участники файла (шапка, тесты), по номеру.
    pub fn peers(&self) -> Vec<PeerInfo> {
        self.presence
            .peers
            .iter()
            .map(|(id, p)| PeerInfo {
                id: *id,
                user: p.user.clone(),
                name: p.name.clone(),
                page: p.state.as_ref().and_then(|s| s.page),
            })
            .collect()
    }

    /// За кем следуем.
    pub fn following(&self) -> Option<u32> {
        self.presence.follow
    }

    /// Следовать за участником (`None` — перестать): камера и страница повторяют его вид.
    pub fn follow(&mut self, id: Option<u32>) {
        let id = id.filter(|id| self.presence.peers.contains_key(id));
        self.set_follow(id);
        if id.is_some() {
            self.follow_peer();
        }
        self.emit();
    }

    pub(crate) fn set_follow(&mut self, id: Option<u32>) {
        if self.presence.follow != id {
            self.presence.follow = id;
            self.out.events.push(EditorEvent::Following(id));
        }
    }

    /// Действие на холсте кончает следование (поведение, как в Figma).
    pub(crate) fn stop_following(&mut self) {
        if self.presence.follow.is_some() && !self.presence.following_switch {
            self.set_follow(None);
        }
    }

    /// Вид участника, за которым следуем: его страница и камера, вписанная в свой холст.
    fn follow_peer(&mut self) {
        let Some(id) = self.presence.follow else {
            return;
        };
        let Some(state) = self.presence.peers.get(&id).and_then(|p| p.state.clone()) else {
            return;
        };
        if let Some(page) = state.page {
            if page != self.page && self.doc.page(page).is_ok() {
                self.presence.following_switch = true;
                let _ = self.set_page(page);
                self.presence.following_switch = false;
            }
        }
        if state.page == Some(self.page) {
            let mut camera = self.camera.clone();
            fit_view(&mut camera, state.view);
            if camera != self.camera {
                self.camera = camera;
                self.presence.revision += 1;
            }
        }
    }

    /// Своё действие начинается: слой чужого превью уходит из сцены до него, иначе правка
    /// прочла бы чужое превью как документ. Пары `presence_hold`/`presence_release`
    /// вкладываются.
    pub fn presence_hold(&mut self) {
        self.presence.holds += 1;
        self.drop_remote_preview();
    }

    pub fn presence_release(&mut self) {
        self.presence.holds = self.presence.holds.saturating_sub(1);
        self.emit();
    }

    pub(crate) fn drop_remote_preview(&mut self) {
        if self.scene.has_remote_preview() {
            self.scene.set_remote_preview(&[]);
        }
        self.presence.applied = None;
    }

    /// Слой чужого превью по нынешним жестам участников этой страницы. Нет его, пока идёт
    /// своё: жест, правка текста или вектора, удержание.
    pub(crate) fn sync_remote_preview(&mut self) {
        let own = self.presence.holds > 0
            || self.is_busy()
            || self.text_edit.is_some()
            || self.vector_edit.is_some()
            || self.panel.is_some();
        if own {
            self.drop_remote_preview();
            return;
        }
        let key = (
            self.presence.preview_rev,
            self.scene.generation(),
            self.scene_epoch,
            self.page,
        );
        if self.presence.applied == Some(key) {
            return;
        }
        // Базы — из документа: прежний слой сначала снимается.
        if self.scene.has_remote_preview() {
            self.scene.set_remote_preview(&[]);
        }
        let mut patches: Vec<(NodeId, PropsPatch)> = Vec::new();
        for p in self.presence.peers.values() {
            if !p.on_page(self.page) {
                continue;
            }
            let Some(g) = p.state.as_ref().and_then(|s| s.gesture) else {
                continue;
            };
            let Some(ids) = p.selection() else { continue };
            patches.extend(crate::tools::select::gesture_patches(
                &self.scene,
                ids,
                &g,
                &self.fonts,
            ));
        }
        if !patches.is_empty() {
            self.scene.set_remote_preview(&patches);
        }
        self.presence.applied = Some((
            self.presence.preview_rev,
            self.scene.generation(),
            self.scene_epoch,
            self.page,
        ));
    }

    /// Курсоры и рамки участников этой страницы для оверлея. Рамки считаются по сцене при её
    /// смене, курсоры — каждый кадр (плавное движение).
    pub(crate) fn remote_marks(&self) -> Arc<[RemoteMark]> {
        let key = (
            self.presence.revision,
            self.scene.change_counter(),
            self.scene.generation(),
            self.page,
        );
        if let Some((k, marks)) = &*self.presence.marks.borrow() {
            if *k == key {
                return marks.clone();
            }
        }
        let now = self.presence.now;
        let page = self.page;
        let scene = &self.scene;
        let marks: Vec<RemoteMark> = self
            .presence
            .peers
            .values()
            .filter(|p| p.on_page(page))
            .take(PEERS_DRAWN)
            .map(|p| {
                let state = p.state.as_ref().expect("на странице — есть состояние");
                let (boxes, frame) = selection_marks(scene, p, state);
                RemoteMark {
                    color: p.color,
                    name: p.name.clone(),
                    cursor: p.cursor.map(|c| c.at(now)),
                    boxes,
                    frame,
                }
            })
            .collect();
        let marks: Arc<[RemoteMark]> = Arc::from(marks);
        *self.presence.marks.borrow_mut() = Some((key, marks.clone()));
        marks
    }

    /// Номер того, что видно от участников: входит в ключ кадра.
    pub(crate) fn presence_revision(&self) -> u64 {
        self.presence.revision
    }

    /// Своё присутствие для других: страница, вид, курсор над холстом, выделение и жест.
    pub fn own_presence(&self) -> OwnPresence {
        let cursor = self
            .last_pointer
            .filter(|_| self.pointer_inside)
            .map(|e| self.camera.screen_to_world(e.point))
            .filter(|p| p.x.is_finite() && p.y.is_finite());
        OwnPresence {
            page: self.page,
            view: self.camera.visible_world_rect(),
            cursor,
            selection_version: self.selection.version(),
            gesture: if self.tool.is_busy() {
                self.tool.gesture()
            } else {
                None
            },
        }
    }
}

/// Своё присутствие: выделение — номером версии, узлы и рамку ядро берёт у редактора сам.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnPresence {
    pub page: PageId,
    pub view: Rect,
    pub cursor: Option<Point>,
    pub selection_version: u64,
    pub gesture: Option<Gesture>,
}

/// Рамки чужого выделения: узлы, которые есть в сцене (не больше `REMOTE_BOXES_MAX`), и общая
/// рамка. Пока части не пришли или их больше предела — рамка из кадра, сдвинутая жестом.
fn selection_marks(
    scene: &Scene,
    p: &Peer,
    state: &PeerState,
) -> (Vec<(Affine, Rect)>, Option<Rect>) {
    let Some(head) = state.selection else {
        return (Vec::new(), None);
    };
    let fallback = || {
        let r = match state.gesture {
            Some(g) => g.transform.transform_rect_bbox(head.bounds),
            None => head.bounds,
        };
        (Vec::new(), Some(r))
    };
    let Some(ids) = p.selection() else {
        return fallback();
    };
    if ids.len() < head.count as usize {
        return fallback();
    }
    let mut boxes = Vec::new();
    let mut union: Option<Rect> = None;
    let mut found = 0;
    for id in ids.iter() {
        let Some(n) = scene.node(*id) else { continue };
        let Some(b) = n.local_bounds else { continue };
        if !n.props.visible {
            continue;
        }
        found += 1;
        if boxes.len() < REMOTE_BOXES_MAX {
            boxes.push((n.world, b));
        }
        let w = n.world.transform_rect_bbox(b);
        union = Some(union.map_or(w, |u| u.union(w)));
    }
    if found == 0 {
        return (Vec::new(), None);
    }
    // У одного узла его рамка и есть рамка выделения.
    let frame = if found > 1 { union } else { None };
    (boxes, frame)
}

/// Камера так, чтобы вид участника целиком был виден в своём холсте (по центру).
pub fn fit_view(camera: &mut Camera, view: Rect) {
    let Size { width, height } = camera.viewport();
    if view.width() <= 0.0 || view.height() <= 0.0 || width <= 0.0 || height <= 0.0 {
        return;
    }
    let zoom = (width / view.width()).min(height / view.height());
    camera.set_zoom(zoom);
    camera.set_center(view.center());
}
