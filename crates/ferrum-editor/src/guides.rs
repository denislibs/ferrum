//! Линейки и направляющие в редакторе (спека подсистемы 2, раздел 9): где они на
//! холсте, начало отсчёта линеек и кэш линий. Рисует их рендер, данные — в документе.
//!
//! Как в Figma, направляющие видны, пока включены линейки: «Вид → Направляющие»
//! дополнительно прячет их при включённых линейках.

use std::cell::RefCell;
use std::sync::Arc;

use ferrum_doc::{Guide, GuideAxis, GuideOwner, NodeKind};
use ferrum_render::{GuideMark, RulersOverlay};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Point, Rect};

use crate::editor::Editor;
use crate::geometry::world_box;

/// Направляющая на холсте.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacedGuide {
    pub owner: GuideOwner,
    pub guide: Guide,
    pub mark: GuideMark,
    /// Фрейм повёрнут: линия косая, к ней нет ни привязки, ни попадания.
    pub rotated: bool,
}

/// Линии текущей страницы; считаются заново, только когда изменился ключ.
#[derive(Debug, Default)]
pub(crate) struct GuideCache {
    /// Номер пересчёта: `FrameKey` сравнивает его, а не список.
    pub revision: u64,
    key: Option<(u64, u64, u64)>,
    pub lines: Arc<Vec<PlacedGuide>>,
    /// Фреймы верхнего уровня с направляющими и метка состава, при которой они
    /// найдены: обход корней страницы — только при изменении состава сцены.
    owners: Vec<Slot>,
    owners_key: Option<(u64, u64)>,
}

/// Начало отсчёта и полоса выделения: считаются обходом выделения, поэтому до
/// смены сцены или выделения хранятся (`Editor::overlay` зовётся на каждый кадр).
pub(crate) type RulersCache = RefCell<Option<((u64, u64, u64), RulersOverlay)>>;

/// Верхний узел цепочки родителей.
fn root_of(scene: &Scene, mut slot: Slot) -> Slot {
    while let Some(p) = scene.at(slot).parent {
        slot = p;
    }
    slot
}

/// Фрейм без поворота и наклона: его оси — оси мира.
pub(crate) fn axis_aligned(scene: &Scene, slot: Slot) -> bool {
    let [a, b, c, d, ..] = scene.at(slot).world.as_coeffs();
    b.abs() < 1e-9 && c.abs() < 1e-9 && a > 0.0 && d > 0.0
}

/// Линия фрейма в мире: от края до края его рамки. У повёрнутого — косая.
fn frame_line(scene: &Scene, slot: Slot, g: Guide) -> Line {
    let n = scene.at(slot);
    let (w, h) = (n.props.w, n.props.h);
    let (a, b) = match g.axis {
        GuideAxis::X => (Point::new(g.offset, 0.0), Point::new(g.offset, h)),
        GuideAxis::Y => (Point::new(0.0, g.offset), Point::new(w, g.offset)),
    };
    Line::new(n.world * a, n.world * b)
}

/// Линия страницы: концы задают только её ось, рисуется через весь холст.
pub(crate) fn page_line(g: Guide) -> Line {
    match g.axis {
        GuideAxis::X => Line::new((g.offset, 0.0), (g.offset, 1.0)),
        GuideAxis::Y => Line::new((0.0, g.offset), (1.0, g.offset)),
    }
}

impl Editor {
    /// Линейки на холсте: включены, и интерфейс не скрыт (Ctrl/Cmd+\ прячет и их).
    pub fn rulers_visible(&self) -> bool {
        self.settings.rulers && !self.settings.ui_hidden
    }

    /// Направляющие на холсте и как цели привязки: как в Figma — только при линейках.
    pub fn guides_visible(&self) -> bool {
        self.settings.rulers && self.settings.guides
    }

    /// Показ линеек и направляющих — настройка вида, хранит её интерфейс.
    /// `None` — не менять. Шага истории нет.
    pub fn set_view(&mut self, rulers: Option<bool>, guides: Option<bool>) {
        if let Some(v) = rulers {
            self.settings.rulers = v;
        }
        if let Some(v) = guides {
            self.settings.guides = v;
        }
        self.emit();
    }

    /// Сообщает интерфейсу новый показ: настройку хранит он.
    pub(crate) fn push_view(&mut self) {
        self.out
            .events
            .push(crate::editor::EditorEvent::ViewChanged {
                rulers: self.settings.rulers,
                guides: self.settings.guides,
            });
    }

    /// Начало отсчёта линеек — левый верхний угол фрейма верхнего уровня, под которым
    /// всё выделение (без поворота), иначе ноль страницы; полоса — рамка выделения.
    pub(crate) fn rulers_overlay(&self) -> Option<RulersOverlay> {
        if !self.rulers_visible() {
            return None;
        }
        let key = (
            self.scene_epoch,
            self.scene.change_counter(),
            self.selection.version(),
        );
        if let Some((k, r)) = *self.rulers_cache.borrow() {
            if k == key {
                return Some(r);
            }
        }
        let mut band: Option<Rect> = None;
        let mut top: Option<Option<Slot>> = None;
        for id in self.selection.iter() {
            let Some(slot) = self.scene.slot(*id) else {
                continue;
            };
            if let Some(b) = world_box(&self.scene, slot) {
                band = Some(band.map_or(b, |u| u.union(b)));
            }
            let root = root_of(&self.scene, slot);
            top = match top {
                None => Some(Some(root)),
                Some(Some(t)) if t == root => top,
                _ => Some(None),
            };
        }
        let origin = top
            .flatten()
            .filter(|s| self.scene.at(*s).props.kind == NodeKind::Frame)
            .filter(|s| axis_aligned(&self.scene, *s))
            .and_then(|s| world_box(&self.scene, s))
            .map_or(Point::ZERO, |b| b.origin());
        let r = RulersOverlay { origin, band };
        *self.rulers_cache.borrow_mut() = Some((key, r));
        Some(r)
    }

    /// Пересчитывает линии, если изменились сцена, страница или её направляющие.
    pub(crate) fn refresh_guides(&mut self) {
        let key = (
            self.scene.generation(),
            self.scene.change_counter(),
            self.doc.guides_revision(),
        );
        if self.guides.key == Some(key) {
            return;
        }
        let owners_key = (self.scene.generation(), self.scene.structure_stamp());
        if self.guides.owners_key != Some(owners_key) {
            let scene = &self.scene;
            self.guides.owners = scene
                .roots()
                .iter()
                .copied()
                .filter(|s| {
                    let p = &scene.at(*s).props;
                    p.kind == NodeKind::Frame && p.guides.is_some()
                })
                .collect();
            self.guides.owners_key = Some(owners_key);
        }
        let mut lines = Vec::new();
        for g in self.doc.page_guides(self.page) {
            lines.push(PlacedGuide {
                owner: GuideOwner::Page(self.page),
                guide: g,
                mark: GuideMark {
                    line: page_line(g),
                    infinite: true,
                },
                rotated: false,
            });
        }
        for &slot in &self.guides.owners {
            let Some(n) = self.scene.node_at(slot) else {
                continue;
            };
            let Some(list) = n.props.guides.as_ref() else {
                continue;
            };
            if !self.scene.is_visible(slot) {
                continue;
            }
            let rotated = !axis_aligned(&self.scene, slot);
            for g in list.iter() {
                lines.push(PlacedGuide {
                    owner: GuideOwner::Frame(n.id),
                    guide: *g,
                    mark: GuideMark {
                        line: frame_line(&self.scene, slot, *g),
                        infinite: false,
                    },
                    rotated,
                });
            }
        }
        // Пустой список остаётся пустым без новой ревизии, если и был пустым.
        if !(lines.is_empty() && self.guides.lines.is_empty()) {
            self.guides.lines = Arc::new(lines);
            self.guides.revision += 1;
        }
        self.guides.key = Some(key);
    }

    /// Метки направляющих для оверлея.
    pub(crate) fn guide_marks(&self) -> Vec<GuideMark> {
        if !self.guides_visible() {
            return Vec::new();
        }
        self.guides.lines.iter().map(|g| g.mark).collect()
    }
}
