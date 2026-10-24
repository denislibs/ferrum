//! Позиция за концом списка в `move_nodes` — «в конец», в том числе `usize::MAX`.

use ferrum_doc::*;

#[test]
fn index_past_the_end_moves_block_to_the_end() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |p| {
            let mut ids = Vec::new();
            for _ in 0..4 {
                ids.push(p.create_node(None, None, NodeKind::Rect)?);
            }
            Ok(ids)
        })
        .unwrap();
    // Перемещаемые узлы уже среди детей цели: раньше `usize::MAX` переполнялся
    // при пересчёте позиции.
    for index in [usize::MAX, usize::MAX - 1, 4, 100] {
        doc.transact(page, vec![], |p| {
            p.move_nodes(&[ids[0], ids[1]], None, index)?;
            Ok(vec![])
        })
        .unwrap();
        let kids = doc.page(page).unwrap().children(None);
        assert_eq!(&kids[2..], &[ids[0], ids[1]], "index {index}");
        doc.undo().unwrap();
        assert_eq!(doc.page(page).unwrap().children(None), ids);
    }
}
