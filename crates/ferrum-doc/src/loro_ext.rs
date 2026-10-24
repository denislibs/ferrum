//! Единственное место, где модель обращается к `loro-internal` в обход публичного
//! API `loro`.
//!
//! Публичный `LoroTree` не отдаёт позицию узла среди соседей и ребёнка по позиции,
//! а список соседей на каждую операцию стоил бы O(соседей). Обработчик дерева из
//! `loro-internal` делает и то и другое за O(log n). Это внутренний API: при
//! обновлении Loro первым делом проверяется этот модуль и его тест.

use loro::{ContainerTrait, LoroTree, TreeID, TreeParentId};

/// Позиция узла среди детей его родителя в Loro (с учётом узлов, скрытых от модели).
pub(crate) fn index_of(tree: &LoroTree, tid: TreeID) -> Option<usize> {
    tree.to_handler().get_index_by_tree_id(&tid)
}

/// Ребёнок родителя на позиции `index` в Loro; `None` — позиция за концом списка.
pub(crate) fn child_at(tree: &LoroTree, parent: TreeParentId, index: usize) -> Option<TreeID> {
    tree.to_handler().get_child_at(&parent, index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use loro::LoroDoc;

    /// Контрольная точка обновления Loro: поведение внутреннего API, на которое
    /// опирается страница.
    #[test]
    fn index_and_child_lookup_match_children_list() {
        let doc = LoroDoc::new();
        let tree = doc.get_tree("t");
        tree.enable_fractional_index(0);
        let a = tree.create(TreeParentId::Root).unwrap();
        let b = tree.create(TreeParentId::Root).unwrap();
        let c = tree.create_at(TreeParentId::Root, 1).unwrap();
        let kid = tree.create(TreeParentId::Node(b)).unwrap();
        let order = tree.children(TreeParentId::Root).unwrap();
        assert_eq!(order, vec![a, c, b]);
        for (i, t) in order.iter().enumerate() {
            assert_eq!(index_of(&tree, *t), Some(i));
            assert_eq!(child_at(&tree, TreeParentId::Root, i), Some(*t));
        }
        assert_eq!(index_of(&tree, kid), Some(0));
        assert_eq!(child_at(&tree, TreeParentId::Node(b), 0), Some(kid));
        assert_eq!(child_at(&tree, TreeParentId::Root, 3), None);
    }
}
