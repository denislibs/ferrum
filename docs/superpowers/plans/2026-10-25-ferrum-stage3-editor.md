# Ferrum, подсистема 1, этап 3: редактор без браузера — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Крейт `ferrum-editor` — редактор без браузера: события ввода простыми структурами, инструменты (перемещение, фрейм, прямоугольник, эллипс, линия, рука, изображение) как конечные автоматы за трейтом `Tool`, правила выделения Figma, перемещение, копия с Alt, перенос во фрейм, ресайз (в том числе групп), поворот, привязки к соседям и пикселям, единая таблица команд с раскладкой клавиш (отличия, конфликты, сброс, импорт и экспорт JSON), свой формат буфера обмена, пресеты фреймов, сценарные и property-тесты и контрольный бенчмарк на 100 000 узлов.

**Architecture:** `Editor` владеет документом (`ferrum-doc`), сценой (`ferrum-scene`), камерой (`ferrum-render::Camera`), выделением, инструментом и раскладкой клавиш. Ввод приходит методом `Editor::handle(Input)`; инструмент получает контекст `Ctx` и, пока кнопка нажата, меняет только превью сцены (`Scene::set_preview`) и черновики — временные узлы, которых нет в документе (`Scene::set_drafts`, новая возможность сцены). При отпускании жест записывается одной транзакцией — одним шагом отмены; Esc убирает превью без следа в истории. После каждой транзакции, отмены и повтора редактор забирает изменения всех страниц и применяет изменения текущей к сцене. Наружу редактор отдаёт `scene()`, `camera()` и `overlay()` для рендера, курсор и события (выделение, инструмент, курсор, «нужен кадр», байты буфера обмена, запросы к интерфейсу). Ничего не зависит от браузера, ОС и часов: время — из событий, поэтому одна последовательность событий всегда даёт один документ.

**Tech Stack:** Rust 1.96, kurbo 0.13.1, serde 1.0.229 и serde_json 1.0.152 (те же версии уже в `Cargo.lock`: их тянут criterion и loro), thiserror 2, proptest 1.12. Новых внешних пакетов нет.

**Спека:** `docs/superpowers/specs/2026-10-23-ferrum-core-design.md`, разделы 3, 4.2, 5.5, 6, 7, 8.3, 12.

## Global Constraints

- Rust toolchain `1.96.0`, edition 2021. Зависимости — точными версиями (`=x.y.z`); `serde` и `serde_json` берутся в тех версиях, что уже есть в `Cargo.lock` (опубликованы не позже 2026-10-10). Этап не добавляет в `Cargo.lock` ни одного внешнего пакета — только запись `ferrum-editor`.
- `ferrum-editor` не зависит от браузера и ОС: ни `web-sys`, ни системного времени, ни случайных чисел. Время двойного клика берётся из событий. Порядок обхода `HashMap` не влияет на результат: всё, что попадает в документ и выделение, упорядочено по порядку отрисовки или по входу.
- Один жест — один шаг отмены. Пока кнопка нажата, изменения живут только в превью и черновиках сцены; при отпускании пишется одна транзакция; Esc, потеря указателя и потеря фокуса окна отменяют жест без следа в истории. После каждой транзакции, отмены и повтора редактор забирает `take_changes` всех страниц и применяет изменения текущей к сцене.
- Никакой работы O(n) или O(соседей) на событие в горячих путях: кандидаты привязок и названий фреймов — запросами к R-tree сцены, рамка выделения — перебором детей контекста (если их не больше 4096) или запросом к R-tree. Каждая операция, которую видит пользователь, включая отмену пачки, есть в бенчмарке.
- Данные извне не доверенные: буфер обмена (размеры, количества, ссылки на родителя только назад, числа приводятся к пределам сцены, хэш изображения проверяется), JSON раскладки и пресетов. Ошибка разбора не меняет ни документ, ни историю, ни раскладку.
- Поля `Camera` закрыты — только сеттеры и геттеры; оверлей строит рендер с `effective_dpr`; результаты `query_rect` фильтруются по `is_visible`; номера версий сцены уникальны, а не подряд.
- Поворот — в градусах против часовой стрелки на экране, вокруг левого верхнего угла узла (как в модели). Координаты событий — CSS-пиксели вьюпорта.
- Тексты ошибок и названия команд — на русском, для пользователя. Комментарии в коде — на русском, объясняют «почему».
- Цели бенчмарка (100 000 узлов, release, нативно): движение указателя при перетаскивании 1000 выделенных с превью и привязками ≤ 4 мс; движение одного узла с привязками к 100 соседям ≤ 1 мс; наведение (hit-test и выбор объекта) ≤ 0.2 мс; обновление рамки выделения ≤ 4 мс (и в типичном отдалении, и через всю страницу); запись перетаскивания 1000 узлов ≤ 20 мс; отмена его ≤ 20 мс.

---

## Этапы подсистемы 1

Подсистема 1 делится на шесть этапов (таблица — в плане этапа 1, `docs/superpowers/plans/2026-10-23-ferrum-stage1-document-model.md`). Этот документ — план этапа 3:

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Модель документа (готов) | `ferrum-doc` | 3, 5 |
| 2. Сцена и рендер (готов) | `ferrum-scene`, `ferrum-render` | 6, 8 |
| **3. Редактор без браузера** (этот план) | `ferrum-editor`, черновики сцены, бенчмарк `editor_report` | 7 |
| 4. Worker и обмен данными | `ferrum-wasm`, OffscreenCanvas, `vello_gpu` на WebGPU/WebGL2 | 4.2–4.3, 8.1–8.2 |
| 5. Интерфейс | `apps/web` | 4.4, 9 |
| 6. Файлы и десктоп | `FileStore`, автосохранение, Tauri, E2E | 10, 11, 12 |

---

## Структура файлов этапа 3

| Файл | Ответственность |
|---|---|
| `Cargo.toml` | крейт `ferrum-editor` в workspace, `serde` и `serde_json` |
| `crates/ferrum-doc/src/page.rs` | `move_nodes` с позицией `usize::MAX` — «в конец», без переполнения |
| `crates/ferrum-scene/src/{scene,lib}.rs` | черновики жеста: `Draft`, `set_drafts`, `is_draft`; `clear_preview` убирает и их |
| `crates/ferrum-editor/src/input.rs` | события ввода, модификаторы, платформа |
| `crates/ferrum-editor/src/commands.rs` | таблица команд: ключи, русские названия, сочетания Figma |
| `crates/ferrum-editor/src/keymap.rs` | раскладка: умолчания + отличия, конфликты, сброс, JSON, сочетания браузера |
| `crates/ferrum-editor/src/geometry.rs` | рамка выделения, ручки, ресайз, поворот, разложение матриц в свойства |
| `crates/ferrum-editor/src/snap.rs` | привязки к соседям и родительскому фрейму через R-tree |
| `crates/ferrum-editor/src/selection.rs` | правила выделения, рамка выделения, фрейм для переноса, названия фреймов |
| `crates/ferrum-editor/src/presets.rs`, `assets/frame_presets.json` | пресеты фреймов |
| `crates/ferrum-editor/src/clipboard.rs` | свой формат буфера обмена и его проверка |
| `crates/ferrum-editor/src/tools/` | трейт `Tool`, контекст, «Перемещение», рисование, рука |
| `crates/ferrum-editor/src/ops.rs` | копии поддеревьев, перенос с сохранением положения |
| `crates/ferrum-editor/src/editor.rs` | `Editor`: ввод, инструменты, события, отмена, страницы |
| `crates/ferrum-editor/src/actions.rs` | команды: правки выделения, порядок, вид, буфер обмена, пресеты |
| `crates/ferrum-editor/tests/` | сценарии, раскладка, формат буфера, property-тест |
| `crates/ferrum-editor/examples/editor_report.rs` | контрольный бенчмарк этапа |

Все команды запускаются из корня репозитория.

---

### Task 1: Черновики жеста в сцене и перенос «в конец» без переполнения

Сцене нужны временные узлы. Пока фигуру рисуют, её нет в документе, а превью умеет только править свойства существующих узлов; копии при перетаскивании с Alt тоже не должны попадать в документ до отпускания. Черновик — узел сцены с зарезервированным идентификатором: Loro не выдаёт узлам пир `u64::MAX` (`LoroDoc::set_peer_id(u64::MAX)` возвращает `InvalidPeerID`), поэтому `NodeId` вида `n@18446744073709551615` не совпадёт ни с одним узлом документа. Черновики стоят верхними детьми своих родителей, двигаются обычным `set_preview` и уходят вместе с превью. `Scene::apply` на время обновления убирает их и ставит обратно: позиции в `Changes` посчитаны по документу, где черновиков нет.

Вторая правка — ошибка этапа 1, найденная прототипом: `move_nodes(ids, parent, usize::MAX)` по контракту «за концом — в конец», но если перемещаемые узлы уже дети цели, пересчёт позиции делал `usize::MAX + 1` — паника в debug, а в release блок вставал в начало списка.

**Files:**
- Modify: `crates/ferrum-doc/src/page.rs`
- Modify: `crates/ferrum-scene/src/scene.rs`
- Modify: `crates/ferrum-scene/src/lib.rs`
- Test: `crates/ferrum-doc/tests/move_to_end.rs`
- Test: `crates/ferrum-scene/tests/drafts.rs`

**Interfaces:**
- Consumes: API этапов 1–2.
- Produces: `ferrum_scene::Draft { id, parent, props }`; `Scene::set_drafts(&[Draft])` (заменяет черновики целиком; неизвестный родитель или занятый `id` — черновик пропускается с поддеревом), `Scene::is_draft(id)`, `has_preview()` учитывает черновики, `clear_preview()` убирает и их; `Tx::move_nodes` принимает любой индекс за концом, включая `usize::MAX`.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-doc/tests/move_to_end.rs`:

````rust
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
````

`crates/ferrum-scene/tests/drafts.rs`:

````rust
//! Черновики жеста: временные узлы, которых нет в документе.

use ferrum_doc::{Document, NodeId, NodeKind, PageId, Props, PropsPatch};
use ferrum_scene::{testing::snapshot, Draft, Scene};
use kurbo::{Point, Rect};

/// Идентификатор черновика: Loro не выдаёт узлам пир `u64::MAX`, поэтому такой
/// `NodeId` не совпадёт ни с одним узлом документа.
fn draft_id(n: u32) -> NodeId {
    format!("{n}@{}", u64::MAX).parse().unwrap()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Props {
    Props {
        x,
        y,
        w,
        h,
        ..Props::defaults(NodeKind::Rect)
    }
}

/// Фрейм (0, 0, 400×300) с прямоугольником внутри и прямоугольник в корне.
fn sample() -> (Document, PageId, [NodeId; 3]) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |p| {
            let f = p.create_node(None, None, NodeKind::Frame)?;
            p.set_props(
                f,
                &PropsPatch {
                    w: Some(400.0),
                    h: Some(300.0),
                    ..Default::default()
                },
            )?;
            let a = p.create_node(Some(f), None, NodeKind::Rect)?;
            let b = p.create_node(None, None, NodeKind::Rect)?;
            p.set_props(
                b,
                &PropsPatch {
                    x: Some(600.0),
                    ..Default::default()
                },
            )?;
            Ok(vec![f, a, b])
        })
        .unwrap();
    doc.take_changes(page);
    (doc, page, [ids[0], ids[1], ids[2]])
}

fn fresh(doc: &mut Document, page: PageId) -> Scene {
    Scene::from_page(doc.page(page).unwrap())
}

#[test]
fn drafts_are_top_children_and_go_away_with_preview() {
    let (mut doc, page, [f, a, _]) = sample();
    let mut scene = fresh(&mut doc, page);
    let before = scene.change_counter();
    let d = draft_id(0);
    scene.set_drafts(&[Draft {
        id: d,
        parent: Some(f),
        props: rect(200.0, 100.0, 50.0, 50.0),
    }]);
    assert!(scene.change_counter() > before);
    assert!(scene.has_preview());
    assert!(scene.is_draft(d));
    assert!(!scene.is_draft(a));
    let fs = scene.slot(f).unwrap();
    let kids = &scene.at(fs).children;
    assert_eq!(kids.len(), 2);
    assert_eq!(
        scene.at(*kids.last().unwrap()).id,
        d,
        "черновик — верхний ребёнок"
    );
    assert_eq!(
        scene.node(d).unwrap().world_bounds,
        Some(Rect::new(200.0, 100.0, 250.0, 150.0))
    );
    assert_eq!(scene.hit_test(Point::new(220.0, 120.0), 0.0), Some(d));

    scene.clear_preview();
    assert!(!scene.has_preview());
    assert!(scene.node(d).is_none());
    assert!(!scene.is_draft(d));
    assert_eq!(snapshot(&scene), snapshot(&fresh(&mut doc, page)));
}

#[test]
fn draft_subtrees_move_by_preview_and_bad_drafts_are_skipped() {
    let (mut doc, page, [f, a, b]) = sample();
    let mut scene = fresh(&mut doc, page);
    let (g, c, orphan, dup) = (draft_id(0), draft_id(1), draft_id(2), draft_id(3));
    scene.set_drafts(&[
        Draft {
            id: g,
            parent: None,
            props: Props::defaults(NodeKind::Group),
        },
        Draft {
            id: c,
            parent: Some(g),
            props: rect(10.0, 10.0, 20.0, 20.0),
        },
        // Родителя нет ни в документе, ни среди черновиков.
        Draft {
            id: orphan,
            parent: Some(draft_id(99)),
            props: rect(0.0, 0.0, 5.0, 5.0),
        },
        // Идентификатор уже занят узлом документа.
        Draft {
            id: a,
            parent: None,
            props: rect(0.0, 0.0, 5.0, 5.0),
        },
        Draft {
            id: dup,
            parent: Some(c),
            props: rect(1.0, 1.0, 2.0, 2.0),
        },
    ]);
    assert!(scene.node(orphan).is_none());
    assert_eq!(
        scene.node(a).unwrap().parent,
        scene.slot(f),
        "узел документа не тронут"
    );
    assert_eq!(scene.node(dup).unwrap().parent, scene.slot(c));
    assert_eq!(
        scene.node(g).unwrap().subtree_bounds,
        Some(Rect::new(10.0, 10.0, 30.0, 30.0))
    );
    scene.set_preview(&[(
        g,
        PropsPatch {
            x: Some(100.0),
            ..Default::default()
        },
    )]);
    assert_eq!(
        scene.node(c).unwrap().world_bounds,
        Some(Rect::new(110.0, 10.0, 130.0, 30.0))
    );
    // Новый список заменяет старый целиком.
    scene.set_drafts(&[Draft {
        id: orphan,
        parent: Some(b),
        props: rect(0.0, 0.0, 5.0, 5.0),
    }]);
    assert!(scene.node(g).is_none() && scene.node(c).is_none());
    assert!(scene.is_draft(orphan));
    scene.clear_preview();
    assert_eq!(snapshot(&scene), snapshot(&fresh(&mut doc, page)));
}

#[test]
fn apply_keeps_drafts_on_top_with_their_preview() {
    let (mut doc, page, [f, a, _]) = sample();
    let mut scene = fresh(&mut doc, page);
    let d = draft_id(0);
    scene.set_drafts(&[Draft {
        id: d,
        parent: Some(f),
        props: rect(0.0, 0.0, 10.0, 10.0),
    }]);
    scene.set_preview(&[(
        d,
        PropsPatch {
            x: Some(50.0),
            ..Default::default()
        },
    )]);
    // Документ получает нового ребёнка фрейма поверх всех — черновик остаётся выше.
    let n = doc
        .transact(page, vec![], |p| {
            Ok(vec![p.create_node(Some(f), None, NodeKind::Ellipse)?])
        })
        .unwrap()[0];
    let changes = doc.take_changes(page);
    scene.apply(&changes, doc.page(page).unwrap());
    let kids: Vec<NodeId> = scene
        .at(scene.slot(f).unwrap())
        .children
        .iter()
        .map(|s| scene.at(*s).id)
        .collect();
    assert_eq!(kids, vec![a, n, d]);
    assert_eq!(
        scene.node(d).unwrap().props.x,
        50.0,
        "превью черновика сохранилось"
    );
    scene.clear_preview();
    assert_eq!(snapshot(&scene), snapshot(&fresh(&mut doc, page)));
}
````

- [ ] **Step 2: Убедиться, что тесты падают**

Run: `cargo test -p ferrum-doc --test move_to_end`
Expected: FAIL — паника `attempt to add with overflow` в `page.rs`.

Run: `cargo test -p ferrum-scene --test drafts`
Expected: ошибка компиляции — нет `Draft` и `set_drafts`.

- [ ] **Step 3: Насыщающее сложение в `anchor`**

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        let mut at = index;
        for q in inside {
            if q <= at {
                at += 1;
            } else {
                break;
            }
````

на:

````rust
        let mut at = index;
        for q in inside {
            if q <= at {
                // `index` за концом (вплоть до `usize::MAX`) значит «в конец»: без
                // насыщения сложение переполнилось бы и блок встал бы в начало.
                at = at.saturating_add(1);
            } else {
                break;
            }
````

- [ ] **Step 4: Черновики в сцене**

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
    }
}

/// Узлы хранятся кусками постоянной ёмкости, а не одним `Vec`: перераспределение
/// массива на 100 000 узлов (~35 МБ) копирует его страницами с копированием при
/// записи, и первая запись в каждую страницу после этого стоит микросекунды —
````

на:

````rust
    }
}

/// Временный узел жеста, которого нет в документе: фигура, которую рисуют, или копия
/// при перетаскивании с Alt. Живёт только в сцене до конца жеста.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// Идентификатор, которого нет в документе (редактор берёт зарезервированный).
    pub id: NodeId,
    /// Родитель: узел документа или черновик раньше в том же списке; `None` — корень.
    pub parent: Option<NodeId>,
    pub props: Props,
}

/// Узлы хранятся кусками постоянной ёмкости, а не одним `Vec`: перераспределение
/// массива на 100 000 узлов (~35 МБ) копирует его страницами с копированием при
/// записи, и первая запись в каждую страницу после этого стоит микросекунды —
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
    /// Растёт при каждом изменении сцены (`apply`, превью): холст по нему понимает,
    /// что нужен новый кадр, не сравнивая сцены.
    changes: u64,
}

impl Scene {
````

на:

````rust
    /// Растёт при каждом изменении сцены (`apply`, превью): холст по нему понимает,
    /// что нужен новый кадр, не сравнивая сцены.
    changes: u64,
    /// Черновики жеста в порядке `set_drafts` и их слоты (в том же порядке).
    drafts: Vec<Draft>,
    draft_slots: Vec<Slot>,
}

impl Scene {
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
            return;
        }
        let counter = self.changes + 1;
        if self.try_apply(changes, page).is_none() {
            // Сцена разошлась с документом (например, построена по кэшу, который
            // документ отверг) — перестраиваем целиком, превью сохраняется.
````

на:

````rust
            return;
        }
        let counter = self.changes + 1;
        // Черновиков нет в документе, а позиции в `changes` посчитаны без них:
        // на время обновления они убираются и потом ставятся обратно поверх.
        let drafts = std::mem::take(&mut self.drafts);
        let draft_patches: Vec<(NodeId, PropsPatch)> = self
            .draft_slots
            .iter()
            .filter_map(|s| {
                let (_, patch) = self.preview.get(s)?;
                Some((self.at(*s).id, patch.clone()))
            })
            .collect();
        self.remove_drafts();
        if self.try_apply(changes, page).is_none() {
            // Сцена разошлась с документом (например, построена по кэшу, который
            // документ отверг) — перестраиваем целиком, превью сохраняется.
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
                .collect();
            *self = Scene::from_page(page);
            self.set_preview(&preview);
        }
        // Пересборка целиком создаёт новую сцену: счётчик не должен откатиться.
        self.changes = counter;
````

на:

````rust
                .collect();
            *self = Scene::from_page(page);
            self.set_preview(&preview);
        }
        if !drafts.is_empty() {
            self.set_drafts(&drafts);
            let mut patches: Vec<(NodeId, PropsPatch)> = self
                .preview
                .iter()
                .map(|(&s, (_, patch))| (self.at(s).id, patch.clone()))
                .collect();
            patches.extend(draft_patches);
            self.set_preview(&patches);
        }
        // Пересборка целиком создаёт новую сцену: счётчик не должен откатиться.
        self.changes = counter;
````

В `crates/ferrum-scene/src/scene.rs` заменить:

````rust
        self.flush_tree();
    }

    /// Убирает превью: все узлы возвращаются к свойствам документа.
    pub fn clear_preview(&mut self) {
        self.set_preview(&[]);
    }

    pub fn has_preview(&self) -> bool {
        !self.preview.is_empty()
    }

    /// Узлы, чьи собственные границы пересекают прямоугольник (в мире), — запрос по
````

на:

````rust
        self.flush_tree();
    }

    /// Убирает превью и черновики: все узлы возвращаются к свойствам документа.
    pub fn clear_preview(&mut self) {
        self.set_preview(&[]);
        self.set_drafts(&[]);
    }

    /// Есть ли временные правки или черновики жеста.
    pub fn has_preview(&self) -> bool {
        !self.preview.is_empty() || !self.drafts.is_empty()
    }

    /// Заменяет черновики жеста целиком. Каждый встаёт верхним ребёнком своего
    /// родителя в порядке списка. Черновик с неизвестным родителем или с
    /// идентификатором, который уже есть в сцене, пропускается вместе с поддеревом.
    /// Двигать черновики можно обычным `set_preview`.
    pub fn set_drafts(&mut self, drafts: &[Draft]) {
        let had = !self.draft_slots.is_empty();
        let regrouped = self.remove_drafts();
        let mut dirty = Vec::new();
        for d in drafts {
            if self.by_id.contains_key(&d.id) {
                continue;
            }
            let parent = match d.parent {
                Some(p) => match self.slot(p) {
                    Some(s) => Some(s),
                    None => continue,
                },
                None => None,
            };
            let slot = self.alloc(d.id, d.props.clone());
            self.link(slot, parent);
            self.drafts.push(d.clone());
            self.draft_slots.push(slot);
            dirty.push(slot);
        }
        if had || !dirty.is_empty() {
            self.changes += 1;
        }
        let regrouped = regrouped.into_iter().filter(|p| self.alive(*p)).collect();
        self.recompute(dirty, regrouped);
        self.flush_tree();
    }

    /// Узел — черновик жеста, а не узел документа.
    pub fn is_draft(&self, id: NodeId) -> bool {
        self.slot(id).is_some_and(|s| self.draft_slots.contains(&s))
    }

    /// Убирает черновики из списков детей и из таблицы. Черновики всегда верхние
    /// дети своих родителей, поэтому позиции остальных детей не меняются.
    /// Возвращает родителей-узлы документа: их границы нужно пересчитать.
    fn remove_drafts(&mut self) -> Vec<Slot> {
        self.drafts.clear();
        let slots = std::mem::take(&mut self.draft_slots);
        let mut parents = Vec::new();
        // С конца: дети черновиков уходят раньше родителей-черновиков.
        for &s in slots.iter().rev() {
            if !self.alive(s) {
                continue;
            }
            let parent = self.at(s).parent;
            let i = self.index[s as usize] as usize;
            let list = self.list_mut(parent);
            debug_assert_eq!(list.get(i), Some(&s), "черновик расходится со списком");
            list.remove(i);
            self.remove_subtree(s);
            if let Some(p) = parent {
                parents.push(p);
            }
        }
        parents.retain(|p| self.alive(*p) && !slots.contains(p));
        parents
    }

    /// Узлы, чьи собственные границы пересекают прямоугольник (в мире), — запрос по
````

В `crates/ferrum-scene/src/lib.rs` заменить:

````rust
mod scene;
pub mod shape;

pub use scene::{testing, Scene, SceneNode, Slot};
````

на:

````rust
mod scene;
pub mod shape;

pub use scene::{testing, Draft, Scene, SceneNode, Slot};
````

- [ ] **Step 5: Запустить тесты**

Run: `cargo test -p ferrum-doc --test move_to_end`
Expected: PASS, `1 passed`.

Run: `cargo test -p ferrum-scene --test drafts`
Expected: PASS, `3 passed`.

Run: `cargo test -p ferrum-doc -p ferrum-scene`
Expected: PASS — остальные наборы этапов 1–2 без изменений.

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-doc crates/ferrum-scene
git commit -m "feat(scene): черновики жеста и перенос в конец списка без переполнения"
```

---

### Task 2: Крейт `ferrum-editor`: события ввода, таблица команд, раскладка клавиш

Ввод описан простыми структурами — такими, какие основной поток (этап 5) положит в кольцевой буфер: точка в CSS-пикселях, кнопка, модификаторы, время события; колесо, щипок; клавиша с `code` (физическая, как `KeyboardEvent.code`) и `key` (символ на раскладке, только для подписей). Текстовый ввод — в подсистеме 3.

Таблица команд одна на всё: меню, палитра (подсистема 2) и горячие клавиши вызывают команды по `CommandId`. Сочетания по умолчанию повторяют Figma; `Mod` — Cmd на macOS и Ctrl на Windows и Linux; на Windows и Linux у повтора есть ещё Ctrl+Y. Сопоставление — строго по `code` и точному набору модификаторов, поэтому V и М на русской раскладке — одна клавиша `KeyV`. Знаки (`[`, `]`, `\`) тоже сопоставляются по физической клавише US-раскладки, как в Figma.

Раскладка хранит только отличия от умолчаний: список сочетаний команды, который отличается от умолчания; совпавший с умолчанием перестаёт быть отличием. `bind` с конфликтом без замены ничего не меняет и возвращает конфликт — интерфейс предлагает заменить; с заменой снимает сочетание с других команд. Импорт JSON не доверенный: ошибка формата не меняет раскладку, неизвестные команды и непонятные сочетания пропускаются с предупреждениями. `browser_reserved` отдаёт сочетания, которые браузер перехватывает до страницы (интерфейс помечает их в веб-версии).

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/ferrum-editor/Cargo.toml`
- Create: `crates/ferrum-editor/src/lib.rs`
- Create: `crates/ferrum-editor/src/input.rs`
- Create: `crates/ferrum-editor/src/commands.rs`
- Create: `crates/ferrum-editor/src/keymap.rs`
- Test: `crates/ferrum-editor/tests/keymap.rs`

**Interfaces:**
- Consumes: ничего из других крейтов, кроме `kurbo`, `serde_json`, `thiserror`.
- Produces: `Input { PointerDown, PointerMove, PointerUp, PointerCancel, Wheel, Pinch, KeyDown, KeyUp, Blur }`, `PointerEvent { point, button, modifiers, time_ms }`, `WheelEvent`, `KeyEvent { code, key, modifiers, repeat }`, `Modifiers { shift, alt, ctrl, meta }` с `primary(platform)`, `Platform { Mac, Windows, Linux }`; `CommandId` (41 команда), `COMMANDS`, `CommandId::{key, name, from_key, default_shortcuts}`; `Shortcut { code, mods }` с `parse`, `Display` (канонически Ctrl+Alt+Shift+Meta+код) и `label(platform)`; `Keymap::{new, bindings, default_bindings, is_overridden, lookup, commands_for, conflicts, browser_conflicts, set_bindings, bind, unbind, reset, reset_all, export_json, import_json}`; `browser_reserved(platform)`; `Conflict`, `KeymapError`.

- [ ] **Step 1: Добавить крейт в workspace**

В `Cargo.toml` заменить:

````toml
[workspace]
resolver = "2"
members = ["crates/ferrum-doc", "crates/ferrum-scene", "crates/ferrum-render"]

[workspace.package]
edition = "2021"
````

на:

````toml
[workspace]
resolver = "2"
members = [
    "crates/ferrum-doc",
    "crates/ferrum-scene",
    "crates/ferrum-render",
    "crates/ferrum-editor",
]

[workspace.package]
edition = "2021"
````

В `Cargo.toml` заменить:

````toml
parley = { version = "=0.12.0", default-features = false, features = ["std"] }
# Та же версия, что тянет parley 0.12: контуры глифов для подписей.
skrifa = "=0.44.0"
````

на:

````toml
parley = { version = "=0.12.0", default-features = false, features = ["std"] }
# Та же версия, что тянет parley 0.12: контуры глифов для подписей.
skrifa = "=0.44.0"
# Редактор: таблица команд, раскладка клавиш и пресеты фреймов в JSON. Те же версии,
# что уже есть в Cargo.lock (их тянут criterion и loro).
serde = { version = "=1.0.229", features = ["derive"] }
serde_json = "=1.0.152"
````

`crates/ferrum-editor/Cargo.toml`:

````toml
[package]
name = "ferrum-editor"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
ferrum-doc = { path = "../ferrum-doc" }
ferrum-scene = { path = "../ferrum-scene" }
ferrum-render = { path = "../ferrum-render" }
kurbo.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true

[dev-dependencies]
proptest.workspace = true
````

- [ ] **Step 2: Написать падающие тесты раскладки**

`crates/ferrum-editor/tests/keymap.rs`:

````rust
//! Таблица команд и раскладка клавиш (раздел 7.6 спеки).

use ferrum_editor::*;

fn sc(s: &str) -> Shortcut {
    Shortcut::parse(s).unwrap()
}

fn key(code: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> KeyEvent {
    KeyEvent::new(
        code,
        Modifiers {
            shift,
            alt,
            ctrl,
            meta,
        },
    )
}

/// Все сочетания из разделов 7.1 и 7.6 спеки — как в Figma.
const EXPECTED: &[(CommandId, &[&str])] = &[
    (CommandId::ToolMove, &["KeyV"]),
    (CommandId::ToolFrame, &["KeyF", "KeyA"]),
    (CommandId::ToolRect, &["KeyR"]),
    (CommandId::ToolEllipse, &["KeyO"]),
    (CommandId::ToolLine, &["KeyL"]),
    (CommandId::ToolHand, &["KeyH"]),
    (CommandId::HandHold, &["Space"]),
    (CommandId::ToolImage, &["Mod+Shift+KeyK"]),
    (CommandId::Delete, &["Delete", "Backspace"]),
    (CommandId::Duplicate, &["Mod+KeyD"]),
    (CommandId::Group, &["Mod+KeyG"]),
    (CommandId::Ungroup, &["Mod+Shift+KeyG"]),
    (CommandId::FrameSelection, &["Mod+Alt+KeyG"]),
    (CommandId::BringForward, &["Mod+BracketRight"]),
    (CommandId::SendBackward, &["Mod+BracketLeft"]),
    (CommandId::BringToFront, &["Mod+Alt+BracketRight"]),
    (CommandId::SendToBack, &["Mod+Alt+BracketLeft"]),
    (CommandId::NudgeLeft, &["ArrowLeft"]),
    (CommandId::NudgeRight, &["ArrowRight"]),
    (CommandId::NudgeUp, &["ArrowUp"]),
    (CommandId::NudgeDown, &["ArrowDown"]),
    (CommandId::NudgeLeftBig, &["Shift+ArrowLeft"]),
    (CommandId::NudgeRightBig, &["Shift+ArrowRight"]),
    (CommandId::NudgeUpBig, &["Shift+ArrowUp"]),
    (CommandId::NudgeDownBig, &["Shift+ArrowDown"]),
    (CommandId::ZoomToFit, &["Shift+Digit1"]),
    (CommandId::ZoomToSelection, &["Shift+Digit2"]),
    (CommandId::Zoom100, &["Shift+Digit0"]),
    (CommandId::ToggleVisible, &["Mod+Shift+KeyH"]),
    (CommandId::ToggleLock, &["Mod+Shift+KeyL"]),
    (CommandId::ToggleUi, &["Mod+Backslash"]),
    (CommandId::Copy, &["Mod+KeyC"]),
    (CommandId::Paste, &["Mod+KeyV"]),
    (CommandId::Cut, &["Mod+KeyX"]),
    (CommandId::CopyAsPng, &["Mod+Shift+KeyC"]),
    (CommandId::Undo, &["Mod+KeyZ"]),
    (CommandId::Redo, &["Mod+Shift+KeyZ"]),
    (CommandId::SelectAll, &["Mod+KeyA"]),
    (CommandId::Escape, &["Escape"]),
];

#[test]
fn defaults_match_figma_on_both_platforms() {
    for (platform, primary) in [(Platform::Mac, "Meta"), (Platform::Windows, "Ctrl")] {
        let map = Keymap::new(platform);
        for (cmd, list) in EXPECTED {
            let mut want: Vec<Shortcut> = list
                .iter()
                .map(|s| sc(&s.replace("Mod", primary)))
                .collect();
            if *cmd == CommandId::Redo && platform != Platform::Mac {
                want.push(sc("Ctrl+KeyY"));
            }
            assert_eq!(
                map.bindings(*cmd),
                want.as_slice(),
                "{cmd:?} на {platform:?}"
            );
        }
        assert!(map.conflicts().is_empty(), "{:?}", map.conflicts());
        assert!(
            map.browser_conflicts().is_empty(),
            "умолчания не задевают сочетаний браузера"
        );
    }
    // Каждая команда таблицы — с названием, ключом и сочетанием.
    let map = Keymap::new(Platform::Linux);
    for c in COMMANDS {
        assert!(!c.name.is_empty() && CommandId::from_key(c.key) == Some(c.id));
        assert!(!map.bindings(c.id).is_empty(), "{:?}", c.id);
    }
    assert_eq!(map.bindings(CommandId::Redo).len(), 2);
    assert_eq!(COMMANDS.len(), 41);
}

#[test]
fn lookup_by_physical_code_with_exact_modifiers() {
    let mac = Keymap::new(Platform::Mac);
    assert_eq!(
        mac.lookup(&key("KeyD", false, false, false, true)),
        Some(CommandId::Duplicate)
    );
    assert_eq!(
        mac.lookup(&key("KeyD", true, false, false, false)),
        None,
        "Ctrl на маке — не Cmd"
    );
    let win = Keymap::new(Platform::Windows);
    // Русская раскладка: «в» на клавише KeyD.
    let mut e = key("KeyD", true, false, false, false);
    e.key = "в".into();
    assert_eq!(win.lookup(&e), Some(CommandId::Duplicate));
    assert_eq!(
        win.lookup(&key("Digit1", false, false, true, false)),
        Some(CommandId::ZoomToFit)
    );
    assert_eq!(win.lookup(&key("KeyV", false, false, true, false)), None);
}

#[test]
fn bind_conflicts_replace_unbind_and_reset() {
    let mut map = Keymap::new(Platform::Windows);
    // R занята «Прямоугольником».
    let err = map
        .bind(CommandId::ToolEllipse, sc("KeyR"), false)
        .unwrap_err();
    assert_eq!(err.commands, vec![CommandId::ToolRect]);
    assert_eq!(map.bindings(CommandId::ToolEllipse), &[sc("KeyO")]);
    // С заменой сочетание переходит к эллипсу.
    map.bind(CommandId::ToolEllipse, sc("KeyR"), true).unwrap();
    assert_eq!(
        map.bindings(CommandId::ToolEllipse),
        &[sc("KeyO"), sc("KeyR")]
    );
    assert!(map.bindings(CommandId::ToolRect).is_empty());
    assert!(map.is_overridden(CommandId::ToolRect));
    assert_eq!(
        map.lookup(&key("KeyR", false, false, false, false)),
        Some(CommandId::ToolEllipse)
    );
    // Сброс одной команды возвращает умолчание; конфликт теперь виден.
    map.reset(CommandId::ToolRect);
    assert_eq!(map.bindings(CommandId::ToolRect), &[sc("KeyR")]);
    assert_eq!(map.conflicts().len(), 1);
    assert_eq!(
        map.conflicts()[0].commands,
        vec![CommandId::ToolRect, CommandId::ToolEllipse]
    );
    // Убрать лишнее — список снова равен умолчанию и перестаёт быть отличием.
    map.unbind(CommandId::ToolEllipse, &sc("KeyR"));
    assert!(!map.is_overridden(CommandId::ToolEllipse));
    map.set_bindings(CommandId::Duplicate, vec![]);
    assert_eq!(map.lookup(&key("KeyD", true, false, false, false)), None);
    map.reset_all();
    assert_eq!(map.bindings(CommandId::Duplicate), &[sc("Ctrl+KeyD")]);
    assert!(!map.is_overridden(CommandId::Duplicate));
}

#[test]
fn export_import_round_trip_stores_only_differences() {
    let mut map = Keymap::new(Platform::Mac);
    assert!(map.export_json().contains("\"overrides\": {}"));
    map.set_bindings(CommandId::ToolRect, vec![sc("KeyQ"), sc("Shift+KeyR")]);
    map.set_bindings(CommandId::Duplicate, vec![]);
    let json = map.export_json();
    let mut other = Keymap::new(Platform::Mac);
    assert!(other.import_json(&json).unwrap().is_empty());
    for c in COMMANDS {
        assert_eq!(other.bindings(c.id), map.bindings(c.id), "{:?}", c.id);
    }
    assert_eq!(other.export_json(), json);
    // Неизвестные команды и непонятные сочетания — предупреждения, остальное принято.
    let warnings = other
        .import_json(
            r#"{"format":"ferrum-keymap","version":1,"overrides":{
                "tool.rect":["KeyW","Hyper+KeyQ",5],"no.such":["KeyZ"],"tool.line":"KeyL"}}"#,
        )
        .unwrap();
    assert_eq!(warnings.len(), 4, "{warnings:?}");
    assert_eq!(other.bindings(CommandId::ToolRect), &[sc("KeyW")]);
    assert!(
        !other.is_overridden(CommandId::Duplicate),
        "импорт заменяет отличия целиком"
    );
    // Ошибки формата не меняют раскладку.
    let snapshot = other.export_json();
    for bad in [
        "not json",
        "[]",
        r#"{"format":"other","version":1,"overrides":{}}"#,
        r#"{"format":"ferrum-keymap","overrides":{}}"#,
        r#"{"format":"ferrum-keymap","version":1}"#,
    ] {
        assert!(
            matches!(other.import_json(bad), Err(KeymapError::BadFormat(_))),
            "{bad}"
        );
        assert_eq!(other.export_json(), snapshot);
    }
    assert!(matches!(
        other.import_json(r#"{"format":"ferrum-keymap","version":2,"overrides":{}}"#),
        Err(KeymapError::NewerVersion(2))
    ));
    let huge = format!(
        r#"{{"format":"ferrum-keymap","version":1,"overrides":{{"tool.rect":["{}"]}}}}"#,
        "A".repeat(70_000)
    );
    assert_eq!(other.import_json(&huge), Err(KeymapError::TooLarge));
    assert_eq!(other.export_json(), snapshot);
}

#[test]
fn browser_reserved_combos_are_reported() {
    let reserved = browser_reserved(Platform::Windows);
    assert!(reserved.contains(&sc("Ctrl+KeyW")));
    assert!(reserved.contains(&sc("Ctrl+KeyT")));
    assert!(reserved.contains(&sc("Ctrl+KeyN")));
    assert!(browser_reserved(Platform::Mac).contains(&sc("Meta+KeyW")));
    let mut map = Keymap::new(Platform::Windows);
    map.bind(CommandId::Duplicate, sc("Ctrl+KeyW"), false)
        .unwrap();
    assert_eq!(
        map.browser_conflicts(),
        vec![(CommandId::Duplicate, sc("Ctrl+KeyW"))]
    );
}

#[test]
fn shortcut_text_forms() {
    let s = sc("Shift+Ctrl+KeyZ");
    assert_eq!(s.to_string(), "Ctrl+Shift+KeyZ", "каноническая запись");
    assert_eq!(sc("Cmd+KeyK"), sc("Meta+KeyK"));
    assert_eq!(sc("Meta+Shift+KeyK").label(Platform::Mac), "⇧⌘K");
    assert_eq!(
        sc("Ctrl+Alt+BracketRight").label(Platform::Windows),
        "Ctrl+Alt+]"
    );
    for bad in [
        "",
        "Ctrl+",
        "Ctrl+Ctrl+KeyA",
        "KeyAA",
        "Shift+ShiftLeft",
        "Super+KeyA",
        "F25",
    ] {
        assert!(Shortcut::parse(bad).is_err(), "{bad}");
    }
    for good in ["F12", "Digit0", "Numpad5", "Space", "Alt+Backquote"] {
        assert!(Shortcut::parse(good).is_ok(), "{good}");
    }
}
````

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-editor --test keymap`
Expected: ошибка — у крейта ещё нет `src/lib.rs`.

- [ ] **Step 4: События ввода, таблица команд, раскладка**

`crates/ferrum-editor/src/input.rs`:

````rust
//! События ввода — простые структуры без браузера. Основной поток (этап 5) кладёт
//! их в кольцевой буфер как есть, worker отдаёт редактору по порядку. Координаты —
//! CSS-пиксели вьюпорта холста, время — миллисекунды (`event.timeStamp`).

use kurbo::{Point, Vec2};
use serde::{Deserialize, Serialize};

/// Платформа определяет, какая клавиша главная в сочетаниях: Cmd на macOS, Ctrl на
/// Windows и Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    };
    pub const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    pub const ALT: Modifiers = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    pub const CTRL: Modifiers = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    pub const META: Modifiers = Modifiers {
        meta: true,
        ..Modifiers::NONE
    };

    /// Главная клавиша сочетаний на платформе: Cmd на macOS, Ctrl на остальных.
    pub fn primary(self, platform: Platform) -> bool {
        match platform {
            Platform::Mac => self.meta,
            _ => self.ctrl,
        }
    }

    pub fn with_shift(self) -> Modifiers {
        Modifiers {
            shift: true,
            ..self
        }
    }

    pub fn with_alt(self) -> Modifiers {
        Modifiers { alt: true, ..self }
    }
}

/// Кнопка мыши, нажатая или отпущенная в событии.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Middle,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    /// Точка в CSS-пикселях относительно левого верхнего угла холста.
    pub point: Point,
    pub button: PointerButton,
    pub modifiers: Modifiers,
    /// Время события в миллисекундах; по нему распознаётся двойной клик.
    pub time_ms: f64,
}

impl PointerEvent {
    pub fn new(point: impl Into<Point>, modifiers: Modifiers, time_ms: f64) -> PointerEvent {
        PointerEvent {
            point: point.into(),
            button: PointerButton::Primary,
            modifiers,
            time_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelEvent {
    pub point: Point,
    /// Прокрутка в CSS-пикселях (`deltaMode` уже приведён основным потоком).
    pub delta: Vec2,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// Физическая клавиша, как `KeyboardEvent.code` («KeyV», «Digit1», «BracketLeft»):
    /// по ней сопоставляются сочетания, поэтому они работают на любой раскладке.
    pub code: String,
    /// Символ на текущей раскладке, как `KeyboardEvent.key`. Только для подписей.
    pub key: String,
    pub modifiers: Modifiers,
    /// Автоповтор удерживаемой клавиши.
    pub repeat: bool,
}

impl KeyEvent {
    pub fn new(code: &str, modifiers: Modifiers) -> KeyEvent {
        KeyEvent {
            code: code.to_string(),
            key: String::new(),
            modifiers,
            repeat: false,
        }
    }
}

/// Одно событие ввода. Текстовый ввод — в подсистеме 3.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    PointerDown(PointerEvent),
    PointerMove(PointerEvent),
    PointerUp(PointerEvent),
    /// Указатель потерян (`pointercancel`): жест отменяется.
    PointerCancel,
    Wheel(WheelEvent),
    /// Щипок тачпада в Safari (`gesturechange`): множитель зума к точке.
    Pinch {
        point: Point,
        scale: f64,
    },
    KeyDown(KeyEvent),
    KeyUp(KeyEvent),
    /// Окно потеряло фокус: отпущенных клавиш и кнопок мы уже не увидим.
    Blur,
}
````

`crates/ferrum-editor/src/commands.rs`:

````rust
//! Единая таблица команд: меню интерфейса, палитра команд и горячие клавиши вызывают
//! одно и то же по идентификатору. Сочетания по умолчанию повторяют Figma: на macOS
//! главная клавиша — Cmd, на Windows и Linux — Ctrl (в таблице это `Mod`).

use crate::input::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CommandId {
    ToolMove,
    ToolFrame,
    ToolRect,
    ToolEllipse,
    ToolLine,
    ToolHand,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
    ToolImage,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    CopyAsPng,
    Duplicate,
    Delete,
    SelectAll,
    /// Отменить жест, вернуться к «Перемещению», снять выделение — по очереди.
    Escape,
    Group,
    Ungroup,
    FrameSelection,
    BringForward,
    SendBackward,
    BringToFront,
    SendToBack,
    NudgeLeft,
    NudgeRight,
    NudgeUp,
    NudgeDown,
    NudgeLeftBig,
    NudgeRightBig,
    NudgeUpBig,
    NudgeDownBig,
    ZoomToFit,
    ZoomToSelection,
    Zoom100,
    ZoomIn,
    ZoomOut,
    ToggleVisible,
    ToggleLock,
    ToggleUi,
}

/// Строка таблицы команд.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub id: CommandId,
    /// Стабильный ключ для настроек, меню и JSON раскладки.
    pub key: &'static str,
    /// Название в интерфейсе.
    pub name: &'static str,
    /// Сочетания по умолчанию: `Mod` — Cmd на macOS, Ctrl на остальных.
    pub defaults: &'static [&'static str],
    /// Дополнительные сочетания только для Windows и Linux.
    pub defaults_pc: &'static [&'static str],
}

const fn cmd(
    id: CommandId,
    key: &'static str,
    name: &'static str,
    defaults: &'static [&'static str],
) -> Command {
    Command {
        id,
        key,
        name,
        defaults,
        defaults_pc: &[],
    }
}

use CommandId as C;

/// Все команды. Сочетания сверены с официальным списком горячих клавиш Figma.
pub const COMMANDS: &[Command] = &[
    cmd(C::ToolMove, "tool.move", "Перемещение", &["KeyV"]),
    cmd(C::ToolFrame, "tool.frame", "Фрейм", &["KeyF", "KeyA"]),
    cmd(C::ToolRect, "tool.rect", "Прямоугольник", &["KeyR"]),
    cmd(C::ToolEllipse, "tool.ellipse", "Эллипс", &["KeyO"]),
    cmd(C::ToolLine, "tool.line", "Линия", &["KeyL"]),
    cmd(C::ToolHand, "tool.hand", "Рука", &["KeyH"]),
    cmd(
        C::HandHold,
        "tool.hand_hold",
        "Рука, пока клавиша нажата",
        &["Space"],
    ),
    cmd(
        C::ToolImage,
        "tool.image",
        "Поместить изображение",
        &["Mod+Shift+KeyK"],
    ),
    cmd(C::Undo, "edit.undo", "Отменить", &["Mod+KeyZ"]),
    Command {
        id: C::Redo,
        key: "edit.redo",
        name: "Повторить",
        defaults: &["Mod+Shift+KeyZ"],
        defaults_pc: &["Ctrl+KeyY"],
    },
    cmd(C::Copy, "edit.copy", "Копировать", &["Mod+KeyC"]),
    cmd(C::Cut, "edit.cut", "Вырезать", &["Mod+KeyX"]),
    cmd(C::Paste, "edit.paste", "Вставить", &["Mod+KeyV"]),
    cmd(
        C::CopyAsPng,
        "edit.copy_png",
        "Копировать как PNG",
        &["Mod+Shift+KeyC"],
    ),
    cmd(C::Duplicate, "edit.duplicate", "Дублировать", &["Mod+KeyD"]),
    cmd(
        C::Delete,
        "edit.delete",
        "Удалить",
        &["Delete", "Backspace"],
    ),
    cmd(
        C::SelectAll,
        "edit.select_all",
        "Выделить всё",
        &["Mod+KeyA"],
    ),
    cmd(C::Escape, "edit.escape", "Отмена", &["Escape"]),
    cmd(C::Group, "object.group", "Сгруппировать", &["Mod+KeyG"]),
    cmd(
        C::Ungroup,
        "object.ungroup",
        "Разгруппировать",
        &["Mod+Shift+KeyG"],
    ),
    cmd(
        C::FrameSelection,
        "object.frame_selection",
        "Обернуть во фрейм",
        &["Mod+Alt+KeyG"],
    ),
    cmd(
        C::BringForward,
        "arrange.forward",
        "Переместить вперёд",
        &["Mod+BracketRight"],
    ),
    cmd(
        C::SendBackward,
        "arrange.backward",
        "Переместить назад",
        &["Mod+BracketLeft"],
    ),
    cmd(
        C::BringToFront,
        "arrange.front",
        "На передний план",
        &["Mod+Alt+BracketRight"],
    ),
    cmd(
        C::SendToBack,
        "arrange.back",
        "На задний план",
        &["Mod+Alt+BracketLeft"],
    ),
    cmd(C::NudgeLeft, "nudge.left", "Сдвинуть влево", &["ArrowLeft"]),
    cmd(
        C::NudgeRight,
        "nudge.right",
        "Сдвинуть вправо",
        &["ArrowRight"],
    ),
    cmd(C::NudgeUp, "nudge.up", "Сдвинуть вверх", &["ArrowUp"]),
    cmd(C::NudgeDown, "nudge.down", "Сдвинуть вниз", &["ArrowDown"]),
    cmd(
        C::NudgeLeftBig,
        "nudge.left_big",
        "Сдвинуть влево на 10 px",
        &["Shift+ArrowLeft"],
    ),
    cmd(
        C::NudgeRightBig,
        "nudge.right_big",
        "Сдвинуть вправо на 10 px",
        &["Shift+ArrowRight"],
    ),
    cmd(
        C::NudgeUpBig,
        "nudge.up_big",
        "Сдвинуть вверх на 10 px",
        &["Shift+ArrowUp"],
    ),
    cmd(
        C::NudgeDownBig,
        "nudge.down_big",
        "Сдвинуть вниз на 10 px",
        &["Shift+ArrowDown"],
    ),
    cmd(
        C::ZoomToFit,
        "view.fit_all",
        "Показать всё",
        &["Shift+Digit1"],
    ),
    cmd(
        C::ZoomToSelection,
        "view.fit_selection",
        "Показать выделение",
        &["Shift+Digit2"],
    ),
    cmd(
        C::Zoom100,
        "view.zoom_100",
        "Масштаб 100%",
        &["Shift+Digit0"],
    ),
    cmd(C::ZoomIn, "view.zoom_in", "Приблизить", &["Equal"]),
    cmd(C::ZoomOut, "view.zoom_out", "Отдалить", &["Minus"]),
    cmd(
        C::ToggleVisible,
        "object.toggle_visible",
        "Скрыть или показать",
        &["Mod+Shift+KeyH"],
    ),
    cmd(
        C::ToggleLock,
        "object.toggle_lock",
        "Заблокировать или разблокировать",
        &["Mod+Shift+KeyL"],
    ),
    cmd(
        C::ToggleUi,
        "view.toggle_ui",
        "Скрыть интерфейс",
        &["Mod+Backslash"],
    ),
];

impl CommandId {
    pub fn info(self) -> &'static Command {
        COMMANDS
            .iter()
            .find(|c| c.id == self)
            .expect("каждая команда есть в таблице")
    }

    pub fn key(self) -> &'static str {
        self.info().key
    }

    pub fn name(self) -> &'static str {
        self.info().name
    }

    pub fn from_key(key: &str) -> Option<CommandId> {
        COMMANDS.iter().find(|c| c.key == key).map(|c| c.id)
    }

    /// Сочетания по умолчанию для платформы, как строки с явными модификаторами.
    pub fn default_shortcuts(self, platform: Platform) -> Vec<String> {
        let info = self.info();
        let primary = match platform {
            Platform::Mac => "Meta",
            _ => "Ctrl",
        };
        let pc: &[&str] = if platform == Platform::Mac {
            &[]
        } else {
            info.defaults_pc
        };
        info.defaults
            .iter()
            .chain(pc)
            .map(|s| s.replace("Mod", primary))
            .collect()
    }
}
````

`crates/ferrum-editor/src/keymap.rs`:

````rust
//! Раскладка горячих клавиш: сочетания по умолчанию из таблицы команд плюс
//! пользовательские отличия. Хранятся только отличия — при обновлении Ferrum новые
//! сочетания по умолчанию доходят до пользователя сами.
//!
//! Сочетания сопоставляются по физической клавише (`KeyboardEvent.code`), поэтому
//! буквы и цифры работают на любой раскладке, в том числе на русской: V и М — одна
//! клавиша `KeyV`.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde_json::{json, Value};

use crate::commands::{CommandId, COMMANDS};
use crate::input::{KeyEvent, Modifiers, Platform};

/// Предел размера импортируемого JSON и числа сочетаний у одной команды: файл
/// не доверенный.
pub const MAX_KEYMAP_JSON: usize = 64 * 1024;
pub const MAX_SHORTCUTS_PER_COMMAND: usize = 8;
const FORMAT: &str = "ferrum-keymap";
const VERSION: u64 = 1;

/// Сочетание: физическая клавиша и точный набор модификаторов.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shortcut {
    pub code: String,
    pub mods: Mods,
}

/// Модификаторы сочетания (упорядочиваемые — для детерминированных списков).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl From<Modifiers> for Mods {
    fn from(m: Modifiers) -> Mods {
        Mods {
            ctrl: m.ctrl,
            alt: m.alt,
            shift: m.shift,
            meta: m.meta,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeymapError {
    #[error("Не удалось разобрать сочетание «{0}»")]
    BadShortcut(String),
    #[error("Файл раскладки слишком большой (больше 64 КБ)")]
    TooLarge,
    #[error("Это не файл раскладки Ferrum: {0}")]
    BadFormat(String),
    #[error("Раскладка из более новой версии Ferrum ({0}). Обновите Ferrum")]
    NewerVersion(u64),
}

/// Сочетание уже занято другими командами.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub shortcut: Shortcut,
    pub commands: Vec<CommandId>,
}

/// Допустимые физические клавиши: буквы, цифры, F1–F24, навигация и знаки.
fn valid_code(code: &str) -> bool {
    const NAMED: &[&str] = &[
        "Space",
        "Enter",
        "Escape",
        "Tab",
        "Backspace",
        "Delete",
        "Insert",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "ArrowDown",
        "Minus",
        "Equal",
        "BracketLeft",
        "BracketRight",
        "Backslash",
        "Semicolon",
        "Quote",
        "Comma",
        "Period",
        "Slash",
        "Backquote",
        "IntlBackslash",
        "NumpadAdd",
        "NumpadSubtract",
        "NumpadMultiply",
        "NumpadDivide",
        "NumpadDecimal",
        "NumpadEnter",
    ];
    let one_of = |prefix: &str, ok: fn(&str) -> bool| code.strip_prefix(prefix).is_some_and(ok);
    NAMED.contains(&code)
        || one_of("Key", |r| {
            r.len() == 1 && r.as_bytes()[0].is_ascii_uppercase()
        })
        || one_of("Digit", |r| {
            r.len() == 1 && r.as_bytes()[0].is_ascii_digit()
        })
        || one_of("Numpad", |r| {
            r.len() == 1 && r.as_bytes()[0].is_ascii_digit()
        })
        || one_of("F", |r| {
            r.parse::<u8>().is_ok_and(|n| (1..=24).contains(&n))
        })
}

impl Shortcut {
    pub fn new(code: &str, mods: Modifiers) -> Shortcut {
        Shortcut {
            code: code.to_string(),
            mods: mods.into(),
        }
    }

    /// Разбирает «Ctrl+Shift+KeyZ». Модификаторы — Ctrl, Alt, Shift, Meta (или Cmd),
    /// в любом порядке, без повторов; последней идёт физическая клавиша.
    pub fn parse(s: &str) -> Result<Shortcut, KeymapError> {
        let bad = || KeymapError::BadShortcut(s.chars().take(64).collect());
        let mut parts: Vec<&str> = s.split('+').collect();
        let code = parts.pop().filter(|c| valid_code(c)).ok_or_else(bad)?;
        let mut mods = Mods::default();
        for p in parts {
            let flag = match p {
                "Ctrl" => &mut mods.ctrl,
                "Alt" => &mut mods.alt,
                "Shift" => &mut mods.shift,
                "Meta" | "Cmd" => &mut mods.meta,
                _ => return Err(bad()),
            };
            if *flag {
                return Err(bad());
            }
            *flag = true;
        }
        Ok(Shortcut {
            code: code.to_string(),
            mods,
        })
    }

    /// Подпись для интерфейса на раскладке US: «⇧⌘K» на macOS, «Ctrl+Shift+K» на
    /// остальных. Подпись для текущей раскладки делает интерфейс по
    /// `navigator.keyboard.getLayoutMap()`.
    pub fn label(&self, platform: Platform) -> String {
        let key = key_label(&self.code);
        if platform == Platform::Mac {
            let mut s = String::new();
            for (on, sym) in [
                (self.mods.ctrl, "⌃"),
                (self.mods.alt, "⌥"),
                (self.mods.shift, "⇧"),
                (self.mods.meta, "⌘"),
            ] {
                if on {
                    s.push_str(sym);
                }
            }
            s + &key
        } else {
            let mut parts = Vec::new();
            for (on, name) in [
                (self.mods.ctrl, "Ctrl"),
                (self.mods.alt, "Alt"),
                (self.mods.shift, "Shift"),
                (self.mods.meta, "Win"),
            ] {
                if on {
                    parts.push(name.to_string());
                }
            }
            parts.push(key);
            parts.join("+")
        }
    }
}

fn key_label(code: &str) -> String {
    if let Some(r) = code
        .strip_prefix("Key")
        .or_else(|| code.strip_prefix("Digit"))
    {
        return r.to_string();
    }
    match code {
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Minus" => "-",
        "Equal" => "=",
        "ArrowLeft" => "←",
        "ArrowRight" => "→",
        "ArrowUp" => "↑",
        "ArrowDown" => "↓",
        "Escape" => "Esc",
        other => other,
    }
    .to_string()
}

impl fmt::Display for Shortcut {
    /// Каноническая запись: модификаторы в порядке Ctrl, Alt, Shift, Meta.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.mods.ctrl, "Ctrl"),
            (self.mods.alt, "Alt"),
            (self.mods.shift, "Shift"),
            (self.mods.meta, "Meta"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.code)
    }
}

/// Сочетания, которые браузер перехватывает до страницы (закрыть вкладку, новое
/// окно и т. п.). В веб-версии интерфейс помечает их предупреждением; в Tauri они
/// доступны.
pub fn browser_reserved(platform: Platform) -> Vec<Shortcut> {
    let primary = match platform {
        Platform::Mac => "Meta",
        _ => "Ctrl",
    };
    let mut list: Vec<String> = [
        "KeyW",
        "KeyT",
        "KeyN",
        "Shift+KeyW",
        "Shift+KeyT",
        "Shift+KeyN",
        "Tab",
        "Shift+Tab",
        "PageUp",
        "PageDown",
    ]
    .iter()
    .map(|s| format!("{primary}+{s}"))
    .collect();
    if platform == Platform::Mac {
        // Выйти, скрыть приложение, свернуть окно.
        list.extend(["Meta+KeyQ", "Meta+KeyH", "Meta+KeyM"].map(String::from));
    } else {
        list.extend(["Alt+F4", "F11"].map(String::from));
    }
    let mut out: Vec<Shortcut> = list
        .iter()
        .map(|s| Shortcut::parse(s).expect("встроенный список корректен"))
        .collect();
    out.sort();
    out
}

#[derive(Debug, Clone)]
pub struct Keymap {
    platform: Platform,
    defaults: BTreeMap<CommandId, Vec<Shortcut>>,
    /// Только команды, чьи сочетания отличаются от умолчаний (полный новый список).
    overrides: BTreeMap<CommandId, Vec<Shortcut>>,
    /// Сочетание → команды, которые оно сейчас вызывает (в порядке таблицы).
    index: HashMap<Shortcut, Vec<CommandId>>,
}

impl Keymap {
    pub fn new(platform: Platform) -> Keymap {
        let defaults = COMMANDS
            .iter()
            .map(|c| {
                let list =
                    c.id.default_shortcuts(platform)
                        .iter()
                        .map(|s| Shortcut::parse(s).expect("таблица команд корректна"))
                        .collect();
                (c.id, list)
            })
            .collect();
        let mut map = Keymap {
            platform,
            defaults,
            overrides: BTreeMap::new(),
            index: HashMap::new(),
        };
        map.reindex();
        map
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }

    fn reindex(&mut self) {
        let mut index: HashMap<Shortcut, Vec<CommandId>> = HashMap::new();
        for c in COMMANDS {
            for s in self.bindings(c.id) {
                index.entry(s.clone()).or_default().push(c.id);
            }
        }
        self.index = index;
    }

    /// Действующие сочетания команды.
    pub fn bindings(&self, id: CommandId) -> &[Shortcut] {
        self.overrides
            .get(&id)
            .or_else(|| self.defaults.get(&id))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn default_bindings(&self, id: CommandId) -> &[Shortcut] {
        self.defaults.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn is_overridden(&self, id: CommandId) -> bool {
        self.overrides.contains_key(&id)
    }

    /// Команда для нажатой клавиши. При конфликте — первая по таблице команд.
    pub fn lookup(&self, e: &KeyEvent) -> Option<CommandId> {
        let s = Shortcut {
            code: e.code.clone(),
            mods: e.modifiers.into(),
        };
        self.index.get(&s).and_then(|ids| ids.first().copied())
    }

    /// Команды, которые вызывает сочетание.
    pub fn commands_for(&self, s: &Shortcut) -> Vec<CommandId> {
        self.index.get(s).cloned().unwrap_or_default()
    }

    /// Все сочетания, занятые больше чем одной командой.
    pub fn conflicts(&self) -> Vec<Conflict> {
        let mut out: Vec<Conflict> = self
            .index
            .iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(s, ids)| Conflict {
                shortcut: s.clone(),
                commands: ids.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.shortcut.cmp(&b.shortcut));
        out
    }

    /// Действующие сочетания, которые перехватывает браузер.
    pub fn browser_conflicts(&self) -> Vec<(CommandId, Shortcut)> {
        let reserved = browser_reserved(self.platform);
        let mut out = Vec::new();
        for c in COMMANDS {
            for s in self.bindings(c.id) {
                if reserved.contains(s) {
                    out.push((c.id, s.clone()));
                }
            }
        }
        out
    }

    /// Задаёт список сочетаний команды как есть (конфликты разрешены — их покажет
    /// `conflicts`). Список, совпадающий с умолчанием, перестаёт быть отличием.
    pub fn set_bindings(&mut self, id: CommandId, list: Vec<Shortcut>) {
        let mut unique: Vec<Shortcut> = Vec::with_capacity(list.len());
        for s in list {
            if !unique.contains(&s) {
                unique.push(s);
            }
        }
        let mut list = unique;
        list.truncate(MAX_SHORTCUTS_PER_COMMAND);
        if self.default_bindings(id) == list.as_slice() {
            self.overrides.remove(&id);
        } else {
            self.overrides.insert(id, list);
        }
        self.reindex();
    }

    /// Добавляет сочетание команде. Если оно занято другими командами: без `replace`
    /// — ничего не меняет и возвращает конфликт (интерфейс предлагает заменить),
    /// с `replace` — снимает сочетание с этих команд.
    pub fn bind(&mut self, id: CommandId, s: Shortcut, replace: bool) -> Result<(), Conflict> {
        let others: Vec<CommandId> = self
            .commands_for(&s)
            .into_iter()
            .filter(|c| *c != id)
            .collect();
        if !others.is_empty() {
            if !replace {
                return Err(Conflict {
                    shortcut: s,
                    commands: others,
                });
            }
            for other in others {
                let list = self
                    .bindings(other)
                    .iter()
                    .filter(|x| **x != s)
                    .cloned()
                    .collect();
                self.set_bindings(other, list);
            }
        }
        let mut list = self.bindings(id).to_vec();
        if !list.contains(&s) {
            list.push(s);
        }
        self.set_bindings(id, list);
        Ok(())
    }

    pub fn unbind(&mut self, id: CommandId, s: &Shortcut) {
        let list = self
            .bindings(id)
            .iter()
            .filter(|x| *x != s)
            .cloned()
            .collect();
        self.set_bindings(id, list);
    }

    pub fn reset(&mut self, id: CommandId) {
        self.overrides.remove(&id);
        self.reindex();
    }

    pub fn reset_all(&mut self) {
        self.overrides.clear();
        self.reindex();
    }

    /// Отличия от умолчаний в JSON — то, что хранится в настройках пользователя.
    pub fn export_json(&self) -> String {
        let overrides: serde_json::Map<String, Value> = self
            .overrides
            .iter()
            .map(|(id, list)| {
                let list: Vec<Value> = list.iter().map(|s| Value::from(s.to_string())).collect();
                (id.key().to_string(), Value::from(list))
            })
            .collect();
        let doc = json!({
            "format": FORMAT,
            "version": VERSION,
            "overrides": overrides,
        });
        serde_json::to_string_pretty(&doc).expect("JSON из строк всегда сериализуется")
    }

    /// Заменяет отличия импортированными. Файл не доверенный: при ошибке формата
    /// раскладка не меняется. Неизвестные команды и непонятные сочетания
    /// пропускаются — о них возвращаются предупреждения.
    pub fn import_json(&mut self, text: &str) -> Result<Vec<String>, KeymapError> {
        if text.len() > MAX_KEYMAP_JSON {
            return Err(KeymapError::TooLarge);
        }
        let doc: Value =
            serde_json::from_str(text).map_err(|e| KeymapError::BadFormat(e.to_string()))?;
        let obj = doc
            .as_object()
            .ok_or_else(|| KeymapError::BadFormat("ожидался объект".into()))?;
        if obj.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err(KeymapError::BadFormat("нет поля format".into()));
        }
        let version = obj
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| KeymapError::BadFormat("нет поля version".into()))?;
        if version > VERSION {
            return Err(KeymapError::NewerVersion(version));
        }
        let overrides = obj
            .get("overrides")
            .and_then(Value::as_object)
            .ok_or_else(|| KeymapError::BadFormat("нет поля overrides".into()))?;
        let mut warnings = Vec::new();
        let mut parsed = BTreeMap::new();
        for (key, value) in overrides {
            let Some(id) = CommandId::from_key(key) else {
                warnings.push(format!(
                    "Неизвестная команда «{}» пропущена",
                    key.chars().take(64).collect::<String>()
                ));
                continue;
            };
            let Some(items) = value.as_array() else {
                warnings.push(format!("У команды {key} не список сочетаний — пропущена"));
                continue;
            };
            let mut list = Vec::new();
            for item in items.iter().take(MAX_SHORTCUTS_PER_COMMAND) {
                match item.as_str().map(Shortcut::parse) {
                    Some(Ok(s)) => list.push(s),
                    Some(Err(e)) => warnings.push(format!("{key}: {e}")),
                    None => warnings.push(format!("{key}: сочетание должно быть строкой")),
                }
            }
            if items.len() > MAX_SHORTCUTS_PER_COMMAND {
                warnings.push(format!(
                    "{key}: больше {MAX_SHORTCUTS_PER_COMMAND} сочетаний, лишние пропущены"
                ));
            }
            parsed.insert(id, list);
        }
        self.overrides.clear();
        for (id, list) in parsed {
            self.set_bindings(id, list);
        }
        self.reindex();
        Ok(warnings)
    }
}
````

`crates/ferrum-editor/src/lib.rs`:

````rust
//! Редактор Ferrum без браузера: инструменты, выделение, привязки, команды и
//! горячие клавиши, буфер обмена. Получает события ввода простыми структурами,
//! правит документ транзакциями `ferrum-doc`, держит сцену с превью жеста, камеру и
//! оверлей для `ferrum-render`. Не зависит ни от браузера, ни от ОС и детерминирован:
//! одна и та же последовательность событий даёт один и тот же документ.

pub mod commands;
pub mod input;
pub mod keymap;

pub use commands::{Command, CommandId, COMMANDS};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, Conflict, Keymap, KeymapError, Shortcut};
````

- [ ] **Step 5: Запустить тесты**

Run: `cargo test -p ferrum-editor --test keymap`
Expected: PASS, `6 passed`.

- [ ] **Step 6: Проверить, что новых внешних пакетов нет**

Выполнить:

```bash
# В Cargo.lock добавилась только запись самого крейта: serde и serde_json уже были.
cargo metadata -q --format-version 1 > /dev/null
added=$(git diff -U0 Cargo.lock | grep -c '^+name = ' || true)
test "$added" = 1
git diff -U0 Cargo.lock | grep -q '^+name = "ferrum-editor"$'
```

- [ ] **Step 7: Закоммитить**

```bash
git add Cargo.toml Cargo.lock crates/ferrum-editor
git commit -m "feat(editor): события ввода, таблица команд и раскладка клавиш"
```

---

### Task 3: Геометрия жестов, привязки, правила выделения, пресеты фреймов

Чистые функции над сценой — без редактора и состояния, поэтому проверяются отдельно.

Геометрия. Модель хранит у узла сдвиг, поворот вокруг левого верхнего угла и размер, без масштаба и скоса. Поэтому любая правка жеста считается одинаково: новая локальная матрица узла (с масштабом жеста) раскладывается обратно на положение, поворот и размер (`decompose`). Для сдвига, поворота и ресайза в собственных осях узла это точно; ресайз повёрнутого узла внутри общей рамки нескольких узлов теряет скос (узел остаётся прямоугольником, как в Figma). Ресайз группы масштабирует детей в координатах группы (раздел 5.2), дети вложенных фреймов не масштабируются. Ресайз за противоположную сторону меняет сторону рамки, а не отражает фигуру: отражения в модели нет. Жест каждый раз считается от баз — свойств документа на начало жеста, а не от превью.

Привязки. Порог — 4 экранных пикселя, переведённые в мир по зуму. Кандидаты — соседи (дети родителей двигающихся узлов) и рамки родительских фреймов; видимые, не участвующие в жесте и не их потомки. Ищутся двумя запросами к R-tree по полосам вокруг двигающихся координат в пределах видимой части: вертикальная полоса `[min x − порог, max x + порог] × видимая высота` содержит каждый узел, у которого край или центр ближе порога к одному из двигающихся краёв по x (такая координата лежит в полосе, значит узел её пересекает). Так находятся ровно все привязки в видимой части, а работа зависит от числа узлов рядом, а не от размера страницы.

Правила выделения (раздел 7.2) — как в Figma: клик выбирает объект верхнего уровня, внутри фрейма верхнего уровня (монтажной области) — его ребёнка верхнего уровня; если выделение уже внутри контейнера, соседи выделенных выбираются на их уровне; двойной клик — на уровень глубже, Ctrl/Cmd+клик — самый глубокий. Пустое место фрейма верхнего уровня с детьми не выбирает фрейм (с него начинается рамка), фрейм выбирается кликом по названию над ним. Заблокированные и скрытые узлы прозрачны для клика. Рамка выделения в корне выбирает фреймы верхнего уровня, только если охватывает их целиком: иначе рамка, начатая на холсте, цепляла бы монтажные области, через которые прошла. Кандидаты рамки — дети контекста перебором (до 4096) или запрос к R-tree: запрос через всю страницу возвращает все 100 000 узлов (~14 мс в прототипе), а выбрать можно только детей контекста.

Пресеты — отдельный JSON внутри крейта (раздел 7.5); список дополняется без изменения кода, разбор проверяет размеры.

**Files:**
- Create: `crates/ferrum-editor/src/geometry.rs`
- Create: `crates/ferrum-editor/src/snap.rs`
- Create: `crates/ferrum-editor/src/selection.rs`
- Create: `crates/ferrum-editor/src/presets.rs`
- Create: `crates/ferrum-editor/assets/frame_presets.json`
- Modify: `crates/ferrum-editor/src/lib.rs`
- Test: `crates/ferrum-editor/tests/geometry.rs`

**Interfaces:**
- Consumes: `Scene::{slot, at, node, roots, query_rect, hit_test_all, is_visible, is_draft, order_key}`, `SceneNode::{world, local, local_bounds, props, parent, children}`, `shape::local_transform`, `Camera::{screen_to_world, zoom}`, `LABEL_SIZE`.
- Produces: `geometry::{Pose, decompose, normalize_deg, snap_angle, parent_world, world_box, top_level, SelectionBox, Handle, resize_rect, rect_map, Base, bases, rigid_patches, scale_patches, local_with}`; `snap::{Snapper, Snap, SNAP_THRESHOLD_PX}`; `selection::{Pick, locked, chain, top_hit, resolve, deeper, frame_label_at, root_frame_at, drop_frame_at, marquee, LABEL_HIT_PX, MARQUEE_SCAN_CHILDREN}`; `FramePreset`, `frame_presets()`, `parse_presets`, `PresetError`.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-editor/tests/geometry.rs`:

````rust
//! Геометрия жестов, привязки и правила выделения на сцене — без редактора.

use std::collections::HashSet;

use ferrum_doc::{Document, NodeId, NodeKind, PageId, Props, PropsPatch};
use ferrum_editor::geometry::*;
use ferrum_editor::selection::{self, Pick};
use ferrum_editor::snap::Snapper;
use ferrum_editor::{frame_presets, parse_presets};
use ferrum_render::Camera;
use ferrum_scene::{shape, Scene};
use kurbo::{Affine, Point, Rect, Size};

fn patch(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Страница по списку (родитель — индекс раньше в списке, тип, свойства).
fn page(nodes: &[(Option<usize>, NodeKind, PropsPatch)]) -> (Document, PageId, Vec<NodeId>) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |tx| {
            let mut ids: Vec<NodeId> = Vec::new();
            for (parent, kind, p) in nodes {
                let id = tx.create_node(parent.map(|i| ids[i]), None, *kind)?;
                tx.set_props(id, p)?;
                ids.push(id);
            }
            Ok(ids)
        })
        .unwrap();
    (doc, page, ids)
}

#[test]
fn decompose_recovers_pose_and_scale() {
    for (x, y, rotation, w, h) in [
        (0.0, 0.0, 0.0, 10.0, 20.0),
        (15.5, -7.0, 30.0, 100.0, 40.0),
        (-3.0, 4.0, 270.0, 5.0, 5.0),
    ] {
        let props = Props {
            x,
            y,
            rotation,
            w,
            h,
            ..Props::defaults(NodeKind::Rect)
        };
        let m = shape::local_transform(&props);
        let p = decompose(m, w, h);
        assert!(
            close(p.x, x) && close(p.y, y) && close(p.w, w) && close(p.h, h),
            "{p:?}"
        );
        assert!(close(p.rotation, rotation), "{p:?}");
        let s = decompose(m * Affine::scale_non_uniform(2.0, 3.0), w, h);
        assert!(close(s.w, 2.0 * w) && close(s.h, 3.0 * h) && close(s.rotation, rotation));
    }
    assert_eq!(normalize_deg(-90.0), 270.0);
    assert_eq!(normalize_deg(359.9999999999999), 0.0);
    assert_eq!(normalize_deg(720.0 + 15.0), 15.0);
    assert_eq!(snap_angle(37.0, 15.0), 30.0);
}

#[test]
fn resize_rect_anchors_aspect_and_center() {
    let r = Rect::new(0.0, 0.0, 100.0, 50.0);
    let br = resize_rect(
        r,
        Handle::BottomRight,
        Point::new(150.0, 80.0),
        false,
        false,
    );
    assert_eq!(br, Rect::new(0.0, 0.0, 150.0, 80.0));
    // За противоположную сторону: рамка меняет сторону, без отражения.
    let flip = resize_rect(r, Handle::Right, Point::new(-30.0, 999.0), false, false);
    assert_eq!(flip, Rect::new(-30.0, 0.0, 0.0, 50.0));
    // Пропорции: угол — по большему изменению; сторона — другая ось от центра.
    let k = resize_rect(r, Handle::BottomRight, Point::new(300.0, 60.0), true, false);
    assert_eq!(k, Rect::new(0.0, 0.0, 300.0, 150.0));
    let e = resize_rect(r, Handle::Bottom, Point::new(0.0, 100.0), true, false);
    assert_eq!(e, Rect::new(-50.0, 0.0, 150.0, 100.0));
    // От центра.
    let c = resize_rect(r, Handle::TopLeft, Point::new(-10.0, -5.0), false, true);
    assert_eq!(c, Rect::new(-10.0, -5.0, 110.0, 55.0));
    let m = rect_map(r, Rect::new(10.0, 10.0, 210.0, 60.0));
    assert_eq!(m * Point::new(100.0, 50.0), Point::new(210.0, 60.0));
    assert_eq!(m * Point::ZERO, Point::new(10.0, 10.0));
    // Нулевая сторона не масштабируется.
    let line = Rect::new(0.0, 0.0, 0.0, 10.0);
    let m = rect_map(line, Rect::new(5.0, 0.0, 25.0, 20.0));
    assert_eq!(m * Point::new(0.0, 10.0), Point::new(5.0, 20.0));
}

#[test]
fn group_scale_patches_children_and_rigid_moves_roots() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Group, PropsPatch::default()),
        (Some(0), NodeKind::Rect, patch(10.0, 10.0, 20.0, 20.0)),
        (Some(0), NodeKind::Line, patch(40.0, 10.0, 30.0, 0.0)),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let b = bases(&scene, &[ids[0]], true);
    assert_eq!(b.len(), 3, "группа и её дети");
    let patches = scale_patches(&b, Affine::scale(2.0), false);
    assert_eq!(patches.len(), 2, "у группы своей правки нет");
    let rect = &patches[0].1;
    assert_eq!(
        (rect.x, rect.y, rect.w, rect.h),
        (Some(20.0), Some(20.0), Some(40.0), Some(40.0))
    );
    let line = &patches[1].1;
    assert_eq!((line.w, line.h), (Some(60.0), None), "у линии высоты нет");
    // Поворот на 90° вокруг (20, 20): прямоугольник встаёт с левым верхним углом (30, 10).
    let t = Affine::translate((20.0, 20.0))
        * Affine::rotate(std::f64::consts::FRAC_PI_2)
        * Affine::translate((-20.0, -20.0));
    let rb = bases(&scene, &[ids[1]], false);
    let p = &rigid_patches(&rb, t, false)[0].1;
    assert!(
        close(p.x.unwrap(), 30.0) && close(p.y.unwrap(), 10.0),
        "{p:?}"
    );
    assert_eq!(p.rotation, Some(270.0));
    // Пиксельная привязка округляет положение неповёрнутых и размеры.
    let s = scale_patches(&rb, Affine::scale(1.33), true);
    assert_eq!((s[0].1.x, s[0].1.w), (Some(13.0), Some(27.0)));
}

#[test]
fn selection_box_and_top_level() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Group, PropsPatch::default()),
        (Some(0), NodeKind::Rect, patch(10.0, 10.0, 20.0, 20.0)),
        (None, NodeKind::Rect, patch(100.0, 0.0, 10.0, 10.0)),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert_eq!(top_level(&scene, &[ids[1], ids[0]]), vec![ids[0]]);
    let one = SelectionBox::of(&scene, &[ids[1]]).unwrap();
    assert_eq!(one.rect, Rect::new(0.0, 0.0, 20.0, 20.0));
    assert_eq!(one.point(1.0, 1.0), Point::new(30.0, 30.0));
    let both = SelectionBox::of(&scene, &[ids[0], ids[2]]).unwrap();
    assert_eq!(both.rect, Rect::new(10.0, 0.0, 110.0, 30.0));
    assert!(both.axis_aligned());
}

#[test]
fn snapper_finds_siblings_and_parent_frame_only() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Frame, patch(0.0, 0.0, 400.0, 400.0)),
        (Some(0), NodeKind::Rect, patch(100.0, 100.0, 50.0, 50.0)),
        (Some(0), NodeKind::Rect, patch(300.0, 300.0, 20.0, 20.0)),
        // Другой родитель и скрытый сосед — не кандидаты.
        (None, NodeKind::Rect, patch(203.0, 500.0, 10.0, 10.0)),
        (
            Some(0),
            NodeKind::Rect,
            PropsPatch {
                visible: Some(false),
                ..patch(251.0, 10.0, 10.0, 10.0)
            },
        ),
    ]);
    let scene = Scene::from_page(doc.page(page).unwrap());
    let moving = [ids[2]];
    let snapper = Snapper::new(&scene, &moving);
    let view = Rect::new(-1000.0, -1000.0, 1000.0, 1000.0);
    // Левый край на 152: ближе всего правый край соседа (150).
    let s = snapper.snap(
        &scene,
        &[152.0],
        &[],
        Rect::new(152.0, 300.0, 172.0, 320.0),
        4.0,
        view,
    );
    assert_eq!(s.dx, Some(-2.0));
    assert_eq!(s.dy, None);
    assert_eq!(s.lines.len(), 1);
    assert_eq!(s.lines[0].p0.x, 150.0);
    assert_eq!((s.lines[0].p0.y, s.lines[0].p1.y), (100.0, 320.0));
    // 205 — в 2 px от узла другого родителя (203), но он не сосед; центр фрейма — 200.
    let s = snapper.snap(
        &scene,
        &[205.0],
        &[],
        Rect::new(205.0, 300.0, 225.0, 320.0),
        4.0,
        view,
    );
    assert_eq!(s.dx, None, "{s:?}");
    let s = snapper.snap(
        &scene,
        &[202.0],
        &[],
        Rect::new(202.0, 300.0, 222.0, 320.0),
        4.0,
        view,
    );
    assert_eq!(s.dx, Some(-2.0), "центр родительского фрейма");
    // Скрытый сосед (251) не притягивает.
    let s = snapper.snap(
        &scene,
        &[253.0],
        &[],
        Rect::new(253.0, 300.0, 273.0, 320.0),
        4.0,
        view,
    );
    assert_eq!(s.dx, None);
    // Кандидаты вне видимой части не находятся.
    let small = Rect::new(500.0, 500.0, 600.0, 600.0);
    let s = snapper.snap(
        &scene,
        &[152.0],
        &[],
        Rect::new(152.0, 300.0, 172.0, 320.0),
        4.0,
        small,
    );
    assert_eq!(s.dx, None);
}

#[test]
fn selection_rules_on_scene() {
    let (mut doc, page, ids) = page(&[
        (None, NodeKind::Frame, patch(0.0, 0.0, 400.0, 400.0)),
        (Some(0), NodeKind::Group, PropsPatch::default()),
        (Some(1), NodeKind::Rect, patch(10.0, 10.0, 50.0, 50.0)),
        (Some(0), NodeKind::Frame, patch(200.0, 200.0, 100.0, 100.0)),
        (None, NodeKind::Rect, patch(500.0, 0.0, 50.0, 50.0)),
        (None, NodeKind::Frame, patch(600.0, 0.0, 50.0, 50.0)),
    ]);
    let [f, g, a, inner, r, empty] = [ids[0], ids[1], ids[2], ids[3], ids[4], ids[5]];
    let scene = Scene::from_page(doc.page(page).unwrap());
    let hit = selection::top_hit(&scene, Point::new(20.0, 20.0), 0.0).unwrap();
    assert_eq!(scene.at(hit).id, a);
    assert_eq!(selection::resolve(&scene, hit, &[], Pick::Normal), Some(g));
    assert_eq!(selection::resolve(&scene, hit, &[], Pick::Deepest), Some(a));
    assert_eq!(selection::deeper(&scene, hit, &[g]), Some(a));
    // Пустое место фрейма верхнего уровня с детьми — ничего; без детей — сам фрейм.
    let fh = selection::top_hit(&scene, Point::new(150.0, 350.0), 0.0).unwrap();
    assert_eq!(selection::resolve(&scene, fh, &[], Pick::Normal), None);
    let eh = selection::top_hit(&scene, Point::new(620.0, 20.0), 0.0).unwrap();
    assert_eq!(
        selection::resolve(&scene, eh, &[], Pick::Normal),
        Some(empty)
    );
    assert_eq!(
        selection::root_frame_at(&scene, Point::new(150.0, 350.0), 0.0),
        Some(f)
    );
    // Куда класть: самый глубокий фрейм, не из перетаскиваемых.
    let p = Point::new(250.0, 250.0);
    assert_eq!(
        selection::drop_frame_at(&scene, p, 0.0, &HashSet::new()),
        Some(inner)
    );
    assert_eq!(
        selection::drop_frame_at(&scene, p, 0.0, &HashSet::from([inner])),
        Some(f)
    );
    assert_eq!(
        selection::drop_frame_at(&scene, Point::new(520.0, 20.0), 0.0, &HashSet::new()),
        None
    );
    // Название фрейма — полоса над его верхним краем.
    let mut cam = Camera::new(Size::new(1000.0, 800.0), 1.0);
    cam.set_center(Point::new(500.0, 400.0));
    assert_eq!(
        selection::frame_label_at(&scene, &cam, Point::new(620.0, -10.0)),
        Some(empty)
    );
    assert_eq!(
        selection::frame_label_at(&scene, &cam, Point::new(620.0, -40.0)),
        None
    );
    // Рамка: в корне фрейм — только целиком; в контексте фрейма — его дети.
    let m = selection::marquee(&scene, Rect::new(-10.0, -10.0, 560.0, 80.0), None);
    assert_eq!(m, vec![r]);
    let m = selection::marquee(&scene, Rect::new(-10.0, -10.0, 700.0, 500.0), None);
    assert_eq!(m, vec![f, r, empty]);
    let m = selection::marquee(&scene, Rect::new(5.0, 5.0, 250.0, 250.0), Some(f));
    assert_eq!(m, vec![g, inner]);
}

#[test]
fn builtin_presets_match_spec() {
    let names: Vec<(&str, f64, f64)> = frame_presets()
        .iter()
        .map(|p| (p.name.as_str(), p.width, p.height))
        .collect();
    assert_eq!(
        names,
        vec![
            ("iPhone 16", 393.0, 852.0),
            ("iPhone 16 Pro Max", 440.0, 956.0),
            ("Android Compact", 412.0, 917.0),
            ("iPad mini", 744.0, 1133.0),
            ("iPad Pro 11\"", 834.0, 1194.0),
            ("Desktop", 1440.0, 1024.0),
            ("MacBook Air", 1280.0, 832.0),
            ("1920×1080", 1920.0, 1080.0),
            ("Instagram post", 1080.0, 1350.0),
            ("Story", 1080.0, 1920.0),
            ("A4", 595.0, 842.0),
        ]
    );
    assert!(parse_presets(
        r#"{"version":1,"presets":[{"group":"x","name":"y","width":1e9,"height":5}]}"#
    )
    .is_err());
}
````

- [ ] **Step 2: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-editor --test geometry`
Expected: ошибка компиляции — нет модулей `geometry`, `selection`, `snap`, `presets`.

- [ ] **Step 3: Геометрия жестов**

`crates/ferrum-editor/src/geometry.rs`:

````rust
//! Геометрия жестов: рамка выделения, ручки, ресайз и поворот, перевод мировых
//! трансформаций обратно в свойства узлов.
//!
//! Модель хранит у узла только сдвиг, поворот вокруг левого верхнего угла и размер —
//! без масштаба и скоса. Поэтому любая правка считается так: новая локальная
//! матрица узла (с масштабом жеста) раскладывается обратно на положение, поворот и
//! размер (`decompose`). Для поворота и неповёрнутого ресайза это точно; ресайз
//! повёрнутого узла внутри общей рамки неравномерно теряет скос (как и в Figma,
//! где такой узел остаётся прямоугольником).

use std::f64::consts::FRAC_PI_2;

use ferrum_doc::{NodeId, NodeKind, Props, PropsPatch};
use ferrum_scene::{shape, Scene, Slot};
use kurbo::{Affine, Point, Rect, Vec2};

/// Положение узла в координатах родителя — то, что хранит модель.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    /// Градусы против часовой стрелки на экране, в `[0, 360)`.
    pub rotation: f64,
    pub w: f64,
    pub h: f64,
}

/// Угол в `[0, 360)`; почти целые значения округляются, чтобы поворот на 90° не
/// оставлял 89.99999999999999 в документе.
pub fn normalize_deg(deg: f64) -> f64 {
    let r = deg.rem_euclid(360.0);
    let rounded = r.round();
    let r = if (r - rounded).abs() < 1e-9 {
        rounded
    } else {
        r
    };
    if r >= 360.0 {
        0.0
    } else {
        r
    }
}

/// Раскладывает локальную матрицу узла (сдвиг, поворот и, возможно, масштаб жеста)
/// на положение и размер узла с исходным размером `w × h`.
pub fn decompose(m: Affine, w: f64, h: f64) -> Pose {
    let c = m.as_coeffs();
    let u = Vec2::new(c[0], c[1]);
    let v = Vec2::new(c[2], c[3]);
    let (su, sv) = (u.hypot(), v.hypot());
    let angle = if su > 1e-12 {
        u.y.atan2(u.x)
    } else if sv > 1e-12 {
        v.y.atan2(v.x) - FRAC_PI_2
    } else {
        0.0
    };
    Pose {
        x: c[4],
        y: c[5],
        rotation: normalize_deg(-angle.to_degrees()),
        w: w * su,
        h: h * sv,
    }
}

/// Мировая матрица родителя узла (у корневого — единичная).
pub fn parent_world(scene: &Scene, slot: Slot) -> Affine {
    scene
        .at(slot)
        .parent
        .map_or(Affine::IDENTITY, |p| scene.at(p).world)
}

/// Границы формы узла в мире без обводки (у группы — по видимым детям).
pub fn world_box(scene: &Scene, slot: Slot) -> Option<Rect> {
    let n = scene.at(slot);
    n.local_bounds.map(|b| n.world.transform_rect_bbox(b))
}

/// Узлы выделения без тех, у кого выделен предок: предок двигает их сам.
pub fn top_level(scene: &Scene, ids: &[NodeId]) -> Vec<NodeId> {
    let selected: std::collections::HashSet<NodeId> = ids.iter().copied().collect();
    ids.iter()
        .copied()
        .filter(|id| {
            let Some(slot) = scene.slot(*id) else {
                return false;
            };
            let mut cur = scene.at(slot).parent;
            while let Some(p) = cur {
                if selected.contains(&scene.at(p).id) {
                    return false;
                }
                cur = scene.at(p).parent;
            }
            true
        })
        .collect()
}

/// Рамка выделения: прямоугольник в своих координатах и матрица в мир. У одного
/// узла — его повёрнутая рамка, у нескольких — общая рамка в мире.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionBox {
    pub rect: Rect,
    pub to_world: Affine,
}

impl SelectionBox {
    pub fn of(scene: &Scene, ids: &[NodeId]) -> Option<SelectionBox> {
        let slots: Vec<Slot> = ids
            .iter()
            .filter_map(|id| scene.slot(*id))
            .filter(|s| scene.at(*s).local_bounds.is_some())
            .collect();
        if let [one] = slots[..] {
            let n = scene.at(one);
            return Some(SelectionBox {
                rect: n.local_bounds?,
                to_world: n.world,
            });
        }
        let rect = slots
            .iter()
            .filter_map(|s| world_box(scene, *s))
            .reduce(|a, b| a.union(b))?;
        Some(SelectionBox {
            rect,
            to_world: Affine::IDENTITY,
        })
    }

    pub fn center_world(&self) -> Point {
        self.to_world * self.rect.center()
    }

    /// Ось рамки совпадает с осями мира (нет поворота): тогда к ней применимы
    /// привязки по краям.
    pub fn axis_aligned(&self) -> bool {
        let c = self.to_world.as_coeffs();
        c[1].abs() < 1e-12 && c[2].abs() < 1e-12 && c[0] > 0.0 && c[3] > 0.0
    }

    /// Точка рамки по долям ширины и высоты, в мире.
    pub fn point(&self, fx: f64, fy: f64) -> Point {
        let r = self.rect;
        self.to_world * Point::new(r.x0 + fx * r.width(), r.y0 + fy * r.height())
    }
}

/// Ручка рамки выделения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    /// Концы линии: у выделенной линии вместо восьми ручек две.
    LineStart,
    LineEnd,
}

impl Handle {
    pub const BOX: [Handle; 8] = [
        Handle::TopLeft,
        Handle::Top,
        Handle::TopRight,
        Handle::Right,
        Handle::BottomRight,
        Handle::Bottom,
        Handle::BottomLeft,
        Handle::Left,
    ];

    /// Положение ручки в долях рамки.
    pub fn frac(self) -> (f64, f64) {
        match self {
            Handle::TopLeft | Handle::LineStart => (0.0, 0.0),
            Handle::Top => (0.5, 0.0),
            Handle::TopRight => (1.0, 0.0),
            Handle::Right => (1.0, 0.5),
            Handle::BottomRight => (1.0, 1.0),
            Handle::Bottom => (0.5, 1.0),
            Handle::BottomLeft => (0.0, 1.0),
            Handle::Left => (0.0, 0.5),
            Handle::LineEnd => (1.0, 0.0),
        }
    }

    pub fn is_corner(self) -> bool {
        let (fx, fy) = self.frac();
        fx != 0.5 && fy != 0.5
    }
}

/// Новый прямоугольник рамки при перетаскивании ручки в точку `p` (в координатах
/// рамки). Противоположная сторона стоит на месте; с `from_center` — центр.
/// `keep_aspect` сохраняет пропорции. Перетаскивание за противоположную сторону
/// не отражает фигуру (у модели нет отражения), а меняет сторону рамки.
pub fn resize_rect(
    rect: Rect,
    handle: Handle,
    p: Point,
    keep_aspect: bool,
    from_center: bool,
) -> Rect {
    let (fx, fy) = handle.frac();
    let c = rect.center();
    // Одна ось: неподвижная точка и новая протяжённость.
    let axis = |f: f64, lo: f64, hi: f64, center: f64, m: f64| -> (f64, f64) {
        if f == 0.5 {
            (lo, hi)
        } else if from_center {
            let half = (m - center).abs();
            (center - half, center + half)
        } else {
            let anchor = if f == 1.0 { lo } else { hi };
            (anchor.min(m), anchor.max(m))
        }
    };
    let (mut x0, mut x1) = axis(fx, rect.x0, rect.x1, c.x, p.x);
    let (mut y0, mut y1) = axis(fy, rect.y0, rect.y1, c.y, p.y);
    let (w0, h0) = (rect.width(), rect.height());
    if keep_aspect && w0 > 0.0 && h0 > 0.0 {
        let (sx, sy) = ((x1 - x0) / w0, (y1 - y0) / h0);
        let s = if fx == 0.5 {
            sy
        } else if fy == 0.5 {
            sx
        } else {
            sx.max(sy)
        };
        let (w, h) = (w0 * s, h0 * s);
        // Сторона, которая двигается, остаётся с той стороны от неподвижной точки,
        // где курсор; ось без ручки растёт симметрично от центра.
        let place = |f: f64, lo: f64, hi: f64, center: f64, m: f64, len: f64| -> (f64, f64) {
            if f == 0.5 || from_center {
                (center - len / 2.0, center + len / 2.0)
            } else {
                let anchor = if f == 1.0 { lo } else { hi };
                if m >= anchor {
                    (anchor, anchor + len)
                } else {
                    (anchor - len, anchor)
                }
            }
        };
        (x0, x1) = place(fx, rect.x0, rect.x1, c.x, p.x, w);
        (y0, y1) = place(fy, rect.y0, rect.y1, c.y, p.y, h);
    }
    Rect::new(x0, y0, x1, y1)
}

/// Матрица, переводящая прямоугольник `from` в `to` (сдвиг и масштаб по осям).
/// Нулевая сторона не масштабируется: вертикальную линию нельзя растянуть вширь.
pub fn rect_map(from: Rect, to: Rect) -> Affine {
    let sx = if from.width() > 0.0 {
        to.width() / from.width()
    } else {
        1.0
    };
    let sy = if from.height() > 0.0 {
        to.height() / from.height()
    } else {
        1.0
    };
    Affine::translate(to.origin().to_vec2())
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate(-from.origin().to_vec2())
}

/// Исходное состояние узла, который меняет жест: свойства документа на момент
/// начала жеста. Каждое движение считается от него, а не от превью.
#[derive(Debug, Clone)]
pub struct Base {
    pub id: NodeId,
    pub props: Props,
    pub local: Affine,
    /// Индекс родителя в списке баз — у потомков масштабируемой группы.
    pub parent: Option<usize>,
    /// Мировая матрица родителя — у узлов верхнего уровня жеста.
    pub parent_world: Affine,
}

/// Базы для узлов верхнего уровня; с `with_group_children` — и всё поддерево групп:
/// ресайз группы масштабирует её детей (раздел 5.2 спеки).
pub fn bases(scene: &Scene, ids: &[NodeId], with_group_children: bool) -> Vec<Base> {
    let mut out = Vec::new();
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        let pw = parent_world(scene, slot);
        let n = scene.at(slot);
        out.push(Base {
            id: n.id,
            props: n.props.clone(),
            local: n.local,
            parent: None,
            parent_world: pw,
        });
        if !with_group_children || n.props.kind != NodeKind::Group {
            continue;
        }
        // Потомки групп (не фреймов: дети фрейма не масштабируются).
        let mut stack = vec![(slot, out.len() - 1)];
        while let Some((s, at)) = stack.pop() {
            for &c in &scene.at(s).children {
                let cn = scene.at(c);
                out.push(Base {
                    id: cn.id,
                    props: cn.props.clone(),
                    local: cn.local,
                    parent: Some(at),
                    parent_world: Affine::IDENTITY,
                });
                if cn.props.kind == NodeKind::Group {
                    stack.push((c, out.len() - 1));
                }
            }
        }
    }
    out
}

fn pose_patch(base: &Props, pose: Pose, size: bool, pixel_snap: bool) -> PropsPatch {
    let mut pose = pose;
    if pixel_snap {
        if size {
            pose.w = pose.w.round();
            pose.h = pose.h.round();
        }
        if pose.rotation % 90.0 == 0.0 {
            pose.x = pose.x.round();
            pose.y = pose.y.round();
        }
    }
    let mut patch = PropsPatch {
        x: Some(pose.x),
        y: Some(pose.y),
        ..Default::default()
    };
    if pose.rotation != base.rotation {
        patch.rotation = Some(pose.rotation);
    }
    if size && base.kind != NodeKind::Group {
        patch.w = Some(pose.w);
        if base.kind != NodeKind::Line {
            patch.h = Some(pose.h);
        }
    }
    patch
}

/// Правки свойств для жёсткого движения (сдвиг и поворот) `t` в мире.
pub fn rigid_patches(bases: &[Base], t: Affine, pixel_snap: bool) -> Vec<(NodeId, PropsPatch)> {
    bases
        .iter()
        .filter(|b| b.parent.is_none())
        .map(|b| {
            let m = b.parent_world.inverse() * t * b.parent_world * b.local;
            let pose = decompose(m, b.props.w, b.props.h);
            (b.id, pose_patch(&b.props, pose, false, pixel_snap))
        })
        .collect()
}

/// Правки свойств для масштаба `t` в мире: узлы меняют положение и размер, у групп
/// масштабируются дети в координатах группы.
pub fn scale_patches(bases: &[Base], t: Affine, pixel_snap: bool) -> Vec<(NodeId, PropsPatch)> {
    // Масштаб в координатах родителя каждого узла; для детей группы — в
    // координатах группы, посчитанный при обработке самой группы.
    let mut in_parent: Vec<Affine> = Vec::with_capacity(bases.len());
    let mut out = Vec::with_capacity(bases.len());
    for b in bases {
        let tp = match b.parent {
            None => b.parent_world.inverse() * t * b.parent_world,
            Some(i) => {
                let g = &bases[i];
                g.local.inverse() * in_parent[i] * g.local
            }
        };
        in_parent.push(tp);
        if b.props.kind == NodeKind::Group {
            continue;
        }
        let pose = decompose(tp * b.local, b.props.w, b.props.h);
        out.push((b.id, pose_patch(&b.props, pose, true, pixel_snap)));
    }
    out
}

/// Локальная матрица узла по свойствам (с правкой поверх).
pub fn local_with(props: &Props, patch: &PropsPatch) -> Affine {
    let mut p = props.clone();
    patch.apply_to(&mut p);
    shape::local_transform(&p)
}

/// Округление угла до шага `step` градусов.
pub fn snap_angle(deg: f64, step: f64) -> f64 {
    (deg / step).round() * step
}
````

- [ ] **Step 4: Привязки**

`crates/ferrum-editor/src/snap.rs`:

````rust
//! Привязки к краям и центрам соседей и родительского фрейма (раздел 7.3 спеки).
//!
//! Кандидаты ищутся не перебором страницы, а двумя запросами к R-tree сцены — по
//! полосам вокруг двигающейся рамки в пределах видимой части холста. Вертикальная
//! полоса `[min x − порог, max x + порог] × видимая высота` содержит каждый узел,
//! у которого есть край или центр ближе порога к одному из двигающихся краёв по x:
//! такая координата лежит внутри полосы, значит и сам узел её пересекает. Поэтому
//! полосы находят ровно все привязки в видимой части, а работа зависит от числа
//! узлов рядом, а не от размера страницы.

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};
use kurbo::{Line, Rect};

use crate::geometry::world_box;

/// Порог привязки в экранных пикселях.
pub const SNAP_THRESHOLD_PX: f64 = 4.0;
const EPS: f64 = 1e-6;

/// Результат привязки: сдвиги по осям (если привязка сработала) и линии-подсказки.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snap {
    pub dx: Option<f64>,
    pub dy: Option<f64>,
    pub lines: Vec<Line>,
}

/// Кого и к чему привязывать в одном жесте.
#[derive(Debug, Clone)]
pub struct Snapper {
    /// Узлы жеста и черновики: к себе не привязываются.
    excluded: HashSet<NodeId>,
    /// Родители двигающихся узлов: соседи — их дети (`None` — корень страницы).
    parents: HashSet<Option<NodeId>>,
    /// Рамки родительских фреймов в мире: к их краям и центру тоже привязываемся.
    frames: Vec<Rect>,
}

impl Snapper {
    /// Для узлов `moving`: соседи — дети их родителей.
    pub fn new(scene: &Scene, moving: &[NodeId]) -> Snapper {
        let mut parents = HashSet::new();
        for id in moving {
            if let Some(s) = scene.slot(*id) {
                parents.insert(scene.at(s).parent.map(|p| scene.at(p).id));
            }
        }
        Snapper::with_parents(scene, parents, moving)
    }

    /// Для новой фигуры у родителя `parent`.
    pub fn for_parent(scene: &Scene, parent: Option<NodeId>, exclude: &[NodeId]) -> Snapper {
        Snapper::with_parents(scene, HashSet::from([parent]), exclude)
    }

    fn with_parents(
        scene: &Scene,
        parents: HashSet<Option<NodeId>>,
        exclude: &[NodeId],
    ) -> Snapper {
        let mut frames: Vec<Rect> = parents
            .iter()
            .flatten()
            .filter_map(|p| scene.slot(*p))
            .filter(|s| scene.at(*s).props.kind == NodeKind::Frame)
            .filter_map(|s| world_box(scene, s))
            .collect();
        frames.sort_by(|a, b| {
            (a.x0, a.y0, a.x1, a.y1)
                .partial_cmp(&(b.x0, b.y0, b.x1, b.y1))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Snapper {
            excluded: exclude.iter().copied().collect(),
            parents,
            frames,
        }
    }

    /// Можно ли привязываться к узлу: сосед, видим, не участвует в жесте.
    fn eligible(&self, scene: &Scene, slot: Slot) -> bool {
        let n = scene.at(slot);
        if self.excluded.contains(&n.id) || scene.is_draft(n.id) {
            return false;
        }
        let parent = n.parent.map(|p| scene.at(p).id);
        if !self.parents.contains(&parent) || !scene.is_visible(slot) {
            return false;
        }
        // Потомок двигающегося узла едет вместе с ним.
        let mut cur = n.parent;
        while let Some(p) = cur {
            if self.excluded.contains(&scene.at(p).id) {
                return false;
            }
            cur = scene.at(p).parent;
        }
        true
    }

    /// Рамки кандидатов, пересекающих полосу.
    fn candidates(&self, scene: &Scene, band: Rect) -> Vec<Rect> {
        let mut out: Vec<Rect> = scene
            .query_rect(band)
            .into_iter()
            .filter_map(|id| scene.slot(id))
            .filter(|s| self.eligible(scene, *s))
            .filter_map(|s| world_box(scene, s))
            .collect();
        out.extend(self.frames.iter().copied());
        out
    }

    /// Привязывает двигающиеся координаты `xs` и `ys` (края и центр рамки или точка
    /// ручки) к ближайшим краям и центрам соседей не дальше `threshold` (в мире).
    /// `moving` — рамка жеста до привязки (для длины линий), `view` — видимая
    /// часть мира. Пустой список координат выключает ось.
    pub fn snap(
        &self,
        scene: &Scene,
        xs: &[f64],
        ys: &[f64],
        moving: Rect,
        threshold: f64,
        view: Rect,
    ) -> Snap {
        let span = |v: &[f64]| {
            v.iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                    (lo.min(*x), hi.max(*x))
                })
        };
        let x_cands = if xs.is_empty() {
            Vec::new()
        } else {
            let (lo, hi) = span(xs);
            self.candidates(
                scene,
                Rect::new(lo - threshold, view.y0, hi + threshold, view.y1),
            )
        };
        let y_cands = if ys.is_empty() {
            Vec::new()
        } else {
            let (lo, hi) = span(ys);
            self.candidates(
                scene,
                Rect::new(view.x0, lo - threshold, view.x1, hi + threshold),
            )
        };
        let xt = |b: &Rect| [b.x0, (b.x0 + b.x1) / 2.0, b.x1];
        let yt = |b: &Rect| [b.y0, (b.y0 + b.y1) / 2.0, b.y1];
        let best = |vals: &[f64], cands: &[Rect], targets: &dyn Fn(&Rect) -> [f64; 3]| {
            let mut best: Option<f64> = None;
            for b in cands {
                for t in targets(b) {
                    for m in vals {
                        let d = t - m;
                        if d.abs() <= threshold && best.is_none_or(|x| d.abs() < x.abs()) {
                            best = Some(d);
                        }
                    }
                }
            }
            best
        };
        let dx = best(xs, &x_cands, &xt);
        let dy = best(ys, &y_cands, &yt);
        let moved = moving + kurbo::Vec2::new(dx.unwrap_or(0.0), dy.unwrap_or(0.0));
        let mut lines = Vec::new();
        if let Some(d) = dx {
            let hits: Vec<f64> = xs.iter().map(|m| m + d).collect();
            lines.extend(guide_lines(&hits, &x_cands, moved, true));
        }
        if let Some(d) = dy {
            let hits: Vec<f64> = ys.iter().map(|m| m + d).collect();
            lines.extend(guide_lines(&hits, &y_cands, moved, false));
        }
        Snap { dx, dy, lines }
    }
}

/// Линии привязки: по одной на каждую совпавшую координату, от двигающейся рамки
/// до самого дальнего совпавшего соседа.
fn guide_lines(values: &[f64], cands: &[Rect], moving: Rect, vertical: bool) -> Vec<Line> {
    let mut out: Vec<(f64, f64, f64)> = Vec::new();
    for b in cands {
        let targets = if vertical {
            [b.x0, (b.x0 + b.x1) / 2.0, b.x1]
        } else {
            [b.y0, (b.y0 + b.y1) / 2.0, b.y1]
        };
        for t in targets {
            if !values.iter().any(|v| (v - t).abs() < EPS) {
                continue;
            }
            let (lo, hi) = if vertical {
                (moving.y0.min(b.y0), moving.y1.max(b.y1))
            } else {
                (moving.x0.min(b.x0), moving.x1.max(b.x1))
            };
            match out.iter_mut().find(|(v, _, _)| (v - t).abs() < EPS) {
                Some(l) => {
                    l.1 = l.1.min(lo);
                    l.2 = l.2.max(hi);
                }
                None => out.push((t, lo, hi)),
            }
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out.into_iter()
        .map(|(v, lo, hi)| {
            if vertical {
                Line::new((v, lo), (v, hi))
            } else {
                Line::new((lo, v), (hi, v))
            }
        })
        .collect()
}
````

- [ ] **Step 5: Правила выделения**

`crates/ferrum-editor/src/selection.rs`:

````rust
//! Правила выделения на холсте (раздел 7.2 спеки), как в Figma:
//! - клик выбирает объект верхнего уровня; внутри фрейма верхнего уровня (он ведёт
//!   себя как монтажная область) — его ребёнка верхнего уровня;
//! - если выделение уже внутри контейнера, клик по соседям выбирает их на том же уровне;
//! - двойной клик — на уровень глубже, Ctrl/Cmd+клик — самый глубокий объект;
//! - пустое место фрейма верхнего уровня с детьми не выделяет фрейм (с него
//!   начинается рамка), фрейм выбирается кликом по его названию;
//! - заблокированные и скрытые узлы на холсте не выбираются, клик проходит сквозь них.

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_render::{Camera, LABEL_SIZE};
use ferrum_scene::{Scene, Slot};
use kurbo::{Point, Rect};

use crate::geometry::world_box;

/// Высота полосы над фреймом, где лежит его название (в экранных пикселях): кегль
/// подписи с межстрочным и отступ 4 px, как рисует оверлей.
pub const LABEL_HIT_PX: f64 = LABEL_SIZE * 1.5 + 4.0;

/// Как выбирать объект под курсором.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// Обычный клик: уровень по правилам контекста.
    Normal,
    /// Ctrl/Cmd+клик: самый глубокий объект.
    Deepest,
}

/// Заблокирован ли узел или кто-то из его предков.
pub fn locked(scene: &Scene, slot: Slot) -> bool {
    let mut cur = Some(slot);
    while let Some(s) = cur {
        let n = scene.at(s);
        if n.props.locked {
            return true;
        }
        cur = n.parent;
    }
    false
}

/// Цепочка от корня до узла включительно.
pub fn chain(scene: &Scene, slot: Slot) -> Vec<Slot> {
    let mut out = vec![slot];
    let mut cur = scene.at(slot).parent;
    while let Some(p) = cur {
        out.push(p);
        cur = scene.at(p).parent;
    }
    out.reverse();
    out
}

/// Верхний выбираемый узел под точкой (в мире): не черновик и не заблокирован.
/// Скрытые узлы сцена в попадания не включает.
pub fn top_hit(scene: &Scene, world: Point, tolerance: f64) -> Option<Slot> {
    scene
        .hit_test_all(world, tolerance)
        .into_iter()
        .filter(|id| !scene.is_draft(*id))
        .filter_map(|id| scene.slot(id))
        .find(|s| !locked(scene, *s))
}

/// Объект, который выберет клик по узлу `hit` при текущем выделении.
pub fn resolve(scene: &Scene, hit: Slot, selection: &[NodeId], pick: Pick) -> Option<NodeId> {
    if pick == Pick::Deepest {
        return Some(scene.at(hit).id);
    }
    let chain = chain(scene, hit);
    let root = scene.at(chain[0]);
    // Уровень по умолчанию: ребёнок фрейма верхнего уровня или сам объект верхнего уровня.
    let mut level = if root.props.kind == NodeKind::Frame && chain.len() > 1 {
        1
    } else {
        0
    };
    // Выделение внутри контейнера делает соседей выделенных выбираемыми на их уровне.
    let parents: HashSet<Option<Slot>> = selection
        .iter()
        .filter_map(|id| scene.slot(*id))
        .map(|s| scene.at(s).parent)
        .collect();
    for (i, &s) in chain.iter().enumerate().skip(level + 1) {
        if parents.contains(&scene.at(s).parent) {
            level = i;
        }
    }
    let target = chain[level];
    // Пустое место фрейма верхнего уровня с детьми: фрейм не выбирается.
    if level == 0 && root.props.kind == NodeKind::Frame && !root.children.is_empty() {
        return None;
    }
    Some(scene.at(target).id)
}

/// Двойной клик: ребёнок выделенного контейнера на пути к узлу под курсором.
pub fn deeper(scene: &Scene, hit: Slot, selection: &[NodeId]) -> Option<NodeId> {
    let chain = chain(scene, hit);
    let pos = chain
        .iter()
        .rposition(|s| selection.contains(&scene.at(*s).id))?;
    chain.get(pos + 1).map(|s| scene.at(*s).id)
}

/// Фрейм верхнего уровня, по названию которого щёлкнули (точка в экранных пикселях).
/// Название лежит в полосе над левым верхним углом фрейма шириной во фрейм.
pub fn frame_label_at(scene: &Scene, camera: &Camera, screen: Point) -> Option<NodeId> {
    let world = camera.screen_to_world(screen);
    let reach = LABEL_HIT_PX / camera.zoom();
    // Подпись выше фрейма: ищем фреймы, чей верх ниже точки не дальше полосы.
    let probe = Rect::new(world.x, world.y, world.x, world.y + reach);
    let mut best: Option<(Vec<u32>, NodeId)> = None;
    for id in scene.query_rect(probe) {
        let Some(s) = scene.slot(id) else { continue };
        let n = scene.at(s);
        if n.parent.is_some() || n.props.kind != NodeKind::Frame || !n.props.visible {
            continue;
        }
        if n.props.locked || scene.is_draft(id) {
            continue;
        }
        let Some(b) = world_box(scene, s) else {
            continue;
        };
        if world.x >= b.x0 && world.x <= b.x1 && world.y < b.y0 && world.y >= b.y0 - reach {
            let key = scene.order_key(s);
            if best.as_ref().is_none_or(|(k, _)| key > *k) {
                best = Some((key, id));
            }
        }
    }
    best.map(|(_, id)| id)
}

/// Фрейм верхнего уровня под точкой: в нём начинается рамка выделения.
pub fn root_frame_at(scene: &Scene, world: Point, tolerance: f64) -> Option<NodeId> {
    scene
        .hit_test_all(world, tolerance)
        .into_iter()
        .filter_map(|id| scene.slot(id))
        .map(|s| chain(scene, s)[0])
        .find(|&r| scene.at(r).props.kind == NodeKind::Frame && !locked(scene, r))
        .map(|r| scene.at(r).id)
}

/// Самый глубокий фрейм под точкой, куда можно положить узлы: видимый, не
/// заблокированный, не черновик и не из `exclude` (вместе с их потомками).
pub fn drop_frame_at(
    scene: &Scene,
    world: Point,
    tolerance: f64,
    exclude: &HashSet<NodeId>,
) -> Option<NodeId> {
    let excluded = |s: Slot| {
        chain(scene, s)
            .iter()
            .any(|c| exclude.contains(&scene.at(*c).id))
    };
    scene
        .hit_test_all(world, tolerance)
        .into_iter()
        .filter(|id| !scene.is_draft(*id))
        .filter_map(|id| scene.slot(id))
        .filter(|s| !excluded(*s))
        .flat_map(|s| chain(scene, s).into_iter().rev())
        .find(|&s| {
            let n = scene.at(s);
            n.props.kind == NodeKind::Frame
                && !locked(scene, s)
                && !excluded(s)
                && n.geometry().contains(n.world.inverse() * world)
        })
        .map(|s| scene.at(s).id)
}

/// До скольких детей контекста рамка перебирает их напрямую, а не запросом к
/// R-tree: запрос возвращает и всех потомков (рамка через страницу в 100 000 узлов
/// — 100 000 записей, ~14 мс), а выбрать можно только детей контекста.
pub const MARQUEE_SCAN_CHILDREN: usize = 4096;

/// Узлы под рамкой выделения в контексте `context` (дети фрейма или корень
/// страницы): видимые, не заблокированные, пересекающие рамку. Фреймы верхнего
/// уровня выбираются, только если рамка охватывает их целиком: иначе рамка,
/// начатая на холсте, цепляла бы монтажные области, через которые прошла.
/// Порядок — порядок отрисовки, снизу вверх.
///
/// Кандидаты — дети контекста перебором, если их не больше
/// `MARQUEE_SCAN_CHILDREN`, иначе запрос к R-tree по рамке. Оба пути
/// ограничены: перебор — числом детей, запрос — числом узлов под рамкой.
pub fn marquee(scene: &Scene, rect: Rect, context: Option<NodeId>) -> Vec<NodeId> {
    let context_slot = match context {
        Some(c) => match scene.slot(c) {
            Some(s) => Some(s),
            None => return Vec::new(),
        },
        None => None,
    };
    let list = match context_slot {
        Some(s) => &scene.at(s).children[..],
        None => scene.roots(),
    };
    let candidates: Vec<Slot> = if list.len() <= MARQUEE_SCAN_CHILDREN {
        list.to_vec()
    } else {
        scene
            .query_rect(rect)
            .into_iter()
            .filter_map(|id| scene.slot(id))
            .collect()
    };
    let mut found: Vec<(Vec<u32>, NodeId)> = candidates
        .into_iter()
        .map(|s| (s, scene.at(s).id))
        .filter(|(_, id)| !scene.is_draft(*id))
        .filter(|(s, _)| scene.at(*s).parent == context_slot)
        .filter(|(s, _)| scene.is_visible(*s) && !locked(scene, *s))
        .filter(|(s, _)| {
            let Some(b) = world_box(scene, *s) else {
                return false;
            };
            let root_frame = context.is_none() && scene.at(*s).props.kind == NodeKind::Frame;
            if root_frame {
                rect.contains(b.origin()) && rect.contains(Point::new(b.x1, b.y1))
            } else {
                rect.overlaps(b) || rect.contains(b.origin())
            }
        })
        .map(|(s, id)| (scene.order_key(s), id))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.into_iter().map(|(_, id)| id).collect()
}
````

- [ ] **Step 6: Пресеты фреймов**

`crates/ferrum-editor/assets/frame_presets.json`:

````json
{
  "version": 1,
  "presets": [
    { "group": "Телефоны", "name": "iPhone 16", "width": 393, "height": 852 },
    { "group": "Телефоны", "name": "iPhone 16 Pro Max", "width": 440, "height": 956 },
    { "group": "Телефоны", "name": "Android Compact", "width": 412, "height": 917 },
    { "group": "Планшеты", "name": "iPad mini", "width": 744, "height": 1133 },
    { "group": "Планшеты", "name": "iPad Pro 11\"", "width": 834, "height": 1194 },
    { "group": "Десктоп", "name": "Desktop", "width": 1440, "height": 1024 },
    { "group": "Десктоп", "name": "MacBook Air", "width": 1280, "height": 832 },
    { "group": "Десктоп", "name": "1920×1080", "width": 1920, "height": 1080 },
    { "group": "Соцсети", "name": "Instagram post", "width": 1080, "height": 1350 },
    { "group": "Соцсети", "name": "Story", "width": 1080, "height": 1920 },
    { "group": "Бумага", "name": "A4", "width": 595, "height": 842 }
  ]
}
````

`crates/ferrum-editor/src/presets.rs`:

````rust
//! Пресеты размеров фреймов. Список — отдельный JSON (`assets/frame_presets.json`),
//! вшитый в крейт: он дополняется без изменения кода.

use std::sync::OnceLock;

use serde::Deserialize;

/// Наибольшая сторона пресета: больше не бывает ни у экранов, ни у бумаги.
pub const MAX_PRESET_SIDE: f64 = 100_000.0;

const BUILTIN: &str = include_str!("../assets/frame_presets.json");

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FramePreset {
    /// Раздел списка в интерфейсе («Телефоны», «Планшеты»…).
    pub group: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Deserialize)]
struct PresetFile {
    version: u32,
    presets: Vec<FramePreset>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PresetError {
    #[error("Не удалось прочитать список пресетов: {0}")]
    Parse(String),
    #[error("Список пресетов из более новой версии Ferrum ({0})")]
    NewerVersion(u32),
    #[error("Пресет «{0}»: размеры должны быть от 1 до 100000")]
    BadSize(String),
}

/// Разбирает и проверяет список пресетов.
pub fn parse_presets(json: &str) -> Result<Vec<FramePreset>, PresetError> {
    let file: PresetFile =
        serde_json::from_str(json).map_err(|e| PresetError::Parse(e.to_string()))?;
    if file.version > 1 {
        return Err(PresetError::NewerVersion(file.version));
    }
    for p in &file.presets {
        let ok = |v: f64| v.is_finite() && (1.0..=MAX_PRESET_SIDE).contains(&v);
        if !ok(p.width) || !ok(p.height) {
            return Err(PresetError::BadSize(p.name.clone()));
        }
    }
    Ok(file.presets)
}

/// Встроенный список (раздел 7.5 спеки).
pub fn frame_presets() -> &'static [FramePreset] {
    static LIST: OnceLock<Vec<FramePreset>> = OnceLock::new();
    LIST.get_or_init(|| parse_presets(BUILTIN).expect("встроенный список пресетов корректен"))
}
````

`crates/ferrum-editor/src/lib.rs`:

````rust
//! Редактор Ferrum без браузера: инструменты, выделение, привязки, команды и
//! горячие клавиши, буфер обмена. Получает события ввода простыми структурами,
//! правит документ транзакциями `ferrum-doc`, держит сцену с превью жеста, камеру и
//! оверлей для `ferrum-render`. Не зависит ни от браузера, ни от ОС и детерминирован:
//! одна и та же последовательность событий даёт один и тот же документ.

pub mod commands;
pub mod geometry;
pub mod input;
pub mod keymap;
pub mod presets;
pub mod selection;
pub mod snap;

pub use commands::{Command, CommandId, COMMANDS};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, Conflict, Keymap, KeymapError, Shortcut};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
````

- [ ] **Step 7: Запустить тесты**

Run: `cargo test -p ferrum-editor --test geometry`
Expected: PASS, `7 passed`.

- [ ] **Step 8: Закоммитить**

```bash
git add crates/ferrum-editor
git commit -m "feat(editor): геометрия жестов, привязки, правила выделения, пресеты фреймов"
```

---

### Task 4: Формат буфера обмена

Копия самодостаточна: поддеревья со всеми свойствами и структурой плюс байты изображений, на которые ссылаются заливки, — вставка работает и в другом документе. Формат свой и версионированный: `FERRUMCB` · версия · длина JSON · JSON · изображения длиной и байтами. JSON выбран ради читаемости и будущей совместимости (неизвестные типы узлов из новых версий пропускаются вместе с поддеревом), байты изображений идут после него без base64. Корни копии хранят мировое положение и поворот: вставка переводит их в координаты нового родителя. Системный буфер — забота интерфейса (этап 5): редактор отдаёт и принимает байты.

Данные не доверенные: общий размер, число узлов, изображений, заливок и длина имени ограничены; ссылка на родителя — только назад по списку; цвета, режимы и хэши проверяются; числа приводятся к пределам сцены (`shape::sanitize`), у линии высота нулевая; лишние или недостающие байты — ошибка.

**Files:**
- Create: `crates/ferrum-editor/src/clipboard.rs`
- Modify: `crates/ferrum-editor/src/lib.rs`
- Test: `crates/ferrum-editor/tests/clipboard_format.rs`

**Interfaces:**
- Consumes: `Props`, `Fill`, `Stroke`, `Color::{parse, to_hex}`, `NodeKind::{as_str, parse}`, `shape::sanitize`.
- Produces: `ClipPayload { nodes: Vec<ClipNode { parent, props }>, images: Vec<(hash, bytes)>, bounds }` с `encode` и `decode`; `ClipboardError { NotFerrum, NewerVersion, Corrupted, TooLarge, Empty }`; `full_patch(&Props)`; пределы `MAX_PAYLOAD`, `MAX_NODES`, `MAX_IMAGES`, `MAX_PAINTS`, `MAX_NAME`.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-editor/tests/clipboard_format.rs`:

````rust
//! Формат буфера обмена Ferrum: кодирование и разбор не доверенных данных.

use ferrum_doc::{Color, Fill, NodeKind, Props, Stroke, StrokeAlign};
use ferrum_editor::clipboard::*;
use kurbo::Rect;

fn sample() -> ClipPayload {
    let mut frame = Props::defaults(NodeKind::Frame);
    frame.x = 10.0;
    frame.rotation = 15.0;
    let mut pic = Props::defaults(NodeKind::Image);
    pic.fills = vec![Fill::image("ab".repeat(32))];
    pic.strokes = vec![Stroke {
        color: Color::rgba(1, 2, 3, 4),
        width: 2.0,
        align: StrokeAlign::Outside,
    }];
    ClipPayload {
        nodes: vec![
            ClipNode {
                parent: None,
                props: frame,
            },
            ClipNode {
                parent: Some(0),
                props: pic,
            },
            ClipNode {
                parent: None,
                props: Props::defaults(NodeKind::Line),
            },
        ],
        images: vec![("ab".repeat(32), vec![1, 2, 3])],
        bounds: Rect::new(10.0, 0.0, 110.0, 100.0),
    }
}

/// Байты с заменённым JSON (без изображений).
fn with_json(json: &str) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(json.as_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

fn json_of(bytes: &[u8]) -> String {
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    String::from_utf8(bytes[16..16 + len].to_vec()).unwrap()
}

#[test]
fn encode_decode_round_trip() {
    let p = sample();
    let bytes = p.encode();
    assert_eq!(&bytes[..8], MAGIC);
    assert_eq!(ClipPayload::decode(&bytes).unwrap(), p);
}

#[test]
fn structural_damage_is_rejected() {
    let good = sample().encode();
    let mut newer = good.clone();
    newer[8..12].copy_from_slice(&(VERSION + 1).to_le_bytes());
    let mut count = good.clone();
    let at = 16 + json_of(&good).len();
    count[at..at + 4].copy_from_slice(&9u32.to_le_bytes());
    let mut huge_len = good.clone();
    huge_len[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        ClipPayload::decode(b"garbage"),
        Err(ClipboardError::NotFerrum)
    );
    assert_eq!(
        ClipPayload::decode(&good[..7]),
        Err(ClipboardError::NotFerrum)
    );
    assert_eq!(
        ClipPayload::decode(&newer),
        Err(ClipboardError::NewerVersion(VERSION + 1))
    );
    for (bytes, what) in [
        (good[..good.len() - 1].to_vec(), "обрыв"),
        ([good.clone(), vec![0]].concat(), "лишние байты"),
        (count, "число изображений"),
        (huge_len, "длина JSON за концом"),
        (with_json("{\"nodes\":1}"), "не тот JSON"),
    ] {
        assert!(
            matches!(
                ClipPayload::decode(&bytes),
                Err(ClipboardError::Corrupted(_))
            ),
            "{what}"
        );
    }
}

#[test]
fn hostile_values_are_rejected_or_clamped() {
    // Без байтов изображений: `with_json` пишет пустой список картинок.
    let mut base = sample();
    base.images.clear();
    let json = json_of(&base.encode());
    let bad = |from: &str, to: &str| {
        assert!(json.contains(from), "{from}");
        ClipPayload::decode(&with_json(&json.replacen(from, to, 1)))
    };
    let corrupt =
        |r: Result<ClipPayload, ClipboardError>| matches!(r, Err(ClipboardError::Corrupted(_)));
    assert!(
        corrupt(bad("\"parent\":0", "\"parent\":2")),
        "ссылка вперёд"
    );
    assert!(
        corrupt(bad("\"parent\":0", "\"parent\":1")),
        "ссылка на себя"
    );
    assert!(corrupt(bad("#ffffffff", "#nothex")), "цвет");
    assert!(corrupt(bad(
        "\"scaleMode\":\"fill\"",
        "\"scaleMode\":\"stretch\""
    )));
    assert!(corrupt(bad(
        "\"align\":\"outside\"",
        "\"align\":\"middle\""
    )));
    assert!(corrupt(bad(&"ab".repeat(32), "../../etc/passwd")), "хэш");
    assert!(
        corrupt(bad("\"bounds\":[", "\"bounds\":[1e999,")),
        "границы"
    );
    // Огромные числа приводятся к пределам сцены, линия остаётся без высоты.
    let p =
        ClipPayload::decode(&with_json(&json.replacen("\"x\":10.0", "\"x\":1e300", 1))).unwrap();
    assert_eq!(p.nodes[0].props.x, ferrum_scene::shape::COORD_LIMIT);
    let p = ClipPayload::decode(&with_json(&json.replace("\"h\":0.0", "\"h\":50.0"))).unwrap();
    assert_eq!(p.nodes[2].props.h, 0.0);
    // Длинное имя усекается.
    let long = "я".repeat(5000);
    let p = ClipPayload::decode(&with_json(&json.replacen(
        "\"Фрейм\"",
        &format!("\"{long}\""),
        1,
    )))
    .unwrap();
    assert_eq!(p.nodes[0].props.name.chars().count(), MAX_NAME);
    // Неизвестный тип пропускается вместе с поддеревом, индексы пересчитываются.
    let p = ClipPayload::decode(&with_json(&json.replacen(
        "\"kind\":\"frame\"",
        "\"kind\":\"star\"",
        1,
    )))
    .unwrap();
    assert_eq!(p.nodes.len(), 1);
    assert_eq!(p.nodes[0].props.kind, NodeKind::Line);
    // Ничего не осталось — нечего вставлять.
    assert_eq!(
        ClipPayload::decode(&with_json(
            "{\"nodes\":[],\"images\":[],\"bounds\":[0,0,1,1]}"
        )),
        Err(ClipboardError::Empty)
    );
}

#[test]
fn limits_are_enforced() {
    let mut p = sample();
    p.nodes[1].props.fills = vec![Fill::solid(Color::BLACK); MAX_PAINTS + 1];
    assert!(matches!(
        ClipPayload::decode(&p.encode()),
        Err(ClipboardError::TooLarge(_))
    ));
    let big = vec![0u8; MAX_PAYLOAD + 1];
    assert!(matches!(
        ClipPayload::decode(&big),
        Err(ClipboardError::TooLarge(_))
    ));
}
````

- [ ] **Step 2: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-editor --test clipboard_format`
Expected: ошибка компиляции — нет модуля `clipboard`.

- [ ] **Step 3: Реализовать формат**

`crates/ferrum-editor/src/clipboard.rs`:

````rust
//! Свой формат буфера обмена (`web application/x-ferrum`): самодостаточная копия
//! поддеревьев со свойствами, структурой и байтами изображений. Системный буфер —
//! забота интерфейса (этап 5); редактор отдаёт и принимает байты.
//!
//! Раскладка: `FERRUMCB` · версия (u32 LE) · длина JSON (u32 LE) · JSON · число
//! изображений (u32 LE) · для каждого: длина (u32 LE) и байты. JSON описывает узлы в
//! порядке обхода (родитель раньше детей) и хэши изображений в порядке их байтов.
//! У узлов верхнего уровня положение и поворот — мировые, у остальных — как в
//! документе, относительно родителя.
//!
//! Вставка не доверяет данным: размеры и количества ограничены, числа проверяются,
//! ссылки на родителя — только назад, хэш изображения сверяется с байтами.
//! Узлы неизвестного типа (из будущих версий) пропускаются вместе с поддеревом.

use ferrum_doc::{
    Color, Fill, FillKind, NodeKind, Props, PropsPatch, ScaleMode, Stroke, StrokeAlign,
};
use ferrum_scene::shape;
use kurbo::Rect;
use serde::{Deserialize, Serialize};

pub const MAGIC: &[u8; 8] = b"FERRUMCB";
pub const VERSION: u32 = 1;
/// Пределы для не доверенных данных.
pub const MAX_PAYLOAD: usize = 256 * 1024 * 1024;
pub const MAX_NODES: usize = 200_000;
pub const MAX_IMAGES: usize = 4096;
pub const MAX_PAINTS: usize = 64;
pub const MAX_NAME: usize = 1024;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ClipboardError {
    #[error("В буфере обмена не данные Ferrum")]
    NotFerrum,
    #[error("Данные из более новой версии Ferrum ({0}). Обновите Ferrum")]
    NewerVersion(u32),
    #[error("Данные в буфере обмена повреждены: {0}")]
    Corrupted(String),
    #[error("Слишком большая вставка: {0}")]
    TooLarge(&'static str),
    #[error("Нечего вставить")]
    Empty,
}

fn corrupted(s: impl Into<String>) -> ClipboardError {
    ClipboardError::Corrupted(s.into())
}

/// Узел копии: родитель — индекс раньше в списке.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipNode {
    pub parent: Option<usize>,
    pub props: Props,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipPayload {
    pub nodes: Vec<ClipNode>,
    /// Заявленный хэш и байты изображения.
    pub images: Vec<(String, Vec<u8>)>,
    /// Общая рамка узлов верхнего уровня в мире на момент копирования.
    pub bounds: Rect,
}

#[derive(Serialize, Deserialize)]
struct JsonPayload {
    nodes: Vec<JsonNode>,
    images: Vec<String>,
    bounds: [f64; 4],
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonNode {
    parent: Option<usize>,
    kind: String,
    name: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rotation: f64,
    opacity: f64,
    visible: bool,
    locked: bool,
    corner_radius: f64,
    clip_content: bool,
    fills: Vec<JsonFill>,
    strokes: Vec<JsonStroke>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonFill {
    kind: String,
    color: String,
    image_hash: Option<String>,
    scale_mode: String,
    opacity: f64,
    visible: bool,
}

#[derive(Serialize, Deserialize)]
struct JsonStroke {
    color: String,
    width: f64,
    align: String,
}

fn fill_kind(k: FillKind) -> &'static str {
    match k {
        FillKind::Solid => "solid",
        FillKind::Image => "image",
    }
}

fn scale_mode(m: ScaleMode) -> &'static str {
    match m {
        ScaleMode::Fill => "fill",
        ScaleMode::Fit => "fit",
        ScaleMode::Tile => "tile",
    }
}

fn align(a: StrokeAlign) -> &'static str {
    match a {
        StrokeAlign::Inside => "inside",
        StrokeAlign::Center => "center",
        StrokeAlign::Outside => "outside",
    }
}

impl ClipPayload {
    pub fn encode(&self) -> Vec<u8> {
        let json = JsonPayload {
            nodes: self
                .nodes
                .iter()
                .map(|n| {
                    let p = &n.props;
                    JsonNode {
                        parent: n.parent,
                        kind: p.kind.as_str().to_string(),
                        name: p.name.clone(),
                        x: p.x,
                        y: p.y,
                        w: p.w,
                        h: p.h,
                        rotation: p.rotation,
                        opacity: p.opacity,
                        visible: p.visible,
                        locked: p.locked,
                        corner_radius: p.corner_radius,
                        clip_content: p.clip_content,
                        fills: p
                            .fills
                            .iter()
                            .map(|f| JsonFill {
                                kind: fill_kind(f.kind).to_string(),
                                color: f.color.to_hex(),
                                image_hash: f.image_hash.clone(),
                                scale_mode: scale_mode(f.scale_mode).to_string(),
                                opacity: f.opacity,
                                visible: f.visible,
                            })
                            .collect(),
                        strokes: p
                            .strokes
                            .iter()
                            .map(|s| JsonStroke {
                                color: s.color.to_hex(),
                                width: s.width,
                                align: align(s.align).to_string(),
                            })
                            .collect(),
                    }
                })
                .collect(),
            images: self.images.iter().map(|(h, _)| h.clone()).collect(),
            bounds: [
                self.bounds.x0,
                self.bounds.y0,
                self.bounds.x1,
                self.bounds.y1,
            ],
        };
        let text = serde_json::to_vec(&json).expect("сериализация копии");
        let mut out = Vec::with_capacity(
            24 + text.len() + self.images.iter().map(|(_, b)| b.len() + 4).sum::<usize>(),
        );
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(&text);
        out.extend_from_slice(&(self.images.len() as u32).to_le_bytes());
        for (_, bytes) in &self.images {
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<ClipPayload, ClipboardError> {
        if bytes.len() > MAX_PAYLOAD {
            return Err(ClipboardError::TooLarge("больше 256 МБ"));
        }
        let mut r = Reader { bytes, at: 0 };
        if r.take(8).ok() != Some(MAGIC.as_slice()) {
            return Err(ClipboardError::NotFerrum);
        }
        let version = r.u32()?;
        if version > VERSION {
            return Err(ClipboardError::NewerVersion(version));
        }
        let len = r.u32()? as usize;
        let text = r.take(len)?;
        let json: JsonPayload =
            serde_json::from_slice(text).map_err(|e| corrupted(format!("описание узлов: {e}")))?;
        if json.nodes.len() > MAX_NODES {
            return Err(ClipboardError::TooLarge("больше 200 000 узлов"));
        }
        if json.images.len() > MAX_IMAGES {
            return Err(ClipboardError::TooLarge("больше 4096 изображений"));
        }
        let count = r.u32()? as usize;
        if count != json.images.len() {
            return Err(corrupted("число изображений не совпадает"));
        }
        let mut images = Vec::with_capacity(count);
        for hash in json.images {
            let n = r.u32()? as usize;
            images.push((hash, r.take(n)?.to_vec()));
        }
        if r.at != bytes.len() {
            return Err(corrupted("лишние байты в конце"));
        }
        let b = json.bounds;
        if !b.iter().all(|v| v.is_finite()) {
            return Err(corrupted("границы"));
        }
        // Узел неизвестного типа и его потомки пропускаются; индексы родителей
        // пересчитываются под оставшиеся узлы.
        let mut kept: Vec<Option<usize>> = Vec::with_capacity(json.nodes.len());
        let mut nodes = Vec::new();
        for (i, n) in json.nodes.into_iter().enumerate() {
            let parent = match n.parent {
                None => None,
                Some(p) if p < i => match kept[p] {
                    Some(k) => Some(k),
                    None => {
                        kept.push(None);
                        continue;
                    }
                },
                Some(_) => return Err(corrupted("ссылка на родителя вперёд")),
            };
            let Some(kind) = NodeKind::parse(&n.kind) else {
                kept.push(None);
                continue;
            };
            let props = props_from_json(kind, n)?;
            kept.push(Some(nodes.len()));
            nodes.push(ClipNode { parent, props });
        }
        if nodes.is_empty() {
            return Err(ClipboardError::Empty);
        }
        Ok(ClipPayload {
            nodes,
            images,
            bounds: Rect::new(b[0], b[1], b[2], b[3]).abs(),
        })
    }
}

fn props_from_json(kind: NodeKind, n: JsonNode) -> Result<Props, ClipboardError> {
    if n.fills.len() > MAX_PAINTS || n.strokes.len() > MAX_PAINTS {
        return Err(ClipboardError::TooLarge(
            "слишком много заливок или обводок",
        ));
    }
    let color = |s: &str| Color::parse(s).map_err(|_| corrupted("цвет"));
    let mut fills = Vec::with_capacity(n.fills.len());
    for f in n.fills {
        let kind = match f.kind.as_str() {
            "solid" => FillKind::Solid,
            "image" => FillKind::Image,
            _ => return Err(corrupted("вид заливки")),
        };
        let scale_mode = match f.scale_mode.as_str() {
            "fill" => ScaleMode::Fill,
            "fit" => ScaleMode::Fit,
            "tile" => ScaleMode::Tile,
            _ => return Err(corrupted("режим изображения")),
        };
        let image_hash = match f.image_hash {
            Some(h) if h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()) => Some(h),
            Some(_) => return Err(corrupted("хэш изображения")),
            None => None,
        };
        fills.push(Fill {
            kind,
            color: color(&f.color)?,
            image_hash,
            scale_mode,
            opacity: f.opacity,
            visible: f.visible,
        });
    }
    let mut strokes = Vec::with_capacity(n.strokes.len());
    for s in n.strokes {
        let align = match s.align.as_str() {
            "inside" => StrokeAlign::Inside,
            "center" => StrokeAlign::Center,
            "outside" => StrokeAlign::Outside,
            _ => return Err(corrupted("выравнивание обводки")),
        };
        strokes.push(Stroke {
            color: color(&s.color)?,
            width: s.width,
            align,
        });
    }
    let mut props = Props {
        kind,
        name: n.name.chars().take(MAX_NAME).collect(),
        x: n.x,
        y: n.y,
        w: n.w,
        h: n.h,
        rotation: n.rotation,
        opacity: n.opacity,
        visible: n.visible,
        locked: n.locked,
        corner_radius: n.corner_radius,
        clip_content: n.clip_content,
        fills,
        strokes,
    };
    // Нечисловые и огромные значения — к допустимым, как у строк кэша сцены.
    shape::sanitize(&mut props);
    if kind == NodeKind::Line {
        props.h = 0.0;
    }
    Ok(props)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ClipboardError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|e| *e <= self.bytes.len())
            .ok_or_else(|| corrupted("данные обрываются"))?;
        let s = &self.bytes[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, ClipboardError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Полный патч из свойств — для создания копии узла.
pub fn full_patch(p: &Props) -> PropsPatch {
    PropsPatch {
        name: Some(p.name.clone()),
        x: Some(p.x),
        y: Some(p.y),
        w: Some(p.w),
        h: Some(p.h),
        rotation: Some(p.rotation),
        opacity: Some(p.opacity),
        visible: Some(p.visible),
        locked: Some(p.locked),
        corner_radius: Some(p.corner_radius),
        clip_content: Some(p.clip_content),
        fills: Some(p.fills.clone()),
        strokes: Some(p.strokes.clone()),
    }
}
````

`crates/ferrum-editor/src/lib.rs`:

````rust
//! Редактор Ferrum без браузера: инструменты, выделение, привязки, команды и
//! горячие клавиши, буфер обмена. Получает события ввода простыми структурами,
//! правит документ транзакциями `ferrum-doc`, держит сцену с превью жеста, камеру и
//! оверлей для `ferrum-render`. Не зависит ни от браузера, ни от ОС и детерминирован:
//! одна и та же последовательность событий даёт один и тот же документ.

pub mod clipboard;
pub mod commands;
pub mod geometry;
pub mod input;
pub mod keymap;
pub mod presets;
pub mod selection;
pub mod snap;

pub use commands::{Command, CommandId, COMMANDS};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, Conflict, Keymap, KeymapError, Shortcut};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
````

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p ferrum-editor --test clipboard_format`
Expected: PASS, `4 passed`.

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor
git commit -m "feat(editor): свой формат буфера обмена с проверкой данных"
```

---

### Task 5: Редактор и инструменты

`Editor` владеет документом и сценой, а не заимствует их: сцена должна совпадать с документом после каждой правки, отмены и повтора, а это можно гарантировать, только если править документ больше некому. Панели (этап 5) правят через `Editor::transact`, который сам обновляет сцену; рендер получает `scene()`, `camera()` и `overlay()` по ссылке. Инструмент хранится как `Box<dyn Tool>`; на одно событие редактор делит себя на инструмент и контекст `Ctx` (документ, страница, сцена, камера, выделение, наведение, настройки, исходящие события). `Ctx::commit` — единственный путь записи жеста: убирает превью, выполняет транзакцию с выделением «до», заменяет выделение результатом и забирает изменения всех страниц. Счётчик `step_count` растёт только у транзакций, которые создали шаг истории (успешных и с изменениями): по нему тесты проверяют «один жест — один шаг».

«Перемещение» — автомат `Idle → Pressed → Dragging | Marquee | Resizing | Rotating | LineEnd`. При нажатии сначала проверяются ручки (углы раньше середин сторон, сторона рамки ловится по всей длине), затем зона поворота (снаружи угла до 16 px), название фрейма, узел по правилам выделения, пустое место внутри общей рамки нескольких выделенных (тащит их все), иначе — пустой холст. Выделение меняется при нажатии на невыделенный узел; Shift+клик по выделенному и клик по одному из нескольких выделенных — при отпускании без перетаскивания. Двойной клик — не дольше 500 мс и 4 px. Перетаскивание начинается после 3 экранных пикселей. Shift — движение по оси; Alt — копии-черновики двигаются вместо оригиналов и переключаются прямо во время жеста; при отпускании над другим фреймом (самым глубоким под курсором, не из перетаскиваемых), чем родитель узла, за который взялись, узлы с родителем-фреймом или корнем переносятся в него с сохранением положения на экране (из групп узлы не выходят). Ресайз: Shift — пропорции, Alt — от центра, привязка ведёт точку ручки; у выделенной линии вместо рамки — два конца, Shift — шаг 45°. Поворот — вокруг центра рамки, Shift — угол узла (или общий угол нескольких) кратен 15°. Положение и размеры округляются до целых, если включена пиксельная привязка (по умолчанию).

Рисование: фигура — черновик под самым глубоким фреймом в точке нажатия; Shift — квадрат, круг, линия с шагом 45°; Alt — от центра; изображение сохраняет пропорции. Клик без перетаскивания создаёт фигуру 100 × 100 (линию 100, изображение — в исходном размере). После рисования фигура выделена и редактор возвращается к «Перемещению». Рука — H, удержание Space (возвращает прежний инструмент при отпускании) и средняя кнопка поверх любого инструмента. Колесо с Ctrl/Cmd и щипок зумят к курсору (`exp(−deltaY · 0.01)`), без модификатора — панорамируют, Shift+колесо — вбок; если жест идёт, он пересчитывается под новой камерой.

Во время жеста клавиши только меняют модификаторы, Esc отменяет жест; второе нажатие кнопки без отпускания отменяет прежний жест. События интерфейсу шлются только при изменении: выделение, инструмент, курсор, «нужен кадр» (сцена, камера, оверлей), документ изменён, страница, байты буфера, запросы.

Команды здесь — только инструменты, отмена, повтор и Esc; остальные добавит Task 6.

**Files:**
- Create: `crates/ferrum-editor/src/tools/mod.rs`
- Create: `crates/ferrum-editor/src/tools/select.rs`
- Create: `crates/ferrum-editor/src/tools/draw.rs`
- Create: `crates/ferrum-editor/src/tools/hand.rs`
- Create: `crates/ferrum-editor/src/ops.rs`
- Create: `crates/ferrum-editor/src/editor.rs`
- Create: `crates/ferrum-editor/src/actions.rs`
- Modify: `crates/ferrum-editor/src/lib.rs`
- Test: `crates/ferrum-editor/tests/common/mod.rs`
- Test: `crates/ferrum-editor/tests/selection.rs`
- Test: `crates/ferrum-editor/tests/draw.rs`
- Test: `crates/ferrum-editor/tests/transform.rs`

**Interfaces:**
- Consumes: Task 1–4; `Document::{transact, undo, redo, take_changes, pages, page, add_page, images}`, `Restore`, `Scene::{from_page, apply, set_preview, clear_preview, set_drafts, has_preview, change_counter}`, `Camera::{new, zoom_at, pan_by, fit, set_zoom, set_center, world_to_screen, screen_to_world, visible_world_rect}`, `Overlay`, `OverlayTheme`, `HANDLE_SIZE`.
- Produces: `Editor::{new, handle, execute, overlay, cursor, take_events, document, page_doc, page, scene, camera, set_camera, set_viewport, selection, select, set_hover, tool_kind, set_tool, place_image, add_image, add_page, set_page, transact, keymap, keymap_mut, settings, set_settings, set_theme, step_count, is_busy, cancel_gesture}`; `EditorEvent`; `Settings { platform, pixel_snap, snapping, pixel_grid }`; трейт `Tool { kind, pointer_down, pointer_move, pointer_up, key, cancel, is_busy, cursor, overlay }`, `Ctx`, `Flow`, `ToolKind`, `Cursor`, `ToolOverlay`; `SelectTool`, `DrawTool`, `Shape`, `HandTool`.

- [ ] **Step 1: Помощники сценарных тестов**

Стенд: редактор на пустом документе, камера с зумом 1, при которой экранные координаты совпадают с мировыми; события идут с шагом 600 мс, чтобы клики не склеивались в двойные. `assert_one_step` проверяет, что жест дал ровно один шаг истории, а отмена и повтор возвращают документ к состоянию до и после; `assert_scene_clean` — что превью нет и сцена совпадает с построенной заново.

`crates/ferrum-editor/tests/common/mod.rs`:

````rust
//! Общий стенд сценарных тестов: редактор на пустом документе, камера с зумом 1,
//! при которой экранные координаты совпадают с мировыми.
#![allow(dead_code)]

use ferrum_doc::{NodeId, NodeKind, Props, PropsPatch};
use ferrum_editor::*;
use ferrum_render::Camera;
use ferrum_scene::{testing::snapshot, Scene};
use kurbo::{Point, Size, Vec2};

pub const VIEW: Size = Size::new(1000.0, 800.0);

pub struct H {
    pub ed: Editor,
    /// Время событий: каждое следующее позже на 600 мс, чтобы клики не
    /// склеивались в двойные (порог 500 мс) без явного намерения.
    pub t: f64,
}

pub fn patch(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

/// Узлы страницы в порядке обхода: id, родитель, свойства — для сравнения
/// состояний документа до и после отмены.
pub type DocState = Vec<(NodeId, Option<NodeId>, Props)>;

impl H {
    pub fn new() -> H {
        H::with_platform(Platform::Windows)
    }

    pub fn with_platform(platform: Platform) -> H {
        let mut ed = Editor::new(ferrum_doc::Document::new(), None, VIEW, 1.0, platform).unwrap();
        let mut cam = Camera::new(VIEW, 1.0);
        cam.set_center(Point::new(VIEW.width / 2.0, VIEW.height / 2.0));
        ed.set_camera(cam);
        ed.take_events();
        H { ed, t: 1000.0 }
    }

    fn tick(&mut self) -> f64 {
        self.t += 600.0;
        self.t
    }

    pub fn add(&mut self, parent: Option<NodeId>, kind: NodeKind, p: PropsPatch) -> NodeId {
        let sel = self.ed.selection().to_vec();
        let id = self
            .ed
            .transact(|tx| {
                let id = tx.create_node(parent, None, kind)?;
                tx.set_props(id, &p)?;
                Ok(vec![id])
            })
            .unwrap()[0];
        self.ed.select(&sel);
        id
    }

    pub fn rect(&mut self, parent: Option<NodeId>, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        self.add(parent, NodeKind::Rect, patch(x, y, w, h))
    }

    pub fn frame(&mut self, parent: Option<NodeId>, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        self.add(parent, NodeKind::Frame, patch(x, y, w, h))
    }

    pub fn set(&mut self, id: NodeId, p: PropsPatch) {
        let sel = self.ed.selection().to_vec();
        self.ed
            .transact(|tx| {
                tx.set_props(id, &p)?;
                Ok(sel)
            })
            .unwrap();
    }

    pub fn props(&mut self, id: NodeId) -> Props {
        self.ed.page_doc().props(id).unwrap()
    }

    pub fn parent(&mut self, id: NodeId) -> Option<NodeId> {
        self.ed.page_doc().parent(id)
    }

    pub fn children(&mut self, id: Option<NodeId>) -> Vec<NodeId> {
        self.ed.page_doc().children(id)
    }

    pub fn state(&mut self) -> DocState {
        let page = self.ed.page_doc();
        page.walk()
            .into_iter()
            .map(|id| (id, page.parent(id), page.props(id).unwrap()))
            .collect()
    }

    pub fn ev(&mut self, input: Input) {
        self.ed.handle(input);
    }

    pub fn pe(&mut self, x: f64, y: f64, mods: Modifiers) -> PointerEvent {
        let t = self.tick();
        PointerEvent::new((x, y), mods, t)
    }

    pub fn down(&mut self, x: f64, y: f64, mods: Modifiers) {
        let e = self.pe(x, y, mods);
        self.ev(Input::PointerDown(e));
    }

    pub fn mv(&mut self, x: f64, y: f64, mods: Modifiers) {
        let e = self.pe(x, y, mods);
        self.ev(Input::PointerMove(e));
    }

    pub fn up(&mut self, x: f64, y: f64, mods: Modifiers) {
        let e = self.pe(x, y, mods);
        self.ev(Input::PointerUp(e));
    }

    pub fn hover(&mut self, x: f64, y: f64) {
        self.mv(x, y, Modifiers::NONE);
    }

    /// Нажатие, движение в несколько шагов и отпускание.
    pub fn drag(&mut self, from: (f64, f64), to: (f64, f64), mods: Modifiers) {
        self.down(from.0, from.1, mods);
        for i in 1..=4 {
            let k = i as f64 / 4.0;
            self.mv(
                from.0 + (to.0 - from.0) * k,
                from.1 + (to.1 - from.1) * k,
                mods,
            );
        }
        self.up(to.0, to.1, mods);
    }

    /// Нажатие и движение без отпускания.
    pub fn drag_hold(&mut self, from: (f64, f64), to: (f64, f64), mods: Modifiers) {
        self.down(from.0, from.1, mods);
        self.mv((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0, mods);
        self.mv(to.0, to.1, mods);
    }

    pub fn click(&mut self, x: f64, y: f64, mods: Modifiers) {
        self.down(x, y, mods);
        self.up(x, y, mods);
    }

    pub fn dbl(&mut self, x: f64, y: f64) {
        self.click(x, y, Modifiers::NONE);
        // Второе нажатие через 50 мс после первого отпускания.
        self.t -= 550.0;
        self.click(x, y, Modifiers::NONE);
    }

    pub fn key(&mut self, code: &str, mods: Modifiers) {
        self.ev(Input::KeyDown(KeyEvent::new(code, mods)));
        self.ev(Input::KeyUp(KeyEvent::new(code, mods)));
    }

    pub fn sel(&self) -> Vec<NodeId> {
        self.ed.selection().to_vec()
    }

    /// Сцена совпадает со сценой, построенной заново по документу, и превью нет.
    pub fn assert_scene_clean(&mut self) {
        assert!(!self.ed.scene().has_preview(), "превью осталось");
        let fresh = Scene::from_page(self.ed.page_doc());
        assert_eq!(snapshot(self.ed.scene()), snapshot(&fresh));
    }

    /// Жест дал ровно один шаг истории: отмена возвращает `before`, повтор — `after`.
    pub fn assert_one_step(&mut self, steps_before: u64, before: &DocState) {
        assert_eq!(self.ed.step_count(), steps_before + 1, "один шаг истории");
        let after = self.state();
        self.ed.execute(CommandId::Undo);
        assert_eq!(&self.state(), before, "отмена вернула документ");
        self.assert_scene_clean();
        self.ed.execute(CommandId::Redo);
        assert_eq!(self.state(), after, "повтор вернул правку");
        self.assert_scene_clean();
    }
}

pub fn ctrl() -> Modifiers {
    Modifiers::CTRL
}

pub fn shift() -> Modifiers {
    Modifiers::SHIFT
}

pub fn alt() -> Modifiers {
    Modifiers::ALT
}

pub fn none() -> Modifiers {
    Modifiers::NONE
}

pub fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

pub fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}
````

- [ ] **Step 2: Написать падающие сценарии**

`crates/ferrum-editor/tests/selection.rs`:

````rust
//! Правила выделения на холсте (раздел 7.2 спеки).

mod common;

use common::*;
use ferrum_doc::{NodeId, NodeKind, PropsPatch};
use ferrum_editor::*;

struct Page {
    f: NodeId,
    g: NodeId,
    a: NodeId,
    b: NodeId,
    r: NodeId,
    s: NodeId,
    rg: NodeId,
    c: NodeId,
}

/// Фрейм F (100, 100, 400×300) с группой G (a, b) и прямоугольником r; в корне —
/// прямоугольник s и группа RG с прямоугольником c.
fn page(h: &mut H) -> Page {
    let f = h.frame(None, 100.0, 100.0, 400.0, 300.0);
    let g = h.add(Some(f), NodeKind::Group, PropsPatch::default());
    let a = h.rect(Some(g), 20.0, 20.0, 40.0, 40.0);
    let b = h.rect(Some(g), 80.0, 20.0, 40.0, 40.0);
    let r = h.rect(Some(f), 200.0, 150.0, 50.0, 50.0);
    let s = h.rect(None, 600.0, 100.0, 50.0, 50.0);
    let rg = h.add(None, NodeKind::Group, PropsPatch::default());
    let c = h.rect(Some(rg), 700.0, 300.0, 40.0, 40.0);
    Page {
        f,
        g,
        a,
        b,
        r,
        s,
        rg,
        c,
    }
}

#[test]
fn click_selects_top_level_object_in_its_context() {
    let mut h = H::new();
    let p = page(&mut h);
    // Ребёнок фрейма верхнего уровня — группа целиком, не её прямоугольник.
    h.click(130.0, 130.0, none());
    assert_eq!(h.sel(), vec![p.g]);
    h.click(320.0, 270.0, none());
    assert_eq!(h.sel(), vec![p.r]);
    // Объект в корне и ребёнок корневой группы — сам объект и группа.
    h.click(620.0, 120.0, none());
    assert_eq!(h.sel(), vec![p.s]);
    h.click(720.0, 320.0, none());
    assert_eq!(h.sel(), vec![p.rg]);
    // Пустое место фрейма с детьми не выбирает фрейм и снимает выделение.
    h.click(450.0, 350.0, none());
    assert!(h.sel().is_empty());
    // Фрейм выбирается кликом по названию над ним.
    h.click(150.0, 92.0, none());
    assert_eq!(h.sel(), vec![p.f]);
    // Пустой холст снимает выделение.
    h.click(900.0, 700.0, none());
    assert!(h.sel().is_empty());
    assert_eq!(h.ed.step_count(), 8, "выделение не пишет в историю");
}

#[test]
fn double_click_goes_deeper_and_context_sticks() {
    let mut h = H::new();
    let p = page(&mut h);
    h.dbl(130.0, 130.0);
    assert_eq!(h.sel(), vec![p.a], "двойной клик — на уровень глубже");
    // Сосед внутри той же группы выбирается обычным кликом.
    h.click(200.0, 140.0, none());
    assert_eq!(h.sel(), vec![p.b]);
    // Объект вне группы — на своём уровне по умолчанию.
    h.click(320.0, 270.0, none());
    assert_eq!(h.sel(), vec![p.r]);
    // Ctrl+клик — самый глубокий объект сразу.
    h.click(130.0, 130.0, ctrl());
    assert_eq!(h.sel(), vec![p.a]);
    h.click(720.0, 320.0, ctrl());
    assert_eq!(h.sel(), vec![p.c]);
}

#[test]
fn shift_click_toggles() {
    let mut h = H::new();
    let p = page(&mut h);
    h.click(320.0, 270.0, none());
    h.click(130.0, 130.0, shift());
    assert_eq!(h.sel(), vec![p.r, p.g]);
    h.click(320.0, 270.0, shift());
    assert_eq!(h.sel(), vec![p.g], "Shift+клик по выделенному убирает его");
    // Клик по одному из нескольких выделенных оставляет только его.
    h.click(620.0, 120.0, shift());
    assert_eq!(h.sel(), vec![p.g, p.s]);
    h.click(620.0, 120.0, none());
    assert_eq!(h.sel(), vec![p.s]);
}

#[test]
fn locked_and_hidden_nodes_are_click_through() {
    let mut h = H::new();
    let p = page(&mut h);
    let top = h.rect(None, 590.0, 90.0, 70.0, 70.0);
    h.set(
        top,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    h.click(620.0, 120.0, none());
    assert_eq!(h.sel(), vec![p.s], "клик прошёл сквозь заблокированный");
    h.set(
        top,
        PropsPatch {
            locked: Some(false),
            visible: Some(false),
            ..Default::default()
        },
    );
    h.hover(620.0, 120.0);
    assert_eq!(h.ed.overlay().hover, Some(p.s));
    h.click(620.0, 120.0, none());
    assert_eq!(h.sel(), vec![p.s], "скрытый не выбирается");
    // Заблокированный предок закрывает всё поддерево.
    h.set(
        p.g,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    // Ctrl+клик ищет самый глубокий выбираемый — это уже сам фрейм под группой.
    h.click(130.0, 130.0, ctrl());
    assert_eq!(h.sel(), vec![p.f]);
}

#[test]
fn hover_follows_selection_rules() {
    let mut h = H::new();
    let p = page(&mut h);
    h.hover(130.0, 130.0);
    assert_eq!(h.ed.overlay().hover, Some(p.g));
    h.hover(720.0, 320.0);
    assert_eq!(h.ed.overlay().hover, Some(p.rg));
    h.hover(450.0, 350.0);
    assert_eq!(h.ed.overlay().hover, None, "пустое место фрейма с детьми");
    h.hover(900.0, 700.0);
    assert_eq!(h.ed.overlay().hover, None);
    let events = h.ed.take_events();
    assert!(events.contains(&EditorEvent::NeedsRedraw));
}

#[test]
fn marquee_selects_in_context() {
    let mut h = H::new();
    let p = page(&mut h);
    // С пустого холста: объекты верхнего уровня; фрейм — только целиком в рамке.
    h.drag((560.0, 60.0), (900.0, 380.0), none());
    assert_eq!(h.sel(), vec![p.s, p.rg]);
    h.drag((50.0, 50.0), (900.0, 380.0), none());
    assert_eq!(h.sel(), vec![p.s, p.rg], "фрейм задет, но не охвачен");
    h.drag((50.0, 50.0), (900.0, 450.0), none());
    assert_eq!(h.sel(), vec![p.f, p.s, p.rg]);
    // Внутри фрейма: его дети, которые задевает рамка. Сначала снять выделение:
    // нажатие внутри общей рамки выделенных тащило бы их.
    h.click(900.0, 700.0, none());
    h.drag((480.0, 380.0), (210.0, 140.0), none());
    assert_eq!(h.sel(), vec![p.g, p.r]);
    // Скрытые и заблокированные не попадают.
    h.set(
        p.r,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    h.drag((480.0, 380.0), (210.0, 140.0), none());
    assert_eq!(h.sel(), vec![p.g]);
    // Shift добавляет к выделению.
    h.click(620.0, 120.0, none());
    h.drag((480.0, 380.0), (210.0, 140.0), shift());
    assert_eq!(h.sel(), vec![p.s, p.g]);
    // Во время рамки оверлей показывает её, после — нет.
    h.drag_hold((480.0, 380.0), (210.0, 140.0), none());
    assert!(h.ed.overlay().marquee.is_some());
    h.up(210.0, 140.0, none());
    assert!(h.ed.overlay().marquee.is_none());
    let _ = p.b;
}

#[test]
fn escape_during_marquee_restores_selection() {
    let mut h = H::new();
    let p = page(&mut h);
    h.click(620.0, 120.0, none());
    h.drag_hold((480.0, 380.0), (210.0, 140.0), shift());
    assert_eq!(h.sel(), vec![p.s, p.g, p.r]);
    h.key("Escape", none());
    assert_eq!(h.sel(), vec![p.s]);
    assert!(!h.ed.is_busy());
}

#[test]
fn deepest_click_uses_cmd_on_mac() {
    let mut h = H::with_platform(Platform::Mac);
    let p = page(&mut h);
    h.click(130.0, 130.0, Modifiers::META);
    assert_eq!(h.sel(), vec![p.a]);
    // Ctrl на macOS — не главная клавиша: обычный клик, и он выбирает соседа a
    // в том же контексте, а не самый глубокий узел под группой.
    h.click(200.0, 140.0, ctrl());
    assert_eq!(h.sel(), vec![p.b]);
    h.click(450.0, 350.0, none());
    h.click(130.0, 130.0, ctrl());
    assert_eq!(h.sel(), vec![p.g], "без контекста — группа целиком");
}
````

`crates/ferrum-editor/tests/draw.rs`:

````rust
//! Инструменты рисования, рука и зум (раздел 7.1 спеки).

mod common;

use common::*;
use ferrum_doc::{FillKind, NodeKind};
use ferrum_editor::*;
use kurbo::Point;

fn only_child(h: &mut H) -> ferrum_doc::NodeId {
    let roots = h.children(None);
    assert_eq!(roots.len(), 1);
    roots[0]
}

#[test]
fn rect_tool_draws_one_step_selects_and_returns_to_move() {
    let mut h = H::new();
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyR", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
    assert_eq!(h.ed.cursor(), Cursor::Crosshair);
    h.drag_hold((100.0, 100.0), (200.0, 150.0), none());
    // Пока кнопка нажата, фигура — черновик сцены и выделена в оверлее.
    assert!(h.children(None).is_empty(), "документ не тронут");
    assert!(h.ed.scene().has_preview());
    let overlay = h.ed.overlay();
    assert_eq!(overlay.selection.len(), 1);
    assert!(h.ed.scene().is_draft(overlay.selection[0]));
    h.up(200.0, 150.0, none());
    let id = only_child(&mut h);
    let p = h.props(id);
    assert_eq!(p.kind, NodeKind::Rect);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 100.0, 50.0));
    assert_eq!(h.sel(), vec![id]);
    assert_eq!(
        h.ed.tool_kind(),
        ToolKind::Move,
        "после рисования — «Перемещение»"
    );
    h.assert_scene_clean();
    let events = h.ed.take_events();
    assert!(events.contains(&EditorEvent::ToolChanged(ToolKind::Move)));
    assert!(events.contains(&EditorEvent::SelectionChanged(vec![id])));
    assert!(events.contains(&EditorEvent::DocumentChanged));
    h.assert_one_step(steps, &before);
}

#[test]
fn modifiers_square_center_and_angle() {
    let mut h = H::new();
    // Shift — квадрат по большей стороне в сторону курсора.
    h.key("KeyR", none());
    h.drag((100.0, 100.0), (180.0, 130.0), shift());
    let a = h.sel()[0];
    let p = h.props(a);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 80.0, 80.0));
    // Alt — от центра; эллипс с Shift — круг.
    h.key("KeyO", none());
    h.drag((300.0, 300.0), (340.0, 320.0), alt());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.kind, NodeKind::Ellipse);
    assert_eq!((p.x, p.y, p.w, p.h), (260.0, 280.0, 80.0, 40.0));
    h.key("KeyO", none());
    h.drag((500.0, 500.0), (440.0, 530.0), shift());
    let p = h.props(h.sel()[0]);
    assert_eq!((p.x, p.y, p.w, p.h), (440.0, 500.0, 60.0, 60.0));
    // Линия с Shift — шаг 45°.
    h.key("KeyL", none());
    h.drag((100.0, 600.0), (200.0, 590.0), shift());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.kind, NodeKind::Line);
    assert_eq!((p.x, p.y, p.rotation), (100.0, 600.0, 0.0));
    assert!(close(p.w, (100.0f64 * 100.0 + 100.0).sqrt()));
    h.key("KeyL", none());
    h.drag((100.0, 700.0), (160.0, 640.0), shift());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.rotation, 45.0, "вверх-вправо — против часовой");
    assert_eq!(p.h, 0.0);
}

#[test]
fn shapes_nest_into_the_frame_under_the_start_point() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 400.0, 300.0);
    h.key("KeyF", none());
    h.drag((150.0, 150.0), (250.0, 250.0), none());
    let inner = h.sel()[0];
    assert_eq!(h.parent(inner), Some(f), "фрейм во фрейме");
    let p = h.props(inner);
    assert_eq!(
        (p.x, p.y, p.w, p.h),
        (50.0, 50.0, 100.0, 100.0),
        "координаты родителя"
    );
    h.key("KeyR", none());
    h.drag((160.0, 160.0), (200.0, 200.0), none());
    let r = h.sel()[0];
    assert_eq!(h.parent(r), Some(inner), "самый глубокий фрейм");
    assert_eq!((h.props(r).x, h.props(r).y), (10.0, 10.0));
    // Вне фрейма — в корень.
    h.key("KeyR", none());
    h.drag((700.0, 100.0), (750.0, 150.0), none());
    assert_eq!(h.parent(h.sel()[0]), None);
    // A — тоже фрейм.
    h.key("KeyA", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Frame);
}

#[test]
fn click_creates_default_size() {
    let mut h = H::new();
    h.key("KeyF", none());
    h.click(300.0, 200.0, none());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.kind, NodeKind::Frame);
    assert_eq!((p.x, p.y, p.w, p.h), (300.0, 200.0, 100.0, 100.0));
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    h.key("KeyL", none());
    h.click(100.0, 100.0, none());
    let p = h.props(h.sel()[0]);
    assert_eq!((p.kind, p.w, p.h), (NodeKind::Line, 100.0, 0.0));
}

#[test]
fn escape_cancels_drawing_without_history() {
    let mut h = H::new();
    let steps = h.ed.step_count();
    h.key("KeyR", none());
    h.drag_hold((100.0, 100.0), (200.0, 200.0), none());
    assert!(h.ed.scene().has_preview());
    h.key("Escape", none());
    assert!(!h.ed.is_busy());
    h.assert_scene_clean();
    assert_eq!(
        h.ed.tool_kind(),
        ToolKind::Rect,
        "первый Esc только отменяет жест"
    );
    h.up(200.0, 200.0, none());
    assert!(
        h.children(None).is_empty(),
        "отпускание после Esc ничего не создаёт"
    );
    h.key("Escape", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert_eq!(h.ed.step_count(), steps);
    assert!(!h.ed.document().can_undo());
    // Потеря указателя тоже отменяет.
    h.key("KeyR", none());
    h.drag_hold((100.0, 100.0), (200.0, 200.0), none());
    h.ev(Input::PointerCancel);
    h.assert_scene_clean();
    assert!(h.children(None).is_empty());
}

#[test]
fn image_tool_places_by_hash_with_aspect() {
    let mut h = H::new();
    let hash = "ab".repeat(32);
    h.key("KeyK", ctrl().with_shift());
    assert!(h.ed.take_events().contains(&EditorEvent::RequestImage));
    h.ed.place_image(&hash, 200.0, 100.0);
    assert_eq!(h.ed.tool_kind(), ToolKind::Image);
    h.click(50.0, 60.0, none());
    let p = h.props(h.sel()[0]);
    assert_eq!(p.kind, NodeKind::Image);
    assert_eq!((p.x, p.y, p.w, p.h), (50.0, 60.0, 200.0, 100.0));
    assert_eq!(p.fills[0].kind, FillKind::Image);
    assert_eq!(p.fills[0].image_hash.as_deref(), Some(hash.as_str()));
    // Перетаскивание сохраняет пропорции картинки.
    h.ed.place_image(&hash, 200.0, 100.0);
    h.drag((300.0, 300.0), (400.0, 380.0), none());
    let p = h.props(h.sel()[0]);
    assert_eq!((p.w, p.h), (160.0, 80.0));
}

#[test]
fn hand_tool_space_hold_and_middle_button_pan() {
    let mut h = H::new();
    let c0 = h.ed.camera().center();
    h.key("KeyH", none());
    assert_eq!(h.ed.cursor(), Cursor::Grab);
    h.drag_hold((500.0, 400.0), (450.0, 380.0), none());
    assert_eq!(h.ed.cursor(), Cursor::Grabbing);
    h.up(450.0, 380.0, none());
    assert_eq!(
        h.ed.camera().center(),
        c0 + v(50.0, 20.0),
        "содержимое едет с курсором"
    );
    // Space держит руку временно.
    h.key("KeyR", none());
    h.ev(Input::KeyDown(KeyEvent::new("Space", none())));
    assert_eq!(h.ed.tool_kind(), ToolKind::Hand);
    let mut repeat = KeyEvent::new("Space", none());
    repeat.repeat = true;
    h.ev(Input::KeyDown(repeat));
    h.drag((500.0, 400.0), (510.0, 400.0), none());
    h.ev(Input::KeyUp(KeyEvent::new("Space", none())));
    assert_eq!(
        h.ed.tool_kind(),
        ToolKind::Rect,
        "после Space — прежний инструмент"
    );
    assert!(h.children(None).is_empty(), "рука ничего не рисует");
    // Средняя кнопка панорамирует поверх любого инструмента.
    let c1 = h.ed.camera().center();
    let mut e = h.pe(100.0, 100.0, none());
    e.button = PointerButton::Middle;
    h.ev(Input::PointerDown(e));
    e.point = Point::new(130.0, 100.0);
    h.ev(Input::PointerMove(e));
    h.ev(Input::PointerUp(e));
    assert_eq!(h.ed.camera().center(), c1 - v(30.0, 0.0));
    assert!(h.children(None).is_empty());
}

#[test]
fn wheel_zooms_at_cursor_and_pans() {
    let mut h = H::new();
    let cursor = Point::new(200.0, 300.0);
    let anchor = h.ed.camera().screen_to_world(cursor);
    h.ev(Input::Wheel(WheelEvent {
        point: cursor,
        delta: v(0.0, -100.0),
        modifiers: ctrl(),
    }));
    let z = h.ed.camera().zoom();
    assert!(close(z, 1f64.exp()), "{z}");
    let after = h.ed.camera().screen_to_world(cursor);
    assert!(
        (after - anchor).hypot() < 1e-9,
        "точка под курсором на месте"
    );
    // Щипок и предел зума.
    h.ev(Input::Pinch {
        point: cursor,
        scale: 1e6,
    });
    assert_eq!(h.ed.camera().zoom(), ferrum_render::MAX_ZOOM);
    // Без Ctrl колесо панорамирует, с Shift — вбок.
    let c = h.ed.camera().center();
    h.ev(Input::Wheel(WheelEvent {
        point: cursor,
        delta: v(0.0, 256.0),
        modifiers: shift(),
    }));
    let d = h.ed.camera().center() - c;
    assert!(close(d.x, 1.0) && close(d.y, 0.0), "{d:?}");
}

#[test]
fn russian_layout_uses_physical_keys() {
    let mut h = H::new();
    // На русской раскладке R печатает «к», O — «щ»; сочетания по физической клавише.
    let mut e = KeyEvent::new("KeyR", none());
    e.key = "к".into();
    h.ev(Input::KeyDown(e));
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
    let mut e = KeyEvent::new("KeyO", none());
    e.key = "щ".into();
    h.ev(Input::KeyDown(e));
    assert_eq!(h.ed.tool_kind(), ToolKind::Ellipse);
}
````

`crates/ferrum-editor/tests/transform.rs`:

````rust
//! Перемещение, копия с Alt, перенос во фрейм, ресайз, поворот, привязки (разделы
//! 7.1–7.3 спеки). Каждый жест — ровно один шаг истории.

mod common;

use common::*;
use ferrum_doc::{NodeKind, PropsPatch};
use ferrum_editor::*;
use ferrum_render::Camera;
use kurbo::Point;

#[test]
fn drag_moves_after_threshold_in_one_step() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let before = h.state();
    let steps = h.ed.step_count();
    // 2 px — это клик, не перетаскивание.
    h.down(120.0, 120.0, none());
    h.mv(122.0, 120.0, none());
    h.up(122.0, 120.0, none());
    assert_eq!(h.sel(), vec![r]);
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    h.drag_hold((120.0, 120.0), (170.0, 140.0), none());
    // Во время перетаскивания меняется только превью сцены.
    assert_eq!(h.props(r).x, 100.0);
    assert_eq!(h.ed.scene().node(r).unwrap().props.x, 150.0);
    h.up(170.0, 140.0, none());
    let p = h.props(r);
    assert_eq!((p.x, p.y), (150.0, 120.0));
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn shift_locks_axis_and_escape_cancels() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.drag((120.0, 120.0), (190.0, 135.0), shift());
    assert_eq!((h.props(r).x, h.props(r).y), (170.0, 100.0));
    h.drag((190.0, 120.0), (200.0, 60.0), shift());
    assert_eq!((h.props(r).x, h.props(r).y), (170.0, 40.0));
    // Esc посреди перетаскивания: превью убрано, истории нет.
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag_hold((190.0, 60.0), (400.0, 400.0), none());
    h.key("Escape", none());
    h.up(400.0, 400.0, none());
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
}

#[test]
fn alt_drag_duplicates_in_one_step() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 600.0, 600.0);
    let g = h.add(Some(f), NodeKind::Group, PropsPatch::default());
    let a = h.rect(Some(g), 100.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    assert_eq!(h.sel(), vec![g]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag_hold((120.0, 120.0), (220.0, 120.0), alt());
    assert_eq!(h.ed.cursor(), Cursor::Duplicate);
    // Оригинал на месте, двигается черновик-копия, он и выделен в оверлее.
    assert_eq!(h.ed.scene().node(a).unwrap().props.x, 100.0);
    let shown = h.ed.overlay().selection;
    assert!(shown.iter().all(|id| h.ed.scene().is_draft(*id)));
    h.up(220.0, 120.0, alt());
    let kids = h.children(Some(f));
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0], g, "копия над оригиналом");
    let copy = kids[1];
    assert_eq!(h.sel(), vec![copy]);
    assert_eq!(h.props(a).x, 100.0);
    let inner = h.children(Some(copy));
    assert_eq!(inner.len(), 1);
    // Сдвигается копия группы, её ребёнок — на своём месте внутри.
    assert_eq!(h.props(copy).x, 100.0);
    assert_eq!(h.props(inner[0]).x, 100.0);
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
}

#[test]
fn releasing_alt_mid_drag_moves_originals() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.drag_hold((120.0, 120.0), (220.0, 120.0), alt());
    assert!(h.ed.scene().has_preview());
    h.ev(Input::KeyUp(KeyEvent::new("AltLeft", none())));
    assert_eq!(h.ed.cursor(), Cursor::Default);
    h.up(220.0, 120.0, none());
    assert_eq!(h.children(None), vec![r]);
    assert_eq!(h.props(r).x, 200.0);
    h.assert_scene_clean();
}

#[test]
fn dragging_into_and_out_of_frames_reparents() {
    let mut h = H::new();
    let f = h.frame(None, 300.0, 100.0, 400.0, 400.0);
    h.set(
        f,
        PropsPatch {
            rotation: Some(90.0),
            ..Default::default()
        },
    );
    // Фрейм повёрнут на 90°: его мир — x ∈ [300, 700], y ∈ [-300, 100].
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let before = h.state();
    let steps = h.ed.step_count();
    h.drag((110.0, 110.0), (510.0, -90.0), none());
    assert_eq!(h.parent(r), Some(f), "во фрейм под курсором");
    // На экране на месте: левый верхний угол (500, -100).
    let n = h.ed.scene().node(r).unwrap();
    let corner = n.world * Point::ZERO;
    assert!(
        (corner - Point::new(500.0, -100.0)).hypot() < 1e-9,
        "{corner:?}"
    );
    assert_eq!(h.props(r).rotation, 270.0, "поворот компенсирует фрейм");
    h.assert_scene_clean();
    h.assert_one_step(steps, &before);
    // Наружу — снова в корень.
    h.drag((510.0, -90.0), (110.0, 610.0), none());
    assert_eq!(h.parent(r), None);
    let p = h.props(r);
    assert!(
        close(p.x, 100.0) && close(p.y, 600.0) && p.rotation == 0.0,
        "{p:?}"
    );
    // Узел внутри группы из группы не выходит.
    let g = h.add(None, NodeKind::Group, PropsPatch::default());
    let c = h.rect(Some(g), 800.0, 600.0, 20.0, 20.0);
    h.click(810.0, 610.0, ctrl());
    assert_eq!(h.sel(), vec![c]);
    h.drag((810.0, 610.0), (510.0, -90.0), none());
    assert_eq!(h.parent(c), Some(g));
}

#[test]
fn resize_handles_modifiers_and_rotated_nodes() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    h.click(150.0, 125.0, none());
    let before = h.state();
    let steps = h.ed.step_count();
    // Правый нижний угол.
    h.hover(200.0, 150.0);
    assert!(matches!(h.ed.cursor(), Cursor::Resize { .. }));
    h.drag((200.0, 150.0), (260.0, 170.0), none());
    let p = h.props(r);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 160.0, 70.0));
    h.assert_one_step(steps, &before);
    // Shift — пропорции; Alt — от центра.
    h.drag((260.0, 170.0), (420.0, 180.0), shift());
    let p = h.props(r);
    assert!(close(p.w / p.h, 160.0 / 70.0), "{p:?}");
    assert_eq!((p.x, p.y), (100.0, 100.0));
    let c = (p.x + p.w / 2.0, p.y + p.h / 2.0);
    h.drag(
        (p.x + p.w, p.y + p.h / 2.0),
        (p.x + p.w + 20.0, p.y + p.h / 2.0),
        alt(),
    );
    let q = h.props(r);
    assert_eq!(q.w, p.w + 40.0, "от центра — обе стороны");
    assert_eq!((q.x + q.w / 2.0, q.y + q.h / 2.0), c);
    // Левый край: правая сторона стоит.
    let right = q.x + q.w;
    h.drag((q.x, q.y + q.h / 2.0), (q.x - 30.0, q.y + 5.0), none());
    let s = h.props(r);
    assert_eq!((s.x + s.w, s.h), (right, q.h));
    assert_eq!(s.x, q.x - 30.0);
    // Повёрнутый узел меняет размер в своих осях.
    let t = h.rect(None, 500.0, 500.0, 100.0, 40.0);
    h.set(
        t,
        PropsPatch {
            rotation: Some(90.0),
            ..Default::default()
        },
    );
    // Повёрнут на 90° против часовой: мир — x ∈ [500, 540], y ∈ [400, 500].
    h.click(520.0, 450.0, none());
    assert_eq!(h.sel(), vec![t]);
    // Ручка «справа» узла теперь сверху (500 + 20, 400).
    h.drag((520.0, 400.0), (520.0, 350.0), none());
    let p = h.props(t);
    assert_eq!((p.w, p.h, p.rotation), (150.0, 40.0, 90.0));
    assert_eq!((p.x, p.y), (500.0, 500.0), "левая сторона на месте");
    h.assert_scene_clean();
}

#[test]
fn group_resize_scales_children_in_one_transaction() {
    let mut h = H::new();
    let g = h.add(None, NodeKind::Group, PropsPatch::default());
    let a = h.rect(Some(g), 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(Some(g), 150.0, 150.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    assert_eq!(h.sel(), vec![g]);
    let before = h.state();
    let steps = h.ed.step_count();
    // Группа 100..200 тянется за правый нижний угол до 300: масштаб 2.
    h.drag((200.0, 200.0), (300.0, 300.0), none());
    let (pa, pb) = (h.props(a), h.props(b));
    assert_eq!((pa.x, pa.y, pa.w, pa.h), (100.0, 100.0, 100.0, 100.0));
    assert_eq!((pb.x, pb.y, pb.w, pb.h), (200.0, 200.0, 100.0, 100.0));
    let pg = h.props(g);
    assert_eq!((pg.x, pg.y), (0.0, 0.0), "группа сама не сдвинулась");
    h.assert_one_step(steps, &before);
}

#[test]
fn multi_selection_resizes_and_rotates_as_one_box() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 300.0, 100.0, 100.0, 100.0);
    h.click(150.0, 150.0, none());
    h.click(350.0, 150.0, shift());
    // Общая рамка 100..400 × 100..200; левый край ведём к 0 — масштаб 4/3.
    h.drag((100.0, 150.0), (0.0, 150.0), none());
    let (pa, pb) = (h.props(a), h.props(b));
    assert_eq!((pa.x, pa.w), (0.0, 133.0), "пиксельная привязка округляет");
    assert_eq!(pb.x + pb.w, 400.0);
    // Поворот общей рамки на 90° за зону у угла.
    let before = h.state();
    let steps = h.ed.step_count();
    let (pa, pb) = (h.props(a), h.props(b));
    let center = Point::new((pa.x + pb.x + pb.w) / 2.0, 150.0);
    // Зона поворота — снаружи правого нижнего угла рамки.
    let start = Point::new(pb.x + pb.w + 8.0, 208.0);
    h.hover(start.x, start.y);
    assert!(
        matches!(h.ed.cursor(), Cursor::Rotate { .. }),
        "{:?}",
        h.ed.cursor()
    );
    let r = (start - center).hypot();
    let a0 = (start - center).atan2();
    let a1 = a0 + std::f64::consts::FRAC_PI_2;
    let end = center + kurbo::Vec2::new(a1.cos(), a1.sin()) * r;
    h.drag((start.x, start.y), (end.x, end.y), shift());
    assert_eq!(h.props(a).rotation, 270.0, "по часовой на экране");
    assert_eq!(h.props(b).rotation, 270.0);
    h.assert_one_step(steps, &before);
}

#[test]
fn rotation_keeps_center_and_shift_snaps_to_15() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 100.0, 50.0);
    h.click(150.0, 125.0, none());
    let center = Point::new(150.0, 125.0);
    let start = Point::new(208.0, 158.0);
    let a0 = (start - center).atan2();
    // Против часовой на экране на ~37° с Shift — до 30°.
    let a1 = a0 - 37f64.to_radians();
    let end = center + kurbo::Vec2::new(a1.cos(), a1.sin()) * (start - center).hypot();
    h.drag((start.x, start.y), (end.x, end.y), shift());
    let p = h.props(r);
    assert_eq!(p.rotation, 30.0);
    let n = h.ed.scene().node(r).unwrap();
    let c = n.world * Point::new(50.0, 25.0);
    assert!((c - center).hypot() < 1.0, "центр на месте: {c:?}");
    h.assert_scene_clean();
}

#[test]
fn line_endpoints_drag_with_45_degree_steps() {
    let mut h = H::new();
    let l = h.add(None, NodeKind::Line, patch(100.0, 100.0, 100.0, 0.0));
    h.click(150.0, 100.0, none());
    assert_eq!(h.sel(), vec![l]);
    h.drag((200.0, 100.0), (300.0, 100.0), none());
    let p = h.props(l);
    assert_eq!((p.x, p.y, p.w, p.rotation), (100.0, 100.0, 200.0, 0.0));
    // Начало ведём вниз от неподвижного конца (300, 100): с Shift — строго вертикально.
    h.drag((100.0, 100.0), (290.0, 290.0), shift());
    let p = h.props(l);
    let len = (10.0f64 * 10.0 + 190.0 * 190.0).sqrt();
    assert!(close(p.x, 300.0) && close(p.y, 100.0 + len), "{p:?}");
    assert!(close(p.w, len));
    assert_eq!(
        p.rotation, 90.0,
        "от начала к концу — вверх, против часовой"
    );
}

#[test]
fn snapping_to_sibling_edges_with_screen_threshold() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 300.0, 300.0, 50.0, 50.0);
    h.click(320.0, 320.0, none());
    // Левый край b (300) ведём к 203: до правого края a (200) — 3 px.
    h.drag_hold((320.0, 320.0), (223.0, 320.0), none());
    assert_eq!(
        h.ed.scene().node(b).unwrap().props.x,
        200.0,
        "привязка к краю"
    );
    let lines = h.ed.overlay().snap_lines;
    assert!(
        lines.iter().any(|l| l.p0.x == 200.0 && l.p1.x == 200.0),
        "{lines:?}"
    );
    h.up(223.0, 320.0, none());
    assert_eq!(h.props(b).x, 200.0);
    assert!(h.ed.overlay().snap_lines.is_empty());
    // 6 px — дальше порога.
    h.drag((220.0, 320.0), (226.0, 320.0), none());
    assert_eq!(h.props(b).x, 206.0);
    // Порог в экранных пикселях: при зуме 2 те же 6 px экрана — 3 px мира, привязка есть.
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(2.0);
    cam.set_center(Point::new(250.0, 250.0));
    h.ed.set_camera(cam);
    let s = |p: Point| h.ed.camera().world_to_screen(p);
    let from = s(Point::new(230.0, 320.0));
    let to = s(Point::new(226.0, 320.0));
    h.drag((from.x, from.y), (to.x, to.y), none());
    assert_eq!(h.props(b).x, 200.0);
    let _ = a;
}

#[test]
fn snapping_to_parent_frame_and_centers() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    let r = h.rect(Some(f), 50.0, 50.0, 40.0, 40.0);
    h.click(60.0, 60.0, none());
    // Центр r (70) ведём к 198: центр фрейма — 200.
    h.drag((60.0, 60.0), (188.0, 60.0), none());
    assert_eq!(h.props(r).x, 180.0);
    // Края соседей вне видимой полосы не мешают: далёкий узел другого родителя.
    let other = h.rect(None, 2.0, 900.0, 10.0, 10.0);
    h.drag((190.0, 60.0), (0.0 + 13.0, 60.0), none());
    assert_eq!(h.props(r).x, 0.0, "левый край фрейма");
    let _ = other;
}

#[test]
fn pixel_snapping_rounds_and_can_be_disabled() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_zoom(4.0);
    cam.set_center(Point::new(125.0, 125.0));
    h.ed.set_camera(cam);
    let s = |h: &H, p: Point| h.ed.camera().world_to_screen(p);
    let (a, b) = (
        s(&h, Point::new(110.0, 110.0)),
        s(&h, Point::new(130.3, 110.6)),
    );
    h.drag((a.x, a.y), (b.x, b.y), none());
    assert_eq!((h.props(r).x, h.props(r).y), (120.0, 101.0));
    let mut settings = h.ed.settings().clone();
    settings.pixel_snap = false;
    settings.snapping = false;
    h.ed.set_settings(settings);
    let (a, b) = (
        s(&h, Point::new(130.0, 110.0)),
        s(&h, Point::new(140.3, 120.6)),
    );
    h.drag((a.x, a.y), (b.x, b.y), none());
    let p = h.props(r);
    assert!(close(p.x, 130.3) && close(p.y, 111.6), "{p:?}");
}

#[test]
fn locked_nodes_do_not_move() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    h.set(
        r,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(h.sel(), vec![r], "выделение заблокированного остаётся");
    let steps = h.ed.step_count();
    h.drag((120.0, 120.0), (300.0, 300.0), none());
    assert_eq!(h.props(r).x, 100.0);
    assert_eq!(h.ed.step_count(), steps);
}
````

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-editor --test selection`
Expected: ошибка компиляции — нет `Editor`, `ToolKind`, `Cursor`.

- [ ] **Step 4: Трейт инструмента и контекст**

`crates/ferrum-editor/src/tools/mod.rs`:

````rust
//! Инструменты холста — конечные автоматы за трейтом `Tool` (раздел 7.1 спеки).
//! Инструмент получает события указателя и модификаторов вместе с контекстом
//! `Ctx` (документ, сцена, камера, выделение) и сам решает, когда записать правку.
//!
//! Правило всех жестов: пока кнопка нажата, изменения живут только в превью сцены
//! (`Scene::set_preview`, черновики); при отпускании записывается одна транзакция —
//! один шаг отмены; Esc убирает превью, и в истории ничего не остаётся.

mod draw;
mod hand;
mod select;

pub use draw::{DrawTool, Shape};
pub use hand::HandTool;
pub use select::SelectTool;

use ferrum_doc::{DocResult, Document, NodeId, PageId, Tx};
use ferrum_render::Camera;
use ferrum_scene::Scene;
use kurbo::{Line, Point, Rect};

use crate::editor::{EditorEvent, Settings};
use crate::input::{Modifiers, PointerEvent};

/// Допуск попадания и порог начала перетаскивания — в экранных пикселях.
pub const HIT_TOLERANCE_PX: f64 = 4.0;
pub const DRAG_THRESHOLD_PX: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Move,
    Frame,
    Rect,
    Ellipse,
    Line,
    Hand,
    Image,
}

/// Курсор над холстом. Углы — в градусах на экране: 0 — горизонтальная стрелка
/// ресайза, 90 — вертикальная; интерфейс подбирает ближайший CSS-курсор.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cursor {
    Default,
    Crosshair,
    Grab,
    Grabbing,
    /// Перетаскивание с Alt создаёт копию.
    Duplicate,
    Resize {
        angle: f64,
    },
    Rotate {
        angle: f64,
    },
}

/// Чем инструмент дополняет оверлей.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolOverlay {
    pub marquee: Option<Rect>,
    pub snap_lines: Vec<Line>,
    /// Выделение на время жеста (рисуемая фигура выделена, пока её тянут).
    pub selection: Option<Vec<NodeId>>,
}

/// Что делать редактору после отпускания кнопки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Stay,
    /// Инструмент закончил работу (фигура нарисована): вернуться к «Перемещению».
    Finished,
}

/// Всё, с чем работает инструмент. Создаётся редактором на одно событие.
pub struct Ctx<'a> {
    pub(crate) doc: &'a mut Document,
    pub(crate) page: PageId,
    pub(crate) scene: &'a mut Scene,
    pub(crate) camera: &'a mut Camera,
    pub(crate) selection: &'a mut Vec<NodeId>,
    pub(crate) hover: &'a mut Option<NodeId>,
    pub(crate) settings: &'a Settings,
    pub(crate) out: &'a mut Outbox,
}

/// Исходящее: события для интерфейса, счётчики.
#[derive(Debug, Default)]
pub(crate) struct Outbox {
    pub events: Vec<EditorEvent>,
    /// Шагов истории, созданных редактором (транзакции с изменениями).
    pub steps: u64,
    next_draft: u32,
}

impl Ctx<'_> {
    pub fn scene(&self) -> &Scene {
        self.scene
    }

    pub fn camera(&self) -> &Camera {
        self.camera
    }

    pub fn selection(&self) -> &[NodeId] {
        self.selection
    }

    pub fn settings(&self) -> &Settings {
        self.settings
    }

    pub(crate) fn to_world(&self, screen: Point) -> Point {
        self.camera.screen_to_world(screen)
    }

    /// 4 экранных пикселя в мировых единицах.
    pub(crate) fn tolerance(&self) -> f64 {
        HIT_TOLERANCE_PX / self.camera.zoom()
    }

    pub(crate) fn view(&self) -> Rect {
        self.camera.visible_world_rect()
    }

    /// Свежий идентификатор черновика. Loro не выдаёт узлам пир `u64::MAX`
    /// (`LoroDoc::set_peer_id` его отвергает), поэтому такой `NodeId` не совпадёт
    /// ни с одним узлом документа.
    pub(crate) fn draft_id(&mut self) -> NodeId {
        draft_id(&mut self.out.next_draft)
    }

    /// Записывает правку одной транзакцией: превью убирается, транзакция
    /// выполняется по выделению «до», выделение заменяется результатом, сцена
    /// получает изменения. `f` видит сцену без превью — состояние документа. При
    /// ошибке документ откатывается сам, а интерфейс получает сообщение.
    pub(crate) fn commit<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(&Tx, &Scene) -> DocResult<Vec<NodeId>>,
    {
        self.scene.clear_preview();
        let scene: &Scene = self.scene;
        let result = self
            .doc
            .transact(self.page, self.selection.clone(), |tx| f(tx, scene));
        let changed = sync(self.doc, self.page, self.scene, self.out);
        match result {
            Ok(sel) => {
                if changed {
                    self.out.steps += 1;
                }
                *self.selection = sel;
                true
            }
            Err(e) => {
                self.out.events.push(EditorEvent::Error(e.to_string()));
                false
            }
        }
    }
}

pub(crate) fn draft_id(next: &mut u32) -> NodeId {
    let n = *next;
    *next = next.wrapping_add(1);
    format!("{n}@{}", u64::MAX)
        .parse()
        .expect("идентификатор черновика корректен")
}

/// Забирает изменения всех страниц и применяет изменения текущей к сцене.
/// Изменения других страниц отбрасываются: их сцена строится заново при
/// переключении. Возвращает, было ли что-то изменено.
pub(crate) fn sync(doc: &mut Document, page: PageId, scene: &mut Scene, out: &mut Outbox) -> bool {
    let mut any = false;
    for (p, _) in doc.pages() {
        let changes = doc.take_changes(p);
        if changes.is_empty() {
            continue;
        }
        any = true;
        if p == page {
            if let Ok(pd) = doc.page(p) {
                scene.apply(&changes, pd);
            }
        }
    }
    if any {
        out.events.push(EditorEvent::DocumentChanged);
    }
    any
}

/// Инструмент холста.
pub trait Tool {
    fn kind(&self) -> ToolKind;
    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent);
    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent);
    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow;
    /// Модификаторы изменились во время жеста (нажали или отпустили Shift, Alt).
    fn key(&mut self, cx: &mut Ctx, modifiers: Modifiers);
    /// Отменить жест: превью убирается, документ не меняется.
    fn cancel(&mut self, cx: &mut Ctx);
    /// Идёт жест (кнопка нажата).
    fn is_busy(&self) -> bool;
    fn cursor(&self) -> Cursor;
    fn overlay(&self) -> ToolOverlay;
}
````

- [ ] **Step 5: Правки для инструментов и команд**

`crates/ferrum-editor/src/ops.rs`:

````rust
//! Правки документа, общие для инструментов и команд: копии поддеревьев, перенос к
//! другому родителю с сохранением положения на экране.

use ferrum_doc::{DocResult, NodeId, PropsPatch, Tx};
use ferrum_scene::Scene;
use kurbo::{Affine, Vec2};

use crate::clipboard::full_patch;
use crate::geometry::{decompose, parent_world};

/// Сдвиг в мире, переведённый в координаты родителя узла (только линейная часть).
pub fn local_delta(parent_world: Affine, d: Vec2) -> Vec2 {
    let c = parent_world.inverse().as_coeffs();
    Vec2::new(c[0] * d.x + c[2] * d.y, c[1] * d.x + c[3] * d.y)
}

/// Положение и поворот узла с мировой матрицей `world` у родителя с мировой
/// матрицей `parent_world` — чтобы после переноса узел остался на месте.
pub fn reposition(parent_world: Affine, world: Affine) -> PropsPatch {
    let pose = decompose(parent_world.inverse() * world, 1.0, 1.0);
    PropsPatch {
        x: Some(pose.x),
        y: Some(pose.y),
        rotation: Some(pose.rotation),
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
````

- [ ] **Step 6: Инструмент «Перемещение»**

`crates/ferrum-editor/src/tools/select.rs`:

````rust
//! Инструмент «Перемещение» (V): выделение, перемещение, рамка выделения, ресайз
//! за восемь ручек (у линии — за концы), поворот за зоной угловых ручек.
//!
//! Состояния: `Idle → Pressed → Dragging | Marquee | Resizing | Rotating | LineEnd`.
//! Из `Pressed` жест начинается, когда указатель ушёл на 3 экранных пикселя; иначе
//! отпускание — это клик.

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind, PropsPatch};
use ferrum_render::HANDLE_SIZE;
use ferrum_scene::Draft;
use kurbo::{Affine, Line, Point, Rect, Vec2};

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay, DRAG_THRESHOLD_PX};
use crate::geometry::{
    bases, normalize_deg, rect_map, resize_rect, rigid_patches, scale_patches, snap_angle,
    top_level, world_box, Base, Handle, SelectionBox,
};
use crate::input::{Modifiers, PointerEvent};
use crate::ops::{duplicate, local_delta, reposition};
use crate::selection::{
    deeper, drop_frame_at, frame_label_at, locked, resolve, root_frame_at, top_hit, Pick,
};
use crate::snap::{Snapper, SNAP_THRESHOLD_PX};

/// Двойной клик: не дольше 500 мс и не дальше 4 px от первого.
pub const DOUBLE_CLICK_MS: f64 = 500.0;
pub const DOUBLE_CLICK_PX: f64 = 4.0;
/// Ручка ловится в радиусе чуть больше её половины; край рамки — в 4 px.
const HANDLE_HIT_PX: f64 = HANDLE_SIZE / 2.0 + 2.0;
const EDGE_HIT_PX: f64 = 4.0;
/// Зона поворота — снаружи угловой ручки, до 16 px от угла.
const ROTATE_HIT_PX: f64 = 16.0;
/// Шаг поворота с Shift.
pub const ROTATE_STEP_DEG: f64 = 15.0;

/// Что оказалось под указателем при нажатии.
#[derive(Debug, Clone, PartialEq)]
enum Target {
    Node {
        id: NodeId,
        /// Изменить выделение при клике без перетаскивания.
        on_click: OnClick,
    },
    Handle(Handle),
    Rotate,
    Empty {
        /// Фрейм верхнего уровня, в котором начата рамка (её контекст).
        context: Option<NodeId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum OnClick {
    Nothing,
    /// Shift+клик по выделенному убирает его из выделения.
    Remove(NodeId),
    /// Клик по одному из нескольких выделенных оставляет только его.
    SelectOnly(NodeId),
}

#[derive(Debug, Clone)]
struct Press {
    screen: Point,
    world: Point,
    target: Target,
}

/// Узел, который двигается: исходное положение и перевод мирового сдвига к осям
/// родителя.
#[derive(Debug, Clone)]
struct Moving {
    id: NodeId,
    x: f64,
    y: f64,
    parent_world: Affine,
}

#[derive(Debug, Clone)]
struct Drag {
    start: Point,
    /// Узлы выделения верхнего уровня (без заблокированных).
    originals: Vec<Moving>,
    /// Копии при перетаскивании с Alt — черновики, двигаются вместо оригиналов.
    copies: Option<Vec<Moving>>,
    box0: Rect,
    snapper: Snapper,
    /// Родитель узла, за который взялись: если курсор отпущен над другим фреймом,
    /// узлы переносятся в него.
    grab_parent: Option<NodeId>,
    delta: Vec2,
}

#[derive(Debug, Clone)]
struct Marquee {
    start: Point,
    context: Option<NodeId>,
    /// Выделение до рамки: с Shift рамка добавляет к нему.
    base: Vec<NodeId>,
    rect: Rect,
}

#[derive(Debug, Clone)]
struct Resize {
    handle: Handle,
    sbox: SelectionBox,
    bases: Vec<Base>,
    snapper: Snapper,
    transform: Affine,
}

#[derive(Debug, Clone)]
struct Rotate {
    pivot: Point,
    start_angle: f64,
    bases: Vec<Base>,
    /// Поворот единственного узла до жеста: с Shift угол узла кратен 15°.
    single: Option<f64>,
    transform: Affine,
    angle_deg: f64,
}

#[derive(Debug, Clone)]
struct LineEnd {
    handle: Handle,
    id: NodeId,
    parent_world: Affine,
    /// Концы линии в мире до жеста.
    p0: Point,
    p1: Point,
    patch: Option<PropsPatch>,
}

#[derive(Debug, Clone)]
enum State {
    Idle,
    Pressed(Press),
    Dragging(Drag),
    Marquee(Marquee),
    Resizing(Resize),
    Rotating(Rotate),
    LineEnd(LineEnd),
}

pub struct SelectTool {
    state: State,
    cursor: Cursor,
    lines: Vec<Line>,
    last: Option<PointerEvent>,
    /// Время и точка последнего клика — для двойного клика.
    last_click: Option<(f64, Point)>,
}

impl Default for SelectTool {
    fn default() -> Self {
        SelectTool::new()
    }
}

/// Что под точкой на рамке выделения: ручка или зона поворота.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BoxHit {
    Handle(Handle),
    Rotate,
}

/// Угол на экране от центра рамки к точке — для курсора ресайза или поворота.
fn screen_angle(cx: &Ctx, sbox: &SelectionBox, p: Point) -> f64 {
    let c = cx.camera.world_to_screen(sbox.center_world());
    let s = cx.camera.world_to_screen(p);
    let v = s - c;
    if v.hypot() < 1e-9 {
        0.0
    } else {
        normalize_deg(v.y.atan2(v.x).to_degrees())
    }
}

impl SelectTool {
    pub fn new() -> SelectTool {
        SelectTool {
            state: State::Idle,
            cursor: Cursor::Default,
            lines: Vec::new(),
            last: None,
            last_click: None,
        }
    }

    /// Единственная выделенная линия.
    fn single_line(cx: &Ctx) -> Option<NodeId> {
        match cx.selection[..] {
            [id] => cx
                .scene
                .node(id)
                .filter(|n| n.props.kind == NodeKind::Line)
                .map(|_| id),
            _ => None,
        }
    }

    /// Ручка или зона поворота под экранной точкой.
    fn box_hit(cx: &Ctx, screen: Point) -> Option<(BoxHit, f64)> {
        let sel = editable(cx, cx.selection);
        if sel.is_empty() {
            return None;
        }
        let sbox = SelectionBox::of(cx.scene, &sel)?;
        let to_screen = |p: Point| cx.camera.world_to_screen(p);
        if Self::single_line(cx).is_some() {
            for h in [Handle::LineStart, Handle::LineEnd] {
                let (fx, fy) = h.frac();
                let p = sbox.point(fx, fy);
                if (to_screen(p) - screen).hypot() <= HANDLE_HIT_PX {
                    return Some((BoxHit::Handle(h), screen_angle(cx, &sbox, p)));
                }
            }
            return None;
        }
        // Сначала ручки (углы раньше середин сторон), потом стороны, потом поворот.
        let mut handles = Handle::BOX;
        handles.sort_by_key(|h| !h.is_corner());
        for h in handles {
            let (fx, fy) = h.frac();
            let p = sbox.point(fx, fy);
            if (to_screen(p) - screen).hypot() <= HANDLE_HIT_PX {
                return Some((BoxHit::Handle(h), screen_angle(cx, &sbox, p)));
            }
        }
        let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let sides = [Handle::Top, Handle::Right, Handle::Bottom, Handle::Left];
        for (i, h) in sides.iter().enumerate() {
            let a = to_screen(sbox.point(corners[i].0, corners[i].1));
            let b = to_screen(sbox.point(corners[(i + 1) % 4].0, corners[(i + 1) % 4].1));
            let t = ((screen - a).dot(b - a) / (b - a).hypot2().max(1e-12)).clamp(0.0, 1.0);
            if (a.lerp(b, t) - screen).hypot() <= EDGE_HIT_PX {
                let (fx, fy) = h.frac();
                return Some((
                    BoxHit::Handle(*h),
                    screen_angle(cx, &sbox, sbox.point(fx, fy)),
                ));
            }
        }
        let inside = {
            let local = sbox.to_world.inverse() * cx.to_world(screen);
            sbox.rect.contains(local)
        };
        if !inside {
            for (fx, fy) in corners {
                let p = sbox.point(fx, fy);
                if (to_screen(p) - screen).hypot() <= ROTATE_HIT_PX {
                    return Some((BoxHit::Rotate, screen_angle(cx, &sbox, p)));
                }
            }
        }
        None
    }

    /// Узел, который выберет клик в точке, с учётом модификаторов.
    fn pick_at(cx: &Ctx, screen: Point, mods: Modifiers, double: bool) -> Option<NodeId> {
        if let Some(frame) = frame_label_at(cx.scene, cx.camera, screen) {
            return Some(frame);
        }
        let world = cx.to_world(screen);
        let hit = top_hit(cx.scene, world, cx.tolerance())?;
        if double {
            if let Some(d) = deeper(cx.scene, hit, cx.selection) {
                return Some(d);
            }
        }
        let pick = if mods.primary(cx.settings.platform) {
            Pick::Deepest
        } else {
            Pick::Normal
        };
        resolve(cx.scene, hit, cx.selection, pick)
    }

    fn update_hover(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        if let Some((hit, angle)) = Self::box_hit(cx, e.point) {
            *cx.hover = None;
            self.cursor = match hit {
                BoxHit::Handle(_) => Cursor::Resize { angle },
                BoxHit::Rotate => Cursor::Rotate { angle },
            };
            return;
        }
        self.cursor = Cursor::Default;
        *cx.hover = Self::pick_at(cx, e.point, e.modifiers, false);
    }

    fn begin(&mut self, cx: &mut Ctx, press: Press, e: &PointerEvent) {
        let world = press.world;
        self.state = match press.target {
            Target::Node { id, .. } => {
                let ids = editable(cx, &top_level(cx.scene, cx.selection));
                let Some(box0) = union_box(cx, &ids) else {
                    self.state = State::Idle;
                    return;
                };
                let originals = moving(cx, &ids);
                let grab_parent = cx
                    .scene
                    .node(id)
                    .and_then(|n| n.parent)
                    .map(|p| cx.scene.at(p).id);
                State::Dragging(Drag {
                    start: world,
                    snapper: Snapper::new(cx.scene, &ids),
                    originals,
                    copies: None,
                    box0,
                    grab_parent,
                    delta: Vec2::ZERO,
                })
            }
            Target::Empty { context } => {
                let base = if e.modifiers.shift {
                    cx.selection.clone()
                } else {
                    Vec::new()
                };
                State::Marquee(Marquee {
                    start: world,
                    context,
                    base,
                    rect: Rect::from_points(world, world),
                })
            }
            Target::Handle(handle @ (Handle::LineStart | Handle::LineEnd)) => {
                let Some(id) = Self::single_line(cx) else {
                    self.state = State::Idle;
                    return;
                };
                let n = cx.scene.node(id).expect("линия в сцене");
                let parent_world = n.parent.map_or(Affine::IDENTITY, |p| cx.scene.at(p).world);
                State::LineEnd(LineEnd {
                    handle,
                    id,
                    parent_world,
                    p0: n.world * Point::ZERO,
                    p1: n.world * Point::new(n.props.w, 0.0),
                    patch: None,
                })
            }
            Target::Handle(handle) => {
                let ids = editable(cx, &top_level(cx.scene, cx.selection));
                let Some(sbox) = SelectionBox::of(cx.scene, &ids) else {
                    self.state = State::Idle;
                    return;
                };
                State::Resizing(Resize {
                    handle,
                    sbox,
                    bases: bases(cx.scene, &ids, true),
                    snapper: Snapper::new(cx.scene, &ids),
                    transform: Affine::IDENTITY,
                })
            }
            Target::Rotate => {
                let ids = editable(cx, &top_level(cx.scene, cx.selection));
                let Some(sbox) = SelectionBox::of(cx.scene, &ids) else {
                    self.state = State::Idle;
                    return;
                };
                let pivot = sbox.center_world();
                let single = match ids[..] {
                    [id] => cx.scene.node(id).map(|n| n.props.rotation),
                    _ => None,
                };
                State::Rotating(Rotate {
                    pivot,
                    start_angle: (world - pivot).atan2(),
                    bases: bases(cx.scene, &ids, false),
                    single,
                    transform: Affine::IDENTITY,
                    angle_deg: 0.0,
                })
            }
        };
    }

    fn update(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let p = cx.to_world(e.point);
        let mods = e.modifiers;
        let threshold = SNAP_THRESHOLD_PX / cx.camera.zoom();
        let view = cx.view();
        self.lines.clear();
        match &mut self.state {
            State::Idle | State::Pressed(_) => {}
            State::Dragging(d) => {
                // Alt включает и выключает копию прямо во время перетаскивания.
                if mods.alt != d.copies.is_some() {
                    if mods.alt {
                        d.copies = Some(make_copies(cx, &d.originals));
                    } else {
                        d.copies = None;
                        cx.scene.set_drafts(&[]);
                    }
                }
                let mut delta = p - d.start;
                let (mut lock_x, mut lock_y) = (false, false);
                if mods.shift {
                    if delta.x.abs() >= delta.y.abs() {
                        delta.y = 0.0;
                        lock_y = true;
                    } else {
                        delta.x = 0.0;
                        lock_x = true;
                    }
                }
                let moved = d.box0 + delta;
                let (mut snapped_x, mut snapped_y) = (false, false);
                if cx.settings.snapping {
                    let xs = [moved.x0, moved.center().x, moved.x1];
                    let ys = [moved.y0, moved.center().y, moved.y1];
                    let s = d.snapper.snap(
                        cx.scene,
                        if lock_x { &[] } else { &xs },
                        if lock_y { &[] } else { &ys },
                        moved,
                        threshold,
                        view,
                    );
                    if let Some(dx) = s.dx {
                        delta.x += dx;
                        snapped_x = true;
                    }
                    if let Some(dy) = s.dy {
                        delta.y += dy;
                        snapped_y = true;
                    }
                    self.lines = s.lines;
                }
                if cx.settings.pixel_snap {
                    if !snapped_x && !lock_x {
                        delta.x = (d.box0.x0 + delta.x).round() - d.box0.x0;
                    }
                    if !snapped_y && !lock_y {
                        delta.y = (d.box0.y0 + delta.y).round() - d.box0.y0;
                    }
                }
                d.delta = delta;
                let movers = d.copies.as_ref().unwrap_or(&d.originals);
                let patches: Vec<(NodeId, PropsPatch)> = movers
                    .iter()
                    .map(|m| {
                        let dl = local_delta(m.parent_world, delta);
                        (
                            m.id,
                            PropsPatch {
                                x: Some(m.x + dl.x),
                                y: Some(m.y + dl.y),
                                ..Default::default()
                            },
                        )
                    })
                    .collect();
                cx.scene.set_preview(&patches);
            }
            State::Marquee(m) => {
                m.rect = Rect::from_points(m.start, p);
                let found = crate::selection::marquee(cx.scene, m.rect, m.context);
                let mut sel = m.base.clone();
                let have: HashSet<NodeId> = sel.iter().copied().collect();
                sel.extend(found.into_iter().filter(|id| !have.contains(id)));
                *cx.selection = sel;
            }
            State::Resizing(r) => {
                let mut p = p;
                let (fx, fy) = r.handle.frac();
                if cx.settings.snapping && r.sbox.axis_aligned() && !mods.shift {
                    let xs = if fx != 0.5 { vec![p.x] } else { vec![] };
                    let ys = if fy != 0.5 { vec![p.y] } else { vec![] };
                    let s = r.snapper.snap(
                        cx.scene,
                        &xs,
                        &ys,
                        Rect::from_points(p, p),
                        threshold,
                        view,
                    );
                    p.x += s.dx.unwrap_or(0.0);
                    p.y += s.dy.unwrap_or(0.0);
                    self.lines = s.lines;
                }
                let local = r.sbox.to_world.inverse() * p;
                let new = resize_rect(r.sbox.rect, r.handle, local, mods.shift, mods.alt);
                r.transform =
                    r.sbox.to_world * rect_map(r.sbox.rect, new) * r.sbox.to_world.inverse();
                let patches = scale_patches(&r.bases, r.transform, cx.settings.pixel_snap);
                cx.scene.set_preview(&patches);
            }
            State::Rotating(r) => {
                let a = (p - r.pivot).atan2() - r.start_angle;
                // На экране ось Y вниз: поворот по часовой — это минус в модели.
                let mut deg = -a.to_degrees();
                if mods.shift {
                    deg = match r.single {
                        Some(rot0) => snap_angle(rot0 + deg, ROTATE_STEP_DEG) - rot0,
                        None => snap_angle(deg, ROTATE_STEP_DEG),
                    };
                }
                r.angle_deg = deg;
                r.transform = Affine::translate(r.pivot.to_vec2())
                    * Affine::rotate(-deg.to_radians())
                    * Affine::translate(-r.pivot.to_vec2());
                let patches = rigid_patches(&r.bases, r.transform, cx.settings.pixel_snap);
                cx.scene.set_preview(&patches);
                let angle = (p - r.pivot).atan2().to_degrees();
                self.cursor = Cursor::Rotate {
                    angle: normalize_deg(angle),
                };
            }
            State::LineEnd(l) => {
                let mut p = p;
                if cx.settings.pixel_snap {
                    p = Point::new(p.x.round(), p.y.round());
                }
                let (mut a, mut b) = match l.handle {
                    Handle::LineStart => (p, l.p1),
                    _ => (l.p0, p),
                };
                if mods.shift {
                    // Шаг 45° вокруг неподвижного конца.
                    let (anchor, free) = if l.handle == Handle::LineStart {
                        (b, a)
                    } else {
                        (a, b)
                    };
                    let v = free - anchor;
                    let ang = snap_angle(v.atan2().to_degrees(), 45.0).to_radians();
                    let len = v.hypot();
                    let snapped = anchor + Vec2::new(ang.cos(), ang.sin()) * len;
                    if l.handle == Handle::LineStart {
                        a = snapped;
                    } else {
                        b = snapped;
                    }
                }
                let patch = line_patch(l.parent_world, a, b);
                cx.scene.set_preview(&[(l.id, patch.clone())]);
                l.patch = Some(patch);
            }
        }
    }

    fn commit(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let state = std::mem::replace(&mut self.state, State::Idle);
        self.lines.clear();
        match state {
            State::Idle => {}
            State::Pressed(press) => {
                cx.scene.clear_preview();
                if let Target::Node { on_click, .. } = &press.target {
                    match on_click {
                        OnClick::Nothing => {}
                        OnClick::Remove(id) => cx.selection.retain(|s| s != id),
                        OnClick::SelectOnly(id) => *cx.selection = vec![*id],
                    }
                }
                self.last_click = Some((e.time_ms, e.point));
            }
            State::Dragging(d) => commit_drag(cx, d, cx.to_world(e.point)),
            State::Marquee(_) => {}
            State::Resizing(r) => {
                let patches = scale_patches(&r.bases, r.transform, cx.settings.pixel_snap);
                commit_patches(cx, patches);
            }
            State::Rotating(r) => {
                if r.angle_deg == 0.0 {
                    cx.scene.clear_preview();
                    return;
                }
                let patches = rigid_patches(&r.bases, r.transform, cx.settings.pixel_snap);
                commit_patches(cx, patches);
            }
            State::LineEnd(l) => match l.patch {
                Some(patch) => commit_patches(cx, vec![(l.id, patch)]),
                None => cx.scene.clear_preview(),
            },
        }
    }
}

/// Узлы, которые жест может менять: есть в сцене, не черновики, не заблокированы.
fn editable(cx: &Ctx, ids: &[NodeId]) -> Vec<NodeId> {
    ids.iter()
        .copied()
        .filter(|id| {
            cx.scene
                .slot(*id)
                .is_some_and(|s| !cx.scene.is_draft(*id) && !locked(cx.scene, s))
        })
        .collect()
}

fn union_box(cx: &Ctx, ids: &[NodeId]) -> Option<Rect> {
    ids.iter()
        .filter_map(|id| cx.scene.slot(*id))
        .filter_map(|s| world_box(cx.scene, s))
        .reduce(|a, b| a.union(b))
}

fn moving(cx: &Ctx, ids: &[NodeId]) -> Vec<Moving> {
    ids.iter()
        .filter_map(|id| {
            let s = cx.scene.slot(*id)?;
            let n = cx.scene.at(s);
            Some(Moving {
                id: *id,
                x: n.props.x,
                y: n.props.y,
                parent_world: n.parent.map_or(Affine::IDENTITY, |p| cx.scene.at(p).world),
            })
        })
        .collect()
}

/// Черновики-копии поддеревьев для перетаскивания с Alt. Возвращает их корни.
fn make_copies(cx: &mut Ctx, originals: &[Moving]) -> Vec<Moving> {
    let mut drafts = Vec::new();
    let mut roots = Vec::new();
    for m in originals {
        let Some(slot) = cx.scene.slot(m.id) else {
            continue;
        };
        let root = cx.draft_id();
        let parent = cx.scene.at(slot).parent.map(|p| cx.scene.at(p).id);
        drafts.push(Draft {
            id: root,
            parent,
            props: cx.scene.at(slot).props.clone(),
        });
        let mut stack = vec![(slot, root)];
        while let Some((src, dst)) = stack.pop() {
            let children = cx.scene.at(src).children.clone();
            for c in children {
                let id = cx.draft_id();
                drafts.push(Draft {
                    id,
                    parent: Some(dst),
                    props: cx.scene.at(c).props.clone(),
                });
                stack.push((c, id));
            }
        }
        roots.push(Moving {
            id: root,
            ..m.clone()
        });
    }
    cx.scene.set_drafts(&drafts);
    roots
}

fn line_patch(parent_world: Affine, a: Point, b: Point) -> PropsPatch {
    let inv = parent_world.inverse();
    let (a, b) = (inv * a, inv * b);
    let v = b - a;
    PropsPatch {
        x: Some(a.x),
        y: Some(a.y),
        w: Some(v.hypot()),
        rotation: Some(normalize_deg(-v.atan2().to_degrees())),
        ..Default::default()
    }
}

fn commit_patches(cx: &mut Ctx, patches: Vec<(NodeId, PropsPatch)>) {
    let sel = cx.selection.clone();
    cx.commit(|tx, _| {
        for (id, patch) in &patches {
            tx.set_props(*id, patch)?;
        }
        Ok(sel)
    });
}

fn commit_drag(cx: &mut Ctx, d: Drag, pointer: Point) {
    let delta = d.delta;
    if d.copies.is_some() {
        // Копии: одна транзакция создаёт их на новом месте и выделяет.
        let ids: Vec<NodeId> = d.originals.iter().map(|m| m.id).collect();
        cx.commit(|tx, scene| duplicate(tx, scene, &ids, delta));
        return;
    }
    // Узлы на тех же местах — менять нечего, шага в истории нет.
    if delta == Vec2::ZERO {
        cx.scene.clear_preview();
        return;
    }
    cx.scene.clear_preview();
    let ids: HashSet<NodeId> = d.originals.iter().map(|m| m.id).collect();
    let target = drop_frame_at(cx.scene, pointer, cx.tolerance(), &ids);
    let reparent = target != d.grab_parent;
    let sel = cx.selection.clone();
    cx.commit(|tx, scene| {
        let shift = Affine::translate(delta);
        // Переносятся узлы, чей родитель — фрейм или корень: из группы при
        // перетаскивании не выходят (группа растёт вместе с ними, как в Figma).
        let movable = |m: &Moving| {
            let Some(s) = scene.slot(m.id) else {
                return false;
            };
            scene
                .at(s)
                .parent
                .is_none_or(|p| scene.at(p).props.kind == NodeKind::Frame)
        };
        let mut moved: Vec<(Vec<u32>, &Moving)> = Vec::new();
        for m in &d.originals {
            if reparent && movable(m) {
                let s = scene.slot(m.id).expect("проверено выше");
                moved.push((scene.order_key(s), m));
            } else {
                let dl = local_delta(m.parent_world, delta);
                tx.set_props(
                    m.id,
                    &PropsPatch {
                        x: Some(m.x + dl.x),
                        y: Some(m.y + dl.y),
                        ..Default::default()
                    },
                )?;
            }
        }
        if !moved.is_empty() {
            moved.sort_by(|a, b| a.0.cmp(&b.0));
            let target_world = target
                .and_then(|t| scene.node(t))
                .map_or(Affine::IDENTITY, |n| n.world);
            let ids: Vec<NodeId> = moved.iter().map(|(_, m)| m.id).collect();
            tx.move_nodes(&ids, target, usize::MAX)?;
            for (_, m) in moved {
                let world = shift * scene.node(m.id).expect("в сцене").world;
                tx.set_props(m.id, &reposition(target_world, world))?;
            }
        }
        Ok(sel)
    });
}

impl Tool for SelectTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Move
    }

    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        self.last = Some(*e);
        let double = self.last_click.is_some_and(|(t, p)| {
            e.time_ms - t <= DOUBLE_CLICK_MS && (e.point - p).hypot() <= DOUBLE_CLICK_PX
        });
        if double {
            self.last_click = None;
        }
        let world = cx.to_world(e.point);
        let shift = e.modifiers.shift;
        let target = if let Some((hit, _)) = Self::box_hit(cx, e.point) {
            match hit {
                BoxHit::Handle(h) => Target::Handle(h),
                BoxHit::Rotate => Target::Rotate,
            }
        } else if let Some(id) = Self::pick_at(cx, e.point, e.modifiers, double) {
            let selected = cx.selection.contains(&id);
            let on_click = if !selected {
                if shift {
                    cx.selection.push(id);
                } else {
                    *cx.selection = vec![id];
                }
                OnClick::Nothing
            } else if shift {
                OnClick::Remove(id)
            } else if cx.selection.len() > 1 {
                OnClick::SelectOnly(id)
            } else {
                OnClick::Nothing
            };
            Target::Node { id, on_click }
        } else if let Some(first) = inside_selection(cx, world) {
            // Пустое место внутри общей рамки нескольких выделенных: тащим их все.
            Target::Node {
                id: first,
                on_click: OnClick::Nothing,
            }
        } else {
            if !shift {
                cx.selection.clear();
            }
            Target::Empty {
                context: root_frame_at(cx.scene, world, cx.tolerance()),
            }
        };
        *cx.hover = None;
        self.state = State::Pressed(Press {
            screen: e.point,
            world,
            target,
        });
    }

    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        self.last = Some(*e);
        if let State::Pressed(press) = &self.state {
            if (e.point - press.screen).hypot() < DRAG_THRESHOLD_PX {
                return;
            }
            let press = press.clone();
            self.begin(cx, press, e);
        }
        match self.state {
            State::Idle => self.update_hover(cx, e),
            _ => self.update(cx, e),
        }
    }

    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow {
        self.last = Some(*e);
        if !matches!(self.state, State::Idle | State::Pressed(_)) {
            self.update(cx, e);
        }
        self.commit(cx, e);
        self.cursor = Cursor::Default;
        Flow::Stay
    }

    fn key(&mut self, cx: &mut Ctx, modifiers: Modifiers) {
        if let Some(mut last) = self.last {
            last.modifiers = modifiers;
            self.last = Some(last);
            if !matches!(self.state, State::Idle | State::Pressed(_)) {
                self.update(cx, &last);
            }
        }
    }

    fn cancel(&mut self, cx: &mut Ctx) {
        if let State::Marquee(m) = &self.state {
            *cx.selection = m.base.clone();
        }
        self.state = State::Idle;
        self.lines.clear();
        self.cursor = Cursor::Default;
        cx.scene.clear_preview();
    }

    fn is_busy(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    fn cursor(&self) -> Cursor {
        match &self.state {
            State::Dragging(d) if d.copies.is_some() => Cursor::Duplicate,
            _ => self.cursor,
        }
    }

    fn overlay(&self) -> ToolOverlay {
        let mut out = ToolOverlay {
            snap_lines: self.lines.clone(),
            ..Default::default()
        };
        match &self.state {
            State::Marquee(m) => out.marquee = Some(m.rect),
            State::Dragging(d) => {
                if let Some(c) = &d.copies {
                    out.selection = Some(c.iter().map(|m| m.id).collect());
                }
            }
            _ => {}
        }
        out
    }
}

/// Первый выделенный узел, если точка внутри общей рамки нескольких выделенных.
fn inside_selection(cx: &Ctx, world: Point) -> Option<NodeId> {
    if cx.selection.len() < 2 {
        return None;
    }
    let ids = editable(cx, cx.selection);
    let b = union_box(cx, &ids)?;
    b.contains(world).then(|| ids[0])
}
````

- [ ] **Step 7: Рисование и рука**

`crates/ferrum-editor/src/tools/draw.rs`:

````rust
//! Рисование фигур: фрейм (F, A), прямоугольник (R), эллипс (O), линия (L) и
//! размещение изображения (Ctrl/Cmd+Shift+K). Пока кнопка нажата, фигура — черновик
//! сцены; при отпускании она создаётся одной транзакцией и выделяется, а редактор
//! возвращается к «Перемещению», как в Figma. Shift — квадрат, круг или линия с
//! шагом 45°; Alt — от центра. Фигура встаёт в самый глубокий фрейм под точкой
//! начала.

use std::collections::HashSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props};
use ferrum_scene::Draft;
use kurbo::{Affine, Point, Rect, Vec2};

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay, DRAG_THRESHOLD_PX};
use crate::clipboard::full_patch;
use crate::geometry::{decompose, snap_angle};
use crate::input::{Modifiers, PointerEvent};
use crate::selection::drop_frame_at;
use crate::snap::{Snapper, SNAP_THRESHOLD_PX};

/// Размер фигуры, созданной кликом без перетаскивания.
pub const CLICK_SIZE: f64 = 100.0;

/// Что рисуем.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Frame,
    Rect,
    Ellipse,
    Line,
    /// Изображение по хэшу байтов (байты кладёт в документ интерфейс) и его
    /// исходный размер: клик ставит его в этом размере, перетаскивание сохраняет
    /// пропорции.
    Image {
        hash: String,
        width: f64,
        height: f64,
    },
}

impl Shape {
    fn kind(&self) -> NodeKind {
        match self {
            Shape::Frame => NodeKind::Frame,
            Shape::Rect => NodeKind::Rect,
            Shape::Ellipse => NodeKind::Ellipse,
            Shape::Line => NodeKind::Line,
            Shape::Image { .. } => NodeKind::Image,
        }
    }
}

#[derive(Debug, Clone)]
struct Start {
    screen: Point,
    world: Point,
    parent: Option<NodeId>,
    parent_world: Affine,
    snapper: Snapper,
}

#[derive(Debug, Clone)]
enum State {
    Idle,
    Pressed(Start),
    Drawing {
        start: Start,
        draft: NodeId,
        props: Props,
    },
}

pub struct DrawTool {
    shape: Shape,
    state: State,
    lines: Vec<kurbo::Line>,
    last: Option<PointerEvent>,
}

impl DrawTool {
    pub fn new(shape: Shape) -> DrawTool {
        DrawTool {
            shape,
            state: State::Idle,
            lines: Vec::new(),
            last: None,
        }
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// Свойства фигуры от точки `a` до точки `b` (в мире).
    fn props_between(&self, start: &Start, a: Point, b: Point, mods: Modifiers) -> Props {
        let kind = self.shape.kind();
        let mut props = Props::defaults(kind);
        if let Shape::Image { hash, .. } = &self.shape {
            props.fills = vec![Fill::image(hash.clone())];
        }
        let inv = start.parent_world.inverse();
        if kind == NodeKind::Line {
            let mut v = b - a;
            if mods.shift {
                let ang = snap_angle(v.atan2().to_degrees(), 45.0).to_radians();
                v = Vec2::new(ang.cos(), ang.sin()) * v.hypot();
            }
            let from = if mods.alt { a - v } else { a };
            let len = if mods.alt { 2.0 * v.hypot() } else { v.hypot() };
            let world = Affine::translate(from.to_vec2()) * Affine::rotate(v.atan2());
            let pose = decompose(inv * world, len, 0.0);
            props.x = pose.x;
            props.y = pose.y;
            props.w = pose.w;
            props.rotation = pose.rotation;
            return props;
        }
        let mut v = b - a;
        let aspect = match &self.shape {
            Shape::Image { width, height, .. } if *width > 0.0 && *height > 0.0 => {
                Some(width / height)
            }
            _ if mods.shift => Some(1.0),
            _ => None,
        };
        if let Some(ratio) = aspect {
            // Сторона по большему смещению, знак — в сторону курсора.
            let w = v.x.abs().max(v.y.abs() * ratio);
            let h = w / ratio;
            v = Vec2::new(w.copysign(v.x), h.copysign(v.y));
        }
        let rect = if mods.alt {
            Rect::from_points(a - v, a + v)
        } else {
            Rect::from_points(a, a + v)
        };
        let pose = decompose(inv * Affine::translate(rect.origin().to_vec2()), 1.0, 1.0);
        props.x = pose.x;
        props.y = pose.y;
        props.rotation = pose.rotation;
        props.w = rect.width();
        props.h = rect.height();
        props
    }

    /// Свойства фигуры, созданной кликом: стандартный размер от точки клика.
    fn click_props(&self, start: &Start) -> Props {
        let (w, h) = match &self.shape {
            Shape::Image { width, height, .. } => (*width, *height),
            Shape::Line => (CLICK_SIZE, 0.0),
            _ => (CLICK_SIZE, CLICK_SIZE),
        };
        let b = start.world + Vec2::new(w, h);
        let mut p = self.props_between(start, start.world, b, Modifiers::NONE);
        if self.shape.kind() == NodeKind::Line {
            return p;
        }
        p.w = w;
        p.h = h;
        p
    }

    fn create(&self, cx: &mut Ctx, parent: Option<NodeId>, props: Props) {
        let kind = props.kind;
        cx.commit(|tx, _| {
            let id = tx.create_node(parent, None, kind)?;
            tx.set_props(id, &full_patch(&props))?;
            Ok(vec![id])
        });
    }

    fn update(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let State::Drawing { start, draft, .. } = &self.state else {
            return;
        };
        let (start, draft) = (start.clone(), *draft);
        let mut p = cx.to_world(e.point);
        self.lines.clear();
        if cx.settings.snapping {
            let threshold = SNAP_THRESHOLD_PX / cx.camera.zoom();
            let s = start.snapper.snap(
                cx.scene,
                &[p.x],
                &[p.y],
                Rect::from_points(start.world, p),
                threshold,
                cx.view(),
            );
            p.x += s.dx.unwrap_or(0.0);
            p.y += s.dy.unwrap_or(0.0);
            self.lines = s.lines;
        }
        if cx.settings.pixel_snap {
            p = Point::new(p.x.round(), p.y.round());
        }
        let props = self.props_between(&start, start.world, p, e.modifiers);
        cx.scene.set_drafts(&[Draft {
            id: draft,
            parent: start.parent,
            props: props.clone(),
        }]);
        self.state = State::Drawing {
            start,
            draft,
            props,
        };
    }
}

impl Tool for DrawTool {
    fn kind(&self) -> ToolKind {
        match self.shape {
            Shape::Frame => ToolKind::Frame,
            Shape::Rect => ToolKind::Rect,
            Shape::Ellipse => ToolKind::Ellipse,
            Shape::Line => ToolKind::Line,
            Shape::Image { .. } => ToolKind::Image,
        }
    }

    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        self.last = Some(*e);
        let mut world = cx.to_world(e.point);
        if cx.settings.pixel_snap {
            world = Point::new(world.x.round(), world.y.round());
        }
        let parent = drop_frame_at(cx.scene, world, cx.tolerance(), &HashSet::new());
        let parent_world = parent
            .and_then(|p| cx.scene.node(p))
            .map_or(Affine::IDENTITY, |n| n.world);
        *cx.hover = None;
        self.state = State::Pressed(Start {
            screen: e.point,
            world,
            parent,
            parent_world,
            snapper: Snapper::for_parent(cx.scene, parent, &[]),
        });
    }

    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        self.last = Some(*e);
        if let State::Pressed(start) = &self.state {
            if (e.point - start.screen).hypot() < DRAG_THRESHOLD_PX {
                return;
            }
            let start = start.clone();
            let draft = cx.draft_id();
            let props = Props::defaults(self.shape.kind());
            self.state = State::Drawing {
                start,
                draft,
                props,
            };
        }
        self.update(cx, e);
    }

    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow {
        self.last = Some(*e);
        self.update(cx, e);
        let state = std::mem::replace(&mut self.state, State::Idle);
        self.lines.clear();
        match state {
            State::Idle => Flow::Stay,
            State::Pressed(start) => {
                let props = self.click_props(&start);
                self.create(cx, start.parent, props);
                Flow::Finished
            }
            State::Drawing { start, props, .. } => {
                self.create(cx, start.parent, props);
                Flow::Finished
            }
        }
    }

    fn key(&mut self, cx: &mut Ctx, modifiers: Modifiers) {
        if let Some(mut last) = self.last {
            last.modifiers = modifiers;
            self.last = Some(last);
            self.update(cx, &last);
        }
    }

    fn cancel(&mut self, cx: &mut Ctx) {
        self.state = State::Idle;
        self.lines.clear();
        cx.scene.clear_preview();
    }

    fn is_busy(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    fn cursor(&self) -> Cursor {
        Cursor::Crosshair
    }

    fn overlay(&self) -> ToolOverlay {
        ToolOverlay {
            snap_lines: self.lines.clone(),
            selection: match &self.state {
                State::Drawing { draft, .. } => Some(vec![*draft]),
                _ => None,
            },
            ..Default::default()
        }
    }
}
````

`crates/ferrum-editor/src/tools/hand.rs`:

````rust
//! Рука (H, удержание Space, средняя кнопка): панорамирование холста. Документ не
//! меняет, поэтому и отменять нечего.

use kurbo::Point;

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay};
use crate::input::{Modifiers, PointerEvent};

#[derive(Default)]
pub struct HandTool {
    last: Option<Point>,
}

impl HandTool {
    pub fn new() -> HandTool {
        HandTool::default()
    }
}

impl Tool for HandTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Hand
    }

    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        *cx.hover = None;
        self.last = Some(e.point);
    }

    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        if let Some(last) = self.last {
            cx.camera.pan_by(e.point - last);
            self.last = Some(e.point);
        }
    }

    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow {
        self.pointer_move(cx, e);
        self.last = None;
        Flow::Stay
    }

    fn key(&mut self, _: &mut Ctx, _: Modifiers) {}

    fn cancel(&mut self, _: &mut Ctx) {
        self.last = None;
    }

    fn is_busy(&self) -> bool {
        self.last.is_some()
    }

    fn cursor(&self) -> Cursor {
        if self.last.is_some() {
            Cursor::Grabbing
        } else {
            Cursor::Grab
        }
    }

    fn overlay(&self) -> ToolOverlay {
        ToolOverlay::default()
    }
}
````

- [ ] **Step 8: Редактор**

`crates/ferrum-editor/src/editor.rs`:

````rust
//! Редактор без браузера: принимает события ввода и команды, правит документ
//! транзакциями, держит сцену, камеру, выделение и инструмент, отдаёт оверлей,
//! курсор и события для интерфейса.
//!
//! Редактор владеет документом и сценой, а не заимствует их: сцена должна совпадать
//! с документом после каждой правки, отмены и повтора, и это можно гарантировать,
//! только если править документ больше некому. Панели (этап 5) правят через
//! `Editor::transact`, который сам обновляет сцену. В worker'е (этап 4) редактор —
//! единственный владелец всего состояния ядра, рендер получает `scene()`,
//! `camera()` и `overlay()` по ссылке.

use ferrum_doc::{DocError, DocResult, Document, NodeId, PageDoc, PageId, Tx};
use ferrum_render::{Camera, Overlay, OverlayTheme};
use ferrum_scene::Scene;
use kurbo::{Point, Size, Vec2};

use crate::commands::CommandId;
use crate::input::{Input, KeyEvent, Platform, PointerButton, PointerEvent, WheelEvent};
use crate::keymap::Keymap;
use crate::tools::{
    sync, Ctx, Cursor, DrawTool, Flow, HandTool, Outbox, SelectTool, Shape, Tool, ToolKind,
    ToolOverlay,
};

/// Множитель зума колесом с Ctrl/Cmd и щипком: `exp(-deltaY · k)`.
pub const WHEEL_ZOOM_SPEED: f64 = 0.01;

/// Настройки редактора (хранятся у пользователя, не в документе).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub platform: Platform,
    /// Привязка положения и размеров к целым пикселям (по умолчанию включена).
    pub pixel_snap: bool,
    /// Привязка к краям и центрам соседей.
    pub snapping: bool,
    /// Пиксельная сетка при зуме от 800%.
    pub pixel_grid: bool,
}

impl Settings {
    pub fn new(platform: Platform) -> Settings {
        Settings {
            platform,
            pixel_snap: true,
            snapping: true,
            pixel_grid: true,
        }
    }
}

/// События для интерфейса. Забираются `take_events` раз за кадр.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorEvent {
    SelectionChanged(Vec<NodeId>),
    ToolChanged(ToolKind),
    CursorChanged(Cursor),
    /// Изменились сцена, камера или оверлей — нужен новый кадр.
    NeedsRedraw,
    /// Документ изменён правкой, отменой или повтором.
    DocumentChanged,
    PageChanged(PageId),
    /// Готовы байты для системного буфера обмена (формат `web application/x-ferrum`).
    Clipboard(Vec<u8>),
    /// Интерфейсу пора прочитать системный буфер и вызвать `paste`.
    PasteRequested,
    /// Скопировать как PNG: интерфейс экспортирует узлы и кладёт картинку в буфер.
    ExportPng(Vec<NodeId>),
    /// Выбрать файл изображения и вызвать `place_image`.
    RequestImage,
    /// Скрыть или показать интерфейс (Ctrl+\).
    ToggleUi,
    /// Сообщение об ошибке для пользователя.
    Error(String),
}

/// Что видел интерфейс в прошлый раз — события шлются только при изменении.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    selection: Vec<NodeId>,
    tool: ToolKind,
    cursor: Cursor,
    frame: FrameKey,
}

#[derive(Debug, Clone, PartialEq)]
struct FrameKey {
    scene: u64,
    camera: Camera,
    selection: Vec<NodeId>,
    hover: Option<NodeId>,
    tool: ToolOverlay,
    theme: OverlayTheme,
    pixel_grid: bool,
}

pub struct Editor {
    pub(crate) doc: Document,
    pub(crate) page: PageId,
    pub(crate) scene: Scene,
    pub(crate) camera: Camera,
    pub(crate) selection: Vec<NodeId>,
    pub(crate) hover: Option<NodeId>,
    pub(crate) tool: Box<dyn Tool>,
    /// Инструмент, который рука временно заменила на время удержания клавиши,
    /// и физическая клавиша удержания.
    held: Option<(Box<dyn Tool>, String)>,
    /// Панорамирование средней кнопкой поверх любого инструмента.
    pan: Option<HandTool>,
    pub(crate) keymap: Keymap,
    pub(crate) settings: Settings,
    theme: OverlayTheme,
    pub(crate) out: Outbox,
    last_pointer: Option<PointerEvent>,
    seen: Seen,
}

impl Editor {
    /// Редактор для страницы `page` (по умолчанию первой) с вьюпортом в CSS-пикселях.
    pub fn new(
        mut doc: Document,
        page: Option<PageId>,
        viewport: Size,
        dpr: f64,
        platform: Platform,
    ) -> DocResult<Editor> {
        let page = match page.or_else(|| doc.pages().first().map(|p| p.0)) {
            Some(p) => p,
            None => return Err(DocError::PageNotFound("—".into())),
        };
        let scene = Scene::from_page(doc.page(page)?);
        // Изменения, накопленные до редактора, уже в сцене.
        for (p, _) in doc.pages() {
            doc.take_changes(p);
        }
        let camera = Camera::new(viewport, dpr);
        let theme = OverlayTheme::dark();
        let tool: Box<dyn Tool> = Box::new(SelectTool::new());
        let seen = Seen {
            selection: Vec::new(),
            tool: ToolKind::Move,
            cursor: Cursor::Default,
            frame: FrameKey {
                scene: scene.change_counter(),
                camera: camera.clone(),
                selection: Vec::new(),
                hover: None,
                tool: ToolOverlay::default(),
                theme: theme.clone(),
                pixel_grid: true,
            },
        };
        Ok(Editor {
            doc,
            page,
            scene,
            camera,
            selection: Vec::new(),
            hover: None,
            tool,
            held: None,
            pan: None,
            keymap: Keymap::new(platform),
            settings: Settings::new(platform),
            theme,
            out: Outbox::default(),
            last_pointer: None,
            seen,
        })
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Текущая страница документа (для чтения панелями и тестами).
    pub fn page_doc(&mut self) -> &PageDoc {
        self.doc
            .page(self.page)
            .expect("текущая страница загружена при открытии")
    }

    pub fn page(&self) -> PageId {
        self.page
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn selection(&self) -> &[NodeId] {
        &self.selection
    }

    pub fn tool_kind(&self) -> ToolKind {
        self.tool.kind()
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    pub fn keymap_mut(&mut self) -> &mut Keymap {
        &mut self.keymap
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn set_settings(&mut self, settings: Settings) {
        if settings.platform != self.settings.platform {
            // Раскладка другой платформы — с теми же пользовательскими отличиями.
            let overrides = self.keymap.export_json();
            self.keymap = Keymap::new(settings.platform);
            let _ = self.keymap.import_json(&overrides);
        }
        self.settings = settings;
        self.emit();
    }

    pub fn set_theme(&mut self, theme: OverlayTheme) {
        self.theme = theme;
        self.emit();
    }

    /// Шагов истории, созданных редактором (жесты и команды с изменениями).
    pub fn step_count(&self) -> u64 {
        self.out.steps
    }

    /// Идёт ли жест (кнопка нажата).
    pub fn is_busy(&self) -> bool {
        self.tool.is_busy() || self.pan.is_some()
    }

    pub fn take_events(&mut self) -> Vec<EditorEvent> {
        std::mem::take(&mut self.out.events)
    }

    /// Камера целиком (восстановление вида страницы, тесты).
    pub fn set_camera(&mut self, camera: Camera) {
        self.camera = camera;
        self.refresh_gesture();
        self.emit();
    }

    pub fn set_viewport(&mut self, viewport: Size, dpr: f64) {
        self.camera.set_viewport(viewport);
        self.camera.set_dpr(dpr);
        self.emit();
    }

    /// Курсор над холстом.
    pub fn cursor(&self) -> Cursor {
        if self.pan.is_some() {
            Cursor::Grabbing
        } else {
            self.tool.cursor()
        }
    }

    /// Описание оверлея для рендера.
    pub fn overlay(&self) -> Overlay {
        let t = self.tool.overlay();
        Overlay {
            theme: self.theme.clone(),
            selection: t.selection.unwrap_or_else(|| self.selection.clone()),
            hover: if self.is_busy() { None } else { self.hover },
            marquee: t.marquee,
            snap_lines: t.snap_lines,
            frame_labels: true,
            pixel_grid: self.settings.pixel_grid,
        }
    }

    /// Разделяет редактор на инструмент и контекст для него.
    pub(crate) fn split(&mut self) -> (&mut Box<dyn Tool>, Ctx<'_>) {
        let Editor {
            doc,
            page,
            scene,
            camera,
            selection,
            hover,
            settings,
            out,
            tool,
            ..
        } = self;
        (
            tool,
            Ctx {
                doc,
                page: *page,
                scene,
                camera,
                selection,
                hover,
                settings,
                out,
            },
        )
    }

    pub(crate) fn ctx(&mut self) -> Ctx<'_> {
        self.split().1
    }

    /// Обрабатывает одно событие ввода.
    pub fn handle(&mut self, input: Input) {
        match input {
            Input::PointerDown(e) => self.pointer_down(e),
            Input::PointerMove(e) => self.pointer_move(e),
            Input::PointerUp(e) => self.pointer_up(e),
            Input::PointerCancel => self.cancel_gesture(),
            Input::Wheel(e) => self.wheel(e),
            Input::Pinch { point, scale } => {
                self.camera.zoom_at(point, scale);
                self.refresh_gesture();
            }
            Input::KeyDown(e) => self.key_down(e),
            Input::KeyUp(e) => self.key_up(e),
            Input::Blur => {
                self.cancel_gesture();
                self.release_hold();
            }
        }
        self.emit();
    }

    fn pointer_down(&mut self, e: PointerEvent) {
        self.last_pointer = Some(e);
        match e.button {
            PointerButton::Middle => {
                self.cancel_gesture();
                let mut hand = HandTool::new();
                hand.pointer_down(&mut self.ctx(), &e);
                self.pan = Some(hand);
            }
            PointerButton::Secondary => {}
            PointerButton::Primary => {
                if self.pan.is_some() {
                    return;
                }
                // Нажатие без отпускания предыдущего (событие потеряно) — прежний
                // жест отменяется, а не записывается.
                if self.tool.is_busy() {
                    self.cancel_gesture();
                }
                let (tool, mut cx) = self.split();
                tool.pointer_down(&mut cx, &e);
            }
        }
    }

    fn pointer_move(&mut self, e: PointerEvent) {
        self.last_pointer = Some(e);
        if let Some(mut hand) = self.pan.take() {
            hand.pointer_move(&mut self.ctx(), &e);
            self.pan = Some(hand);
            return;
        }
        let (tool, mut cx) = self.split();
        tool.pointer_move(&mut cx, &e);
    }

    fn pointer_up(&mut self, e: PointerEvent) {
        self.last_pointer = Some(e);
        if e.button == PointerButton::Middle {
            if let Some(mut hand) = self.pan.take() {
                hand.pointer_up(&mut self.ctx(), &e);
            }
            return;
        }
        if e.button != PointerButton::Primary || !self.tool.is_busy() {
            return;
        }
        let (tool, mut cx) = self.split();
        if tool.pointer_up(&mut cx, &e) == Flow::Finished {
            self.set_tool(ToolKind::Move);
        }
    }

    fn wheel(&mut self, e: WheelEvent) {
        if e.modifiers.ctrl || e.modifiers.meta {
            self.camera
                .zoom_at(e.point, (-e.delta.y * WHEEL_ZOOM_SPEED).exp());
        } else {
            // Колесо мыши с Shift прокручивает вбок.
            let d = if e.modifiers.shift && e.delta.x == 0.0 {
                Vec2::new(e.delta.y, 0.0)
            } else {
                e.delta
            };
            self.camera.pan_by(-d);
        }
        self.refresh_gesture();
    }

    /// Камера сдвинулась во время жеста: та же экранная точка теперь над другим
    /// местом мира, жест пересчитывается.
    fn refresh_gesture(&mut self) {
        if let (true, Some(e)) = (self.tool.is_busy(), self.last_pointer) {
            let (tool, mut cx) = self.split();
            tool.pointer_move(&mut cx, &e);
        }
    }

    fn key_down(&mut self, e: KeyEvent) {
        let cmd = self.keymap.lookup(&e);
        if self.is_busy() {
            // Во время жеста клавиши только меняют модификаторы; Esc отменяет жест.
            if cmd == Some(CommandId::Escape) {
                self.cancel_gesture();
            } else {
                let (tool, mut cx) = self.split();
                tool.key(&mut cx, e.modifiers);
            }
            return;
        }
        let Some(cmd) = cmd else {
            return;
        };
        if cmd == CommandId::HandHold {
            if !e.repeat && self.held.is_none() && self.tool.kind() != ToolKind::Hand {
                let prev = std::mem::replace(&mut self.tool, Box::new(HandTool::new()));
                self.held = Some((prev, e.code.clone()));
                self.hover = None;
            }
            return;
        }
        self.run(cmd);
    }

    fn key_up(&mut self, e: KeyEvent) {
        if self.held.as_ref().is_some_and(|(_, code)| *code == e.code) {
            self.release_hold();
            return;
        }
        if self.tool.is_busy() {
            let (tool, mut cx) = self.split();
            tool.key(&mut cx, e.modifiers);
        }
    }

    /// Возвращает инструмент, заменённый рукой на время удержания клавиши.
    fn release_hold(&mut self) {
        if let Some((prev, _)) = self.held.take() {
            if self.tool.is_busy() {
                let (tool, mut cx) = self.split();
                tool.cancel(&mut cx);
            }
            self.tool = prev;
        }
    }

    /// Отменяет жест: превью убирается, документ и история не меняются.
    pub fn cancel_gesture(&mut self) {
        if let Some(mut hand) = self.pan.take() {
            hand.cancel(&mut self.ctx());
        }
        if self.tool.is_busy() {
            let (tool, mut cx) = self.split();
            tool.cancel(&mut cx);
        }
    }

    /// Выбирает инструмент (тулбар, горячая клавиша). Жест текущего отменяется.
    pub fn set_tool(&mut self, kind: ToolKind) {
        self.cancel_gesture();
        self.held = None;
        self.hover = None;
        self.tool = match kind {
            ToolKind::Move => Box::new(SelectTool::new()),
            ToolKind::Frame => Box::new(DrawTool::new(Shape::Frame)),
            ToolKind::Rect => Box::new(DrawTool::new(Shape::Rect)),
            ToolKind::Ellipse => Box::new(DrawTool::new(Shape::Ellipse)),
            ToolKind::Line => Box::new(DrawTool::new(Shape::Line)),
            ToolKind::Hand => Box::new(HandTool::new()),
            // Изображению нужны хэш и размер: интерфейс сначала выбирает файл.
            ToolKind::Image => {
                self.out.events.push(EditorEvent::RequestImage);
                Box::new(SelectTool::new())
            }
        };
        self.emit();
    }

    /// Инструмент размещения изображения: клик ставит его в исходном размере,
    /// перетаскивание задаёт размер с сохранением пропорций.
    pub fn place_image(&mut self, hash: &str, width: f64, height: f64) {
        self.set_tool(ToolKind::Move);
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if !ok(width) || !ok(height) {
            self.out
                .events
                .push(EditorEvent::Error("У изображения нет размера".into()));
            return;
        }
        self.tool = Box::new(DrawTool::new(Shape::Image {
            hash: hash.to_string(),
            width,
            height,
        }));
        self.emit();
    }

    /// Выполняет команду (меню, палитра, горячая клавиша). Во время жеста
    /// выполняется только Esc — он отменяет жест.
    pub fn execute(&mut self, cmd: CommandId) {
        if self.is_busy() {
            if cmd == CommandId::Escape {
                self.cancel_gesture();
            }
        } else {
            self.run(cmd);
        }
        self.emit();
    }

    /// Правка документа из панелей одним шагом отмены; сцена обновляется сама.
    pub fn transact<F>(&mut self, f: F) -> DocResult<Vec<NodeId>>
    where
        F: FnOnce(&Tx) -> DocResult<Vec<NodeId>>,
    {
        self.cancel_gesture();
        let before = self.selection.clone();
        let result = self.doc.transact(self.page, before, f);
        let changed = sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        if let Ok(sel) = &result {
            if changed {
                self.out.steps += 1;
            }
            self.selection = self.alive(sel);
        }
        self.emit();
        result
    }

    /// Выделение из панели слоёв. Неизвестные узлы пропускаются.
    pub fn select(&mut self, ids: &[NodeId]) {
        self.cancel_gesture();
        self.selection = self.alive(ids);
        self.emit();
    }

    /// Подсветка наведения из панели слоёв.
    pub fn set_hover(&mut self, id: Option<NodeId>) {
        if !self.is_busy() {
            self.hover = id;
            self.emit();
        }
    }

    pub(crate) fn alive(&self, ids: &[NodeId]) -> Vec<NodeId> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if self.scene.slot(*id).is_some() && !self.scene.is_draft(*id) && !out.contains(id) {
                out.push(*id);
            }
        }
        out
    }

    /// Кладёт байты изображения в документ и возвращает их хэш — для
    /// `place_image` и `insert_image`. Байты не попадают в историю отмены.
    pub fn add_image(&mut self, bytes: Vec<u8>) -> String {
        self.doc.images.add(bytes)
    }

    /// Новая страница в конце списка (в историю отмены не попадает, как и в
    /// `ferrum-doc`). Текущая страница не меняется.
    pub fn add_page(&mut self, name: &str) -> DocResult<PageId> {
        self.doc.add_page(name)
    }

    /// Переключает страницу: сцена строится заново, выделение снимается.
    pub fn set_page(&mut self, page: PageId) -> DocResult<()> {
        self.cancel_gesture();
        // Изменения всех страниц уже в сценах или не нужны: новая сцена строится
        // по документу целиком.
        for (p, _) in self.doc.pages() {
            self.doc.take_changes(p);
        }
        let pd = self.doc.page(page)?;
        self.scene = Scene::from_page(pd);
        self.page = page;
        self.selection.clear();
        self.hover = None;
        self.out.events.push(EditorEvent::PageChanged(page));
        self.emit();
        Ok(())
    }

    pub(crate) fn undo_redo(&mut self, redo: bool) {
        let result = if redo {
            self.doc.redo()
        } else {
            self.doc.undo()
        };
        sync(&mut self.doc, self.page, &mut self.scene, &mut self.out);
        match result {
            Ok(Some(restore)) => {
                if restore.page != self.page {
                    if let Ok(pd) = self.doc.page(restore.page) {
                        self.scene = Scene::from_page(pd);
                        self.page = restore.page;
                        self.out.events.push(EditorEvent::PageChanged(restore.page));
                    }
                }
                self.selection = self.alive(&restore.selection);
            }
            Ok(None) => {}
            Err(e) => self.out.events.push(EditorEvent::Error(e.to_string())),
        }
    }

    /// Шлёт события об изменившемся выделении, инструменте, курсоре и кадре.
    pub(crate) fn emit(&mut self) {
        let overlay_selection = self.overlay().selection;
        let now = Seen {
            selection: self.selection.clone(),
            tool: self.tool.kind(),
            cursor: self.cursor(),
            frame: FrameKey {
                scene: self.scene.change_counter(),
                camera: self.camera.clone(),
                selection: overlay_selection,
                hover: if self.is_busy() { None } else { self.hover },
                tool: self.tool.overlay(),
                theme: self.theme.clone(),
                pixel_grid: self.settings.pixel_grid,
            },
        };
        if now.selection != self.seen.selection {
            self.out
                .events
                .push(EditorEvent::SelectionChanged(now.selection.clone()));
        }
        if now.tool != self.seen.tool {
            self.out.events.push(EditorEvent::ToolChanged(now.tool));
        }
        if now.cursor != self.seen.cursor {
            self.out.events.push(EditorEvent::CursorChanged(now.cursor));
        }
        if now.frame != self.seen.frame {
            self.out.events.push(EditorEvent::NeedsRedraw);
        }
        self.seen = now;
    }

    /// Точка мира в центре вьюпорта.
    pub(crate) fn view_center(&self) -> Point {
        self.camera.center()
    }
}
````

`crates/ferrum-editor/src/actions.rs`:

````rust
//! Команды из таблицы. На этом шаге — инструменты, отмена, повтор и Esc;
//! правки выделения, порядок слоёв, вид и буфер обмена добавляет следующая задача.

use crate::commands::CommandId;
use crate::editor::{Editor, EditorEvent};
use crate::tools::ToolKind;

impl Editor {
    pub(crate) fn run(&mut self, cmd: CommandId) {
        use CommandId as C;
        match cmd {
            C::ToolMove => self.set_tool(ToolKind::Move),
            C::ToolFrame => self.set_tool(ToolKind::Frame),
            C::ToolRect => self.set_tool(ToolKind::Rect),
            C::ToolEllipse => self.set_tool(ToolKind::Ellipse),
            C::ToolLine => self.set_tool(ToolKind::Line),
            C::ToolHand => self.set_tool(ToolKind::Hand),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
            C::ToolImage => self.out.events.push(EditorEvent::RequestImage),
            C::Undo => self.undo_redo(false),
            C::Redo => self.undo_redo(true),
            C::Escape => {
                if self.tool.kind() != ToolKind::Move {
                    self.set_tool(ToolKind::Move);
                } else {
                    self.selection.clear();
                }
            }
            _ => {}
        }
    }
}
````

`crates/ferrum-editor/src/lib.rs`:

````rust
//! Редактор Ferrum без браузера: инструменты, выделение, привязки, команды и
//! горячие клавиши, буфер обмена. Получает события ввода простыми структурами,
//! правит документ транзакциями `ferrum-doc`, держит сцену с превью жеста, камеру и
//! оверлей для `ferrum-render`. Не зависит ни от браузера, ни от ОС и детерминирован:
//! одна и та же последовательность событий даёт один и тот же документ.

mod actions;
pub mod clipboard;
pub mod commands;
mod editor;
pub mod geometry;
pub mod input;
pub mod keymap;
mod ops;
pub mod presets;
pub mod selection;
pub mod snap;
pub mod tools;

pub use commands::{Command, CommandId, COMMANDS};
pub use editor::{Editor, EditorEvent, Settings, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, Conflict, Keymap, KeymapError, Shortcut};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

- [ ] **Step 9: Запустить тесты**

Run: `cargo test -p ferrum-editor --test selection`
Expected: PASS, `8 passed`.

Run: `cargo test -p ferrum-editor --test draw`
Expected: PASS, `9 passed`.

Run: `cargo test -p ferrum-editor --test transform`
Expected: PASS, `14 passed`.

- [ ] **Step 10: Закоммитить**

```bash
git add crates/ferrum-editor
git commit -m "feat(editor): редактор, инструменты перемещения, рисования и руки"
```

---

### Task 6: Команды, буфер обмена в редакторе, пресеты

Каждая правка — одна транзакция. Удаление и дубликат (Ctrl+D — копия прямо над оригиналом, на том же месте, выделена). Группа (Ctrl+G) и «обернуть во фрейм» (Ctrl+Alt+G) встают на место верхнего выделенного узла; узлы остаются на экране там же; группа хранит `x = y = 0` — её границы считает сцена, а фрейм получает общую рамку выделения. Разгруппировка (Ctrl+Shift+G) применяется к группам и фреймам: дети встают на их место у родителя. Порядок слоёв: вперёд и назад — обмен с невыделенным соседом, на передний и задний план — блоком. Стрелки — 1 px, с Shift — 10 px в осях экрана. Скрыть или показать, заблокировать или разблокировать: если хоть один узел видим (не заблокирован), все переходят в обратное состояние. «Выделить всё» — соседи в контексте выделения, видимые и не заблокированные. Вид: «показать всё», «к выделению» (отступ 48 px), 100%, приблизить и отдалить вдвое от центра. Esc без жеста: сначала возвращает к «Перемещению», затем снимает выделение. Команда без изменений шага истории не создаёт.

Копирование отдаёт событие с байтами формата Task 4; вырезание — копия и удаление. Вставка (байты от интерфейса): в единственный выделенный фрейм, иначе к родителю выделения, иначе в корень; на исходном месте, если оно видно и помещается в цель, иначе — в центр цели (фрейма или видимой части); новые узлы получают новые идентификаторы и выделяются; байты изображений из копии кладутся в документ, если такого хэша там ещё нет. Ctrl+V и Ctrl+Shift+C только просят интерфейс прочитать системный буфер и экспортировать PNG — сами байты приходят и уходят через `paste` и `ExportPng`. Изображение из системного буфера или перетащенного файла — `insert_image`: в центр видимой части или выделенного фрейма. Фрейм из пресета встаёт справа от содержимого страницы с зазором 100 px, выделяется, и камера переезжает к нему, если его не видно.

**Files:**
- Modify: `crates/ferrum-editor/src/actions.rs`
- Modify: `crates/ferrum-editor/src/lib.rs`
- Test: `crates/ferrum-editor/tests/commands.rs`
- Test: `crates/ferrum-editor/tests/clipboard.rs`

**Interfaces:**
- Consumes: Task 4–5; `Tx::{create_node, set_props, move_node, move_nodes, delete_nodes, children, props}`.
- Produces: все команды таблицы; `Editor::{copy, paste, insert_image, add_frame_preset}`; `FIT_PADDING_PX`, `PRESET_GAP`.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-editor/tests/commands.rs`:

````rust
//! Команды из таблицы (раздел 7.6 спеки): каждая правка — один шаг отмены.

mod common;

use common::*;
use ferrum_doc::NodeKind;
use ferrum_editor::*;
use kurbo::Point;

fn world_corner(h: &H, id: ferrum_doc::NodeId) -> Point {
    h.ed.scene().node(id).unwrap().world * Point::ZERO
}

#[test]
fn delete_and_duplicate() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(None, 300.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyD", ctrl());
    let roots = h.children(None);
    assert_eq!(roots.len(), 3);
    assert_eq!(roots[0], a);
    let copy = roots[1];
    assert_eq!(h.sel(), vec![copy], "копия над оригиналом и выделена");
    assert_eq!(h.props(copy).x, 100.0, "на том же месте");
    h.assert_one_step(steps, &before);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("Delete", none());
    assert_eq!(h.children(None), vec![a, b]);
    assert!(h.sel().is_empty());
    h.assert_one_step(steps, &before);
    h.click(320.0, 120.0, none());
    h.key("Backspace", none());
    assert_eq!(h.children(None), vec![a]);
}

#[test]
fn group_ungroup_and_frame_selection_keep_screen_positions() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 800.0, 800.0);
    let a = h.rect(Some(f), 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(Some(f), 300.0, 200.0, 50.0, 50.0);
    let c = h.rect(Some(f), 500.0, 500.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    h.click(320.0, 220.0, shift());
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("KeyG", ctrl());
    let g = h.sel()[0];
    assert_eq!(h.props(g).kind, NodeKind::Group);
    assert_eq!(
        h.children(Some(f)),
        vec![g, c],
        "группа на месте верхнего выделенного"
    );
    assert_eq!(h.children(Some(g)), vec![a, b]);
    assert_eq!(world_corner(&h, a), Point::new(100.0, 100.0));
    h.assert_one_step(steps, &before);
    // Разгруппировка возвращает детей на место группы.
    let steps = h.ed.step_count();
    let before = h.state();
    h.key("KeyG", ctrl().with_shift());
    assert_eq!(h.children(Some(f)), vec![a, b, c]);
    assert_eq!(h.sel(), vec![a, b]);
    assert_eq!(world_corner(&h, b), Point::new(300.0, 200.0));
    h.assert_one_step(steps, &before);
    // Обернуть во фрейм: фрейм по общей рамке, дети в его координатах.
    h.key("KeyG", ctrl().with_alt());
    let fr = h.sel()[0];
    let p = h.props(fr);
    assert_eq!(
        (p.kind, p.x, p.y, p.w, p.h),
        (NodeKind::Frame, 100.0, 100.0, 250.0, 150.0)
    );
    assert_eq!((h.props(a).x, h.props(a).y), (0.0, 0.0));
    assert_eq!((h.props(b).x, h.props(b).y), (200.0, 100.0));
    assert_eq!(world_corner(&h, b), Point::new(300.0, 200.0));
    h.assert_scene_clean();
}

#[test]
fn z_order_commands() {
    let mut h = H::new();
    let ids: Vec<_> = (0..4)
        .map(|i| h.rect(None, 100.0 + 10.0 * i as f64, 100.0, 50.0, 50.0))
        .collect();
    let [a, b, c, d] = [ids[0], ids[1], ids[2], ids[3]];
    h.ed.select(&[a]);
    h.key("BracketRight", ctrl());
    assert_eq!(h.children(None), vec![b, a, c, d]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.key("BracketRight", ctrl().with_alt());
    assert_eq!(h.children(None), vec![b, c, d, a]);
    h.assert_one_step(steps, &before);
    h.key("BracketLeft", ctrl());
    assert_eq!(h.children(None), vec![b, c, a, d]);
    h.key("BracketLeft", ctrl().with_alt());
    assert_eq!(h.children(None), vec![a, b, c, d]);
    // Несколько выделенных сдвигаются вместе, верхний уже наверху не лезет дальше.
    h.ed.select(&[b, d]);
    h.key("BracketRight", ctrl());
    assert_eq!(h.children(None), vec![a, c, b, d]);
    let steps = h.ed.step_count();
    h.ed.select(&[d]);
    h.key("BracketRight", ctrl());
    assert_eq!(h.ed.step_count(), steps, "без изменений — без шага");
}

#[test]
fn nudge_hide_lock_select_all_and_escape() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 800.0, 800.0);
    let a = h.rect(Some(f), 100.0, 100.0, 50.0, 50.0);
    let b = h.rect(Some(f), 300.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    h.key("ArrowRight", none());
    h.key("ArrowDown", shift());
    assert_eq!((h.props(a).x, h.props(a).y), (101.0, 110.0));
    h.key("ArrowLeft", shift());
    h.key("ArrowUp", none());
    assert_eq!((h.props(a).x, h.props(a).y), (91.0, 109.0));
    // Выделить всё — соседи в контексте выделения.
    h.key("KeyA", ctrl());
    assert_eq!(h.sel(), vec![a, b]);
    h.key("KeyH", ctrl().with_shift());
    assert!(!h.props(a).visible && !h.props(b).visible);
    h.key("KeyH", ctrl().with_shift());
    assert!(h.props(a).visible && h.props(b).visible);
    h.key("KeyL", ctrl().with_shift());
    assert!(h.props(a).locked && h.props(b).locked);
    h.key("KeyA", ctrl());
    assert!(
        h.sel().is_empty() || h.sel() == vec![f],
        "заблокированные не выделяются"
    );
    h.ed.select(&[a, b]);
    h.key("KeyL", ctrl().with_shift());
    assert!(!h.props(a).locked);
    // Esc: сначала инструмент → «Перемещение», затем снять выделение.
    h.key("KeyR", none());
    h.key("Escape", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    assert!(!h.sel().is_empty());
    h.key("Escape", none());
    assert!(h.sel().is_empty());
    // Выделить всё без выделения — объекты верхнего уровня.
    h.key("KeyA", ctrl());
    assert_eq!(h.sel(), vec![f]);
}

#[test]
fn zoom_commands_and_events() {
    let mut h = H::new();
    let a = h.rect(None, 0.0, 0.0, 2000.0, 1000.0);
    let b = h.rect(None, 5000.0, 5000.0, 100.0, 100.0);
    h.key("Digit1", shift());
    let cam = h.ed.camera().clone();
    let all = cam.visible_world_rect();
    assert!(all.contains(Point::new(0.0, 0.0)) && all.contains(Point::new(5100.0, 5100.0)));
    h.ed.select(&[b]);
    h.key("Digit2", shift());
    assert!(close(h.ed.camera().center().x, 5050.0));
    assert!(h.ed.camera().zoom() > 1.0);
    h.key("Digit0", shift());
    assert_eq!(h.ed.camera().zoom(), 1.0);
    h.key("Equal", none());
    assert_eq!(h.ed.camera().zoom(), 2.0);
    h.key("Minus", none());
    assert_eq!(h.ed.camera().zoom(), 1.0);
    h.ed.take_events();
    h.key("Backslash", ctrl());
    h.key("KeyC", ctrl().with_shift());
    h.key("KeyV", ctrl());
    let events = h.ed.take_events();
    assert!(events.contains(&EditorEvent::ToggleUi));
    assert!(events.contains(&EditorEvent::ExportPng(vec![b])));
    assert!(events.contains(&EditorEvent::PasteRequested));
    let _ = a;
}

#[test]
fn undo_redo_restore_selection_and_skip_during_gesture() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    h.click(120.0, 120.0, none());
    h.drag((120.0, 120.0), (220.0, 120.0), none());
    h.click(900.0, 700.0, none());
    assert!(h.sel().is_empty());
    h.key("KeyZ", ctrl());
    assert_eq!(h.props(a).x, 100.0);
    assert_eq!(h.sel(), vec![a], "отмена вернула выделение");
    h.key("KeyZ", ctrl().with_shift());
    assert_eq!(h.props(a).x, 200.0);
    h.key("KeyZ", ctrl());
    h.key("KeyY", ctrl());
    assert_eq!(h.props(a).x, 200.0, "Ctrl+Y на Windows");
    // Во время жеста команды не выполняются.
    h.drag_hold((220.0, 120.0), (400.0, 400.0), none());
    h.key("KeyZ", ctrl());
    h.key("Delete", none());
    h.up(400.0, 400.0, none());
    assert_eq!(h.children(None), vec![a]);
    assert_eq!(h.props(a).x, 380.0);
}

#[test]
fn undo_switches_back_to_the_edited_page() {
    let mut h = H::new();
    let p1 = h.ed.page();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let p2 = h.ed.add_page("Вторая").unwrap();
    h.ed.set_page(p2).unwrap();
    assert!(h.ed.take_events().contains(&EditorEvent::PageChanged(p2)));
    let b = h.rect(None, 10.0, 10.0, 20.0, 20.0);
    h.ed.set_page(p1).unwrap();
    h.ed.select(&[a]);
    h.key("Delete", none());
    assert!(h.children(None).is_empty());
    h.key("KeyZ", ctrl());
    assert_eq!(h.children(None), vec![a]);
    assert_eq!(h.ed.page(), p1);
    h.ed.take_events();
    // Следующая отмена правит вторую страницу — редактор переходит на неё.
    h.key("KeyZ", ctrl());
    assert_eq!(h.ed.page(), p2);
    assert!(h.ed.take_events().contains(&EditorEvent::PageChanged(p2)));
    assert!(h.children(None).is_empty(), "создание b отменено");
    h.assert_scene_clean();
    h.key("KeyZ", ctrl().with_shift());
    assert_eq!(h.children(None), vec![b]);
    h.assert_scene_clean();
}

#[test]
fn frame_presets_create_frames_to_the_right() {
    let mut h = H::new();
    let presets = frame_presets();
    assert_eq!(presets.len(), 11);
    let iphone = presets.iter().find(|p| p.name == "iPhone 16").unwrap();
    assert_eq!((iphone.width, iphone.height), (393.0, 852.0));
    let a4 = presets.iter().find(|p| p.name == "A4").unwrap();
    assert_eq!((a4.width, a4.height), (595.0, 842.0));
    h.ed.add_frame_preset(iphone);
    let f = h.sel()[0];
    let p = h.props(f);
    assert_eq!((p.kind, p.w, p.h), (NodeKind::Frame, 393.0, 852.0));
    assert_eq!(p.name, "iPhone 16");
    h.ed.add_frame_preset(a4);
    let g = h.sel()[0];
    assert_eq!(h.props(g).x, p.x + 393.0 + PRESET_GAP);
    assert_eq!(h.props(g).y, p.y);
    assert!(parse_presets(
        "{\"version\":1,\"presets\":[{\"group\":\"x\",\"name\":\"y\",\"width\":0,\"height\":5}]}"
    )
    .is_err());
    assert!(matches!(
        parse_presets("{\"version\":9,\"presets\":[]}"),
        Err(PresetError::NewerVersion(9))
    ));
    assert!(parse_presets("[]").is_err());
}

#[test]
fn failed_panel_transaction_rolls_back_without_step() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 50.0, 50.0);
    let steps = h.ed.step_count();
    let before = h.state();
    // Ошибка посреди транзакции (перенос узла в самого себя): откат, без шага.
    let r = h.ed.transact(|tx| {
        tx.set_props(a, &patch(1.0, 1.0, 1.0, 1.0))?;
        tx.move_node(a, Some(a), 0)?;
        Ok(vec![])
    });
    assert!(r.is_err());
    assert_eq!(h.state(), before);
    assert_eq!(h.ed.step_count(), steps);
    h.assert_scene_clean();
}

#[test]
fn editor_uses_keymap_overrides() {
    let mut h = H::new();
    h.ed.keymap_mut()
        .set_bindings(CommandId::ToolRect, vec![Shortcut::parse("KeyQ").unwrap()]);
    h.key("KeyR", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Move);
    h.key("KeyQ", none());
    assert_eq!(h.ed.tool_kind(), ToolKind::Rect);
}
````

`crates/ferrum-editor/tests/clipboard.rs`:

````rust
//! Буфер обмена: свой формат, вставка с новыми идентификаторами, проверка данных.

mod common;

use common::*;
use ferrum_doc::{Fill, NodeKind, PropsPatch};
use ferrum_editor::clipboard::{ClipPayload, ClipboardError, MAGIC};
use ferrum_editor::*;
use kurbo::Point;

fn copy_of(h: &mut H) -> Vec<u8> {
    h.ed.take_events();
    h.key("KeyC", ctrl());
    h.ed.take_events()
        .into_iter()
        .find_map(|e| match e {
            EditorEvent::Clipboard(b) => Some(b),
            _ => None,
        })
        .expect("копия в событиях")
}

#[test]
fn copy_paste_round_trip_with_new_ids_and_images() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 300.0, 200.0);
    let img = h.rect(Some(f), 10.0, 10.0, 50.0, 50.0);
    let bytes = b"image bytes".to_vec();
    let hash = h.ed.add_image(bytes.clone());
    h.set(
        img,
        PropsPatch {
            fills: Some(vec![Fill::image(hash.clone())]),
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    h.ed.select(&[f]);
    let clip = copy_of(&mut h);
    assert_eq!(&clip[..8], MAGIC);
    let payload = ClipPayload::decode(&clip).unwrap();
    assert_eq!(payload.nodes.len(), 2);
    assert_eq!(payload.nodes[1].parent, Some(0));
    assert_eq!(payload.images, vec![(hash.clone(), bytes.clone())]);
    assert_eq!(ClipPayload::decode(&payload.encode()).unwrap(), payload);
    // Вставка: новые узлы на том же месте (оно видно), выделены, один шаг.
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.select(&[]);
    h.ed.paste(&clip).unwrap();
    let roots = h.children(None);
    assert_eq!(roots.len(), 2);
    let copy = roots[1];
    assert_ne!(copy, f);
    assert_eq!(h.sel(), vec![copy]);
    let p = h.props(copy);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 300.0, 200.0));
    let inner = h.children(Some(copy));
    assert_eq!(inner.len(), 1);
    let q = h.props(inner[0]);
    assert_eq!((q.x, q.y, q.rotation), (10.0, 10.0, 30.0));
    assert_eq!(q.fills[0].image_hash.as_deref(), Some(hash.as_str()));
    h.assert_one_step(steps, &before);
    // В другой документ копия приносит байты изображения с собой.
    let mut other = H::new();
    other.ed.paste(&clip).unwrap();
    assert_eq!(
        other.ed.document().images.get(&hash),
        Some(bytes.as_slice())
    );
}

#[test]
fn paste_goes_into_selected_frame_or_to_view_center() {
    let mut h = H::new();
    let r = h.rect(None, 600.0, 600.0, 40.0, 40.0);
    let f = h.frame(None, 0.0, 0.0, 300.0, 300.0);
    h.ed.select(&[r]);
    let bytes = copy_of(&mut h);
    // Выделен фрейм: копия внутрь; исходное место вне фрейма — в его центр.
    h.ed.select(&[f]);
    h.ed.paste(&bytes).unwrap();
    let c = h.sel()[0];
    assert_eq!(h.parent(c), Some(f));
    assert_eq!((h.props(c).x, h.props(c).y), (130.0, 130.0));
    // Исходное место не видно — в центр видимой части.
    let mut cam = ferrum_render::Camera::new(VIEW, 1.0);
    cam.set_center(Point::new(5000.0, 5000.0));
    h.ed.set_camera(cam);
    h.ed.select(&[]);
    h.ed.paste(&bytes).unwrap();
    let d = h.sel()[0];
    assert_eq!(h.parent(d), None);
    assert_eq!((h.props(d).x, h.props(d).y), (4980.0, 4980.0));
}

#[test]
fn cut_is_copy_plus_delete() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    h.ed.take_events();
    h.key("KeyX", ctrl());
    let events = h.ed.take_events();
    let bytes = events
        .iter()
        .find_map(|e| match e {
            EditorEvent::Clipboard(b) => Some(b.clone()),
            _ => None,
        })
        .unwrap();
    assert!(h.children(None).is_empty());
    h.ed.paste(&bytes).unwrap();
    assert_eq!(h.children(None).len(), 1);
    assert_ne!(h.children(None)[0], r);
}

#[test]
fn hostile_payloads_are_rejected_without_changes() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    let good = copy_of(&mut h);
    let before = h.state();
    let steps = h.ed.step_count();
    let json_of = |bytes: &[u8]| {
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        String::from_utf8(bytes[16..16 + len].to_vec()).unwrap()
    };
    let with_json = |json: &str| {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(json.as_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    };
    let json = json_of(&good);
    let mut newer = good.clone();
    newer[8..12].copy_from_slice(&7u32.to_le_bytes());
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (b"garbage".to_vec(), "не ferrum"),
        (good[..good.len() - 1].to_vec(), "обрыв"),
        (newer, "новее"),
        ([good.clone(), vec![0]].concat(), "лишние байты"),
        (
            with_json(&json.replace("\"parent\":null", "\"parent\":5")),
            "ссылка вперёд",
        ),
        (with_json(&json.replace("#d9d9d9ff", "#zz")), "цвет"),
        (
            with_json("{\"nodes\":[],\"images\":[],\"bounds\":[0,0,1,1]}"),
            "пусто",
        ),
        (with_json("{\"nodes\":1}"), "не тот JSON"),
        (
            with_json(&json.replace("\"bounds\":[", "\"bounds\":[1e999,")),
            "границы",
        ),
    ];
    for (bytes, what) in cases {
        assert!(h.ed.paste(&bytes).is_err(), "{what}");
        assert_eq!(h.state(), before, "{what}");
        assert_eq!(h.ed.step_count(), steps, "{what}");
    }
    assert!(matches!(
        ClipPayload::decode(b"garbage"),
        Err(ClipboardError::NotFerrum)
    ));
    // Неизвестный тип пропускается с поддеревом; огромные числа приводятся к пределам.
    let unknown = json
        .replacen("\"kind\":\"rect\"", "\"kind\":\"star\"", 1)
        .replace("\"w\":40.0", "\"w\":1e300");
    assert!(matches!(
        ClipPayload::decode(&with_json(&unknown)),
        Err(ClipboardError::Empty)
    ));
    let huge = json.replace("\"w\":40.0", "\"w\":1e300");
    let p = ClipPayload::decode(&with_json(&huge)).unwrap();
    assert_eq!(p.nodes[0].props.w, ferrum_scene::shape::COORD_LIMIT);
    let _ = NodeKind::Rect;
}
````

- [ ] **Step 2: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-editor --test commands`
Expected: ошибка компиляции — нет `PRESET_GAP` и `add_frame_preset`.

- [ ] **Step 3: Реализовать команды**

`crates/ferrum-editor/src/actions.rs`:

````rust
//! Команды из таблицы: правки выделения, порядок слоёв, вид, буфер обмена.
//! Каждая правка документа — одна транзакция, то есть один шаг отмены.

use std::collections::BTreeSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props, PropsPatch};
use kurbo::{Affine, Point, Rect, Vec2};

use crate::clipboard::{full_patch, ClipNode, ClipPayload, ClipboardError};
use crate::commands::CommandId;
use crate::editor::{Editor, EditorEvent};
use crate::geometry::{decompose, parent_world, top_level, world_box};
use crate::ops::{duplicate, local_delta, reposition};
use crate::presets::FramePreset;
use crate::selection::locked;
use crate::tools::ToolKind;

/// Отступ «Показать всё» и «Показать выделение» в CSS-пикселях.
pub const FIT_PADDING_PX: f64 = 48.0;
/// Зазор между новым фреймом из пресета и содержимым страницы.
pub const PRESET_GAP: f64 = 100.0;

/// Выделенные дети одного родителя: родитель, число детей, позиции и узлы.
type SiblingGroup = (Option<NodeId>, usize, Vec<(usize, NodeId)>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrange {
    Forward,
    Backward,
    Front,
    Back,
}

impl Editor {
    pub(crate) fn run(&mut self, cmd: CommandId) {
        use CommandId as C;
        match cmd {
            C::ToolMove => self.set_tool(ToolKind::Move),
            C::ToolFrame => self.set_tool(ToolKind::Frame),
            C::ToolRect => self.set_tool(ToolKind::Rect),
            C::ToolEllipse => self.set_tool(ToolKind::Ellipse),
            C::ToolLine => self.set_tool(ToolKind::Line),
            C::ToolHand => self.set_tool(ToolKind::Hand),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
            C::ToolImage => self.out.events.push(EditorEvent::RequestImage),
            C::Undo => self.undo_redo(false),
            C::Redo => self.undo_redo(true),
            C::Copy => {
                if let Some(bytes) = self.copy() {
                    self.out.events.push(EditorEvent::Clipboard(bytes));
                }
            }
            C::Cut => {
                if let Some(bytes) = self.copy() {
                    self.out.events.push(EditorEvent::Clipboard(bytes));
                    self.delete_selection();
                }
            }
            C::Paste => self.out.events.push(EditorEvent::PasteRequested),
            C::CopyAsPng => {
                let ids = self.ordered_selection();
                if !ids.is_empty() {
                    self.out.events.push(EditorEvent::ExportPng(ids));
                }
            }
            C::Duplicate => {
                let ids = top_level(&self.scene, &self.selection);
                if !ids.is_empty() {
                    self.ctx()
                        .commit(|tx, scene| duplicate(tx, scene, &ids, Vec2::ZERO));
                }
            }
            C::Delete => self.delete_selection(),
            C::SelectAll => self.select_all(),
            C::Escape => {
                if self.tool.kind() != ToolKind::Move {
                    self.set_tool(ToolKind::Move);
                } else {
                    self.selection.clear();
                }
            }
            C::Group => self.wrap(NodeKind::Group),
            C::FrameSelection => self.wrap(NodeKind::Frame),
            C::Ungroup => self.ungroup(),
            C::BringForward => self.arrange(Arrange::Forward),
            C::SendBackward => self.arrange(Arrange::Backward),
            C::BringToFront => self.arrange(Arrange::Front),
            C::SendToBack => self.arrange(Arrange::Back),
            C::NudgeLeft => self.nudge(Vec2::new(-1.0, 0.0)),
            C::NudgeRight => self.nudge(Vec2::new(1.0, 0.0)),
            C::NudgeUp => self.nudge(Vec2::new(0.0, -1.0)),
            C::NudgeDown => self.nudge(Vec2::new(0.0, 1.0)),
            C::NudgeLeftBig => self.nudge(Vec2::new(-10.0, 0.0)),
            C::NudgeRightBig => self.nudge(Vec2::new(10.0, 0.0)),
            C::NudgeUpBig => self.nudge(Vec2::new(0.0, -10.0)),
            C::NudgeDownBig => self.nudge(Vec2::new(0.0, 10.0)),
            C::ZoomToFit => {
                if let Some(b) = self.scene.bounds() {
                    self.camera.fit(b, FIT_PADDING_PX);
                }
            }
            C::ZoomToSelection => {
                let b = self
                    .selection
                    .iter()
                    .filter_map(|id| self.scene.slot(*id))
                    .filter_map(|s| world_box(&self.scene, s))
                    .reduce(|a, b| a.union(b));
                if let Some(b) = b {
                    self.camera.fit(b, FIT_PADDING_PX);
                }
            }
            C::Zoom100 => self.camera.set_zoom(1.0),
            C::ZoomIn => self.zoom_center(2.0),
            C::ZoomOut => self.zoom_center(0.5),
            C::ToggleVisible => self.toggle(
                |p| p.visible,
                |v| PropsPatch {
                    visible: Some(v),
                    ..Default::default()
                },
            ),
            C::ToggleLock => self.toggle(
                |p| !p.locked,
                |v| PropsPatch {
                    locked: Some(!v),
                    ..Default::default()
                },
            ),
            C::ToggleUi => self.out.events.push(EditorEvent::ToggleUi),
        }
    }

    fn zoom_center(&mut self, factor: f64) {
        let v = self.camera.viewport();
        self.camera
            .zoom_at(Point::new(v.width / 2.0, v.height / 2.0), factor);
    }

    /// Выделение верхнего уровня в порядке отрисовки, снизу вверх.
    fn ordered_selection(&self) -> Vec<NodeId> {
        let mut ids: Vec<(Vec<u32>, NodeId)> = top_level(&self.scene, &self.selection)
            .into_iter()
            .filter_map(|id| self.scene.slot(id).map(|s| (self.scene.order_key(s), id)))
            .collect();
        ids.sort_by(|a, b| a.0.cmp(&b.0));
        ids.into_iter().map(|(_, id)| id).collect()
    }

    fn delete_selection(&mut self) {
        let ids = top_level(&self.scene, &self.selection);
        if ids.is_empty() {
            return;
        }
        self.ctx().commit(|tx, _| {
            tx.delete_nodes(&ids)?;
            Ok(Vec::new())
        });
    }

    /// Все соседи в контексте выделения (или объекты верхнего уровня), видимые и
    /// не заблокированные.
    fn select_all(&mut self) {
        let parent = self
            .selection
            .first()
            .and_then(|id| self.scene.node(*id))
            .and_then(|n| n.parent);
        let list = match parent {
            Some(p) => self.scene.at(p).children.clone(),
            None => self.scene.roots().to_vec(),
        };
        self.selection = list
            .into_iter()
            .filter(|s| self.scene.is_visible(*s) && !locked(&self.scene, *s))
            .map(|s| self.scene.at(s).id)
            .filter(|id| !self.scene.is_draft(*id))
            .collect();
    }

    /// Ctrl+G и Ctrl+Alt+G: оборачивает выделение в группу или фрейм. Контейнер
    /// встаёт на место верхнего выделенного узла; узлы остаются на экране там же.
    fn wrap(&mut self, kind: NodeKind) {
        let ids = self.ordered_selection();
        let Some(&top) = ids.last() else {
            return;
        };
        let top_slot = self.scene.slot(top).expect("выделение в сцене");
        let parent_slot = self.scene.at(top_slot).parent;
        let parent = parent_slot.map(|p| self.scene.at(p).id);
        let pw = parent_world(&self.scene, top_slot);
        let index = self.scene.index(top_slot) as usize + 1;
        // Рамка фрейма — общая рамка узлов в координатах родителя.
        let inv = pw.inverse();
        let frame_box = ids
            .iter()
            .filter_map(|id| self.scene.node(*id))
            .filter_map(|n| {
                n.local_bounds
                    .map(|b| (inv * n.world).transform_rect_bbox(b))
            })
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::ZERO);
        let origin = if kind == NodeKind::Frame {
            frame_box.origin()
        } else {
            Point::ZERO
        };
        self.ctx().commit(|tx, scene| {
            let c = tx.create_node(parent, Some(index), kind)?;
            let mut patch = PropsPatch {
                x: Some(origin.x),
                y: Some(origin.y),
                ..Default::default()
            };
            if kind == NodeKind::Frame {
                patch.w = Some(frame_box.width());
                patch.h = Some(frame_box.height());
            }
            tx.set_props(c, &patch)?;
            let c_world = pw * Affine::translate(origin.to_vec2());
            tx.move_nodes(&ids, Some(c), 0)?;
            for id in &ids {
                let n = scene.node(*id).expect("выделение в сцене");
                if n.parent == parent_slot {
                    // Тот же родитель: сдвиг точно, без ошибок округления матриц.
                    tx.set_props(
                        *id,
                        &PropsPatch {
                            x: Some(n.props.x - origin.x),
                            y: Some(n.props.y - origin.y),
                            ..Default::default()
                        },
                    )?;
                } else {
                    tx.set_props(*id, &reposition(c_world, n.world))?;
                }
            }
            Ok(vec![c])
        });
    }

    /// Ctrl+Shift+G: дети групп и фреймов выделения встают на их место у родителя.
    fn ungroup(&mut self) {
        let mut ids: Vec<(Vec<u32>, NodeId)> = top_level(&self.scene, &self.selection)
            .into_iter()
            .filter_map(|id| self.scene.slot(id).map(|s| (s, id)))
            .filter(|(s, _)| {
                let n = self.scene.at(*s);
                matches!(n.props.kind, NodeKind::Group | NodeKind::Frame) && !n.children.is_empty()
            })
            .map(|(s, id)| (self.scene.order_key(s), id))
            .collect();
        if ids.is_empty() {
            return;
        }
        // Сверху вниз: перенос в верхнее место не сдвигает позиции нижних.
        ids.sort_by(|a, b| b.0.cmp(&a.0));
        self.ctx().commit(|tx, scene| {
            let mut selection = Vec::new();
            for (_, g) in &ids {
                let slot = scene.slot(*g).expect("в сцене");
                let parent = scene.at(slot).parent.map(|p| scene.at(p).id);
                let pw = parent_world(scene, slot);
                let index = scene.index(slot) as usize;
                let children = tx.children(Some(*g));
                tx.move_nodes(&children, parent, index)?;
                for c in &children {
                    let world = scene.node(*c).expect("в сцене").world;
                    tx.set_props(*c, &reposition(pw, world))?;
                }
                tx.delete_nodes(&[*g])?;
                selection.extend(children);
            }
            Ok(selection)
        });
    }

    fn arrange(&mut self, op: Arrange) {
        let ids = self.ordered_selection();
        if ids.is_empty() {
            return;
        }
        // Узлы по родителям: позиция, число соседей.
        let mut groups: Vec<SiblingGroup> = Vec::new();
        for id in &ids {
            let s = self.scene.slot(*id).expect("в сцене");
            let parent_slot = self.scene.at(s).parent;
            let parent = parent_slot.map(|p| self.scene.at(p).id);
            let len = match parent_slot {
                Some(p) => self.scene.at(p).children.len(),
                None => self.scene.roots().len(),
            };
            let item = (self.scene.index(s) as usize, *id);
            match groups.iter_mut().find(|g| g.0 == parent) {
                Some(g) => g.2.push(item),
                None => groups.push((parent, len, vec![item])),
            }
        }
        let selection = self.selection.clone();
        self.ctx().commit(|tx, _| {
            for (parent, len, mut items) in groups {
                items.sort_by_key(|(i, _)| *i);
                let block: Vec<NodeId> = items.iter().map(|(_, id)| *id).collect();
                match op {
                    Arrange::Front => tx.move_nodes(&block, parent, usize::MAX)?,
                    Arrange::Back => tx.move_nodes(&block, parent, 0)?,
                    Arrange::Forward => {
                        // Сверху вниз: каждый выделенный меняется местами с соседом
                        // выше, если тот не выделен.
                        let mut taken: BTreeSet<usize> = items.iter().map(|(i, _)| *i).collect();
                        for (i, id) in items.iter().rev() {
                            if i + 1 < len && !taken.contains(&(i + 1)) {
                                tx.move_node(*id, parent, i + 1)?;
                                taken.remove(i);
                                taken.insert(i + 1);
                            }
                        }
                    }
                    Arrange::Backward => {
                        let mut taken: BTreeSet<usize> = items.iter().map(|(i, _)| *i).collect();
                        for (i, id) in items.iter() {
                            if *i > 0 && !taken.contains(&(i - 1)) {
                                tx.move_node(*id, parent, i - 1)?;
                                taken.remove(i);
                                taken.insert(i - 1);
                            }
                        }
                    }
                }
            }
            Ok(selection)
        });
    }

    /// Стрелки: сдвиг выделения на `d` экранных осей мира (1 или 10 px).
    fn nudge(&mut self, d: Vec2) {
        let ids: Vec<NodeId> = top_level(&self.scene, &self.selection)
            .into_iter()
            .filter(|id| {
                self.scene
                    .slot(*id)
                    .is_some_and(|s| !locked(&self.scene, s))
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        let selection = self.selection.clone();
        self.ctx().commit(|tx, scene| {
            for id in &ids {
                let slot = scene.slot(*id).expect("в сцене");
                let dl = local_delta(parent_world(scene, slot), d);
                let p = tx.props(*id)?;
                tx.set_props(
                    *id,
                    &PropsPatch {
                        x: Some(p.x + dl.x),
                        y: Some(p.y + dl.y),
                        ..Default::default()
                    },
                )?;
            }
            Ok(selection)
        });
    }

    /// Ctrl+Shift+H и Ctrl+Shift+L: если хоть один узел в состоянии «да» —
    /// все переходят в «нет», иначе все в «да».
    fn toggle(&mut self, get: fn(&Props) -> bool, set: fn(bool) -> PropsPatch) {
        let ids = top_level(&self.scene, &self.selection);
        if ids.is_empty() {
            return;
        }
        let any = ids
            .iter()
            .filter_map(|id| self.scene.node(*id))
            .any(|n| get(&n.props));
        let patch = set(!any);
        let selection = self.selection.clone();
        self.ctx().commit(|tx, _| {
            for id in &ids {
                tx.set_props(*id, &patch)?;
            }
            Ok(selection)
        });
    }

    /// Копия выделения в формате буфера обмена Ferrum; `None` — нечего копировать.
    pub fn copy(&mut self) -> Option<Vec<u8>> {
        let ids = self.ordered_selection();
        if ids.is_empty() {
            return None;
        }
        let page = self.doc.page(self.page).ok()?;
        let mut nodes: Vec<ClipNode> = Vec::new();
        for id in &ids {
            let n = self.scene.node(*id)?;
            let mut props = page.props(*id).ok()?;
            // Корень копии — с мировым положением и поворотом.
            let pose = decompose(n.world, 1.0, 1.0);
            props.x = pose.x;
            props.y = pose.y;
            props.rotation = pose.rotation;
            let root = nodes.len();
            nodes.push(ClipNode {
                parent: None,
                props,
            });
            let mut stack = vec![(*id, root)];
            while let Some((src, at)) = stack.pop() {
                for child in page.children(Some(src)) {
                    let props = page.props(child).ok()?;
                    stack.push((child, nodes.len()));
                    nodes.push(ClipNode {
                        parent: Some(at),
                        props,
                    });
                }
            }
        }
        let mut hashes: Vec<String> = nodes
            .iter()
            .flat_map(|n| n.props.fills.iter().filter_map(|f| f.image_hash.clone()))
            .collect();
        hashes.sort();
        hashes.dedup();
        let images = hashes
            .into_iter()
            .filter_map(|h| {
                let bytes = self.doc.images.get(&h)?.to_vec();
                Some((h, bytes))
            })
            .collect();
        let bounds = ids
            .iter()
            .filter_map(|id| self.scene.slot(*id))
            .filter_map(|s| world_box(&self.scene, s))
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::ZERO);
        Some(
            ClipPayload {
                nodes,
                images,
                bounds,
            }
            .encode(),
        )
    }

    /// Куда вставлять: в единственный выделенный фрейм, иначе к родителю
    /// выделения, иначе в корень. Родитель, его мировая матрица и рамка фрейма.
    fn paste_target(&self) -> (Option<NodeId>, Affine, Option<Rect>) {
        let frame_of = |id: NodeId| {
            let s = self.scene.slot(id)?;
            let n = self.scene.at(s);
            Some((Some(id), n.world, world_box(&self.scene, s)))
        };
        if let [one] = self.selection[..] {
            if self
                .scene
                .node(one)
                .is_some_and(|n| n.props.kind == NodeKind::Frame)
            {
                if let Some(t) = frame_of(one) {
                    return t;
                }
            }
        }
        let parent = self
            .selection
            .first()
            .and_then(|id| self.scene.node(*id))
            .and_then(|n| n.parent)
            .map(|p| self.scene.at(p).id);
        match parent.and_then(frame_of) {
            Some((id, world, b)) => {
                let is_frame = id
                    .and_then(|i| self.scene.node(i))
                    .is_some_and(|n| n.props.kind == NodeKind::Frame);
                (id, world, if is_frame { b } else { None })
            }
            None => (None, Affine::IDENTITY, None),
        }
    }

    /// Сдвиг вставки: на месте, если исходное место видно и помещается в цель,
    /// иначе — в центр цели (фрейма или видимой части холста).
    fn paste_offset(&self, bounds: Rect, target_box: Option<Rect>) -> Vec2 {
        let view = self.camera.visible_world_rect();
        let fits = target_box.is_none_or(|t| {
            t.contains(bounds.origin()) && t.contains(Point::new(bounds.x1, bounds.y1))
        });
        let visible = view.intersect(bounds).area() > 0.0 || view.contains(bounds.center());
        if fits && visible {
            Vec2::ZERO
        } else {
            target_box.unwrap_or(view).center() - bounds.center()
        }
    }

    /// Вставка данных Ferrum из буфера обмена одной транзакцией: узлы получают
    /// новые идентификаторы и выделяются. Данные не доверенные; при ошибке документ
    /// и история не меняются.
    pub fn paste(&mut self, bytes: &[u8]) -> Result<(), ClipboardError> {
        self.cancel_gesture();
        let payload = ClipPayload::decode(bytes)?;
        for (hash, data) in &payload.images {
            // Хэш считается заново: байты из буфера могли не совпасть с заявленным.
            // Несовпавшие байты в документе ни на что не ссылаются и в файл не попадут.
            if self.doc.images.get(hash).is_none() {
                self.doc.images.add(data.clone());
            }
        }
        let (target, target_world, target_box) = self.paste_target();
        let offset = self.paste_offset(payload.bounds, target_box);
        let shift = Affine::translate(offset);
        let ok = self.ctx().commit(|tx, _| {
            let mut made: Vec<NodeId> = Vec::with_capacity(payload.nodes.len());
            let mut roots = Vec::new();
            for n in &payload.nodes {
                let mut props = n.props.clone();
                let parent = match n.parent {
                    Some(i) => Some(made[i]),
                    None => {
                        let world = shift * ferrum_scene::shape::local_transform(&props);
                        let pose = decompose(target_world.inverse() * world, 1.0, 1.0);
                        props.x = pose.x;
                        props.y = pose.y;
                        props.rotation = pose.rotation;
                        target
                    }
                };
                let id = tx.create_node(parent, None, props.kind)?;
                tx.set_props(id, &full_patch(&props))?;
                if n.parent.is_none() {
                    roots.push(id);
                }
                made.push(id);
            }
            Ok(roots)
        });
        self.emit();
        if ok {
            Ok(())
        } else {
            Err(ClipboardError::Corrupted("не удалось вставить узлы".into()))
        }
    }

    /// Изображение из системного буфера или перетащенного файла: встаёт в центр
    /// видимой части (или выделенного фрейма) в исходном размере.
    pub fn insert_image(&mut self, hash: &str, width: f64, height: f64) {
        self.cancel_gesture();
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if !ok(width) || !ok(height) {
            self.out
                .events
                .push(EditorEvent::Error("У изображения нет размера".into()));
            self.emit();
            return;
        }
        let (target, target_world, target_box) = self.paste_target();
        let center = target_box.map_or(self.view_center(), |b| b.center());
        let origin = target_world.inverse() * (center - Vec2::new(width / 2.0, height / 2.0));
        let mut props = Props::defaults(NodeKind::Image);
        props.x = origin.x.round();
        props.y = origin.y.round();
        props.w = width;
        props.h = height;
        props.fills = vec![Fill::image(hash)];
        self.ctx().commit(|tx, _| {
            let id = tx.create_node(target, None, NodeKind::Image)?;
            tx.set_props(id, &full_patch(&props))?;
            Ok(vec![id])
        });
        self.emit();
    }

    /// Фрейм из пресета (раздел 7.5): справа от содержимого страницы, выделяется,
    /// камера переезжает к нему, если его не видно.
    pub fn add_frame_preset(&mut self, preset: &FramePreset) {
        self.cancel_gesture();
        let (x, y) = match self.scene.bounds() {
            Some(b) => ((b.x1 + PRESET_GAP).round(), b.y0.round()),
            None => {
                let c = self.view_center();
                (
                    (c.x - preset.width / 2.0).round(),
                    (c.y - preset.height / 2.0).round(),
                )
            }
        };
        let patch = PropsPatch {
            name: Some(preset.name.clone()),
            x: Some(x),
            y: Some(y),
            w: Some(preset.width),
            h: Some(preset.height),
            ..Default::default()
        };
        self.ctx().commit(|tx, _| {
            let id = tx.create_node(None, None, NodeKind::Frame)?;
            tx.set_props(id, &patch)?;
            Ok(vec![id])
        });
        let frame = Rect::new(x, y, x + preset.width, y + preset.height);
        if !self.camera.visible_world_rect().contains(frame.center()) {
            self.camera.set_center(frame.center());
        }
        self.emit();
    }
}
````

`crates/ferrum-editor/src/lib.rs`:

````rust
//! Редактор Ferrum без браузера: инструменты, выделение, привязки, команды и
//! горячие клавиши, буфер обмена. Получает события ввода простыми структурами,
//! правит документ транзакциями `ferrum-doc`, держит сцену с превью жеста, камеру и
//! оверлей для `ferrum-render`. Не зависит ни от браузера, ни от ОС и детерминирован:
//! одна и та же последовательность событий даёт один и тот же документ.

mod actions;
pub mod clipboard;
pub mod commands;
mod editor;
pub mod geometry;
pub mod input;
pub mod keymap;
mod ops;
pub mod presets;
pub mod selection;
pub mod snap;
pub mod tools;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
pub use commands::{Command, CommandId, COMMANDS};
pub use editor::{Editor, EditorEvent, Settings, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, Conflict, Keymap, KeymapError, Shortcut};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
````

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p ferrum-editor --test commands`
Expected: PASS, `10 passed`.

Run: `cargo test -p ferrum-editor --test clipboard`
Expected: PASS, `4 passed`.

Run: `cargo test -p ferrum-editor`
Expected: PASS — `keymap` 6, `geometry` 7, `clipboard_format` 4, `selection` 8, `draw` 9, `transform` 14, `commands` 10, `clipboard` 4.

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-editor
git commit -m "feat(editor): команды правки, порядка и вида, буфер обмена, пресеты фреймов"
```

---

### Task 7: Property-тест: случайные последовательности событий

Детерминированный ГСЧ (ChaCha с фиксированным зерном, без сохранения провалов в файлы). Действия — жесты (инструмент, якорь, сдвиг, модификаторы, переключение модификатора посреди жеста, число шагов, окончание: отпускание, Esc, потеря указателя, потеря фокуса), команды и колесо. Якорь жеста вычисляется при выполнении по текущему состоянию — доля рамки выделения или узла, с долей 1.1 снаружи угла, — поэтому жесты попадают в ручки, зону поворота, края и пустые места, а не в случайные пиксели. После каждого действия: жест закончился, превью и черновиков нет, сцена совпадает с построенной заново (вместе с R-tree), выделены только живые узлы документа. Каждое действие даёт не больше одного шага истории; если шаг появился, отмена возвращает документ (все узлы с родителями и свойствами) к состоянию до действия, повтор — к состоянию после. Счётчики против пустых прогонов требуют, чтобы каждый вид жеста действительно записывался и проверялся отменой.

**Files:**
- Test: `crates/ferrum-editor/tests/property.rs`

**Interfaces:**
- Consumes: Task 5–6, `ferrum_scene::testing::snapshot`.
- Produces: —

- [ ] **Step 1: Написать property-тест**

`crates/ferrum-editor/tests/property.rs`:

````rust
//! Случайные последовательности событий (детерминированный ГСЧ): редактор не
//! падает, после отпускания и Esc превью не остаётся, сцена совпадает с
//! построенной заново, а каждый записанный жест или команда — ровно один шаг,
//! отмена которого возвращает документ к прежнему состоянию.

mod common;

use std::cell::RefCell;

use common::*;
use ferrum_doc::{NodeKind, PropsPatch};
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::*;
use ferrum_scene::Scene;
use kurbo::Point;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestRng, TestRunner};

/// Точка относительно узла или рамки выделения: доли рамки и сдвиг в пикселях.
/// Доля 3 — чуть снаружи угла (зона поворота).
#[derive(Debug, Clone)]
struct Anchor {
    node: u8,
    fx: u8,
    fy: u8,
    jx: i8,
    jy: i8,
}

#[derive(Debug, Clone, Copy)]
enum End {
    Up,
    Escape,
    Cancel,
    Blur,
}

#[derive(Debug, Clone)]
enum Act {
    Gesture {
        tool: u8,
        at: Anchor,
        to: (i16, i16),
        mods: u8,
        /// Шаг, на котором меняется модификатор (Shift или Alt).
        toggle: u8,
        steps: u8,
        end: End,
    },
    Key(u8),
    Wheel {
        dy: i8,
        zoom: bool,
    },
}

const COMMANDS_USED: [CommandId; 22] = [
    CommandId::Undo,
    CommandId::Redo,
    CommandId::Delete,
    CommandId::Duplicate,
    CommandId::Group,
    CommandId::Ungroup,
    CommandId::FrameSelection,
    CommandId::BringForward,
    CommandId::SendBackward,
    CommandId::BringToFront,
    CommandId::SendToBack,
    CommandId::NudgeRight,
    CommandId::NudgeDownBig,
    CommandId::ToggleVisible,
    CommandId::ToggleLock,
    CommandId::SelectAll,
    CommandId::Escape,
    CommandId::ZoomToFit,
    CommandId::ZoomOut,
    CommandId::Copy,
    CommandId::Cut,
    CommandId::Paste,
];

const TOOL_KEYS: [&str; 6] = ["KeyV", "KeyF", "KeyR", "KeyO", "KeyL", "KeyH"];

fn act() -> impl Strategy<Value = Act> {
    let anchor =
        (any::<u8>(), 0u8..4, 0u8..4, -4i8..5, -4i8..5).prop_map(|(node, fx, fy, jx, jy)| Anchor {
            node,
            fx,
            fy,
            jx,
            jy,
        });
    let end = prop_oneof![
        8 => Just(End::Up),
        1 => Just(End::Escape),
        1 => Just(End::Cancel),
        1 => Just(End::Blur),
    ];
    prop_oneof![
        8 => (
            prop_oneof![6 => Just(0u8), 1 => 1u8..6],
            anchor,
            (-160i16..160, -160i16..160),
            0u8..8,
            0u8..6,
            1u8..5,
            end,
        )
            .prop_map(|(tool, at, to, mods, toggle, steps, end)| Act::Gesture {
                tool,
                at,
                to,
                mods,
                toggle,
                steps,
                end,
            }),
        3 => (0u8..COMMANDS_USED.len() as u8).prop_map(Act::Key),
        1 => (any::<i8>(), any::<bool>()).prop_map(|(dy, zoom)| Act::Wheel { dy, zoom }),
    ]
}

fn mods(bits: u8) -> Modifiers {
    Modifiers {
        shift: bits & 1 != 0,
        alt: bits & 2 != 0,
        ctrl: bits & 4 != 0,
        meta: false,
    }
}

/// Стартовая страница: фрейм с прямоугольником, эллипсом и группой; в корне —
/// прямоугольник, повёрнутый прямоугольник и линия.
fn setup() -> H {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 400.0, 300.0);
    h.rect(Some(f), 20.0, 20.0, 80.0, 60.0);
    h.add(Some(f), NodeKind::Ellipse, patch(150.0, 40.0, 60.0, 60.0));
    let g = h.add(Some(f), NodeKind::Group, PropsPatch::default());
    h.rect(Some(g), 40.0, 150.0, 50.0, 50.0);
    h.rect(Some(g), 110.0, 170.0, 50.0, 50.0);
    h.rect(None, 600.0, 150.0, 100.0, 80.0);
    let t = h.rect(None, 650.0, 400.0, 80.0, 40.0);
    h.set(
        t,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    h.add(None, NodeKind::Line, patch(100.0, 600.0, 200.0, 0.0));
    h
}

/// Экранная точка якоря при текущем состоянии.
fn resolve(h: &H, a: &Anchor) -> Point {
    let scene = h.ed.scene();
    let frac = |f: u8| [0.0, 0.5, 1.0, 1.1][f as usize];
    let world = if a.node.is_multiple_of(3) && !h.ed.selection().is_empty() {
        SelectionBox::of(scene, h.ed.selection()).map(|b| b.point(frac(a.fx), frac(a.fy)))
    } else {
        let mut all = Vec::new();
        let mut stack: Vec<_> = scene.roots().to_vec();
        while let Some(s) = stack.pop() {
            all.push(s);
            stack.extend(scene.at(s).children.iter().copied());
        }
        if all.is_empty() || a.node.is_multiple_of(7) {
            None
        } else {
            let id = scene.at(all[a.node as usize % all.len()]).id;
            SelectionBox::of(scene, &[id]).map(|b| b.point(frac(a.fx), frac(a.fy)))
        }
    };
    let world = world.unwrap_or(Point::new(900.0, 750.0));
    let s = h.ed.camera().world_to_screen(world);
    Point::new(s.x + a.jx as f64, s.y + a.jy as f64)
}

#[derive(Default, Debug)]
struct Stats {
    gesture_steps: u64,
    draw_steps: u64,
    resize_steps: u64,
    rotate_steps: u64,
    duplicate_steps: u64,
    reparent_steps: u64,
    command_steps: u64,
    cancelled_previews: u64,
    marquees: u64,
    undo_checks: u64,
}

fn check_clean(h: &mut H, what: &str) -> Result<(), TestCaseError> {
    prop_assert!(!h.ed.is_busy(), "{what}: жест не закончился");
    prop_assert!(!h.ed.scene().has_preview(), "{what}: превью осталось");
    let fresh = Scene::from_page(h.ed.page_doc());
    prop_assert_eq!(
        ferrum_scene::testing::snapshot(h.ed.scene()),
        ferrum_scene::testing::snapshot(&fresh),
        "{}: сцена разошлась с документом",
        what
    );
    for id in h.ed.selection() {
        prop_assert!(
            h.ed.scene().node(*id).is_some(),
            "{what}: выделен удалённый"
        );
        prop_assert!(!h.ed.scene().is_draft(*id), "{what}: выделен черновик");
    }
    Ok(())
}

fn run(acts: &[Act], stats: &mut Stats) -> Result<(), TestCaseError> {
    let mut h = setup();
    let mut clipboard: Option<Vec<u8>> = None;
    for (i, a) in acts.iter().enumerate() {
        let what = format!("шаг {i}: {a:?}");
        let before = h.state();
        let steps = h.ed.step_count();
        let parents: Vec<_> = before.iter().map(|(id, p, _)| (*id, *p)).collect();
        let mut kind = "";
        match a {
            Act::Gesture {
                tool,
                at,
                to,
                mods: bits,
                toggle,
                steps: n,
                end,
            } => {
                if *tool > 0 {
                    h.key(TOOL_KEYS[*tool as usize], none());
                }
                let m = mods(*bits);
                let p = resolve(&h, at);
                h.hover(p.x, p.y);
                kind = match (h.ed.tool_kind(), h.ed.cursor()) {
                    (ToolKind::Move, Cursor::Resize { .. }) => "resize",
                    (ToolKind::Move, Cursor::Rotate { .. }) => "rotate",
                    (ToolKind::Move, _) => "move",
                    (ToolKind::Hand, _) => "hand",
                    _ => "draw",
                };
                h.down(p.x, p.y, m);
                let mut cur = m;
                for k in 1..=*n {
                    if k == *toggle {
                        // Модификатор меняется посреди жеста.
                        cur.shift = !cur.shift;
                        cur.alt = !cur.alt && bits & 4 == 0;
                        h.ev(Input::KeyDown(KeyEvent::new("ShiftLeft", cur)));
                    }
                    let t = k as f64 / *n as f64;
                    h.mv(p.x + to.0 as f64 * t, p.y + to.1 as f64 * t, cur);
                    if h.ed.overlay().marquee.is_some() {
                        stats.marquees += 1;
                    }
                }
                if h.ed.cursor() == Cursor::Duplicate {
                    kind = "duplicate";
                }
                let had_preview = h.ed.scene().has_preview();
                let (ex, ey) = (p.x + to.0 as f64, p.y + to.1 as f64);
                match end {
                    End::Up => h.up(ex, ey, cur),
                    End::Escape => {
                        h.ev(Input::KeyDown(KeyEvent::new("Escape", none())));
                        h.up(ex, ey, cur);
                    }
                    End::Cancel => h.ev(Input::PointerCancel),
                    End::Blur => h.ev(Input::Blur),
                }
                if !matches!(end, End::Up) {
                    prop_assert_eq!(
                        h.ed.step_count(),
                        steps,
                        "{}: отменённый жест записан",
                        what
                    );
                    if had_preview {
                        stats.cancelled_previews += 1;
                    }
                }
            }
            Act::Key(k) => {
                let cmd = COMMANDS_USED[*k as usize];
                kind = "command";
                if cmd == CommandId::Paste {
                    if let Some(bytes) = &clipboard {
                        h.ed.paste(bytes)
                            .map_err(|e| TestCaseError::fail(format!("{what}: {e}")))?;
                    }
                } else {
                    h.ed.execute(cmd);
                }
                for e in h.ed.take_events() {
                    if let EditorEvent::Clipboard(b) = e {
                        clipboard = Some(b);
                    }
                }
                if matches!(cmd, CommandId::Undo | CommandId::Redo) {
                    kind = "history";
                }
            }
            Act::Wheel { dy, zoom } => {
                let m = if *zoom { ctrl() } else { none() };
                h.ev(Input::Wheel(WheelEvent {
                    point: Point::new(500.0, 400.0),
                    delta: v(0.0, *dy as f64),
                    modifiers: m,
                }));
            }
        }
        check_clean(&mut h, &what)?;
        let made = h.ed.step_count() - steps;
        prop_assert!(made <= 1, "{}: больше одного шага ({})", what, made);
        if kind == "history" || made == 0 {
            continue;
        }
        let after = h.state();
        match kind {
            "command" => stats.command_steps += 1,
            "draw" => stats.draw_steps += 1,
            "resize" => stats.resize_steps += 1,
            "rotate" => stats.rotate_steps += 1,
            "duplicate" => stats.duplicate_steps += 1,
            _ => {}
        }
        if kind != "command" {
            stats.gesture_steps += 1;
            let moved = after
                .iter()
                .any(|(id, p, _)| parents.iter().any(|(q, pp)| q == id && pp != p));
            if moved {
                stats.reparent_steps += 1;
            }
        }
        // Отмена возвращает документ к состоянию до шага, повтор — к состоянию после.
        h.ed.execute(CommandId::Undo);
        prop_assert_eq!(&h.state(), &before, "{}: отмена не вернула документ", what);
        check_clean(&mut h, &what)?;
        h.ed.execute(CommandId::Redo);
        prop_assert_eq!(&h.state(), &after, "{}: повтор не вернул правку", what);
        check_clean(&mut h, &what)?;
        stats.undo_checks += 1;
    }
    Ok(())
}

#[test]
fn random_event_sequences_keep_editor_consistent() {
    let stats = RefCell::new(Stats::default());
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 96,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    runner
        .run(&prop::collection::vec(act(), 1..40), |acts| {
            run(&acts, &mut stats.borrow_mut())
        })
        .unwrap();
    let st = stats.into_inner();
    eprintln!("{st:?}");
    // Против пустых прогонов: каждый вид жеста действительно записывался и
    // проверялся отменой.
    assert!(st.gesture_steps > 300, "{st:?}");
    assert!(st.draw_steps > 40, "{st:?}");
    assert!(st.resize_steps > 30, "{st:?}");
    assert!(st.rotate_steps > 10, "{st:?}");
    assert!(st.duplicate_steps > 20, "{st:?}");
    assert!(st.reparent_steps > 10, "{st:?}");
    assert!(st.command_steps > 150, "{st:?}");
    assert!(st.cancelled_previews > 30, "{st:?}");
    assert!(st.marquees > 30, "{st:?}");
}
````

- [ ] **Step 2: Запустить тест**

Run: `cargo test -p ferrum-editor --test property`
Expected: PASS, `1 passed`, около 2 с в debug. В прототипе за прогон набиралось 524 записанных жеста (149 рисований, 113 ресайзов, 18 поворотов, 96 копий с Alt, 27 переносов во фрейм), 207 команд с изменениями, 195 отменённых жестов с превью и 731 проверку отменой — пороги в тесте взяты с запасом.

Проверка, что тест не пустой (в прототипе, после неё код вернуть): если в `Ctx::commit` заменить `sync(…)` на `self.doc.take_changes(self.page)` (сцена перестаёт получать изменения), тест падает на первом записанном жесте с «сцена разошлась с документом»; если в `commit_patches` перед основной транзакцией записать ещё одну (переименование первого узла), — с «больше одного шага (2)».

- [ ] **Step 3: Закоммитить**

```bash
git add crates/ferrum-editor/tests/property.rs
git commit -m "test(editor): случайные последовательности событий против отмены и сцены"
```

---

### Task 8: Контрольный бенчмарк и финальная проверка

`editor_report` строит страницу из 100 000 узлов генератором этапа 1 (фреймы по 100: фрейм 1440 × 1024 и 99 фигур 120 × 80 сеткой) и гоняет редактор теми же событиями, что придут из основного потока. Замеры — медианы; худшие значения печатаются справочно. Перетаскивание 1000 выделенных — детей первых фреймов верхнего ряда: в превью 1000 узлов, привязки ищутся по полосам через видимую часть. Запись и отмена — пять повторов: отпускание (транзакция 1000 правок и обновление сцены) и `Undo`. Наведение — 400 движений по видимой части без кнопки. Рамка выделения — с пустого холста при зуме 0.1 (~100 фреймов в кадре) и через всю страницу после «показать всё»: обе цели ≤ 4 мс.

**Files:**
- Create: `crates/ferrum-editor/examples/editor_report.rs`

**Interfaces:**
- Consumes: всё выше, `ferrum_doc::fixture::fill`.
- Produces: бенчмарк, код выхода 1 при провале.

- [ ] **Step 1: Написать бенчмарк**

`crates/ferrum-editor/examples/editor_report.rs`:

````rust
//! Контрольный бенчмарк редактора:
//! `cargo run --release -p ferrum-editor --example editor_report`.
//! Страница из 100 000 узлов; сверяет замеры с целями этапа 3 и печатает таблицу
//! PASS/FAIL (код выхода 1 при провале).

use std::time::{Duration, Instant};

use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeId};
use ferrum_editor::{CommandId, Editor, Input, Modifiers, Platform, PointerEvent};
use ferrum_render::Camera;
use kurbo::{Point, Size};

const NODES: usize = 100_000;
const VIEWPORT: Size = Size::new(1920.0, 1080.0);
/// Движений указателя в замере одного жеста.
const MOVES: usize = 60;
/// Повторов записи и отмены перетаскивания (медиана).
const COMMIT_RUNS: usize = 5;

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn worst(runs: &[Duration]) -> Duration {
    runs.iter().copied().max().unwrap_or_default()
}

fn ms(d: Duration) -> String {
    format!("{:.3} мс", d.as_secs_f64() * 1000.0)
}

struct Row {
    metric: String,
    value: String,
    limit: &'static str,
    pass: Option<bool>,
}

struct Report(Vec<Row>);

impl Report {
    fn gate(&mut self, metric: &str, d: Duration, limit_ms: f64, limit: &'static str) {
        self.0.push(Row {
            metric: metric.to_string(),
            value: ms(d),
            limit,
            pass: Some(d.as_secs_f64() * 1000.0 <= limit_ms),
        });
    }

    fn check(&mut self, metric: &str, ok: bool, value: String, limit: &'static str) {
        self.0.push(Row {
            metric: metric.to_string(),
            value,
            limit,
            pass: Some(ok),
        });
    }

    fn info(&mut self, metric: &str, value: String) {
        self.0.push(Row {
            metric: metric.to_string(),
            value,
            limit: "—",
            pass: None,
        });
    }
}

/// Часы событий: каждое следующее на 600 мс позже — клики не склеиваются в двойные.
struct Clock(f64);

impl Clock {
    fn at(&mut self, p: Point) -> PointerEvent {
        self.0 += 600.0;
        PointerEvent::new(p, Modifiers::NONE, self.0)
    }
}

fn timed(ed: &mut Editor, input: Input) -> Duration {
    let t = Instant::now();
    ed.handle(input);
    let d = t.elapsed();
    ed.take_events();
    d
}

/// Экранная точка мира.
fn screen(ed: &Editor, x: f64, y: f64) -> Point {
    ed.camera().world_to_screen(Point::new(x, y))
}

/// Перетаскивание от экранной точки `from`: время каждого движения (превью и
/// привязки) и состояние «кнопка ещё нажата».
fn drag_moves(ed: &mut Editor, clock: &mut Clock, from: Point, step: f64) -> Vec<Duration> {
    ed.handle(Input::PointerDown(clock.at(from)));
    // Первое движение начинает жест (порог 3 px); оно тоже в замере.
    (1..=MOVES)
        .map(|i| {
            let p = Point::new(from.x + step * i as f64, from.y + 0.37 * i as f64);
            timed(ed, Input::PointerMove(clock.at(p)))
        })
        .collect()
}

fn main() {
    let mut report = Report(Vec::new());

    // Документ из 100 000 узлов: фреймы по 100 (фрейм + 99 фигур 120×80 сеткой 10×10).
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids: Vec<NodeId> = doc.transact(page, vec![], |p| fill(p, NODES)).unwrap();
    let mut ed = Editor::new(doc, Some(page), VIEWPORT, 1.0, Platform::Mac).unwrap();
    assert_eq!(ed.scene().len(), NODES);
    let mut cam = Camera::new(VIEWPORT, 1.0);
    cam.set_center(Point::new(960.0, 540.0));
    ed.set_camera(cam.clone());
    let mut clock = Clock(0.0);

    // 1. Перетаскивание 1000 выделенных: дети первых фреймов верхнего ряда.
    let children: Vec<NodeId> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 100 != 0)
        .map(|(_, id)| *id)
        .take(1000)
        .collect();
    ed.select(&children);
    // Первый ребёнок первого фрейма: (0, 0) — 120×80.
    let grab = screen(&ed, 60.0, 40.0);
    let moves = drag_moves(&mut ed, &mut clock, grab, 1.7);
    assert!(
        ed.is_busy() && ed.scene().has_preview(),
        "перетаскивание идёт"
    );
    report.gate(
        "Движение при перетаскивании 1000 выделенных: превью и привязки (медиана)",
        median(moves.clone()),
        4.0,
        "≤ 4 мс",
    );
    report.info(
        "То же, худшее движение (включая начало жеста)",
        ms(worst(&moves)),
    );
    let end = Point::new(grab.x + 1.7 * MOVES as f64, grab.y + 0.37 * MOVES as f64);
    ed.handle(Input::PointerUp(clock.at(end)));
    ed.execute(CommandId::Undo);
    ed.take_events();

    // 2. Запись перетаскивания 1000 узлов одной транзакцией и её отмена.
    let mut commits = Vec::new();
    let mut undos = Vec::new();
    let steps0 = ed.step_count();
    for _ in 0..COMMIT_RUNS {
        ed.select(&children);
        drag_moves(&mut ed, &mut clock, grab, 1.7);
        commits.push(timed(&mut ed, Input::PointerUp(clock.at(end))));
        let t = Instant::now();
        ed.execute(CommandId::Undo);
        undos.push(t.elapsed());
        ed.take_events();
    }
    assert_eq!(
        ed.step_count(),
        steps0 + COMMIT_RUNS as u64,
        "каждый жест — один шаг"
    );
    report.gate(
        "Запись перетаскивания 1000 узлов (отпускание: транзакция и сцена)",
        median(commits),
        20.0,
        "≤ 20 мс",
    );
    report.gate(
        "Отмена перетаскивания 1000 узлов (документ и сцена)",
        median(undos),
        20.0,
        "≤ 20 мс",
    );

    // 3. Один узел среди 98 соседей и фрейма: привязки к ним.
    let one = children[50];
    ed.select(&[one]);
    let n = ed.scene().node(one).unwrap();
    let c = n.world_bounds.unwrap().center();
    let from = screen(&ed, c.x, c.y);
    let moves = drag_moves(&mut ed, &mut clock, from, 2.3);
    let lines = ed.overlay().snap_lines.len();
    report.gate(
        "Движение одного узла с привязками к 100 соседям (медиана)",
        median(moves.clone()),
        1.0,
        "≤ 1 мс",
    );
    report.info(
        "То же: худшее движение / линий привязки в конце",
        format!("{} / {lines}", ms(worst(&moves))),
    );
    ed.execute(CommandId::Escape);
    ed.handle(Input::PointerUp(clock.at(from)));
    ed.take_events();

    // 4. Наведение: hit-test и правила выделения на каждое движение без кнопки.
    ed.select(&[]);
    let hovers: Vec<Duration> = (0..400)
        .map(|i| {
            let p = Point::new((i * 37 % 1900) as f64 + 7.0, (i * 53 % 1060) as f64 + 9.0);
            timed(&mut ed, Input::PointerMove(clock.at(p)))
        })
        .collect();
    report.gate(
        "Наведение: hit-test и выбор объекта (медиана)",
        median(hovers.clone()),
        0.2,
        "≤ 0.2 мс",
    );
    report.info("То же, худшее движение", ms(worst(&hovers)));

    // 5. Рамка выделения: с пустого холста при отдалении, ~100 фреймов в кадре.
    let mut far = Camera::new(VIEWPORT, 1.0);
    far.set_zoom(0.1);
    far.set_center(Point::new(9600.0, 5400.0));
    ed.set_camera(far);
    let start = screen(&ed, 1520.0, -60.0);
    ed.handle(Input::PointerDown(clock.at(start)));
    let marquee: Vec<Duration> = (1..=MOVES)
        .map(|i| {
            let k = i as f64 / MOVES as f64;
            let p = Point::new(start.x + 1800.0 * k, start.y + 1000.0 * k);
            timed(&mut ed, Input::PointerMove(clock.at(p)))
        })
        .collect();
    let selected = ed.selection().len();
    report.gate(
        "Обновление рамки выделения на странице 100 000 (медиана, ~100 фреймов в рамке)",
        median(marquee.clone()),
        4.0,
        "≤ 4 мс",
    );
    report.info(
        "То же: худшее обновление / выделено фреймов в конце",
        format!("{} / {selected}", ms(worst(&marquee))),
    );
    ed.handle(Input::PointerUp(clock.at(start)));

    // Рамка через всю страницу при «показать всё» — справочно.
    ed.execute(CommandId::ZoomToFit);
    let corner = screen(&ed, -100.0, -100.0);
    ed.handle(Input::PointerDown(clock.at(corner)));
    let far_corner = screen(&ed, 80_000.0, 24_000.0);
    ed.handle(Input::PointerMove(
        clock.at(Point::new(corner.x + 5.0, corner.y + 5.0)),
    ));
    let whole = timed(&mut ed, Input::PointerMove(clock.at(far_corner)));
    let frames = ed.selection().len();
    report.gate(
        "Обновление рамки через всю страницу 100 000 (выделяется 1000 фреймов)",
        whole,
        4.0,
        "≤ 4 мс",
    );
    report.check(
        "Рамка через всю страницу выделила все фреймы верхнего уровня",
        frames == NODES / 100,
        format!("{frames}"),
        "1000",
    );
    ed.handle(Input::PointerUp(clock.at(far_corner)));
    report.check(
        "Превью не осталось после жестов",
        !ed.scene().has_preview() && !ed.is_busy(),
        format!("{}", ed.scene().has_preview()),
        "false",
    );

    println!("| Метрика | Значение | Цель | Итог |");
    println!("|---|---|---|---|");
    let mut failed = false;
    for r in &report.0 {
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
    println!("Замер нативный, release, 100 000 узлов. WASM ожидается в 1.5–2 раза медленнее.");
    if failed {
        std::process::exit(1);
    }
}
````

- [ ] **Step 2: Запустить бенчмарк**

Run: `cargo run --release -p ferrum-editor --example editor_report`
Expected: таблица, все строки с целью — PASS, код выхода 0. В прототипе (Apple M1 Pro): движение при перетаскивании 1000 — 1.5 мс; запись — 4.9 мс; отмена — 2.8 мс; один узел среди 100 соседей — 0.008 мс; наведение — 0.001 мс; рамка при отдалении — 0.04 мс (худшее 0.4 мс); рамка через всю страницу — 0.15 мс.

- [ ] **Step 3: Полная проверка**

Run: `cargo fmt --check`
Expected: без вывода, код 0.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: без предупреждений.

Run: `cargo clippy -p ferrum-render --all-targets --features gpu -- -D warnings`
Expected: без предупреждений.

Run: `cargo test --workspace`
Expected: PASS — все наборы этапов 1–3.

Run: `cargo bench --workspace --no-run`
Expected: сборка бенчмарков без ошибок.

Run: `cargo run --release -p ferrum-render --example render_report`
Expected: все строки PASS, код 0 — черновики сцены не замедлили рендер.

Run: `cargo run --release -p ferrum-doc --example bench_report`
Expected: все строки PASS, код 0.

- [ ] **Step 4: Закоммитить**

```bash
git add crates/ferrum-editor/examples/editor_report.rs
git commit -m "bench(editor): контрольный бенчмарк редактора на 100 000 узлов"
```

---

## Что этап 3 сознательно не делает

- **Связь с интерфейсом и worker'ом.** Кольцевой буфер ввода, `postMessage`, генерация TypeScript-типов событий, системный буфер обмена, диалог выбора файла, CSS-курсоры и подписи сочетаний по `navigator.keyboard.getLayoutMap()` — этапы 4 и 5. Редактор отдаёт события и байты, интерфейс доставляет их.
- **Хранение настроек и раскладки.** Отличия раскладки сериализуются в JSON, но кладёт их в IndexedDB или файл конфигурации интерфейс (этапы 5–6).
- **Подсказки расстояний и измерения с Alt** — позже (раздел 2 спеки). Привязки — только к краям и центрам соседей и родительского фрейма в видимой части холста.
- **Живой перенос во фрейм во время перетаскивания.** Превью меняет только свойства, поэтому узел, который тащат во фрейм, во время жеста не обрезается им; родитель меняется при отпускании.
- **Автоматическое усыновление при рисовании фрейма поверх объектов** (в Figma узлы, целиком накрытые новым фреймом, становятся его детьми) — вместе с панелью слоёв.
- **Ограничения (constraints) детей фрейма при его ресайзе и масштабирование скругления и обводок при ресайзе групп** — с auto layout (подсистема 5). Дети фрейма при ресайзе фрейма стоят на месте.
- **Перенос копий Alt+перетаскивания во фрейм под курсором.** Копии создаются прямо над оригиналами у их родителей; перенос во фрейм при отпускании — только у самих перетаскиваемых узлов.
- **Привязка копий Alt+перетаскивания к их оригиналам.** Оригиналы во время жеста стоят на месте, но в кандидаты привязки не входят: копия привязывается к соседям, как сами двигающиеся узлы.
- **Повтор вставки со смещением, «умный» дубликат (повтор последнего сдвига)** — с полировкой оболочки (подсистема 2).
- **Текст, перо, булевы операции, auto layout, компоненты, прототипы, совместная работа** — другие подсистемы.
- **Экспорт PNG выделения из нескольких узлов** — `ExportPng` передаёт список узлов, а склейку делает этап 4 через `Renderer::export_png`.
