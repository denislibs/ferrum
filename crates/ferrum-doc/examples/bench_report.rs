//! Контрольный бенчмарк модели документа:
//! `cargo run --release -p ferrum-doc --example bench_report`.
//! Сверяет замеры с целями этапа 1 и печатает таблицу PASS/FAIL (код выхода 1 при провале).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ferrum_doc::container::{decode, encode};
use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeKind, PropsPatch};

/// Считает живые байты в куче — точнее, чем RSS процесса.
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn heap_mb() -> f64 {
    LIVE.load(Ordering::Relaxed) as f64 / 1_048_576.0
}

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed())
}

fn ms(d: Duration) -> String {
    format!("{:.1} мс", d.as_secs_f64() * 1000.0)
}

struct Row {
    metric: String,
    value: String,
    limit: &'static str,
    pass: Option<bool>,
}

fn main() {
    let mut rows = Vec::new();
    for &n in &[100_000usize, 500_000] {
        // Цели этапа — для 100 000 узлов; 500 000 — справочно.
        let gate = n == 100_000;
        let mut add = |metric: &str, value: String, limit: &'static str, pass: bool| {
            rows.push(Row {
                metric: format!("{metric} ({n})"),
                value,
                limit: if gate { limit } else { "—" },
                pass: gate.then_some(pass),
            })
        };

        let (bytes, build) = time(|| {
            let mut doc = Document::new();
            let page = doc.pages()[0].0;
            doc.transact(page, vec![], |p| fill(p, n).map(|_| vec![]))
                .unwrap();
            encode(&doc).unwrap()
        });
        add("Генерация и сохранение", ms(build), "—", true);
        add(
            "Размер файла",
            format!("{:.1} МБ", bytes.len() as f64 / 1_048_576.0),
            "≤ 30 МБ",
            bytes.len() <= 30 << 20,
        );

        let before = heap_mb();
        let (opened, open) = time(|| decode(&bytes).unwrap());
        let mut doc = opened.document;
        let page = doc.pages()[0].0;
        let cached = doc.cached_scene(page).map(<[_]>::len).unwrap_or(0);
        assert_eq!(cached, n, "кэш сцены должен покрывать все узлы");
        add(
            "Открытие до первого кадра (кэш сцены)",
            ms(open),
            "≤ 300 мс",
            open <= Duration::from_millis(300),
        );

        let (_, hydrate) = time(|| doc.page(page).unwrap().walk().len());
        add(
            "Фоновая загрузка Loro",
            ms(hydrate),
            "≤ 1500 мс",
            hydrate <= Duration::from_millis(1500),
        );
        let memory = heap_mb() - before;
        add(
            "Память открытого документа",
            format!("{memory:.0} МБ"),
            "≤ 400 МБ",
            memory <= 400.0,
        );

        let ids = doc.page(page).unwrap().walk();
        let mut edit_total = Duration::ZERO;
        for i in 0..200 {
            let id = ids[(i * 97) % n];
            let (_, d) = time(|| {
                doc.transact(page, vec![id], |p| {
                    p.set_props(
                        id,
                        &PropsPatch {
                            x: Some(i as f64),
                            ..Default::default()
                        },
                    )?;
                    Ok(vec![id])
                })
                .unwrap()
            });
            edit_total += d;
        }
        let edit = edit_total / 200;
        add(
            "Правка свойства (транзакция)",
            format!("{:.3} мс", edit.as_secs_f64() * 1000.0),
            "≤ 1 мс",
            edit <= Duration::from_millis(1),
        );

        let (_, undo) = time(|| doc.undo().unwrap());
        add(
            "Undo одной правки",
            ms(undo),
            "≤ 50 мс",
            undo <= Duration::from_millis(50),
        );

        let (_, batch) = time(|| {
            doc.transact(page, vec![], |p| {
                for (i, id) in ids.iter().take(1000).enumerate() {
                    p.set_props(
                        *id,
                        &PropsPatch {
                            x: Some(i as f64),
                            y: Some(1.0),
                            ..Default::default()
                        },
                    )?;
                }
                Ok(vec![])
            })
            .unwrap()
        });
        add(
            "Сдвиг 1000 узлов одной транзакцией",
            ms(batch),
            "≤ 50 мс",
            batch <= Duration::from_millis(50),
        );

        let (_, undo_batch) = time(|| doc.undo().unwrap());
        add(
            "Undo сдвига 1000 узлов",
            ms(undo_batch),
            "≤ 50 мс",
            undo_batch <= Duration::from_millis(50),
        );

        let (_, save) = time(|| encode(&doc).unwrap());
        add(
            "Полное сохранение (периодически и при закрытии)",
            ms(save),
            "≤ 1500 мс",
            save <= Duration::from_millis(1500),
        );

        // Плоская страница: удаление 1000 узлов не должно зависеть от числа соседей.
        {
            let mut flat = Document::new();
            let flat_page = flat.pages()[0].0;
            let mut flat_ids = Vec::with_capacity(n);
            flat.transact(flat_page, vec![], |p| {
                for _ in 0..n {
                    flat_ids.push(p.create_node(None, None, NodeKind::Rect)?);
                }
                Ok(vec![])
            })
            .unwrap();
            let to_delete: Vec<_> = flat_ids.iter().step_by(n / 1000).copied().collect();
            assert_eq!(to_delete.len(), 1000);
            let (_, delete) = time(|| {
                flat.transact(flat_page, vec![], |p| {
                    p.delete_nodes(&to_delete)?;
                    Ok(vec![])
                })
                .unwrap()
            });
            add(
                "Удаление 1000 узлов с плоской страницы",
                ms(delete),
                "≤ 50 мс",
                delete <= Duration::from_millis(50),
            );

            // Отмена удаления восстанавливает узлы по одному: каждая вставка не должна
            // перебирать соседей.
            let (_, undo_delete) = time(|| flat.undo().unwrap());
            add(
                "Undo удаления 1000 узлов",
                ms(undo_delete),
                "≤ 50 мс",
                undo_delete <= Duration::from_millis(50),
            );

            // Группировка (Ctrl+G) разбросанных по плоской странице узлов в новый фрейм.
            let (_, group) = time(|| {
                flat.transact(flat_page, vec![], |p| {
                    let frame = p.create_node(None, None, NodeKind::Frame)?;
                    p.move_nodes(&to_delete, Some(frame), 0)?;
                    Ok(vec![frame])
                })
                .unwrap()
            });
            add(
                "Перемещение 1000 узлов во фрейм",
                ms(group),
                "≤ 50 мс",
                group <= Duration::from_millis(50),
            );

            let (_, undo_group) = time(|| flat.undo().unwrap());
            add(
                "Undo перемещения 1000 узлов",
                ms(undo_group),
                "≤ 50 мс",
                undo_group <= Duration::from_millis(50),
            );
        }
    }

    println!("| Метрика | Значение | Цель | Итог |");
    println!("|---|---|---|---|");
    let mut failed = false;
    for r in &rows {
        let verdict = match r.pass {
            Some(true) => "PASS",
            Some(false) => {
                failed = true;
                "FAIL"
            }
            None => "—",
        };
        println!("| {} | {} | {} | {} |", r.metric, r.value, r.limit, verdict);
    }
    println!();
    println!(
        "Замер нативный, release. WASM обычно в 1.5–2 раза медленнее — цели заданы с этим запасом."
    );
    if failed {
        std::process::exit(1);
    }
}
