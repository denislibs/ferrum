//! Регрессионные бенчмарки модели документа: `cargo bench -p ferrum-doc`.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use ferrum_doc::fixture::page_with_nodes;
use ferrum_doc::{PageDoc, PropsPatch};

const N: usize = 10_000;

fn bench(c: &mut Criterion) {
    let (page, ids) = page_with_nodes(N).unwrap();
    let snapshot = page.export_snapshot().unwrap();

    c.bench_function("export_snapshot_10k", |b| {
        b.iter(|| page.export_snapshot().unwrap())
    });

    c.bench_function("open_and_read_all_10k", |b| {
        b.iter(|| {
            let p = PageDoc::from_snapshot(&snapshot).unwrap();
            p.walk()
                .into_iter()
                .map(|id| p.props(id).unwrap())
                .collect::<Vec<_>>()
        })
    });

    c.bench_function("set_props_single", |b| {
        let mut x = 0.0;
        b.iter(|| {
            x += 1.0;
            page.set_props(
                ids[N / 2],
                &PropsPatch {
                    x: Some(x),
                    ..Default::default()
                },
            )
            .unwrap();
            page.commit_for_tests();
        })
    });

    c.bench_function("move_1000_in_one_commit", |b| {
        b.iter_batched(
            || PageDoc::from_snapshot(&snapshot).unwrap(),
            |p| {
                for (i, id) in ids.iter().take(1000).enumerate() {
                    p.set_props(
                        *id,
                        &PropsPatch {
                            x: Some(i as f64),
                            y: Some(5.0),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                }
                p.commit_for_tests();
            },
            BatchSize::LargeInput,
        )
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
