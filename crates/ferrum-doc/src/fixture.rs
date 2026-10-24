//! Генератор тестовых страниц для бенчмарков. Не для продакшен-кода.

use crate::{
    Color, DocResult, Fill, NodeId, NodeKind, PageDoc, PropsPatch, Stroke, StrokeAlign, Tx,
};

/// Отдельная страница из `nodes` узлов.
pub fn page_with_nodes(nodes: usize) -> DocResult<(PageDoc, Vec<NodeId>)> {
    let page = PageDoc::new();
    let ids = fill(&page.tx_for_tests(), nodes)?;
    page.commit_for_tests();
    Ok((page, ids))
}

/// Наполняет страницу `nodes` узлами: фреймы по 100 узлов (фрейм + 99 фигур),
/// у каждой фигуры заданы координаты, имя, заливка и обводка — как в реальном макете.
/// Подходит и для вызова внутри `Document::transact`.
pub fn fill(page: &Tx, nodes: usize) -> DocResult<Vec<NodeId>> {
    let mut ids = Vec::with_capacity(nodes);
    let frames = nodes.div_ceil(100);
    for f in 0..frames {
        let frame = page.create_node(None, None, NodeKind::Frame)?;
        page.set_props(
            frame,
            &PropsPatch {
                name: Some(format!("Экран {f}")),
                x: Some((f % 50) as f64 * 1600.0),
                y: Some((f / 50) as f64 * 1200.0),
                w: Some(1440.0),
                h: Some(1024.0),
                ..Default::default()
            },
        )?;
        ids.push(frame);
        for i in 0..99 {
            if ids.len() == nodes {
                break;
            }
            let kind = if i % 3 == 0 {
                NodeKind::Ellipse
            } else {
                NodeKind::Rect
            };
            let id = page.create_node(Some(frame), None, kind)?;
            page.set_props(
                id,
                &PropsPatch {
                    name: Some(format!("Слой {i}")),
                    x: Some((i % 10) as f64 * 140.0),
                    y: Some((i / 10) as f64 * 100.0),
                    w: Some(120.0),
                    h: Some(80.0),
                    corner_radius: Some(8.0),
                    fills: Some(vec![Fill::solid(Color::rgba(i as u8, 120, 200, 255))]),
                    strokes: Some(vec![Stroke {
                        color: Color::BLACK,
                        width: 1.0,
                        align: StrokeAlign::Inside,
                    }]),
                    ..Default::default()
                },
            )?;
            ids.push(id);
        }
    }
    Ok(ids)
}
