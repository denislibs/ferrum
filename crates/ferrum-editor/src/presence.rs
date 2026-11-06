//! Другие участники файла (спека подсистемы 4, раздел 9): их курсоры с плавным движением,
//! рамки выделения, живое превью их жестов и следование за участником; своё присутствие для
//! них — `Editor::own_presence`.
//!
//! Данные из сети не доверены: числа проверил разбор кадра, здесь — смысл (страница, узлы,
//! сколько участников рисуется). Живое превью меняет только сцену — слой
//! `Scene::set_remote_preview` под своим превью — и никогда документ. Цель нажатия выбирается по
//! тому, что видно (со слоем), а каждое своё действие (`Editor::own_action`: нажатие
//! инструмента, команда, правка из панелей, копия, вход в правку) снимает слой до первого
//! чтения опор записи, и пока идёт свой жест, правка текста или вектора и удержание ядра
//! (`Editor::presence_hold`), слоя нет: иначе базы своего действия прочли бы чужое превью и
//! записали бы его в документ.

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
/// Живое превью чужих жестов **(поведение)**: узлов у одного участника не больше
/// `PREVIEW_PEER_NODES` (с поддеревом, которое меняет жест), у всех вместе — не больше
/// `PREVIEW_NODES`, участников с превью — не больше `PREVIEW_PEERS`, первые по номеру. Сверх
/// пределов превью участника нет: его курсор и рамка выделения, сдвинутая матрицей жеста,
/// остаются. Пересборка слоя — работа ядра на каждой пачке, пока кто-то тянет: пределы держат
/// её цену, сколько бы участников ни прислал сервер (ревью безопасности, H2). Перетаскивание
/// 1000 узлов в них не упирается.
pub const PREVIEW_PEER_NODES: usize = 2000;
pub const PREVIEW_NODES: usize = 3000;
pub const PREVIEW_PEERS: usize = 8;
/// Узлы чужого выделения хранятся и обходятся у выделений не больше `PEER_SELECTION_MAX`
/// узлов, у всех участников вместе — не больше `REMOTE_IDS_MAX`. Больше **(поведение)** — части
/// не хранятся, выделение участника видно рамкой из его кадра (сдвинутой матрицей жеста) без
/// рамок узлов и без превью: иначе память ядра и обход на смене сцены росли бы с каждым
/// соединением (ревью безопасности, M3). Выделение больше предела превью всё равно не получает.
pub const PEER_SELECTION_MAX: usize = PREVIEW_PEER_NODES;
pub const REMOTE_IDS_MAX: usize = 100_000;
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

/// Состояние участника из его последнего кадра. `page` — его страница, даже если её здесь нет
/// (удалена, ещё не пришла); `None` — номер не разобран.
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
    /// Части нынешней версии; когда пришли все, их узлы переходят в `ids`, а здесь остаются
    /// пустые отметки.
    parts: Vec<Option<Vec<NodeId>>>,
    /// Выделение целиком, когда пришли все части его версии.
    ids: Option<Arc<[NodeId]>>,
    seen: f64,
    /// Растёт при всём, от чего зависят рамки выделения участника: его выделение, части, жест,
    /// страница.
    marks_rev: u64,
    /// Рамки выделения и с чем они посчитаны: курсор движется каждый кадр, а рамки — только на
    /// смене выделения участника или сцены (ревью безопасности, M3).
    marks: RefCell<Option<(MarksKey, SelectionMarks)>>,
}

/// Рамки узлов выделения и общая рамка.
type SelectionMarks = (Vec<(Affine, Rect)>, Option<Rect>);
/// Версия рамок участника, сцена (счётчик изменений со слоем превью, `generation`,
/// `scene_epoch`), страница и есть ли превью участника в слое.
type MarksKey = (u64, u64, u64, u64, PageId, bool);

impl Peer {
    fn on_page(&self, page: PageId) -> bool {
        self.state.as_ref().is_some_and(|s| s.page == Some(page))
    }

    /// Выделение целиком, если пришли все части нынешней версии и узлов столько, сколько
    /// сказано в заголовке.
    fn selection(&self) -> Option<&Arc<[NodeId]>> {
        let head = self.state.as_ref()?.selection?;
        let ids = self.ids.as_ref()?;
        (head.rev == self.rev && ids.len() == head.count as usize).then_some(ids)
    }

    /// Сколько узлов выделения хранится.
    fn stored(&self) -> usize {
        self.ids.as_ref().map_or(0, |ids| ids.len())
            + self.parts.iter().flatten().map(Vec::len).sum::<usize>()
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
/// Версия превью, сцена (`generation`, `scene_epoch`), её документ без чужого слоя
/// (`Scene::own_change_counter`: чужая или своя правка узла под превью — слой заново от нового
/// документа; само наложение слоя этот счётчик не трогает, цикла нет) и страница.
type PreviewKey = (u64, u64, u64, u64, PageId);
/// Версия выделения, сцена (`generation`, `scene_epoch`) и счётчик её изменений без чужого слоя
/// (`Scene::own_change_counter`): наложение слоя рамку не старит.
type OwnBoundsKey = (u64, u64, u64, u64);

#[derive(Default)]
pub(crate) struct Presence {
    peers: BTreeMap<u32, Peer>,
    /// Растёт при всём, что видно на холсте: кадр нужен заново.
    pub(crate) revision: u64,
    /// Растёт, когда меняется живое превью: жест участника, его выделение, уход.
    preview_rev: u64,
    /// С чем посчитан слой чужого превью в сцене (`PreviewKey`).
    applied: Option<PreviewKey>,
    /// Участники, чьё превью сейчас в слое (остальные с жестом — за пределами).
    previewed: Vec<u32>,
    /// Своё действие идёт (`Editor::presence_hold`): слоя нет.
    holds: u32,
    /// Идёт пачка записей (`Editor::presence_batch`): `emit` — в её конце.
    batch: u32,
    pub(crate) follow: Option<u32>,
    /// Страница, на которую следование переключает само: это не действие человека.
    following_switch: bool,
    marks: RefCell<Option<MarksCache>>,
    /// Рамка своего выделения по документу, посчитанная перед наложением слоя, и с чем она
    /// верна (`Editor::own_bounds_key`).
    own_bounds: Option<(OwnBoundsKey, Option<Rect>)>,
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
    /// Записи одной пачки присутствия: слой чужого превью пересобирается и события уходят один раз
    /// в конце, а не на каждую запись (иначе пачка из N записей с жестами стоила бы N пересборок
    /// по всем участникам — ревью безопасности, H2). Пачки вкладываются.
    pub fn presence_batch(&mut self, f: impl FnOnce(&mut Editor)) {
        self.presence.batch += 1;
        f(self);
        self.presence.batch -= 1;
        if self.presence.batch == 0 {
            self.emit();
        }
    }

    /// `emit` после записи присутствия — сразу или в конце пачки.
    fn presence_emit(&mut self) {
        if self.presence.batch == 0 {
            self.emit();
        }
    }

    /// Известен ли участник.
    pub fn has_peer(&self, id: u32) -> bool {
        self.presence.peers.contains_key(&id)
    }

    /// Сколько участников известно.
    pub fn peer_count(&self) -> usize {
        self.presence.peers.len()
    }

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
            marks_rev: 0,
            marks: RefCell::new(None),
        });
        p.user = Arc::from(user);
        p.name = Arc::from(name);
        p.color = color;
        p.seen = now;
        self.presence.bump(false);
        self.presence_emit();
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
        if preview {
            p.marks_rev += 1;
        }
        self.presence.bump(preview);
        if self.presence.follow == Some(id) {
            self.follow_peer();
        }
        self.presence_emit();
    }

    /// Часть чужого выделения. Часть другой версии начинает её заново. Узлов больше, чем сказано
    /// в заголовке, больше `PEER_SELECTION_MAX` или больше `REMOTE_IDS_MAX` у всех вместе — часть
    /// не хранится: выделение видно рамкой из кадра.
    pub fn presence_part(&mut self, id: u32, rev: u32, index: u8, parts: u8, ids: Vec<NodeId>) {
        let now = self.presence.now;
        let others: usize = self
            .presence
            .peers
            .iter()
            .filter(|(k, _)| **k != id)
            .map(|(_, p)| p.stored())
            .sum();
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
        } else if p.ids.is_some() {
            // Версия уже собрана: повтор части ничего не меняет.
            return;
        }
        let count = p
            .state
            .as_ref()
            .and_then(|s| s.selection)
            .filter(|h| h.rev == rev)
            .map_or(PEER_SELECTION_MAX, |h| h.count as usize);
        let mine = p.stored() - p.parts[index as usize].as_ref().map_or(0, Vec::len) + ids.len();
        if mine > count.min(PEER_SELECTION_MAX) || others + mine > REMOTE_IDS_MAX {
            return;
        }
        p.parts[index as usize] = Some(ids);
        if p.parts.iter().all(Option::is_some) {
            // Узлы переходят в `ids` целиком, в частях — пустые отметки: не храним дважды.
            let all: Vec<NodeId> = p
                .parts
                .iter_mut()
                .flat_map(|part| part.as_mut().map(std::mem::take).unwrap_or_default())
                .collect();
            p.ids = Some(Arc::from(all));
        }
        p.marks_rev += 1;
        self.presence.bump(true);
        self.presence_emit();
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
        self.presence_emit();
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
        self.presence_emit();
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
    /// Страница, которой ещё нет, — переход, как только она дойдёт (`apply_remote`).
    pub(crate) fn follow_peer(&mut self) {
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
        self.own_action();
    }

    pub fn presence_release(&mut self) {
        self.presence.holds = self.presence.holds.saturating_sub(1);
        self.emit();
    }

    /// Своё действие начинается: нажатие инструмента (`press_tool` — после выбора цели по виду),
    /// команда (`run`), правка из панелей (`begin_doc_edit`, поля и стиль текста), копия, вход
    /// в правку текста и вектора (`start_text_edit`, `start_vector_edit`), удержание ядра. Всё
    /// это читает свойства и рамки из сцены и пишет по ним документ, поэтому чужой слой уходит
    /// до первого такого чтения; `emit` в конце вызова вернёт его поверх нового документа, если
    /// своего жеста или правки не осталось. Остальной ввод (движение, колесо, модификаторы,
    /// отпускание без жеста, потеря фокуса) по сцене не пишет и слой не трогает; запись внутри
    /// идущего жеста, правки текста или вектора идёт без слоя — его снимает `emit`
    /// (`sync_remote_preview`).
    pub(crate) fn own_action(&mut self) {
        self.drop_remote_preview();
    }

    pub(crate) fn drop_remote_preview(&mut self) {
        if self.scene.has_remote_preview() {
            self.scene.set_remote_preview(&[]);
        }
        self.presence.applied = None;
        self.presence.previewed.clear();
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
        if self.presence.applied == Some(self.preview_key()) {
            return;
        }
        // Базы — из документа: прежний слой сначала снимается.
        if self.scene.has_remote_preview() {
            self.scene.set_remote_preview(&[]);
        }
        let mut patches: Vec<(NodeId, PropsPatch)> = Vec::new();
        let mut previewed = Vec::new();
        for (id, p) in &self.presence.peers {
            if previewed.len() == PREVIEW_PEERS {
                break;
            }
            if !p.on_page(self.page) {
                continue;
            }
            let Some(g) = p.state.as_ref().and_then(|s| s.gesture) else {
                continue;
            };
            let Some(ids) = p.selection() else { continue };
            let budget = PREVIEW_PEER_NODES.min(PREVIEW_NODES - patches.len());
            // Выделение больше остатка — превью нет, и обходить его не нужно.
            if ids.len() > budget {
                continue;
            }
            let Some(own) =
                crate::tools::select::gesture_patches(&self.scene, ids, &g, &self.fonts, budget)
            else {
                continue;
            };
            if !own.is_empty() {
                patches.extend(own);
                previewed.push(*id);
            }
        }
        self.presence.previewed = previewed;
        if !patches.is_empty() {
            // Сцена сейчас — документ: своя рамка для других считается здесь, а не под слоем, и
            // только если с прошлого раза сменились выделение или документ — не на каждый кадр
            // участника (проход по своему выделению, до 100 000 узлов).
            let key = self.own_bounds_key();
            if self.presence.own_bounds.is_none_or(|(k, _)| k != key) {
                self.presence.own_bounds = Some((key, self.selection_world_bounds()));
            }
            self.scene.set_remote_preview(&patches);
        }
        self.presence.applied = Some(self.preview_key());
    }

    fn preview_key(&self) -> PreviewKey {
        (
            self.presence.preview_rev,
            self.scene.generation(),
            self.scene_epoch,
            self.scene.own_change_counter(),
            self.page,
        )
    }

    /// Курсоры и рамки участников этой страницы для оверлея. Курсоры — каждый кадр (плавное
    /// движение), рамки выделения — из кэша участника, пока не сменились его выделение, жест или
    /// сцена.
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
            .iter()
            .filter(|(_, p)| p.on_page(page))
            .take(PEERS_DRAWN)
            .map(|(id, p)| {
                let state = p.state.as_ref().expect("на странице — есть состояние");
                let previewed = self.presence.previewed.contains(id);
                let mk = (
                    p.marks_rev,
                    scene.change_counter(),
                    scene.generation(),
                    self.scene_epoch,
                    page,
                    previewed,
                );
                let mut cache = p.marks.borrow_mut();
                let (boxes, frame) = match &*cache {
                    Some((k, m)) if *k == mk => m.clone(),
                    _ => {
                        let m = selection_marks(scene, p, state, previewed);
                        *cache = Some((mk, m.clone()));
                        m
                    }
                };
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

    fn own_bounds_key(&self) -> OwnBoundsKey {
        (
            self.selection.version(),
            self.scene.generation(),
            self.scene_epoch,
            self.scene.own_change_counter(),
        )
    }

    /// Мировая рамка выделенных узлов по сцене.
    fn selection_world_bounds(&self) -> Option<Rect> {
        self.selection
            .iter()
            .filter_map(|id| {
                let n = self.scene.node(*id)?;
                Some(n.world.transform_rect_bbox(n.local_bounds?))
            })
            .reduce(|a, b| a.union(b))
    }

    /// Рамка своего выделения для других участников — по документу, без чужого превью (своё
    /// превью жеста в ней есть: его видят и другие). Под чужим слоем — рамка, посчитанная перед
    /// его наложением; если с тех пор сменились выделение или документ, слой накладывается
    /// заново и рамка считается с ним.
    pub fn own_selection_bounds(&mut self) -> Option<Rect> {
        let fresh = |me: &Self| {
            me.presence
                .own_bounds
                .filter(|(k, _)| *k == me.own_bounds_key())
                .map(|(_, b)| b)
        };
        if !self.scene.has_remote_preview() {
            return self.selection_world_bounds();
        }
        if let Some(b) = fresh(self) {
            return b;
        }
        self.presence.applied = None;
        self.sync_remote_preview();
        if !self.scene.has_remote_preview() {
            return self.selection_world_bounds();
        }
        let b = fresh(self);
        debug_assert!(b.is_some(), "слой наложен вместе с рамкой");
        b.unwrap_or_else(|| {
            // Не бывает: слой только что наложен вместе с рамкой. На всякий случай — по сцене без
            // слоя, и слой обратно; событий нет: сцена та же, что была.
            self.drop_remote_preview();
            let b = self.selection_world_bounds();
            self.sync_remote_preview();
            b
        })
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
/// рамка. Пока части не пришли, узлов больше предела или жест участника идёт без превью (за
/// пределами превью) — рамка из кадра, сдвинутая жестом.
fn selection_marks(scene: &Scene, p: &Peer, state: &PeerState, previewed: bool) -> SelectionMarks {
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
    if state.gesture.is_some() && !previewed {
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
