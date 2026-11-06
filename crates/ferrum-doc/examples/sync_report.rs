//! Контрольный замер чужих правок (спека подсистемы 4, раздел 13.1):
//! `cargo run --release -p ferrum-doc --example sync_report`. Страница из 100 000 узлов: пачка из одной
//! чужой правки вместе с обновлением зеркала и `take_changes`. Код выхода 1 при провале цели.

use std::time::{Duration, Instant};

use ferrum_doc::container::{decode, encode_full};
use ferrum_doc::fixture::fill;
use ferrum_doc::*;

const NODES: usize = 100_000;
const TARGET: Duration = Duration::from_millis(2);

fn percentile(v: &mut [Duration], p: usize) -> Duration {
    v.sort();
    v[(v.len() * p).div_ceil(100).max(1) - 1]
}

fn open(bytes: &[u8], page: PageId) -> Document {
    let mut d = decode(bytes).unwrap().document;
    d.sync_start();
    d.page(page).unwrap();
    for (k, v) in d.take_sync_subscribes() {
        d.sync_subscribed(k, v);
    }
    d
}

struct Scene {
    a: Document,
    b: Document,
    page: PageId,
    ids: Vec<NodeId>,
}

fn scene() -> Scene {
    let mut seed = Document::new();
    let page = seed.pages()[0].0;
    let ids = seed.transact(page, vec![], |tx| fill(tx, NODES)).unwrap();
    let bytes = encode_full(&seed).unwrap();
    println!("файл: {:.1} МБ", bytes.len() as f64 / 1e6);
    let a = open(&bytes, page);
    let b = open(&bytes, page);
    Scene { a, b, page, ids }
}

fn one(
    s: &mut Scene,
    name: &str,
    target: Option<Duration>,
    runs: usize,
    edit: impl Fn(&mut Document, PageId, &[NodeId], usize),
) -> bool {
    let mut times = Vec::with_capacity(runs);
    let mut first = Duration::ZERO;
    for i in 0..runs {
        edit(&mut s.a, s.page, &s.ids, i);
        let mut applied = Duration::ZERO;
        for batch in s.a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
            let t = Instant::now();
            s.b.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
            s.b.take_changes(s.page);
            applied += t.elapsed();
            s.a.sync_acked(batch.doc, &batch.end);
        }
        if i == 0 {
            first = applied;
        }
        times.push(applied);
    }
    let (p50, p95, max) = (
        percentile(&mut times, 50),
        percentile(&mut times, 95),
        percentile(&mut times, 100),
    );
    let ok = target.is_none_or(|t| p95 <= t);
    println!(
        "{} {name}: первая {first:.2?}, p50 {p50:.2?}, p95 {p95:.2?}, наибольшая {max:.2?}{}",
        if ok { "PASS" } else { "FAIL" },
        target
            .map(|t| format!(" (цель ≤ {t:?})"))
            .unwrap_or_default(),
    );
    ok
}

fn set_x(d: &mut Document, page: PageId, id: NodeId, x: f64) {
    d.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                x: Some(x),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap();
}

fn main() {
    let mut s = scene();
    let mut ok = true;
    ok &= one(
        &mut s,
        "свойство чужого узла",
        Some(TARGET),
        500,
        |d, p, ids, i| {
            set_x(d, p, ids[(i * 37) % ids.len()], i as f64 + 0.5);
        },
    );
    ok &= one(
        &mut s,
        "новый узел",
        Some(TARGET),
        200,
        |d, p, ids, _| {
            let frame = ids[50_000];
            d.transact(p, vec![], |tx| {
                Ok(vec![tx.create_node(Some(frame), None, NodeKind::Rect)?])
            })
            .unwrap();
        },
    );
    ok &= one(
        &mut s,
        "перенос к другому родителю",
        Some(TARGET),
        200,
        |d, p, ids, i| {
            let node = ids[1 + (i % 50)];
            let frame = ids[(i % 2 + 1) * 100];
            d.transact(p, vec![], |tx| {
                tx.move_node(node, Some(frame), 0)?;
                Ok(vec![node])
            })
            .unwrap();
        },
    );
    ok &= one(
        &mut s,
        "удаление узла",
        Some(TARGET),
        200,
        |d, p, ids, i| {
            let node = ids[70_001 + i];
            d.transact(p, vec![], |tx| {
                tx.delete_nodes(&[node])?;
                Ok(vec![])
            })
            .unwrap();
        },
    );
    // Группировка: новый фрейм и перенос в него двух известных узлов одной пачкой.
    ok &= one(
        &mut s,
        "группировка: новый фрейм и два переноса",
        Some(TARGET),
        200,
        |d, p, ids, i| {
            let (x, y) = (ids[80_001 + i * 2], ids[80_002 + i * 2]);
            d.transact(p, vec![], |tx| {
                let f = tx.create_node(None, None, NodeKind::Frame)?;
                tx.move_node(x, Some(f), 0)?;
                tx.move_node(y, Some(f), 1)?;
                Ok(vec![f])
            })
            .unwrap();
        },
    );
    // Своя правка свойства рядом: Loro сливает без отката.
    {
        let mut times = Vec::new();
        let (page, ids) = (s.page, s.ids.clone());
        for i in 0..200 {
            set_x(&mut s.b, page, ids[(i * 13) % ids.len()], i as f64 + 0.25);
            set_x(
                &mut s.a,
                page,
                ids[(i * 17 + 3) % ids.len()],
                i as f64 + 0.75,
            );
            for batch in s.a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                let t = Instant::now();
                s.b.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
                s.b.take_changes(page);
                times.push(t.elapsed());
                s.a.sync_acked(batch.doc, &batch.end);
            }
            for batch in s.b.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                s.a.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
                s.a.take_changes(page);
                s.b.sync_acked(batch.doc, &batch.end);
            }
        }
        let (p50, p95) = (percentile(&mut times, 50), percentile(&mut times, 95));
        let pass = p95 <= TARGET;
        ok &= pass;
        println!("{} свойство параллельно своей правке свойства: p50 {p50:.2?}, p95 {p95:.2?} (цель ≤ {TARGET:?})", if pass { "PASS" } else { "FAIL" });
    }
    // Правки дерева с обеих сторон: Loro откатывает всё дерево. Цель недостижима, цифра — для описания PR.
    {
        let mut times = Vec::new();
        let (page, ids) = (s.page, s.ids.clone());
        for i in 0..20 {
            let frame = ids[20_000];
            s.b.transact(page, vec![], |tx| {
                Ok(vec![tx.create_node(Some(frame), None, NodeKind::Rect)?])
            })
            .unwrap();
            s.a.transact(page, vec![], |tx| {
                Ok(vec![tx.create_node(
                    Some(frame),
                    Some(i),
                    NodeKind::Rect,
                )?])
            })
            .unwrap();
            for batch in s.a.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                let t = Instant::now();
                s.b.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
                s.b.take_changes(page);
                times.push(t.elapsed());
                s.a.sync_acked(batch.doc, &batch.end);
            }
            for batch in s.b.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
                s.a.apply_remote(batch.doc, &[&batch.bytes]).unwrap();
                s.a.take_changes(page);
                s.b.sync_acked(batch.doc, &batch.end);
            }
        }
        let first = times[0];
        let (p50, max) = (percentile(&mut times, 50), percentile(&mut times, 100));
        println!("ВНЕ ЦЕЛИ правка дерева параллельно своей правке дерева: первая {first:.2?}, p50 {p50:.2?}, наибольшая {max:.2?}");
    }
    // Цена подписки на своей правке: страница без подписки против страницы с ней.
    {
        let mut plain = Document::new();
        let p = plain.pages()[0].0;
        let t = Instant::now();
        plain.transact(p, vec![], |tx| fill(tx, 10_000)).unwrap();
        let without = t.elapsed();
        let mut heard = Document::new();
        heard.sync_start();
        let p = heard.pages()[0].0;
        let t = Instant::now();
        heard.transact(p, vec![], |tx| fill(tx, 10_000)).unwrap();
        println!(
            "своя большая правка, 10 000 узлов: без подписки {without:.1?}, с подпиской {:.1?}",
            t.elapsed()
        );
    }
    if !ok {
        std::process::exit(1);
    }
}
