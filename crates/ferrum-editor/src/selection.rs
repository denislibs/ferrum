//! Правила выделения на холсте (раздел 7.2 спеки), как в Figma:
//! - клик выбирает объект верхнего уровня; внутри фрейма верхнего уровня (он ведёт
//!   себя как монтажная область) — его ребёнка верхнего уровня;
//! - если выделение уже внутри контейнера, клик по соседям выбирает их на том же уровне;
//! - двойной клик — на уровень глубже, Ctrl/Cmd+клик — самый глубокий объект;
//! - пустое место фрейма верхнего уровня с детьми не выделяет фрейм (с него
//!   начинается рамка), фрейм выбирается кликом по его названию;
//! - заблокированные и скрытые узлы на холсте не выбираются, клик проходит сквозь них.

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_render::{Camera, LABEL_SIZE};
use ferrum_scene::{Scene, Slot};
use kurbo::{Point, Rect};

use crate::geometry::world_box;

/// Высота полосы над фреймом, где лежит его название (в экранных пикселях): кегль
/// подписи с межстрочным и отступ 4 px, как рисует оверлей.
pub const LABEL_HIT_PX: f64 = LABEL_SIZE * 1.5 + 4.0;

/// Как выбирать объект под курсором.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// Обычный клик: уровень по правилам контекста.
    Normal,
    /// Ctrl/Cmd+клик: самый глубокий объект.
    Deepest,
}

/// Заблокирован ли узел или кто-то из его предков.
pub fn locked(scene: &Scene, slot: Slot) -> bool {
    let mut cur = Some(slot);
    while let Some(s) = cur {
        let n = scene.at(s);
        if n.props.locked {
            return true;
        }
        cur = n.parent;
    }
    false
}

/// Цепочка от корня до узла включительно.
pub fn chain(scene: &Scene, slot: Slot) -> Vec<Slot> {
    let mut out = vec![slot];
    let mut cur = scene.at(slot).parent;
    while let Some(p) = cur {
        out.push(p);
        cur = scene.at(p).parent;
    }
    out.reverse();
    out
}

/// Верхний выбираемый узел под точкой (в мире): не черновик и не заблокирован.
/// Скрытые узлы сцена в попадания не включает.
pub fn top_hit(scene: &Scene, world: Point, tolerance: f64) -> Option<Slot> {
    scene
        .hit_test_all(world, tolerance)
        .into_iter()
        .filter(|id| !scene.is_draft(*id))
        .filter_map(|id| scene.slot(id))
        .find(|s| !locked(scene, *s))
}

/// Объект, который выберет клик по узлу `hit` при текущем выделении.
pub fn resolve(scene: &Scene, hit: Slot, selection: &[NodeId], pick: Pick) -> Option<NodeId> {
    if pick == Pick::Deepest {
        return Some(scene.at(hit).id);
    }
    let chain = chain(scene, hit);
    let root = scene.at(chain[0]);
    // Уровень по умолчанию: ребёнок фрейма верхнего уровня или сам объект верхнего уровня.
    let mut level = if root.props.kind == NodeKind::Frame && chain.len() > 1 {
        1
    } else {
        0
    };
    // Выделение внутри контейнера делает соседей выделенных выбираемыми на их уровне.
    let parents: HashSet<Option<Slot>> = selection
        .iter()
        .filter_map(|id| scene.slot(*id))
        .map(|s| scene.at(s).parent)
        .collect();
    for (i, &s) in chain.iter().enumerate().skip(level + 1) {
        if parents.contains(&scene.at(s).parent) {
            level = i;
        }
    }
    let target = chain[level];
    // Пустое место фрейма верхнего уровня с детьми: фрейм не выбирается.
    if level == 0 && root.props.kind == NodeKind::Frame && !root.children.is_empty() {
        return None;
    }
    Some(scene.at(target).id)
}

/// Двойной клик: ребёнок выделенного контейнера на пути к узлу под курсором.
pub fn deeper(scene: &Scene, hit: Slot, selection: &[NodeId]) -> Option<NodeId> {
    let chain = chain(scene, hit);
    let pos = chain
        .iter()
        .rposition(|s| selection.contains(&scene.at(*s).id))?;
    chain.get(pos + 1).map(|s| scene.at(*s).id)
}

/// Фрейм верхнего уровня, по названию которого щёлкнули (точка в экранных пикселях).
/// Название лежит в полосе над левым верхним углом фрейма шириной во фрейм.
pub fn frame_label_at(scene: &Scene, camera: &Camera, screen: Point) -> Option<NodeId> {
    let world = camera.screen_to_world(screen);
    let reach = LABEL_HIT_PX / camera.zoom();
    // Подпись выше фрейма: ищем фреймы, чей верх ниже точки не дальше полосы.
    let probe = Rect::new(world.x, world.y, world.x, world.y + reach);
    let mut best: Option<(Vec<u32>, NodeId)> = None;
    for id in scene.query_rect(probe) {
        let Some(s) = scene.slot(id) else { continue };
        let n = scene.at(s);
        if n.parent.is_some() || n.props.kind != NodeKind::Frame || !n.props.visible {
            continue;
        }
        if n.props.locked || scene.is_draft(id) {
            continue;
        }
        let Some(b) = world_box(scene, s) else {
            continue;
        };
        if world.x >= b.x0 && world.x <= b.x1 && world.y < b.y0 && world.y >= b.y0 - reach {
            let key = scene.order_key(s);
            if best.as_ref().is_none_or(|(k, _)| key > *k) {
                best = Some((key, id));
            }
        }
    }
    best.map(|(_, id)| id)
}

/// Фрейм верхнего уровня под точкой: в нём начинается рамка выделения. Черновики
/// жеста (их фреймы тоже временные) не выбираются.
pub fn root_frame_at(scene: &Scene, world: Point, tolerance: f64) -> Option<NodeId> {
    scene
        .hit_test_all(world, tolerance)
        .into_iter()
        .filter_map(|id| scene.slot(id))
        .map(|s| chain(scene, s)[0])
        .find(|&r| {
            scene.at(r).props.kind == NodeKind::Frame
                && !locked(scene, r)
                && !scene.is_draft(scene.at(r).id)
        })
        .map(|r| scene.at(r).id)
}

/// Самый глубокий фрейм под точкой, куда можно положить узлы: видимый, не
/// заблокированный, не черновик и не из `exclude` (вместе с их потомками).
pub fn drop_frame_at(
    scene: &Scene,
    world: Point,
    tolerance: f64,
    exclude: &HashSet<NodeId>,
) -> Option<NodeId> {
    let excluded = |s: Slot| {
        chain(scene, s)
            .iter()
            .any(|c| exclude.contains(&scene.at(*c).id))
    };
    scene
        .hit_test_all(world, tolerance)
        .into_iter()
        .filter(|id| !scene.is_draft(*id))
        .filter_map(|id| scene.slot(id))
        .filter(|s| !excluded(*s))
        .flat_map(|s| chain(scene, s).into_iter().rev())
        .find(|&s| {
            let n = scene.at(s);
            n.props.kind == NodeKind::Frame
                && !locked(scene, s)
                && !excluded(s)
                && n.geometry().contains(n.world.inverse() * world)
        })
        .map(|s| scene.at(s).id)
}

/// До скольких детей контекста рамка перебирает их напрямую, а не запросом к
/// R-tree: запрос возвращает и всех потомков (рамка через страницу в 100 000 узлов
/// — 100 000 записей, ~14 мс), а выбрать можно только детей контекста.
pub const MARQUEE_SCAN_CHILDREN: usize = 4096;

/// Узлы под рамкой выделения в контексте `context` (дети фрейма или корень
/// страницы): видимые, не заблокированные, пересекающие рамку. Фреймы верхнего
/// уровня выбираются, только если рамка охватывает их целиком (края включительно:
/// фрейм вровень с краем рамки охвачен): иначе рамка, начатая на холсте, цепляла
/// бы монтажные области, через которые прошла. Порядок — порядок отрисовки, снизу вверх.
/// Рамка нормализуется: углы можно задавать в любом порядке.
///
/// Кандидаты — дети контекста перебором, если их не больше
/// `MARQUEE_SCAN_CHILDREN`, иначе запрос к R-tree по рамке. Оба пути
/// ограничены: перебор — числом детей, запрос — числом узлов под рамкой.
pub fn marquee(scene: &Scene, rect: Rect, context: Option<NodeId>) -> Vec<NodeId> {
    let rect = rect.abs();
    let context_slot = match context {
        Some(c) => match scene.slot(c) {
            Some(s) => Some(s),
            None => return Vec::new(),
        },
        None => None,
    };
    let list = match context_slot {
        Some(s) => &scene.at(s).children[..],
        None => scene.roots(),
    };
    let candidates: Vec<Slot> = if list.len() <= MARQUEE_SCAN_CHILDREN {
        list.to_vec()
    } else {
        scene
            .query_rect(rect)
            .into_iter()
            .filter_map(|id| scene.slot(id))
            .collect()
    };
    let mut found: Vec<(Vec<u32>, NodeId)> = candidates
        .into_iter()
        .map(|s| (s, scene.at(s).id))
        .filter(|(_, id)| !scene.is_draft(*id))
        .filter(|(s, _)| scene.at(*s).parent == context_slot)
        .filter(|(s, _)| scene.is_visible(*s) && !locked(scene, *s))
        .filter(|(s, _)| {
            let Some(b) = world_box(scene, *s) else {
                return false;
            };
            let root_frame = context.is_none() && scene.at(*s).props.kind == NodeKind::Frame;
            if root_frame {
                // Края включительно: `Rect::contains` не берёт правый и нижний край,
                // а фрейм вровень с рамкой должен охватываться.
                b.x0 >= rect.x0 && b.x1 <= rect.x1 && b.y0 >= rect.y0 && b.y1 <= rect.y1
            } else {
                rect.overlaps(b)
            }
        })
        .map(|(s, id)| (scene.order_key(s), id))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.into_iter().map(|(_, id)| id).collect()
}
