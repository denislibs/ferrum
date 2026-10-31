//! Булевы операции над выделением (спека подсистемы 3, раздел 10): Alt+Shift+U,
//! S, I, E (Option+Shift на macOS, как в Figma) и выпадающий список тулбара. Выделение оборачивается булевой группой
//! на месте верхнего слоя одним шагом отмены; у выделенных булевых групп та же
//! команда меняет операцию.
//!
//! Как в Figma: группа берёт заливки и обводки верхнего слоя (у «Вычитания» —
//! нижнего, из которого вычитают), называется по операции, фреймы в булеву
//! группу не входят, нужно хотя бы два слоя.

use crate::editor::{Editor, EditorEvent};
use ferrum_doc::{BooleanOp, NodeId, NodeKind, Props, PropsPatch};

/// Слой, чей стиль берёт новая булева группа: у обычной группы — её верхний
/// (у «Вычитания» — нижний) лист, есть у него заливки или нет; вложенная
/// булева группа и фрейм берутся как есть.
fn style_source(editor: &Editor, id: NodeId, top: bool) -> Option<Props> {
    let n = editor.scene.node(id)?;
    if n.props.kind != NodeKind::Group {
        return Some(n.props.clone());
    }
    let mut kids: Vec<NodeId> = n.children.iter().map(|&c| editor.scene.at(c).id).collect();
    if top {
        kids.reverse();
    }
    kids.into_iter().find_map(|c| style_source(editor, c, top))
}

impl Editor {
    /// Булева операция `op` над выделением (одна команда — один шаг).
    pub(crate) fn boolean(&mut self, op: BooleanOp) {
        self.finish_vector_edit();
        let ids = self.ordered_selection();
        if ids.is_empty() {
            return;
        }
        let layers: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| !self.is_frame(*id))
            .collect();
        let frames = layers.len() < ids.len();
        // Булевы группы (фреймы не в счёт) — у них меняется операция.
        if !layers.is_empty() && layers.iter().all(|id| self.is_kind(*id, NodeKind::Boolean)) {
            self.set_boolean_op(&layers, op);
            return;
        }
        if layers.len() < 2 {
            let msg = if frames {
                "Булевы операции не применяются к фреймам"
            } else {
                "Для булевой операции выделите хотя бы два слоя"
            };
            self.out.events.push(EditorEvent::Error(msg.into()));
            return;
        }
        let top = *layers.last().expect("слоёв не меньше двух");
        let source = if op == BooleanOp::Subtract {
            style_source(self, layers[0], false)
        } else {
            style_source(self, top, true)
        };
        let mut patch = PropsPatch {
            name: Some(op.name().into()),
            boolean_op: Some(op),
            ..Default::default()
        };
        if let Some(s) = source {
            patch.fills = Some(s.fills);
            patch.strokes = Some(s.strokes);
            patch.stroke_cap_start = Some(s.stroke_style.cap_start);
            patch.stroke_cap_end = Some(s.stroke_style.cap_end);
            patch.stroke_join = Some(s.stroke_style.join);
            patch.stroke_miter_limit = Some(s.stroke_style.miter_limit);
        }
        if self.wrap_with(layers, NodeKind::Boolean, patch) && frames {
            self.out.events.push(EditorEvent::Error(
                "Фреймы в булеву группу не входят".into(),
            ));
        }
    }

    fn is_kind(&self, id: NodeId, kind: NodeKind) -> bool {
        self.scene.node(id).is_some_and(|n| n.props.kind == kind)
    }

    fn is_frame(&self, id: NodeId) -> bool {
        self.is_kind(id, NodeKind::Frame)
    }

    /// Меняет операцию булевых групп `ids`; имя по умолчанию («Объединение» и
    /// т. п.) меняется вместе с ней, своё — остаётся.
    fn set_boolean_op(&mut self, ids: &[NodeId], op: BooleanOp) {
        let patches: Vec<(NodeId, PropsPatch)> = ids
            .iter()
            .filter_map(|id| {
                let p = &self.scene.node(*id)?.props;
                if p.boolean_op == Some(op) {
                    return None;
                }
                let default_name = BooleanOp::ALL.iter().any(|o| o.name() == p.name);
                Some((
                    *id,
                    PropsPatch {
                        boolean_op: Some(op),
                        name: default_name.then(|| op.name().to_string()),
                        ..Default::default()
                    },
                ))
            })
            .collect();
        if patches.is_empty() {
            return;
        }
        let selection = ids.to_vec();
        self.ctx().commit(|tx, _| {
            for (id, patch) in &patches {
                tx.set_props(*id, patch)?;
            }
            Ok(selection)
        });
    }
}
