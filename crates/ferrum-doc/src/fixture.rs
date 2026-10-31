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

/// Наполняет страницу `texts` текстовыми узлами: фреймы 1440 × 1024 по 100 текстов
/// фиксированного размера 120 × 40 (Inter 14, две строки) — размеры записаны, и
/// открытие страницы раскладки не требует (спека подсистемы 3, раздел 5).
pub fn fill_texts(page: &Tx, texts: usize) -> DocResult<Vec<NodeId>> {
    use std::sync::Arc;

    use crate::{Props, TextContent, TextData, TextProps, TextSizing};

    let mut ids = Vec::with_capacity(texts);
    let frames = texts.div_ceil(100);
    let mut props = TextProps {
        sizing: TextSizing::Fixed,
        ..TextProps::default()
    };
    props.style.size = 14.0;
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
        for i in 0..100 {
            if ids.len() == texts {
                break;
            }
            let mut p = Props::defaults(NodeKind::Text);
            p.name = format!("Текст {i}");
            (p.x, p.y, p.w, p.h) = (
                (i % 10) as f64 * 140.0,
                (i / 10) as f64 * 100.0,
                120.0,
                40.0,
            );
            p.text = Some(Arc::new(TextData {
                props: props.clone(),
                content: TextContent::plain(&format!("Заголовок {i}\nЭкран {f}, подпись")),
            }));
            ids.push(page.create_with_props(Some(frame), None, &p)?);
        }
    }
    Ok(ids)
}

/// Сеть для замеров (подсистема 3, этап 3): замкнутый волнистый контур из `n`
/// точек по эллипсу 2000 × 1000 (как обведённая карта или скан), каждый
/// четвёртый сегмент — кривая, одна область по всей петле.
pub fn wavy_network(n: usize) -> crate::VectorNetwork {
    use crate::{FillRule, VectorNetwork, VectorPoint, VectorRegion, VectorSegment};
    let points = (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            let r = 1.0 + 0.01 * (i as f64 * 0.005).sin();
            VectorPoint::new(1000.0 + 1000.0 * r * a.cos(), 500.0 + 500.0 * r * a.sin())
        })
        .collect();
    let segments = (0..n as u32)
        .map(|i| {
            let mut s = VectorSegment::line(i, (i + 1) % n as u32);
            if i % 4 == 0 {
                s.tangent_start = [0.01, -0.01];
                s.tangent_end = [-0.01, 0.01];
            }
            s
        })
        .collect();
    VectorNetwork {
        points,
        segments,
        regions: vec![VectorRegion {
            rule: FillRule::NonZero,
            loops: vec![(0..n as u32).collect()],
            fills: None,
        }],
    }
}
