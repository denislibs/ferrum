//! Какие узлы изменила правка, отмена или повтор — чтобы сцена пересчитала только
//! их, а не всю страницу.

use crate::page::Inverse;
use crate::types::NodeId;

/// Изменённые узлы одной страницы. Списки берутся из журнала обратных операций,
/// поэтому могут содержать повторы и пересекаться: узел, созданный и удалённый в
/// одной транзакции, окажется и в `created`, и в `deleted`. Потребитель сверяет
/// каждый узел с текущим состоянием страницы, а не с порядком операций.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Changes {
    /// Созданные узлы, в том числе восстановленные отменой удаления.
    pub created: Vec<NodeId>,
    /// Удалённые узлы вместе со всеми удалёнными потомками.
    pub deleted: Vec<NodeId>,
    /// Узлы, сменившие родителя или позицию среди соседей.
    pub moved: Vec<NodeId>,
    /// Узлы с изменёнными свойствами.
    pub props: Vec<NodeId>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Число записей во всех списках, с повторами.
    pub fn len(&self) -> usize {
        self.created.len() + self.deleted.len() + self.moved.len() + self.props.len()
    }

    pub fn extend(&mut self, other: Changes) {
        self.created.extend(other.created);
        self.deleted.extend(other.deleted);
        self.moved.extend(other.moved);
        self.props.extend(other.props);
    }

    /// Добавляет узлы, затронутые операциями. Журнал хранит обратные операции,
    /// поэтому обратная к созданию (`Delete`) означает созданный узел, а обратная к
    /// удалению (`Restore`) — удалённое поддерево.
    pub(crate) fn record(&mut self, ops: &[Inverse]) {
        for op in ops {
            match op {
                Inverse::Delete(id) => self.created.push(*id),
                Inverse::Restore(nodes) => self.deleted.extend(nodes.iter().map(|n| n.id)),
                Inverse::Props(id, _) => self.props.push(*id),
                Inverse::Move { id, .. } => self.moved.push(*id),
                Inverse::Text(id, _) => self.props.push(*id),
            }
        }
    }
}
