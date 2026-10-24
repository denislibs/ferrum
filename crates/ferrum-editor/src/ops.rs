//! Правки документа, общие для инструментов и команд: копии поддеревьев, перенос к
//! другому родителю с сохранением положения на экране.

use ferrum_doc::{DocResult, NodeId, Props, PropsPatch, Tx};
use ferrum_scene::Scene;
use kurbo::{Affine, Vec2};

use crate::clipboard::full_patch;
use crate::geometry::{decompose_props, parent_world};

/// Сдвиг в мире, переведённый в координаты родителя узла (только линейная часть).
pub fn local_delta(parent_world: Affine, d: Vec2) -> Vec2 {
    let c = parent_world.inverse().as_coeffs();
    Vec2::new(c[0] * d.x + c[2] * d.y, c[1] * d.x + c[3] * d.y)
}

/// Положение, поворот и отражение узла со свойствами `props` и мировой матрицей
/// `world` у родителя с мировой матрицей `parent_world` — чтобы после переноса узел
/// остался на месте. Отражение родителя переходит в отражение узла: узел, вынесенный
/// из отражённого фрейма, выглядит так же. Размер нужен, потому что отражение
/// идёт относительно центра рамки узла.
pub fn reposition(parent_world: Affine, world: Affine, props: &Props) -> PropsPatch {
    let pose = decompose_props(parent_world.inverse() * world, props);
    PropsPatch {
        x: Some(pose.x),
        y: Some(pose.y),
        rotation: Some(pose.rotation),
        flip_x: (pose.flip_x != props.flip_x).then_some(pose.flip_x),
        flip_y: (pose.flip_y != props.flip_y).then_some(pose.flip_y),
        ..Default::default()
    }
}

/// Копирует поддеревья `ids` (узлы верхнего уровня, без их потомков) прямо над
/// оригиналами; корни копий сдвигаются на `offset` в мире. Читает документ, а не
/// сцену: копия повторяет записанные свойства. Возвращает корни копий в порядке
/// отрисовки, снизу вверх.
pub fn duplicate(tx: &Tx, scene: &Scene, ids: &[NodeId], offset: Vec2) -> DocResult<Vec<NodeId>> {
    let mut order: Vec<(Vec<u32>, NodeId)> = ids
        .iter()
        .filter_map(|id| scene.slot(*id).map(|s| (scene.order_key(s), *id)))
        .collect();
    // Сверху вниз: вставка над верхним оригиналом не сдвигает позиции нижних.
    order.sort_by(|a, b| b.0.cmp(&a.0));
    let mut copies = Vec::with_capacity(order.len());
    for (_, id) in order {
        let slot = scene.slot(id).expect("отобрано выше");
        let parent = scene.at(slot).parent.map(|p| scene.at(p).id);
        let mut props = tx.props(id)?;
        let d = local_delta(parent_world(scene, slot), offset);
        props.x += d.x;
        props.y += d.y;
        let index = scene.index(slot) as usize + 1;
        let root = tx.create_node(parent, Some(index), props.kind)?;
        tx.set_props(root, &full_patch(&props))?;
        // Поддерево — обходом без рекурсии: глубина не ограничена.
        let mut stack = vec![(id, root)];
        while let Some((src, dst)) = stack.pop() {
            for child in tx.children(Some(src)) {
                let p = tx.props(child)?;
                let copy = tx.create_node(Some(dst), None, p.kind)?;
                tx.set_props(copy, &full_patch(&p))?;
                stack.push((child, copy));
            }
        }
        copies.push(root);
    }
    copies.reverse();
    Ok(copies)
}
