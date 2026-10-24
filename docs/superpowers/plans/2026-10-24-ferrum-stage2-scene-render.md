# Ferrum, подсистема 1, этап 2: сцена и рендер — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Крейты `ferrum-scene` (мировые трансформации, границы, R-tree, hit-testing, превью жеста, обновление по изменениям документа) и `ferrum-render` (камера, списки отрисовки, отсечение, LOD, растровый кэш фреймов, оверлеи с подписями, экспорт PNG, бэкенды `vello_cpu` и `vello_gpu`), эталонные картинки и контрольный бенчмарк на 100 000 узлов.

**Architecture:** `ferrum-doc` сообщает, какие узлы изменила транзакция, отмена или повтор (`Document::take_changes`). `ferrum-scene` держит плотную таблицу узлов с мировыми матрицами (`kurbo::Affine`, f64), границами узла и его поддерева и R-tree (`rstar`) по мировым границам; по `Changes` пересчитываются только затронутые узлы, их потомки и предки. `ferrum-render` обходит сцену, отбрасывая поддеревья вне вьюпорта по границам поддерева, и воспроизводит в бэкенд списки отрисовки узлов в локальных координатах (строятся один раз на изменение вида узла). Бэкенд — трейт с общим подмножеством API `vello_cpu` и `vello_gpu` (sparse strips): пути разбираются на каждом кадре, поэтому кэшируются описание и контуры узла, а не закодированные фрагменты сцены. При сильном отдалении фреймы верхнего уровня рисуются из растрового кэша.

**Tech Stack:** Rust 1.96, kurbo 0.13.1, rstar 0.12.2, vello_cpu 0.3.0, vello_gpu 0.3.0 (фича `gpu`), parley 0.12.0, skrifa 0.44.0, thiserror 2, proptest 1.12; шрифт Onest (SIL OFL 1.1).

**Спека:** `docs/superpowers/specs/2026-10-23-ferrum-core-design.md`, разделы 3, 4, 5.5, 6, 8, 12.

## Global Constraints

- Rust toolchain `1.96.0`, edition 2021. Все зависимости зафиксированы точными версиями (`=x.y.z`), и каждая версия в `Cargo.lock` опубликована не позже 2026-10-10 (не моложе 14 дней на 2026-10-24); шаги «Закрепить зависимости» приводят `Cargo.lock` к этим версиям.
- `rstar = "=0.12.2"`, не 0.13.0: в 0.13.0 вставка после `bulk_load` падает с «This is a bug in rstar» (листья на разной глубине; исправление georust/rstar#254 не выпущено). Проверено на таблице из 100 000 узлов.
- `ferrum-scene` и `ferrum-render` не видят Loro: только публичный API `ferrum-doc`. `ferrum-render` не знает об инструментах: получает сцену, камеру и описание оверлеев.
- Классический `vello` (compute) не используется. Рендер рисует только через трейт `RenderBackend`; `vello_cpu` — эталонные тесты, экспорт, запасной рендер; `vello_gpu` — за фичей `gpu`, на этом этапе только сборка и кодирование кадра (поверхности, устройство и загрузка изображений — этап 4).
- Ни одна операция не делает O(соседей) или O(n) работы на каждый изменённый узел; всё, что видит пользователь, включая отмену пачек, есть в бенчмарке.
- Данные из файла (строки кэша сцены, PNG) не доверенные: нечисловые и бесконечные значения, отрицательные размеры, повторы `NodeId`, огромные картинки не роняют и не вешают рендер.
- Поворот — в градусах, вокруг левого верхнего угла узла, положительный — против часовой стрелки на экране (как в Figma).
- Матрица «мир → пиксели» собирается в `f64` относительно центра камеры; в `f32` переводятся только экранные координаты (внутри vello).
- Тексты ошибок — на русском, для пользователя. Комментарии в коде — на русском, объясняют «почему».
- Цели бенчмарка (100 000 узлов, release, нативно): построение сцены из кэша ≤ 100 мс; `Scene::apply` для 1000 узлов (правка, перенос, удаление и их отмена) и превью перетаскивания 1000 узлов ≤ 5 мс; hit-test ≤ 0.1 мс; отсечение и воспроизведение списков в бэкенд для типичного вьюпорта (~2000 узлов) ≤ 4 мс без растеризации; вся страница при отдалении ≤ 8 мс; растеризация CPU — справочно.

---

## Этапы подсистемы 1

Подсистема 1 делится на шесть этапов (таблица — в плане этапа 1, `docs/superpowers/plans/2026-10-23-ferrum-stage1-document-model.md`). Этот документ — план этапа 2:

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Модель документа (готов) | `ferrum-doc` | 3, 5 |
| **2. Сцена и рендер** (этот план) | `ferrum-scene`, `ferrum-render`, эталонные картинки через `vello_cpu`, бенчмарк `render_report` | 6, 8 |
| 3. Редактор без браузера | `ferrum-editor` | 7 |
| 4. Worker и обмен данными | `ferrum-wasm`, OffscreenCanvas, `vello_gpu` на WebGPU/WebGL2 | 4.2–4.3, 8.1–8.2 |
| 5. Интерфейс | `apps/web` | 4.4, 9 |
| 6. Файлы и десктоп | `FileStore`, автосохранение, Tauri, E2E | 10, 11, 12 |

---

## Структура файлов этапа 2

| Файл | Ответственность |
|---|---|
| `Cargo.toml` | новые крейты и зависимости workspace |
| `crates/ferrum-doc/src/loro_ext.rs` | единственная точка обращения к `loro-internal` |
| `crates/ferrum-doc/src/changes.rs` | `Changes`: какие узлы изменила правка, отмена, повтор |
| `crates/ferrum-doc/src/{page,document,codec,lib}.rs` | `placement`, `take_changes`, ссылки скрытых узлов на изображения |
| `crates/ferrum-scene/src/shape.rs` | локальная трансформация, проверка свойств, форма, кольца обводок, точное попадание |
| `crates/ferrum-scene/src/scene.rs` | `Scene`: таблица узлов, трансформации, границы, R-tree, `apply`, превью, hit-test |
| `crates/ferrum-render/src/backend.rs` | трейт `RenderBackend`, `CpuBackend` (`vello_cpu`) |
| `crates/ferrum-render/src/gpu.rs` | `GpuBackend` (`vello_gpu`, фича `gpu`) |
| `crates/ferrum-render/src/camera.rs` | камера: центр, зум, экран ↔ мир, `devicePixelRatio` |
| `crates/ferrum-render/src/image.rs` | поставщик изображений, проверенное декодирование PNG |
| `crates/ferrum-render/src/display.rs` | список отрисовки узла и его контуры |
| `crates/ferrum-render/src/draw.rs` | обход сцены: отсечение, LOD, растровый кэш, воспроизведение |
| `crates/ferrum-render/src/text.rs` | подписи Onest: раскладка `parley`, контуры глифов `skrifa` |
| `crates/ferrum-render/src/overlay.rs` | оверлеи и тема дизайн-системы |
| `crates/ferrum-render/src/lib.rs` | `Renderer`: кадр, экспорт PNG |
| `crates/ferrum-render/assets/fonts/` | `Onest-wght.ttf` и его лицензия `OFL.txt` |
| `crates/ferrum-render/tests/golden/*.png` | эталонные картинки |
| `crates/ferrum-render/examples/render_report.rs` | контрольный бенчмарк этапа |

Все команды запускаются из корня репозитория.

---
### Task 1: Изменения в `ferrum-doc`: уведомления об изменениях, ссылки скрытых узлов, обёртка `loro-internal`

Три доработки после этапа 1. Сцене нужно знать, какие узлы изменились, — иначе каждый кадр после правки пришлось бы перестраивать 100 000 узлов. Изображения, на которые ссылаются только узлы неизвестного типа (скрытые от модели), терялись при пересохранении: ссылки страницы брались из зеркала, где этих узлов нет. Обращения к внутреннему API `loro-internal` собраны в один модуль — единая точка проверки при обновлении Loro.

**Files:**
- Create: `crates/ferrum-doc/src/loro_ext.rs`
- Create: `crates/ferrum-doc/src/changes.rs`
- Modify: `crates/ferrum-doc/src/lib.rs`
- Modify: `crates/ferrum-doc/src/codec.rs`
- Modify: `crates/ferrum-doc/src/page.rs`
- Modify: `crates/ferrum-doc/src/document.rs`
- Modify: `Cargo.lock`
- Test: `crates/ferrum-doc/tests/changes.rs`

**Interfaces:**
- Consumes: API этапа 1.
- Produces: `Changes { created, deleted, moved, props: Vec<NodeId> }` с `is_empty`, `len`, `extend`; `Document::take_changes(page) -> Changes` — изменения страницы, накопленные транзакциями, отменами и повторами с прошлого вызова (при ошибке транзакции — затронутые узлы, сцена сверит их со страницей); `PageDoc::placement(id) -> Option<(Option<NodeId>, Option<usize>)>` — родитель и позиция за O(log n) (позиция `None`, если на странице есть скрытые узлы); `PageDoc::image_refs` включает ссылки скрытых узлов.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-doc/tests/changes.rs`:

````rust
use ferrum_doc::*;

fn sorted(mut ids: Vec<NodeId>) -> Vec<String> {
    let mut s: Vec<String> = ids.drain(..).map(|id| id.to_string()).collect();
    s.sort();
    s.dedup();
    s
}

fn x(v: f64) -> PropsPatch {
    PropsPatch {
        x: Some(v),
        ..Default::default()
    }
}

/// Фрейм с двумя детьми и прямоугольник в корне; изменения создания уже забраны.
fn setup() -> (Document, PageId, [NodeId; 4]) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = Vec::new();
    doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        ids.push(f);
        ids.push(p.create_node(Some(f), None, NodeKind::Rect)?);
        ids.push(p.create_node(Some(f), None, NodeKind::Ellipse)?);
        ids.push(p.create_node(None, None, NodeKind::Rect)?);
        Ok(vec![])
    })
    .unwrap();
    let created = doc.take_changes(page);
    assert_eq!(sorted(created.created), sorted(ids.clone()));
    assert!(
        doc.take_changes(page).is_empty(),
        "take_changes забирает изменения"
    );
    (doc, page, [ids[0], ids[1], ids[2], ids[3]])
}

#[test]
fn each_operation_kind_is_reported() {
    let (mut doc, page, [f, a, b, r]) = setup();

    doc.transact(page, vec![], |p| p.set_props(a, &x(5.0)).map(|_| vec![]))
        .unwrap();
    let c = doc.take_changes(page);
    assert_eq!(c.props, vec![a]);
    assert!(c.created.is_empty() && c.deleted.is_empty() && c.moved.is_empty());

    doc.transact(page, vec![], |p| p.move_node(r, Some(f), 0).map(|_| vec![]))
        .unwrap();
    let c = doc.take_changes(page);
    assert_eq!(c.moved, vec![r]);
    assert_eq!(c.len(), 1);

    doc.transact(page, vec![], |p| p.delete_nodes(&[f]).map(|_| vec![]))
        .unwrap();
    let c = doc.take_changes(page);
    assert_eq!(sorted(c.deleted), sorted(vec![f, a, b, r]));
    assert!(c.created.is_empty() && c.moved.is_empty() && c.props.is_empty());
}

#[test]
fn undo_and_redo_report_their_changes() {
    let (mut doc, page, [f, a, b, r]) = setup();
    doc.transact(page, vec![], |p| p.delete_nodes(&[f]).map(|_| vec![]))
        .unwrap();
    doc.take_changes(page);

    doc.undo().unwrap();
    let c = doc.take_changes(page);
    assert_eq!(sorted(c.created), sorted(vec![f, a, b]));
    assert!(c.deleted.is_empty());

    doc.redo().unwrap();
    let c = doc.take_changes(page);
    assert_eq!(sorted(c.deleted), sorted(vec![f, a, b]));

    doc.undo().unwrap();
    doc.take_changes(page);
    doc.transact(page, vec![], |p| {
        p.set_props(r, &x(9.0))?;
        p.move_node(b, None, 0)?;
        Ok(vec![])
    })
    .unwrap();
    doc.take_changes(page);
    doc.undo().unwrap();
    let c = doc.take_changes(page);
    assert_eq!(c.props, vec![r]);
    assert_eq!(c.moved, vec![b]);
    doc.redo().unwrap();
    let c = doc.take_changes(page);
    assert_eq!(c.props, vec![r]);
    assert_eq!(c.moved, vec![b]);
    assert_eq!(doc.page(page).unwrap().props(r).unwrap().x, 9.0);
}

#[test]
fn changes_accumulate_until_taken_and_stay_per_page() {
    let (mut doc, p1, [_, a, _, r]) = setup();
    let p2 = doc.add_page("Вторая").unwrap();
    doc.transact(p1, vec![], |p| p.set_props(a, &x(1.0)).map(|_| vec![]))
        .unwrap();
    let other = doc
        .transact(p2, vec![], |p| {
            Ok(vec![p.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    doc.transact(p1, vec![], |p| p.set_props(r, &x(2.0)).map(|_| vec![]))
        .unwrap();
    assert_eq!(doc.take_changes(p1).props, vec![a, r]);
    assert_eq!(doc.take_changes(p2).created, vec![other]);
    assert!(doc.take_changes(p1).is_empty());
}

#[test]
fn failed_transaction_reports_touched_nodes_and_keeps_page() {
    let (mut doc, page, [_, a, _, _]) = setup();
    let err = doc.transact(page, vec![], |p| {
        p.set_props(a, &x(42.0))?;
        Err(DocError::CyclicMove)
    });
    assert!(err.is_err());
    assert_eq!(doc.page(page).unwrap().props(a).unwrap().x, 0.0);
    // Откат вернул узел; сцене достаточно сверить его со страницей.
    assert_eq!(doc.take_changes(page).props, vec![a]);
}

#[test]
fn placement_matches_children_order() {
    let (mut doc, page, [f, a, b, r]) = setup();
    doc.transact(page, vec![], |p| p.move_node(r, Some(f), 1).map(|_| vec![]))
        .unwrap();
    let p = doc.page(page).unwrap();
    assert_eq!(p.children(Some(f)), vec![a, r, b]);
    for (i, id) in [a, r, b].into_iter().enumerate() {
        assert_eq!(p.placement(id), Some((Some(f), Some(i))));
    }
    assert_eq!(p.placement(f), Some((None, Some(0))));
    doc.transact(page, vec![], |p| p.delete_nodes(&[a]).map(|_| vec![]))
        .unwrap();
    assert_eq!(doc.page(page).unwrap().placement(a), None);
}
````

- [ ] **Step 2: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-doc --test changes`
Expected: ошибка компиляции — нет `take_changes` и `placement`.

- [ ] **Step 3: Вынести обращения к `loro-internal` в отдельный модуль**

`crates/ferrum-doc/src/loro_ext.rs`:

````rust
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
````

- [ ] **Step 4: Добавить `Changes`**

`crates/ferrum-doc/src/changes.rs`:

````rust
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
            }
        }
    }
}
````

`crates/ferrum-doc/src/lib.rs`:

````rust
//! Модель документа Ferrum на Loro. Остальные крейты работают только через этот API
//! и не видят Loro напрямую.

mod binary;
mod changes;
mod codec;
pub mod container;
mod document;
mod error;
mod history;
mod images;
mod loro_ext;
mod page;
mod scene_cache;
mod types;

pub use changes::Changes;
pub use document::{Document, Restore, SCHEMA_VERSION};
pub use error::{DocError, DocResult};
pub use images::ImageStore;
pub use page::{PageDoc, Tx};
pub use scene_cache::SceneRow;
pub use types::*;

#[doc(hidden)]
pub mod fixture;
````

- [ ] **Step 5: Читать ссылки на изображения у узлов неизвестного типа**

В `crates/ferrum-doc/src/codec.rs` заменить:

````rust
            None => d.strokes,
        },
    })
}

/// Элементы, которые не удалось разобрать, пропускаются: лучше открыть файл
````

на:

````rust
            None => d.strokes,
        },
    })
}

/// Хэши изображений в заливках узла — без разбора остальных свойств и типа узла:
/// так читаются и узлы неизвестного типа, скрытые от модели.
pub(crate) fn read_image_refs(map: &LoroMap) -> Vec<String> {
    match value(map, key::FILLS) {
        Some(v) => read_list(&v, read_fill)
            .into_iter()
            .filter_map(|f| f.image_hash)
            .collect(),
        None => Vec::new(),
    }
}

/// Элементы, которые не удалось разобрать, пропускаются: лучше открыть файл
````

- [ ] **Step 6: Страница: `placement`, ссылки скрытых узлов, `loro_ext`**

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use loro::{
    ContainerTrait, ExportMode, LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId,
    ValueOrContainer,
};

use crate::codec;
use crate::error::{DocError, DocResult};
use crate::scene_cache::SceneRow;
use crate::types::*;

````

на:

````rust
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use loro::{ExportMode, LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer};

use crate::codec;
use crate::error::{DocError, DocResult};
use crate::loro_ext;
use crate::scene_cache::SceneRow;
use crate::types::*;

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    journal: RefCell<Option<Vec<Inverse>>>,
    /// При загрузке встретились узлы неизвестного типа (см. `rebuild_from_loro`).
    unknown_nodes: Cell<bool>,
    /// Индекс и зеркало взяты из кэша сцены, а не прочитаны из Loro.
    from_cache: Cell<bool>,
}
````

на:

````rust
    journal: RefCell<Option<Vec<Inverse>>>,
    /// При загрузке встретились узлы неизвестного типа (см. `rebuild_from_loro`).
    unknown_nodes: Cell<bool>,
    /// Изображения, на которые ссылаются скрытые узлы неизвестного типа и их
    /// потомки. Зеркало их не видит, но при сохранении эти байты терять нельзя.
    hidden_refs: RefCell<Vec<String>>,
    /// Индекс и зеркало взяты из кэша сцены, а не прочитаны из Loro.
    from_cache: Cell<bool>,
}
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
            state: RefCell::default(),
            journal: RefCell::default(),
            unknown_nodes: Cell::default(),
            from_cache: Cell::default(),
        }
    }
````

на:

````rust
            state: RefCell::default(),
            journal: RefCell::default(),
            unknown_nodes: Cell::default(),
            hidden_refs: RefCell::default(),
            from_cache: Cell::default(),
        }
    }
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
    /// он и его поддерево не попадают в индекс и для модели не существуют — их не
    /// видно в `children`, `walk` и кэше сцены, но в снапшоте Loro они остаются.
    /// Чтобы правка не потеряла их (удаление предка, отмена), документ с такой
    /// страницей открывается только для чтения (`has_unknown_nodes`).
    fn rebuild_from_loro(&self) -> DocResult<()> {
        let mut state = State::default();
        let mut unknown = false;
        let mut stack = self.tree.children(TreeParentId::Root).unwrap_or_default();
        while let Some(tid) = stack.pop() {
            let id = self.stable_id(tid);
````

на:

````rust
    /// он и его поддерево не попадают в индекс и для модели не существуют — их не
    /// видно в `children`, `walk` и кэше сцены, но в снапшоте Loro они остаются.
    /// Чтобы правка не потеряла их (удаление предка, отмена), документ с такой
    /// страницей открывается только для чтения (`has_unknown_nodes`), а ссылки
    /// скрытого поддерева на изображения запоминаются для сохранения.
    fn rebuild_from_loro(&self) -> DocResult<()> {
        let mut state = State::default();
        let mut unknown = false;
        let mut hidden_refs = Vec::new();
        let mut stack = self.tree.children(TreeParentId::Root).unwrap_or_default();
        while let Some(tid) = stack.pop() {
            let id = self.stable_id(tid);
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
                Ok(props) => props,
                Err(DocError::UnknownKind(..)) => {
                    unknown = true;
                    continue;
                }
                Err(e) => return Err(e),
````

на:

````rust
                Ok(props) => props,
                Err(DocError::UnknownKind(..)) => {
                    unknown = true;
                    self.collect_hidden_refs(tid, &mut hidden_refs)?;
                    continue;
                }
                Err(e) => return Err(e),
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        }
        *self.state.borrow_mut() = state;
        self.unknown_nodes.set(unknown);
        Ok(())
    }

````

на:

````rust
        }
        *self.state.borrow_mut() = state;
        self.unknown_nodes.set(unknown);
        hidden_refs.sort();
        hidden_refs.dedup();
        *self.hidden_refs.borrow_mut() = hidden_refs;
        Ok(())
    }

    /// Хэши изображений в заливках скрытого поддерева (сам узел и все потомки,
    /// какого бы типа они ни были).
    fn collect_hidden_refs(&self, root: TreeID, out: &mut Vec<String>) -> DocResult<()> {
        let mut stack = vec![root];
        while let Some(tid) = stack.pop() {
            out.extend(codec::read_image_refs(&self.tree.get_meta(tid)?));
            stack.extend(
                self.tree
                    .children(TreeParentId::Node(tid))
                    .unwrap_or_default(),
            );
        }
        Ok(())
    }

````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        Ok(())
    }

    /// Позиция узла среди детей его родителя. Публичный `LoroTree` её не отдаёт, а
    /// список соседей на каждое перемещение стоил бы O(соседей); обработчик дерева из
    /// `loro-internal` ищет позицию за O(log n).
    fn index_of(&self, tid: TreeID) -> usize {
        self.tree
            .to_handler()
            .get_index_by_tree_id(&tid)
            .unwrap_or(0)
    }

    /// Переносит узел к новому родителю на позицию `index` среди его детей
````

на:

````rust
        Ok(())
    }

    /// Позиция узла среди детей его родителя за O(log n) (см. `loro_ext`).
    fn index_of(&self, tid: TreeID) -> usize {
        loro_ext::index_of(&self.tree, tid).unwrap_or(0)
    }

    /// Где стоит узел: родитель и позиция среди его детей — та же, что в
    /// `children(parent)`. Один поиск узла и O(log n) на позицию: сцене это нужно,
    /// чтобы вставить созданный или перемещённый узел без перестроения списка
    /// соседей. `None` — узла нет. Позиция `None` — на странице есть скрытые узлы
    /// неизвестного типа: их позиции в Loro не совпадают с позициями в `children`,
    /// и вызывающий код должен перечитать список целиком.
    pub fn placement(&self, id: NodeId) -> Option<(Option<NodeId>, Option<usize>)> {
        let tid = self.tid(id).ok()?;
        let parent = match self.tree.parent(tid)? {
            TreeParentId::Node(p) => Some(self.node_of(p)),
            _ => None,
        };
        let position = if self.has_unknown_nodes() {
            None
        } else {
            loro_ext::index_of(&self.tree, tid)
        };
        Some((parent, position))
    }

    /// Переносит узел к новому родителю на позицию `index` среди его детей
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
                break;
            }
        }
        self.tree.to_handler().get_child_at(&target, at)
    }

    fn record_move(&self, id: NodeId, tid: TreeID) {
````

на:

````rust
                break;
            }
        }
        loro_ext::child_at(&self.tree, target, at)
    }

    fn record_move(&self, id: NodeId, tid: TreeID) {
````

В `crates/ferrum-doc/src/page.rs` заменить:

````rust
        Ok(rows)
    }

    /// Хэши изображений, на которые ссылаются заливки живых узлов страницы.
    pub fn image_refs(&self) -> DocResult<Vec<String>> {
        let state = self.state.borrow();
        let mut refs: Vec<String> = state
            .props
            .values()
            .flat_map(|p| p.fills.iter().filter_map(|f| f.image_hash.clone()))
            .collect();
        refs.sort();
        refs.dedup();
````

на:

````rust
        Ok(rows)
    }

    /// Хэши изображений, на которые ссылаются заливки живых узлов страницы,
    /// включая узлы неизвестного типа, скрытые от модели.
    pub fn image_refs(&self) -> DocResult<Vec<String>> {
        let state = self.state.borrow();
        let mut refs: Vec<String> = state
            .props
            .values()
            .flat_map(|p| p.fills.iter().filter_map(|f| f.image_hash.clone()))
            .chain(self.hidden_refs.borrow().iter().cloned())
            .collect();
        refs.sort();
        refs.dedup();
````

- [ ] **Step 7: Документ: накопление изменений и тесты ссылок скрытых узлов**

Модульные тесты в конце `document.rs` собирают файл, где картинка используется только внутри поддерева узла неизвестного типа, и проверяют оба пути пересохранения: после загрузки страницы и после потери секции ссылок.

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

use loro::{LoroDoc, LoroTree, LoroValue, TreeParentId, ValueOrContainer};

use crate::error::{DocError, DocResult};
use crate::history::{History, HistoryEntry};
use crate::images::ImageStore;
````

на:

````rust

use loro::{LoroDoc, LoroTree, LoroValue, TreeParentId, ValueOrContainer};

use crate::changes::Changes;
use crate::error::{DocError, DocResult};
use crate::history::{History, HistoryEntry};
use crate::images::ImageStore;
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
    loaded: HashMap<PageId, PageDoc>,
    unloaded: HashMap<PageId, UnloadedPage>,
    history: History,
    pub images: ImageStore,
    read_only: bool,
}
````

на:

````rust
    loaded: HashMap<PageId, PageDoc>,
    unloaded: HashMap<PageId, UnloadedPage>,
    history: History,
    /// Изменённые узлы по страницам с последнего `take_changes`.
    changes: HashMap<PageId, Changes>,
    pub images: ImageStore,
    read_only: bool,
}
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
            loaded: HashMap::new(),
            unloaded,
            history: History::default(),
            images,
            read_only: false,
        }
````

на:

````rust
            loaded: HashMap::new(),
            unloaded,
            history: History::default(),
            changes: HashMap::new(),
            images,
            read_only: false,
        }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        self.loaded.remove(&id);
        self.unloaded.remove(&id);
        self.history.forget_page(id);
        Ok(())
    }

````

на:

````rust
        self.loaded.remove(&id);
        self.unloaded.remove(&id);
        self.history.forget_page(id);
        self.changes.remove(&id);
        Ok(())
    }

````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        let result = f(&Tx::new(p));
        let ops = p.take_journal();
        p.commit();
        match result {
            Ok(selection_after) => {
                if !ops.is_empty() {
````

на:

````rust
        let result = f(&Tx::new(p));
        let ops = p.take_journal();
        p.commit();
        // И при ошибке: откат вернул эти узлы в прежнее состояние, а если сорвался —
        // сцене тем более нужно сверить их со страницей.
        self.changes.entry(page).or_default().record(&ops);
        match result {
            Ok(selection_after) => {
                if !ops.is_empty() {
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }
````

на:

````rust
        }
    }

    /// Узлы страницы, изменённые правками, отменой и повтором с прошлого вызова.
    /// Сцена вызывает это раз за кадр и пересчитывает только эти узлы.
    pub fn take_changes(&mut self, page: PageId) -> Changes {
        self.changes.remove(&page).unwrap_or_default()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        self.page(entry.page)?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        // При сбое apply_inverse сам откатывает уже применённую часть шага.
        p.apply_inverse(&entry.ops)
    }

    pub fn undo(&mut self) -> DocResult<Option<Restore>> {
````

на:

````rust
        self.page(entry.page)?;
        let p = self.loaded.get(&entry.page).expect("загружена");
        // При сбое apply_inverse сам откатывает уже применённую часть шага.
        let result = p.apply_inverse(&entry.ops);
        // Применённые операции записывают обратные к себе — это и есть изменения.
        // При сбое сверяем все узлы шага: откат мог и не удаться.
        let changes = self.changes.entry(entry.page).or_default();
        match &result {
            Ok(done) => changes.record(done),
            Err(_) => changes.record(&entry.ops),
        }
        result
    }

    pub fn undo(&mut self) -> DocResult<Option<Restore>> {
````

В `crates/ferrum-doc/src/document.rs` заменить:

````rust
        assert!(matches!(err, Err(DocError::ReadOnly)));
    }

    #[test]
    fn out_of_range_schema_version_opens_read_only() {
        // 2^32 + 1 при усечении до u32 стало бы версией 1 — файл открылся бы на запись.
````

на:

````rust
        assert!(matches!(err, Err(DocError::ReadOnly)));
    }

    /// Документ, где изображение используется только узлом внутри скрытого
    /// поддерева (узел неизвестного типа), сохранённый в файл.
    fn file_with_hidden_image() -> (Vec<u8>, String) {
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        let hash = doc.images.add(b"png bytes".to_vec());
        let star = doc
            .transact(page, vec![], |p| {
                p.create_node(None, None, NodeKind::Rect)?;
                let star = p.create_node(None, None, NodeKind::Group)?;
                let pic = p.create_node(Some(star), None, NodeKind::Rect)?;
                p.set_props(
                    pic,
                    &PropsPatch {
                        fills: Some(vec![crate::Fill::image(hash.clone())]),
                        ..Default::default()
                    },
                )?;
                Ok(vec![star])
            })
            .unwrap()[0];
        doc.page(page)
            .unwrap()
            .force_raw_kind_for_tests(star, Some("star"));
        let bytes = crate::container::encode(&doc).unwrap();
        // Кэш сцены записан по зеркалу, где узел ещё известного типа; файл из новой
        // версии Ferrum такого кэша не содержал бы. Убираем его (id не страницы —
        // секция не сопоставится), чтобы страница загружалась из Loro.
        let bytes = crate::container::testing::map_sections(&bytes, |kind, id, _| {
            if kind == 5 {
                *id = "не-страница".to_string();
            }
        });
        (bytes, hash)
    }

    #[test]
    fn hidden_node_images_survive_resave_after_page_load() {
        let (bytes, hash) = file_with_hidden_image();
        let mut doc = crate::container::decode(&bytes).unwrap().document;
        let page = doc.pages()[0].0;
        // Загрузка страницы: ссылки теперь берутся из зеркала, где скрытых узлов нет.
        assert_eq!(doc.page(page).unwrap().walk().len(), 1);
        let resaved = crate::container::encode(&doc).unwrap();
        let reopened = crate::container::decode(&resaved).unwrap().document;
        assert!(reopened.images.get(&hash).is_some());
    }

    #[test]
    fn hidden_node_images_survive_lost_refs_section() {
        let (bytes, hash) = file_with_hidden_image();
        // Секция ссылок с чужим id: читатель её не сопоставит со страницей и
        // пересчитает ссылки из снапшота.
        let damaged = crate::container::testing::map_sections(&bytes, |kind, id, _| {
            if kind == 3 {
                *id = "не-страница".to_string();
            }
        });
        let opened = crate::container::decode(&damaged).unwrap();
        assert!(opened.warnings.iter().any(|w| w.contains("изображений")));
        let resaved = crate::container::encode(&opened.document).unwrap();
        let reopened = crate::container::decode(&resaved).unwrap().document;
        assert!(reopened.images.get(&hash).is_some());
    }

    #[test]
    fn out_of_range_schema_version_opens_read_only() {
        // 2^32 + 1 при усечении до u32 стало бы версией 1 — файл открылся бы на запись.
````

- [ ] **Step 8: Запустить тесты**

Run: `cargo test -p ferrum-doc`
Expected: PASS — модульные 21, `changes` 5, остальные наборы этапа 1 без изменений.

Проверка, что новые тесты не проходят вхолостую: если временно убрать `.chain(self.hidden_refs.borrow().iter().cloned())` из `PageDoc::image_refs`, оба теста `hidden_node_images_*` падают.

- [ ] **Step 9: Закрепить зависимости этапа 1**

`Cargo.lock` этапа 1 содержит версии, опубликованные после 2026-10-10 (например, `loro-common 1.17.0` при `loro 1.16.2`). Возвращаем их к последним версиям не моложе 14 дней; скрипт проверяет, что каждая версия встала.

Выполнить:

```bash
pins="cc@1 1.7.0
equivalent@1 1.0.2
find-msvc-tools@0.1 0.1.14
generator@0.8 0.8.10
libc@0.2 0.2.190
loro-common@1 1.16.0
loro-kv-store@1 1.16.0
pest@2 2.9.3
pest_derive@2 2.9.3
pest_generator@2 2.9.3
pest_meta@2 2.9.3
serde_json@1 1.0.152
smallvec@1 1.16.3
syn@3 3.0.7
unicode-ident@1 1.0.26
zerocopy@0.8 0.8.62
zerocopy-derive@0.8 0.8.62"
# Порядок важен не всегда: несколько проходов, пока все версии не встанут.
for _ in 1 2 3; do
  while read -r spec want; do
    cargo update -q -p "$spec" --precise "$want" 2>/dev/null || true
  done <<< "$pins"
done
while read -r spec want; do
  name=${spec%@*}
  grep -A1 "^name = \"$name\"$" Cargo.lock | grep -q "^version = \"$want\"$" \
    || { echo "не закреплён $name $want"; exit 1; }
done <<< "$pins"
```

Run: `cargo test -p ferrum-doc`
Expected: PASS, те же числа тестов.

- [ ] **Step 10: Закоммитить**

```bash
git add Cargo.lock crates/ferrum-doc
git commit -m "feat(doc): уведомления об изменениях, ссылки скрытых узлов на изображения"
```

---

### Task 2: Крейт `ferrum-scene`: форма узла и точное попадание

Форма узла — общая для рендера и hit-testing: то, что нарисовано, и то, во что попадает клик, совпадает. Обводка с выравниванием inside/center/outside строится как кольцо (внешний и внутренний контуры, сдвинутые от формы), заливаемое по even-odd: на каждом кадре не нужен расчёт штриха, а для внутренней и внешней обводки не нужны слои с маской.

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/ferrum-scene/Cargo.toml`
- Create: `crates/ferrum-scene/src/lib.rs`
- Create: `crates/ferrum-scene/src/shape.rs`
- Test: `crates/ferrum-scene/tests/shape.rs`

**Interfaces:**
- Consumes: `Props`, `NodeKind`, `Stroke`, `StrokeAlign` из `ferrum-doc`.
- Produces: `shape::local_transform(&Props) -> Affine`; `shape::sanitize(&mut Props)`; `shape::geometry(&Props) -> Geometry` (`None` | `Rect { rect, radius }` | `Ellipse(Rect)` | `Line(f64)`) с `bounds`, `contains`, `fill_path`, `fill_path_into`, `stroke_ring`, `stroke_ring_into`, `inset_path`, `stroke_outset`; `shape::hit(&Props, local: Point, tolerance) -> bool`; константы `PATH_TOLERANCE`, `COORD_LIMIT`.

- [ ] **Step 1: Добавить крейт в workspace**

`Cargo.toml`:

````toml
[workspace]
resolver = "2"
members = ["crates/ferrum-doc", "crates/ferrum-scene"]

[workspace.package]
edition = "2021"
rust-version = "1.96"
license = "UNLICENSED"

[workspace.dependencies]
loro = "=1.16.2"
# loro 1.16.2 не собирается с loro-internal 1.17.x — фиксируем совместимую версию
loro-internal = "=1.16.2"
thiserror = "2.0.20"
blake3 = "1.8"
criterion = "0.8"
proptest = "1.12"
kurbo = "=0.13.1"
# 0.13.0 не годится: после bulk_load вставка падает с «This is a bug in rstar»
# (листья на разной глубине, georust/rstar#254 ещё не выпущен).
rstar = "=0.12.2"
````

`crates/ferrum-scene/Cargo.toml`:

````toml
[package]
name = "ferrum-scene"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
ferrum-doc = { path = "../ferrum-doc" }
kurbo.workspace = true
rstar.workspace = true

[dev-dependencies]
proptest.workspace = true
````

`crates/ferrum-scene/src/lib.rs`:

````rust
//! Сцена Ferrum: производный кэш страницы для рендера и hit-testing — мировые
//! трансформации, границы в мире, R-tree и превью текущего жеста. Документ сцена
//! не меняет и видит только через публичный API `ferrum-doc`.

pub mod shape;
````

- [ ] **Step 2: Написать падающие тесты формы**

`crates/ferrum-scene/tests/shape.rs`:

````rust
use ferrum_doc::{Color, Fill, NodeKind, Props, Stroke, StrokeAlign};
use ferrum_scene::shape::{self, Geometry};
use kurbo::{Point, Rect, Shape};

fn props(kind: NodeKind, w: f64, h: f64) -> Props {
    let mut p = Props::defaults(kind);
    p.w = w;
    p.h = h;
    p
}

fn stroke(width: f64, align: StrokeAlign) -> Stroke {
    Stroke {
        color: Color::BLACK,
        width,
        align,
    }
}

#[test]
fn rotation_is_counterclockwise_about_top_left() {
    let mut p = props(NodeKind::Rect, 100.0, 50.0);
    p.x = 10.0;
    p.y = 20.0;
    p.rotation = 90.0;
    let t = shape::local_transform(&p);
    let top_left = t * Point::new(0.0, 0.0);
    let top_right = t * Point::new(100.0, 0.0);
    assert!((top_left - Point::new(10.0, 20.0)).hypot() < 1e-9);
    // Против часовой стрелки на экране: правый край уходит вверх.
    assert!((top_right - Point::new(10.0, -80.0)).hypot() < 1e-9);
}

#[test]
fn sanitize_replaces_values_unsafe_for_rendering() {
    let mut p = props(NodeKind::Rect, f64::INFINITY, -5.0);
    p.x = f64::NAN;
    p.y = 1e300;
    p.rotation = 370.0;
    p.opacity = 2.0;
    p.corner_radius = -1.0;
    p.fills = vec![Fill {
        opacity: f64::NAN,
        ..Fill::solid(Color::BLACK)
    }];
    p.strokes = vec![stroke(f64::NAN, StrokeAlign::Center)];
    shape::sanitize(&mut p);
    assert_eq!((p.x, p.y), (0.0, shape::COORD_LIMIT));
    assert_eq!((p.w, p.h), (0.0, 0.0));
    assert!((p.rotation - 10.0).abs() < 1e-9);
    assert_eq!(p.opacity, 1.0);
    assert_eq!(p.corner_radius, 0.0);
    assert_eq!(p.fills[0].opacity, 1.0);
    assert_eq!(p.strokes[0].width, 0.0);
}

#[test]
fn rounded_rect_hit_respects_corners() {
    let mut p = props(NodeKind::Rect, 100.0, 100.0);
    p.corner_radius = 30.0;
    assert!(shape::hit(&p, Point::new(50.0, 50.0), 0.0));
    assert!(shape::hit(&p, Point::new(50.0, 1.0), 0.0));
    assert!(!shape::hit(&p, Point::new(2.0, 2.0), 0.0), "срезанный угол");
    assert!(!shape::hit(&p, Point::new(101.0, 50.0), 0.0));
}

#[test]
fn ellipse_hit_excludes_bounding_box_corners() {
    let p = props(NodeKind::Ellipse, 100.0, 50.0);
    assert!(shape::hit(&p, Point::new(50.0, 25.0), 0.0));
    assert!(shape::hit(&p, Point::new(98.0, 25.0), 0.0));
    assert!(!shape::hit(&p, Point::new(5.0, 5.0), 0.0));
}

#[test]
fn thin_line_uses_tolerance_thick_line_its_width() {
    let p = props(NodeKind::Line, 100.0, 0.0);
    assert!(shape::hit(&p, Point::new(50.0, 3.0), 4.0));
    assert!(!shape::hit(&p, Point::new(50.0, 5.0), 4.0));
    assert!(
        shape::hit(&p, Point::new(103.0, 0.0), 4.0),
        "за концом в пределах допуска"
    );
    assert!(!shape::hit(&p, Point::new(105.0, 0.0), 4.0));
    let mut thick = p.clone();
    thick.strokes = vec![stroke(20.0, StrokeAlign::Center)];
    assert!(shape::hit(&thick, Point::new(50.0, 9.0), 1.0));
    assert!(!shape::hit(&thick, Point::new(50.0, 11.0), 1.0));
}

#[test]
fn stroke_only_shape_is_hit_on_its_stroke_band() {
    let mut p = props(NodeKind::Rect, 100.0, 100.0);
    p.fills.clear();
    p.strokes = vec![stroke(10.0, StrokeAlign::Outside)];
    assert!(
        !shape::hit(&p, Point::new(50.0, 50.0), 1.0),
        "пустая середина"
    );
    assert!(
        shape::hit(&p, Point::new(50.0, -8.0), 1.0),
        "внешняя обводка"
    );
    assert!(!shape::hit(&p, Point::new(50.0, -12.0), 1.0));
    p.strokes = vec![stroke(10.0, StrokeAlign::Inside)];
    assert!(
        shape::hit(&p, Point::new(50.0, 8.0), 1.0),
        "внутренняя обводка"
    );
    assert!(!shape::hit(&p, Point::new(50.0, -3.0), 1.0));
    // Тонкая обводка ловится в полосе допуска.
    p.strokes = vec![stroke(1.0, StrokeAlign::Center)];
    assert!(shape::hit(&p, Point::new(50.0, 3.5), 4.0));
}

#[test]
fn frame_without_fills_is_hit_inside() {
    let mut p = props(NodeKind::Frame, 100.0, 100.0);
    p.fills.clear();
    assert!(shape::hit(&p, Point::new(50.0, 50.0), 0.0));
    assert!(!shape::hit(
        &props(NodeKind::Group, 0.0, 0.0),
        Point::ZERO,
        10.0
    ));
}

#[test]
fn stroke_ring_follows_alignment() {
    let p = props(NodeKind::Rect, 100.0, 100.0);
    let g = shape::geometry(&p);
    let ring = |align| g.stroke_ring(&stroke(10.0, align)).unwrap();
    let inside = ring(StrokeAlign::Inside);
    let outside = ring(StrokeAlign::Outside);
    let center = ring(StrokeAlign::Center);
    let close = |a: Rect, b: Rect| (a.x0 - b.x0).abs() + (a.x1 - b.x1).abs() < 1e-6;
    assert!(close(
        inside.bounding_box(),
        Rect::new(0.0, 0.0, 100.0, 100.0)
    ));
    assert!(close(
        outside.bounding_box(),
        Rect::new(-10.0, -10.0, 110.0, 110.0)
    ));
    assert!(close(
        center.bounding_box(),
        Rect::new(-5.0, -5.0, 105.0, 105.0)
    ));
    // Кольцо заливается по even-odd: середина пустая, полоса закрашена.
    let filled = |path: &kurbo::BezPath, pt: (f64, f64)| path.winding(Point::from(pt)) % 2 != 0;
    assert!(filled(&inside, (5.0, 50.0)));
    assert!(!filled(&inside, (15.0, 50.0)));
    assert!(filled(&outside, (-5.0, 50.0)));
    assert!(!filled(&outside, (5.0, 50.0)));
    assert_eq!(g.stroke_outset(&[stroke(10.0, StrokeAlign::Outside)]), 10.0);
    assert_eq!(g.stroke_outset(&[stroke(10.0, StrokeAlign::Inside)]), 0.0);
    // Обводка толще формы: дыры нет, кольцо — сплошная заливка.
    let small = shape::geometry(&props(NodeKind::Rect, 6.0, 6.0));
    let solid = small
        .stroke_ring(&stroke(10.0, StrokeAlign::Inside))
        .unwrap();
    assert!(filled(&solid, (3.0, 3.0)));
    assert_eq!(
        shape::geometry(&props(NodeKind::Group, 1.0, 1.0)),
        Geometry::None
    );
}
````

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-scene --test shape`
Expected: ошибка компиляции — нет модуля `shape`.

- [ ] **Step 4: Реализовать форму**

`crates/ferrum-scene/src/shape.rs`:

````rust
//! Геометрия узла в его локальных координатах: трансформация, контур заливки,
//! кольца обводок и точная проверка попадания. Рендер и hit-testing берут форму
//! отсюда, поэтому то, что нарисовано, и то, во что попадает клик, совпадает.

use ferrum_doc::{NodeKind, Props, Stroke, StrokeAlign};
use kurbo::{Affine, BezPath, Ellipse, Line, ParamCurveNearest, Point, Rect, RoundedRect, Shape};

/// Точность аппроксимации дуг кривыми Безье в локальных единицах. Контур строится
/// один раз на изменение узла и потом масштабируется камерой до 256×, поэтому
/// допуск берётся с запасом: 0.001 × 256 = 0.26 экранного пикселя.
pub const PATH_TOLERANCE: f64 = 0.001;

/// Предел координат и размеров. Значения из файла не проверяются при записи, а
/// бесконечность или 1e300 сломали бы перевод в `f32` при растеризации.
pub const COORD_LIMIT: f64 = 1.0e9;

/// Локальная трансформация узла: сдвиг к `(x, y)` и поворот вокруг левого верхнего
/// угла, как в Figma. Поворот в градусах; положительный — против часовой стрелки на
/// экране (ось Y направлена вниз, поэтому угол в `kurbo` берётся с минусом).
pub fn local_transform(p: &Props) -> Affine {
    Affine::translate((p.x, p.y)) * Affine::rotate(-p.rotation.to_radians())
}

/// Приводит свойства из файла или кэша к допустимым значениям: нечисловые — к
/// нейтральным, размеры — неотрицательные, прозрачность — в `[0, 1]`.
pub fn sanitize(p: &mut Props) {
    fn finite(v: f64, default: f64) -> f64 {
        if v.is_finite() {
            v
        } else {
            default
        }
    }
    p.x = finite(p.x, 0.0).clamp(-COORD_LIMIT, COORD_LIMIT);
    p.y = finite(p.y, 0.0).clamp(-COORD_LIMIT, COORD_LIMIT);
    p.w = finite(p.w, 0.0).clamp(0.0, COORD_LIMIT);
    p.h = finite(p.h, 0.0).clamp(0.0, COORD_LIMIT);
    p.rotation = finite(p.rotation, 0.0).rem_euclid(360.0);
    p.opacity = finite(p.opacity, 1.0).clamp(0.0, 1.0);
    p.corner_radius = finite(p.corner_radius, 0.0).clamp(0.0, COORD_LIMIT);
    for f in &mut p.fills {
        f.opacity = finite(f.opacity, 1.0).clamp(0.0, 1.0);
    }
    for s in &mut p.strokes {
        s.width = finite(s.width, 0.0).clamp(0.0, COORD_LIMIT);
    }
}

/// Собственная форма узла в локальных координатах.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Geometry {
    /// Группа: своей формы нет, границы — объединение детей.
    None,
    /// Фрейм, прямоугольник, изображение: прямоугольник `(0, 0, w, h)` со скруглением.
    Rect { rect: Rect, radius: f64 },
    /// Эллипс, вписанный в `(0, 0, w, h)`.
    Ellipse(Rect),
    /// Линия от `(0, 0)` до `(w, 0)`.
    Line(f64),
}

pub fn geometry(p: &Props) -> Geometry {
    let rect = Rect::new(0.0, 0.0, p.w, p.h);
    match p.kind {
        NodeKind::Group => Geometry::None,
        NodeKind::Frame | NodeKind::Rect | NodeKind::Image => Geometry::Rect {
            rect,
            radius: p.corner_radius.min(p.w / 2.0).min(p.h / 2.0),
        },
        NodeKind::Ellipse => Geometry::Ellipse(rect),
        NodeKind::Line => Geometry::Line(p.w),
    }
}

impl Geometry {
    /// Границы формы без обводки; `None` у группы.
    pub fn bounds(&self) -> Option<Rect> {
        match *self {
            Geometry::None => None,
            Geometry::Rect { rect, .. } | Geometry::Ellipse(rect) => Some(rect),
            Geometry::Line(len) => Some(Rect::new(0.0, 0.0, len, 0.0)),
        }
    }

    /// Лежит ли точка внутри формы (без обводки и допуска). У линии и группы
    /// внутренности нет.
    pub fn contains(&self, p: Point) -> bool {
        self.offset(0.0).is_some_and(|s| s.contains(p))
    }

    /// Контур заливки; у линии и группы заливки нет.
    pub fn fill_path(&self) -> Option<BezPath> {
        let mut path = BezPath::new();
        self.fill_path_into(&mut path).then_some(path)
    }

    /// Дописывает контур заливки в `out` (рендер переиспользует один буфер на кадр);
    /// `false` — заливки нет.
    pub fn fill_path_into(&self, out: &mut BezPath) -> bool {
        match *self {
            Geometry::Rect { rect, radius } => {
                out.extend(rounded(rect, radius).path_elements(PATH_TOLERANCE));
                true
            }
            Geometry::Ellipse(rect) => {
                out.extend(Ellipse::from_rect(rect).path_elements(PATH_TOLERANCE));
                true
            }
            Geometry::None | Geometry::Line(_) => false,
        }
    }

    /// Форма, сдвинутая наружу на `d` (внутрь при `d < 0`); `None` — схлопнулась.
    /// Для прямоугольника смещение точное (радиус меняется на `d`, острые углы
    /// остаются острыми), для эллипса — эллипс с радиусами `r + d`: это не точная
    /// эквидистанта, но на глаз неотличимо и в рендере, и в попадании.
    fn offset(&self, d: f64) -> Option<Offset> {
        match *self {
            Geometry::Rect { rect, radius } => {
                let r = rect.inflate(d, d);
                (r.width() > 0.0 && r.height() > 0.0).then(|| {
                    let radius = if radius > 0.0 {
                        (radius + d).max(0.0)
                    } else {
                        0.0
                    };
                    Offset::Rect(rounded(r, radius))
                })
            }
            Geometry::Ellipse(rect) => {
                let r = rect.inflate(d, d);
                (r.width() > 0.0 && r.height() > 0.0)
                    .then(|| Offset::Ellipse(Ellipse::from_rect(r)))
            }
            Geometry::None | Geometry::Line(_) => None,
        }
    }

    /// Обводка как заливаемое кольцо (правило even-odd): внешний и внутренний
    /// контуры, сдвинутые по выравниванию обводки. Так обводка рисуется одной
    /// заливкой без расчёта штриха на каждом кадре, а выравнивание inside/outside
    /// не требует слоёв с маской. Линия обводится прямоугольником с плоскими концами.
    pub fn stroke_ring(&self, stroke: &Stroke) -> Option<BezPath> {
        let mut path = BezPath::new();
        self.stroke_ring_into(stroke, &mut path).then_some(path)
    }

    /// Дописывает кольцо обводки в `out`; `false` — обводки нет (нулевая ширина
    /// или форма схлопнулась).
    pub fn stroke_ring_into(&self, stroke: &Stroke, out: &mut BezPath) -> bool {
        let w = stroke.width;
        if w <= 0.0 {
            return false;
        }
        if let Geometry::Line(len) = *self {
            out.extend(Rect::new(0.0, -w / 2.0, len, w / 2.0).path_elements(PATH_TOLERANCE));
            return true;
        }
        let (outer, inner) = ring_offsets(stroke.align, w);
        let Some(outline) = self.offset(outer) else {
            return false;
        };
        outline.extend_path(out);
        if let Some(hole) = self.offset(inner) {
            hole.extend_path(out);
        }
        true
    }

    /// Форма, сжатая внутрь на `d`; `None` — схлопнулась (или это линия, группа).
    pub fn inset_path(&self, d: f64) -> Option<BezPath> {
        let mut out = BezPath::new();
        self.offset(-d)?.extend_path(&mut out);
        Some(out)
    }

    /// На сколько обводки выступают за форму наружу.
    pub fn stroke_outset(&self, strokes: &[Stroke]) -> f64 {
        strokes
            .iter()
            .map(|s| match self {
                Geometry::Line(_) => s.width / 2.0,
                _ => ring_offsets(s.align, s.width).0,
            })
            .fold(0.0, f64::max)
    }
}

/// Смещения внешнего и внутреннего контуров кольца обводки относительно формы.
fn ring_offsets(align: StrokeAlign, w: f64) -> (f64, f64) {
    match align {
        StrokeAlign::Inside => (0.0, -w),
        StrokeAlign::Center => (w / 2.0, -w / 2.0),
        StrokeAlign::Outside => (w, 0.0),
    }
}

fn rounded(rect: Rect, radius: f64) -> RoundedRect {
    RoundedRect::from_rect(
        rect,
        radius.min(rect.width() / 2.0).min(rect.height() / 2.0),
    )
}

enum Offset {
    Rect(RoundedRect),
    Ellipse(Ellipse),
}

impl Offset {
    fn extend_path(&self, out: &mut BezPath) {
        match self {
            Offset::Rect(r) => out.extend(r.path_elements(PATH_TOLERANCE)),
            Offset::Ellipse(e) => out.extend(e.path_elements(PATH_TOLERANCE)),
        }
    }

    fn contains(&self, p: Point) -> bool {
        match self {
            Offset::Rect(r) => r.contains(p),
            Offset::Ellipse(e) => e.contains(p),
        }
    }
}

/// Точное попадание точки `local` (в локальных координатах узла) в узел.
///
/// `tolerance` — допуск в мировых единицах для тонких элементов (4 экранных пикселя,
/// переведённые камерой в мир). Масштаба у узлов нет, поэтому локальные единицы
/// равны мировым. Заливка ловит клик внутри формы; фрейм ловит клик всегда, даже без
/// заливок (его можно выделить за пустую область). Обводка ловит клик в полосе
/// шириной не меньше допуска с каждой стороны от своей средней линии. Группа своей
/// формы не имеет — в неё попадают через детей.
pub fn hit(p: &Props, local: Point, tolerance: f64) -> bool {
    let geom = geometry(p);
    if let Geometry::Line(len) = geom {
        let half = p.strokes.iter().map(|s| s.width / 2.0).fold(0.0, f64::max);
        let seg = Line::new((0.0, 0.0), (len, 0.0));
        let nearest = seg.nearest(local, 1e-9);
        return nearest.distance_sq.sqrt() <= half.max(tolerance);
    }
    let filled = p.kind == NodeKind::Frame || p.fills.iter().any(|f| f.visible);
    if filled && geom.contains(local) {
        return true;
    }
    p.strokes.iter().any(|s| {
        if s.width <= 0.0 {
            return false;
        }
        let (outer, inner) = ring_offsets(s.align, s.width);
        let center = (outer + inner) / 2.0;
        let half = ((outer - inner) / 2.0).max(tolerance);
        let inside_outer = geom
            .offset(center + half)
            .is_some_and(|o| o.contains(local));
        let inside_inner = geom
            .offset(center - half)
            .is_some_and(|i| i.contains(local));
        inside_outer && !inside_inner
    })
}
````

- [ ] **Step 5: Запустить тесты**

Run: `cargo test -p ferrum-scene --test shape`
Expected: PASS, `8 passed`. Новые зависимости (kurbo, rstar и их зависимости) уже не моложе 14 дней — закреплять нечего.

- [ ] **Step 6: Закоммитить**

```bash
git add Cargo.toml Cargo.lock crates/ferrum-scene
git commit -m "feat(scene): форма узла, кольца обводок и точное попадание"
```

---

### Task 3: Сцена: таблица узлов, границы, R-tree, обновление по изменениям, превью, hit-test

Сцена строится из строк кэша сцены (до загрузки Loro) или из `PageDoc`. Дальше `apply(&Changes, &PageDoc)` сверяет каждый изменённый узел с текущим состоянием страницы — поэтому повторы, пересечения списков и порядок операций внутри транзакции не важны, а при расхождении сцена перестраивается целиком.

Решения, к которым привели замеры прототипа (100 000 узлов):
- Узлы хранятся кусками по 4096 (`Arena`), а не одним `Vec`: перераспределение массива на ~35 МБ копирует страницы с копированием при записи, и первая запись в каждую страницу после этого стоит микросекунды — создание одного фрейма превращало следующий `apply` из 2 мс в 8–13 мс.
- Позиции узлов среди соседей — отдельный плотный массив `index`: сдвиг позиций после вставки в начало списка пишет в 400 КБ, а не в разбросанные узлы.
- Вставка в список детей — одним слиянием по итоговым позициям из `PageDoc::placement`; список из документа перечитывается только если позиции неизвестны.
- Удаления из R-tree копятся за пересчёт: больше 200 — один проход по дереву с выборкой по пометкам (0.3 мс), иначе по одному (~1.3 мкс). Узлы R-tree по 10–24 записи: пакетное построение вдвое быстрее, чем с параметрами по умолчанию.
- `version` узла растёт только когда меняется его вид в локальных координатах (тип, размер, скругление, заливки, обводки): перемещение не перестраивает список отрисовки. `subtree_version` растёт при любом изменении поддерева — по нему устаревает растровый кэш фрейма.
- Границы поддерева (`subtree_bounds`) пересчитываются вверх по цепочке предков вместе с границами групп; фрейм с обрезкой детей не перебирает.

**Files:**
- Create: `crates/ferrum-scene/src/scene.rs`
- Modify: `crates/ferrum-scene/src/lib.rs`
- Test: `crates/ferrum-scene/tests/scene.rs`

**Interfaces:**
- Consumes: `Changes`, `PageDoc::{placement, props, children, scene_rows}`, `SceneRow`, `fixture::fill`; `shape` из Task 2.
- Produces: `Scene` с `new`, `from_rows(&[SceneRow])`, `from_page(&PageDoc)`, `apply(&Changes, &PageDoc)`, `set_preview(&[(NodeId, PropsPatch)])`, `clear_preview`, `has_preview`, `len`, `is_empty`, `roots() -> &[Slot]`, `slot(id)`, `node(id)`, `at(slot) -> &SceneNode`, `index(slot)`, `bounds()`, `is_visible(slot)`, `query_rect(Rect) -> Vec<NodeId>`, `hit_test(Point, tolerance) -> Option<NodeId>`, `hit_test_all` (верхний первым), `order_key(slot)`; `SceneNode { id, props, parent, children, depth, local, world, local_bounds, world_bounds, subtree_bounds, version, subtree_version }` с `geometry()`; `Slot = u32`; `testing::snapshot(&Scene)` — состояние для сравнения двух сцен.

- [ ] **Step 1: Написать падающие тесты сцены**

`crates/ferrum-scene/tests/scene.rs`:

````rust
use ferrum_doc::container::{decode, encode};
use ferrum_doc::{
    Color, Document, Fill, NodeId, NodeKind, PageId, PropsPatch, SceneRow, Stroke, StrokeAlign,
};
use ferrum_scene::{testing::snapshot, Scene};
use kurbo::{Point, Rect};

fn patch(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

/// Страница: фрейм (100, 100, 400×300) с прямоугольником и группой из двух эллипсов,
/// линия в корне. Возвращает [фрейм, прямоугольник, группа, эллипс 1, эллипс 2, линия].
fn sample() -> (Document, PageId, [NodeId; 6]) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = Vec::new();
    doc.transact(page, vec![], |p| {
        let f = p.create_node(None, None, NodeKind::Frame)?;
        p.set_props(f, &patch(100.0, 100.0, 400.0, 300.0))?;
        let r = p.create_node(Some(f), None, NodeKind::Rect)?;
        p.set_props(r, &patch(10.0, 10.0, 50.0, 40.0))?;
        let g = p.create_node(Some(f), None, NodeKind::Group)?;
        p.set_props(g, &patch(200.0, 100.0, 0.0, 0.0))?;
        let e1 = p.create_node(Some(g), None, NodeKind::Ellipse)?;
        p.set_props(e1, &patch(0.0, 0.0, 20.0, 20.0))?;
        let e2 = p.create_node(Some(g), None, NodeKind::Ellipse)?;
        p.set_props(e2, &patch(30.0, 40.0, 20.0, 20.0))?;
        let l = p.create_node(None, None, NodeKind::Line)?;
        p.set_props(l, &patch(0.0, 600.0, 300.0, 0.0))?;
        ids = vec![f, r, g, e1, e2, l];
        Ok(vec![])
    })
    .unwrap();
    doc.take_changes(page);
    (doc, page, ids.try_into().unwrap())
}

/// Сцена, обновлённая по изменениям, совпадает со сценой, построенной заново.
fn assert_matches_rebuild(scene: &Scene, doc: &mut Document, page: PageId) {
    let fresh = Scene::from_page(doc.page(page).unwrap());
    assert_eq!(snapshot(scene), snapshot(&fresh));
}

fn apply(scene: &mut Scene, doc: &mut Document, page: PageId) {
    let changes = doc.take_changes(page);
    assert!(
        !changes.is_empty(),
        "операция должна сообщить об изменениях"
    );
    scene.apply(&changes, doc.page(page).unwrap());
}

fn close(a: Rect, b: Rect) -> bool {
    (a.x0 - b.x0).abs() < 1e-9
        && (a.y0 - b.y0).abs() < 1e-9
        && (a.x1 - b.x1).abs() < 1e-9
        && (a.y1 - b.y1).abs() < 1e-9
}

#[test]
fn world_transforms_and_bounds_nest() {
    let (mut doc, page, [f, r, g, e1, e2, l]) = sample();
    let scene = Scene::from_page(doc.page(page).unwrap());
    assert_eq!(scene.len(), 6);
    let world = |id| scene.node(id).unwrap().world_bounds.unwrap();
    assert!(close(world(f), Rect::new(100.0, 100.0, 500.0, 400.0)));
    assert!(close(world(r), Rect::new(110.0, 110.0, 160.0, 150.0)));
    assert!(close(world(e1), Rect::new(300.0, 200.0, 320.0, 220.0)));
    // Группа: объединение детей в её координатах.
    let group = scene.node(g).unwrap();
    assert!(close(
        group.local_bounds.unwrap(),
        Rect::new(0.0, 0.0, 50.0, 60.0)
    ));
    assert!(close(world(g), Rect::new(300.0, 200.0, 350.0, 260.0)));
    assert!(close(world(e2), Rect::new(330.0, 240.0, 350.0, 260.0)));
    // Обводка линии (1 px по центру) расширяет границы на полпикселя.
    assert!(close(world(l), Rect::new(-0.5, 599.5, 300.5, 600.5)));
    assert_eq!(scene.node(g).unwrap().depth, 1);
    assert_eq!(scene.index(scene.slot(e2).unwrap()), 1);
    let all = scene.bounds().unwrap();
    assert!(close(all, Rect::new(-0.5, 100.0, 500.0, 600.5)));
}

#[test]
fn rotated_frame_rotates_children() {
    let (mut doc, page, [f, r, ..]) = sample();
    doc.transact(page, vec![], |p| {
        p.set_props(
            f,
            &PropsPatch {
                rotation: Some(90.0),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Ребёнок (10, 10, 50×40) во фрейме, повёрнутом на 90° против часовой вокруг (100, 100).
    let b = scene.node(r).unwrap().world_bounds.unwrap();
    assert!(close(b, Rect::new(110.0, 40.0, 150.0, 90.0)), "{b:?}");
}

#[test]
fn rows_from_file_cache_build_the_same_scene() {
    let (doc, page, _) = sample();
    let bytes = encode(&doc).unwrap();
    let mut opened = decode(&bytes).unwrap().document;
    let rows = opened.cached_scene(page).unwrap().to_vec();
    let from_cache = Scene::from_rows(&rows);
    let from_page = Scene::from_page(opened.page(page).unwrap());
    assert_eq!(snapshot(&from_cache), snapshot(&from_page));
}

#[test]
fn untrusted_rows_are_sanitized_and_duplicates_dropped() {
    let (mut doc, page, [f, r, g, ..]) = sample();
    let mut rows: Vec<SceneRow> = doc.page(page).unwrap().scene_rows().unwrap();
    rows[1].props.x = f64::NAN;
    rows[1].props.w = -10.0;
    // Повтор фрейма в конце таблицы и строка-ребёнок повтора — отбрасываются обе.
    let mut dup = rows[0].clone();
    dup.parent = None;
    rows.push(dup);
    let mut orphan = rows[1].clone();
    orphan.parent = Some(rows.len() as u32 - 1);
    rows.push(orphan);
    let scene = Scene::from_rows(&rows);
    assert_eq!(scene.len(), 6);
    let rect = scene.node(r).unwrap();
    assert_eq!((rect.props.x, rect.props.w), (0.0, 0.0));
    assert_eq!(scene.roots().len(), 2);
    assert_eq!(scene.node(g).unwrap().parent, scene.slot(f));
}

#[test]
fn apply_props_updates_node_descendants_and_ancestors() {
    let (mut doc, page, [f, _, g, e1, e2, _]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    // Сдвиг ребёнка группы меняет границы группы и поддерева фрейма.
    doc.transact(page, vec![], |p| {
        p.set_props(e2, &patch(130.0, 140.0, 20.0, 20.0))
            .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    let group = scene.node(g).unwrap();
    assert!(close(
        group.local_bounds.unwrap(),
        Rect::new(0.0, 0.0, 150.0, 160.0)
    ));
    assert_matches_rebuild(&scene, &mut doc, page);
    // Сдвиг фрейма двигает всех потомков.
    doc.transact(page, vec![], |p| {
        p.set_props(
            f,
            &PropsPatch {
                x: Some(0.0),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    let b = scene.node(e1).unwrap().world_bounds.unwrap();
    assert!(close(b, Rect::new(200.0, 200.0, 220.0, 220.0)));
    assert_matches_rebuild(&scene, &mut doc, page);
}

type Step = dyn Fn(&ferrum_doc::Tx) -> ferrum_doc::DocResult<()>;

#[test]
fn apply_handles_every_structural_operation_and_undo() {
    let (mut doc, page, [f, r, g, e1, _, l]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let steps: Vec<Box<Step>> = vec![
        Box::new(move |p| p.create_node(Some(g), Some(1), NodeKind::Rect).map(|_| ())),
        Box::new(move |p| p.move_node(l, Some(f), 0)),
        Box::new(move |p| p.move_nodes(&[e1, r], None, 0)),
        Box::new(move |p| p.delete_nodes(&[g])),
        Box::new(move |p| {
            // Вынести узел из фрейма и удалить фрейм в одной транзакции.
            p.move_node(l, None, 0)?;
            p.delete_nodes(&[f])
        }),
    ];
    for step in &steps {
        doc.transact(page, vec![], |p| step(p).map(|_| vec![]))
            .unwrap();
        apply(&mut scene, &mut doc, page);
        assert_matches_rebuild(&scene, &mut doc, page);
    }
    for _ in 0..steps.len() {
        doc.undo().unwrap();
        apply(&mut scene, &mut doc, page);
        assert_matches_rebuild(&scene, &mut doc, page);
    }
    for _ in 0..steps.len() {
        doc.redo().unwrap();
        apply(&mut scene, &mut doc, page);
        assert_matches_rebuild(&scene, &mut doc, page);
    }
}

#[test]
fn hit_test_finds_topmost_visible_unclipped_node() {
    let (mut doc, page, [f, r, _, e1, _, l]) = sample();
    let top = doc
        .transact(page, vec![], |p| {
            let top = p.create_node(Some(f), None, NodeKind::Rect)?;
            p.set_props(top, &patch(20.0, 20.0, 30.0, 30.0))?;
            Ok(vec![top])
        })
        .unwrap()[0];
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    // Перекрытие: верхний первым, затем прямоугольник под ним, затем фрейм.
    assert_eq!(
        scene.hit_test_all(Point::new(125.0, 125.0), 0.0),
        vec![top, r, f]
    );
    // Тонкая линия ловится в пределах допуска.
    assert_eq!(scene.hit_test(Point::new(150.0, 603.0), 4.0), Some(l));
    assert_eq!(scene.hit_test(Point::new(150.0, 606.0), 4.0), None);
    // Эллипс: угол его рамки — мимо, в фрейм.
    assert_eq!(scene.hit_test(Point::new(301.0, 201.0), 0.0), Some(f));
    assert_eq!(scene.hit_test(Point::new(310.0, 210.0), 0.0), Some(e1));

    // Скрытый узел и обрезанная фреймом часть не ловятся; заблокированный ловится.
    doc.transact(page, vec![], |p| {
        p.set_props(
            top,
            &PropsPatch {
                visible: Some(false),
                ..Default::default()
            },
        )?;
        p.set_props(
            r,
            &PropsPatch {
                x: Some(-30.0),
                locked: Some(true),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    assert_eq!(scene.hit_test(Point::new(115.0, 125.0), 0.0), Some(r));
    assert_eq!(
        scene.hit_test(Point::new(90.0, 125.0), 0.0),
        None,
        "за краем фрейма"
    );
    assert_eq!(
        scene.query_rect(Rect::new(80.0, 120.0, 85.0, 125.0)),
        vec![r]
    );
}

#[test]
fn hit_test_on_rotated_node() {
    let (mut doc, page, [_, _, _, _, _, l]) = sample();
    let id = doc
        .transact(page, vec![], |p| {
            let id = p.create_node(None, None, NodeKind::Rect)?;
            p.set_props(
                id,
                &PropsPatch {
                    x: Some(1000.0),
                    y: Some(1000.0),
                    w: Some(100.0),
                    h: Some(10.0),
                    rotation: Some(90.0),
                    ..Default::default()
                },
            )?;
            Ok(vec![id])
        })
        .unwrap()[0];
    let scene = Scene::from_page(doc.page(page).unwrap());
    // Повёрнут вокруг (1000, 1000): теперь тянется вверх.
    assert_eq!(scene.hit_test(Point::new(1005.0, 950.0), 0.0), Some(id));
    assert_eq!(scene.hit_test(Point::new(1050.0, 1005.0), 0.0), None);
    let _ = l;
}

#[test]
fn preview_overrides_scene_without_touching_document() {
    let (mut doc, page, [f, r, ..]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let before = snapshot(&scene);
    let moved = PropsPatch {
        x: Some(300.0),
        ..Default::default()
    };
    scene.set_preview(&[(r, moved.clone())]);
    assert!(scene.has_preview());
    assert_eq!(scene.node(r).unwrap().props.x, 300.0);
    assert_eq!(doc.page(page).unwrap().props(r).unwrap().x, 10.0);
    assert!(close(
        scene.node(r).unwrap().world_bounds.unwrap(),
        Rect::new(400.0, 110.0, 450.0, 150.0)
    ));
    assert_eq!(scene.hit_test(Point::new(420.0, 120.0), 0.0), Some(r));

    // Правка документа под превью: свойства документа обновляются, превью остаётся.
    doc.transact(page, vec![], |p| {
        p.set_props(
            r,
            &PropsPatch {
                y: Some(50.0),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    let n = scene.node(r).unwrap();
    assert_eq!((n.props.x, n.props.y), (300.0, 50.0));

    // Новый список превью заменяет прежний; снятие возвращает документ.
    scene.set_preview(&[(f, moved)]);
    assert_eq!(scene.node(r).unwrap().props.x, 10.0);
    scene.clear_preview();
    assert!(!scene.has_preview());
    assert_matches_rebuild(&scene, &mut doc, page);
    assert_ne!(snapshot(&scene), before, "правка y дошла до сцены");
}

#[test]
fn deleting_previewed_node_drops_its_preview() {
    let (mut doc, page, [_, r, ..]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    scene.set_preview(&[(
        r,
        PropsPatch {
            x: Some(1.0),
            ..Default::default()
        },
    )]);
    doc.transact(page, vec![], |p| p.delete_nodes(&[r]).map(|_| vec![]))
        .unwrap();
    apply(&mut scene, &mut doc, page);
    assert!(!scene.has_preview());
    assert!(scene.node(r).is_none());
    assert_matches_rebuild(&scene, &mut doc, page);
}

#[test]
fn strokes_and_clip_shape_subtree_bounds() {
    let (mut doc, page, [f, r, ..]) = sample();
    doc.transact(page, vec![], |p| {
        p.set_props(
            r,
            &PropsPatch {
                x: Some(-50.0),
                strokes: Some(vec![Stroke {
                    color: Color::BLACK,
                    width: 4.0,
                    align: StrokeAlign::Outside,
                }]),
                fills: Some(vec![Fill::solid(Color::WHITE)]),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    let rect = scene.node(r).unwrap().world_bounds.unwrap();
    assert!(close(rect, Rect::new(46.0, 106.0, 104.0, 154.0)));
    // Фрейм обрезает содержимое: выступающий ребёнок не расширяет поддерево.
    let frame = scene.node(f).unwrap();
    assert_eq!(frame.subtree_bounds, frame.world_bounds);
    doc.transact(page, vec![], |p| {
        p.set_props(
            f,
            &PropsPatch {
                clip_content: Some(false),
                ..Default::default()
            },
        )
        .map(|_| vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(page).unwrap());
    let sub = scene.node(f).unwrap().subtree_bounds.unwrap();
    assert!(close(sub, Rect::new(46.0, 100.0, 500.0, 400.0)));
}

#[test]
fn versions_track_appearance_and_subtree() {
    let (mut doc, page, [f, r, ..]) = sample();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    let before = |s: &Scene| {
        let n = s.node(r).unwrap();
        (n.version, s.node(f).unwrap().subtree_version)
    };
    let (v0, sub0) = before(&scene);
    // Сдвиг: вид в локальных координатах тот же, поддерево фрейма изменилось.
    doc.transact(page, vec![], |p| {
        p.set_props(r, &patch(15.0, 10.0, 50.0, 40.0))
            .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    let (v1, sub1) = before(&scene);
    assert_eq!(v1, v0);
    assert!(sub1 > sub0);
    // Размер меняет вид.
    doc.transact(page, vec![], |p| {
        p.set_props(r, &patch(15.0, 10.0, 51.0, 40.0))
            .map(|_| vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    assert!(scene.node(r).unwrap().version > v1);
}

#[test]
fn large_batches_match_rebuild() {
    // Больше порога пакетного обновления R-tree: удаление проходом по дереву.
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |p| ferrum_doc::fixture::fill(p, 3000))
        .unwrap();
    let mut scene = Scene::from_page(doc.page(page).unwrap());
    doc.take_changes(page);
    let moved: Vec<NodeId> = ids.iter().copied().step_by(3).collect();
    doc.transact(page, vec![], |p| {
        for (i, id) in moved.iter().enumerate() {
            p.set_props(
                *id,
                &PropsPatch {
                    x: Some(i as f64),
                    ..Default::default()
                },
            )?;
        }
        let frame = p.create_node(None, None, NodeKind::Frame)?;
        p.move_nodes(&moved[..500], Some(frame), 0)?;
        p.delete_nodes(&moved[500..800])?;
        Ok(vec![])
    })
    .unwrap();
    apply(&mut scene, &mut doc, page);
    assert_matches_rebuild(&scene, &mut doc, page);
    doc.undo().unwrap();
    apply(&mut scene, &mut doc, page);
    assert_matches_rebuild(&scene, &mut doc, page);
}
````

- [ ] **Step 2: Подключить модуль**

`crates/ferrum-scene/src/lib.rs`:

````rust
//! Сцена Ferrum: производный кэш страницы для рендера и hit-testing — мировые
//! трансформации, границы в мире, R-tree и превью текущего жеста. Документ сцена
//! не меняет и видит только через публичный API `ferrum-doc`.

mod scene;
pub mod shape;

pub use scene::{testing, Scene, SceneNode, Slot};
````

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-scene --test scene`
Expected: ошибка компиляции — нет модуля `scene`.

- [ ] **Step 4: Реализовать сцену**

`crates/ferrum-scene/src/scene.rs`:

````rust
//! Таблица сцены: узлы страницы с мировыми трансформациями, границами и R-tree.
//!
//! Сцена — производный кэш документа. Она строится из строк кэша сцены (до
//! загрузки Loro) или из `PageDoc`, а дальше обновляется по `Changes`: пересчитываются
//! только затронутые узлы, их потомки (мировая трансформация) и предки (границы).

use std::collections::{HashMap, HashSet};

use ferrum_doc::{Changes, NodeId, NodeKind, PageDoc, Props, PropsPatch, SceneRow};
use kurbo::{Affine, Point, Rect};
use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{RStarInsertionStrategy, RTree, RTreeParams, SelectionFunction, AABB};

use crate::shape::{self, Geometry};

/// Индекс узла в плотной таблице сцены. Освободившиеся индексы переиспользуются,
/// поэтому хранить их дольше одного кадра нельзя — для этого есть `NodeId`.
pub type Slot = u32;

type Entry = GeomWithData<Rectangle<[f64; 2]>, Slot>;

/// С какого числа удалений из R-tree выгоднее один проход по всему дереву.
const TREE_DRAIN_THRESHOLD: usize = 200;

/// Выборка записей R-tree по пометкам их слотов — для удаления пачкой.
struct Marked<'a> {
    marks: &'a [u32],
    mark: u32,
}

impl SelectionFunction<Entry> for Marked<'_> {
    fn should_unpack_parent(&self, _: &AABB<[f64; 2]>) -> bool {
        true
    }

    fn should_unpack_leaf(&self, e: &Entry) -> bool {
        self.marks[e.data as usize] == self.mark
    }
}

/// Родитель и позиция узла в документе (`PageDoc::placement`).
type Placement = (Option<NodeId>, Option<usize>);

/// Узлы R-tree крупнее, чем по умолчанию (6): пакетное построение 100 000 записей
/// вдвое быстрее, удаление и вставка не дороже, поиск по точке тот же.
struct Params;

impl RTreeParams for Params {
    const MIN_SIZE: usize = 10;
    const MAX_SIZE: usize = 24;
    const REINSERTION_COUNT: usize = 2;
    type DefaultInsertionStrategy = RStarInsertionStrategy;
}

#[derive(Debug, Clone)]
pub struct SceneNode {
    pub id: NodeId,
    /// Свойства после проверки (`shape::sanitize`) и с учётом превью жеста.
    pub props: Props,
    pub parent: Option<Slot>,
    /// Дети снизу вверх, как в документе.
    pub children: Vec<Slot>,
    pub depth: u32,
    pub local: Affine,
    pub world: Affine,
    /// Границы формы в локальных координатах без обводки; у группы — объединение
    /// видимых детей. `None` — пустая группа.
    pub local_bounds: Option<Rect>,
    /// Собственные границы в мире с обводкой — запись в R-tree.
    pub world_bounds: Option<Rect>,
    /// Границы всего видимого поддерева в мире (дети фрейма с обрезкой не выходят
    /// за фрейм); `None` — узел скрыт. По ним рендер отбрасывает невидимое.
    pub subtree_bounds: Option<Rect>,
    /// Растёт, когда меняется вид узла в его локальных координатах: тип, размер,
    /// скругление, заливки, обводки. По нему рендер понимает, что список
    /// отрисовки узла устарел; перемещение и поворот его не меняют.
    pub version: u64,
    /// Растёт при любом изменении в поддереве (свойства, состав, положение детей):
    /// по нему рендер понимает, что растровый кэш фрейма устарел.
    pub subtree_version: u64,
}

impl SceneNode {
    pub fn geometry(&self) -> Geometry {
        shape::geometry(&self.props)
    }
}

/// Узлы хранятся кусками постоянной ёмкости, а не одним `Vec`: перераспределение
/// массива на 100 000 узлов (~35 МБ) копирует его страницами с копированием при
/// записи, и первая запись в каждую страницу после этого стоит микросекунды —
/// создание одного узла превращалось в миллисекунды на следующей правке.
const CHUNK: usize = 4096;

#[derive(Default)]
struct Arena {
    chunks: Vec<Vec<Option<SceneNode>>>,
    len: usize,
}

impl Arena {
    fn get(&self, slot: Slot) -> &Option<SceneNode> {
        let s = slot as usize;
        &self.chunks[s / CHUNK][s % CHUNK]
    }

    fn get_mut(&mut self, slot: Slot) -> &mut Option<SceneNode> {
        let s = slot as usize;
        &mut self.chunks[s / CHUNK][s % CHUNK]
    }

    fn push(&mut self, node: SceneNode) -> Slot {
        if self.len.is_multiple_of(CHUNK) {
            self.chunks.push(Vec::with_capacity(CHUNK));
        }
        self.chunks
            .last_mut()
            .expect("кусок создан выше")
            .push(Some(node));
        self.len += 1;
        (self.len - 1) as Slot
    }
}

#[derive(Default)]
pub struct Scene {
    nodes: Arena,
    free: Vec<Slot>,
    by_id: HashMap<NodeId, Slot>,
    roots: Vec<Slot>,
    rtree: RTree<Entry, Params>,
    next_version: u64,
    /// Узлы под превью жеста: свойства из документа и временная правка поверх них.
    preview: HashMap<Slot, (Props, PropsPatch)>,
    /// Изменения R-tree, накопленные за один пересчёт (см. `flush_tree`).
    tree_removals: Vec<(Slot, Rect)>,
    tree_inserts: Vec<(Slot, Rect)>,
    /// Позиция узла среди детей родителя (или корней страницы) по слотам. Отдельно
    /// от узлов: при вставке в начало списка позиции сдвигаются у всех соседей, и
    /// плотный массив здесь в разы быстрее записи в разбросанные по памяти узлы.
    index: Vec<u32>,
    /// Пометки для обходов без выделения памяти на каждый вызов.
    marks: Vec<u32>,
    mark: u32,
}

impl Scene {
    pub fn new() -> Scene {
        Scene::default()
    }

    /// Сцена по строкам кэша сцены — до загрузки Loro. Строки идут в порядке обхода,
    /// родитель раньше детей. Данные из файла не доверенные: повтор `NodeId` или
    /// ссылка на пропущенную строку отбрасывают строку вместе с её поддеревом.
    pub fn from_rows(rows: &[SceneRow]) -> Scene {
        let mut scene = Scene::new();
        scene.by_id.reserve(rows.len());
        let mut slot_of: Vec<Option<Slot>> = Vec::with_capacity(rows.len());
        for row in rows {
            let parent = match row.parent {
                None => None,
                Some(p) => match slot_of.get(p as usize).copied().flatten() {
                    Some(s) => Some(s),
                    None => {
                        slot_of.push(None);
                        continue;
                    }
                },
            };
            if scene.by_id.contains_key(&row.id) {
                slot_of.push(None);
                continue;
            }
            let slot = scene.alloc(row.id, row.props.clone());
            scene.link(slot, parent);
            slot_of.push(Some(slot));
        }
        scene.rebuild_geometry();
        scene
    }

    /// Сцена по загруженной странице.
    pub fn from_page(page: &PageDoc) -> Scene {
        // Строки кэша сцены — тот же обход, что нужен сцене; при ошибке чтения
        // (страница в несогласованном состоянии) сцена пуста, а не падает.
        Scene::from_rows(&page.scene_rows().unwrap_or_default())
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn roots(&self) -> &[Slot] {
        &self.roots
    }

    pub fn slot(&self, id: NodeId) -> Option<Slot> {
        self.by_id.get(&id).copied()
    }

    pub fn node(&self, id: NodeId) -> Option<&SceneNode> {
        self.slot(id).map(|s| self.at(s))
    }

    /// Узел по индексу. Индекс должен быть живым: из `roots`, `children` или `slot`.
    pub fn at(&self, slot: Slot) -> &SceneNode {
        self.nodes.get(slot).as_ref().expect("живой узел сцены")
    }

    fn at_mut(&mut self, slot: Slot) -> &mut SceneNode {
        self.nodes.get_mut(slot).as_mut().expect("живой узел сцены")
    }

    fn alive(&self, slot: Slot) -> bool {
        self.nodes.get(slot).is_some()
    }

    /// Объединение границ всех видимых узлов — для «показать всё».
    pub fn bounds(&self) -> Option<Rect> {
        self.roots
            .iter()
            .filter_map(|&s| self.at(s).subtree_bounds)
            .reduce(|a, b| a.union(b))
    }

    /// Позиция узла среди детей его родителя (или корней страницы).
    pub fn index(&self, slot: Slot) -> u32 {
        self.index[slot as usize]
    }

    /// Виден ли узел: он сам и все предки не скрыты.
    pub fn is_visible(&self, slot: Slot) -> bool {
        let mut cur = Some(slot);
        while let Some(s) = cur {
            let n = self.at(s);
            if !n.props.visible {
                return false;
            }
            cur = n.parent;
        }
        true
    }

    fn alloc(&mut self, id: NodeId, mut props: Props) -> Slot {
        shape::sanitize(&mut props);
        self.next_version += 1;
        let node = SceneNode {
            id,
            local: shape::local_transform(&props),
            props,
            parent: None,
            children: Vec::new(),
            depth: 0,
            world: Affine::IDENTITY,
            local_bounds: None,
            world_bounds: None,
            subtree_bounds: None,
            version: self.next_version,
            subtree_version: self.next_version,
        };
        let slot = match self.free.pop() {
            Some(s) => {
                *self.nodes.get_mut(s) = Some(node);
                s
            }
            None => {
                self.marks.push(0);
                self.index.push(0);
                self.nodes.push(node)
            }
        };
        self.by_id.insert(id, slot);
        slot
    }

    /// Добавляет узел последним ребёнком родителя.
    fn link(&mut self, slot: Slot, parent: Option<Slot>) {
        let list = self.list_mut(parent);
        let index = list.len() as u32;
        list.push(slot);
        let depth = parent.map_or(0, |p| self.at(p).depth + 1);
        let n = self.at_mut(slot);
        n.parent = parent;
        n.depth = depth;
        self.index[slot as usize] = index;
    }

    fn list(&self, parent: Option<Slot>) -> &Vec<Slot> {
        match parent {
            Some(p) => &self.at(p).children,
            None => &self.roots,
        }
    }

    fn list_mut(&mut self, parent: Option<Slot>) -> &mut Vec<Slot> {
        match parent {
            Some(p) => &mut self.at_mut(p).children,
            None => &mut self.roots,
        }
    }

    fn next_mark(&mut self) -> u32 {
        self.mark = self.mark.wrapping_add(1);
        if self.mark == 0 {
            self.marks.fill(0);
            self.mark = 1;
        }
        self.mark
    }

    /// Полный пересчёт трансформаций, границ и R-tree — после построения.
    fn rebuild_geometry(&mut self) {
        let order = self.preorder(&self.roots.clone());
        for &s in &order {
            self.update_world(s);
        }
        for &s in order.iter().rev() {
            self.update_bounds(s, false);
        }
        let entries: Vec<Entry> = order
            .iter()
            .filter_map(|&s| self.at(s).world_bounds.map(|b| entry(b, s)))
            .collect();
        self.rtree = RTree::bulk_load_with_params(entries);
    }

    /// Поддеревья `starts` в порядке обхода в глубину, родитель раньше детей.
    fn preorder(&self, starts: &[Slot]) -> Vec<Slot> {
        let mut out = Vec::new();
        let mut stack: Vec<Slot> = starts.iter().rev().copied().collect();
        while let Some(s) = stack.pop() {
            out.push(s);
            stack.extend(self.at(s).children.iter().rev());
        }
        out
    }

    fn update_world(&mut self, slot: Slot) {
        let (parent_world, depth) = match self.at(slot).parent {
            Some(p) => {
                let p = self.at(p);
                (p.world, p.depth + 1)
            }
            None => (Affine::IDENTITY, 0),
        };
        let n = self.at_mut(slot);
        n.local = shape::local_transform(&n.props);
        n.world = parent_world * n.local;
        n.depth = depth;
    }

    /// Пересчитывает границы узла по его свойствам и уже готовым границам детей.
    /// `update_tree` — обновить запись в R-tree (при полном построении дерево
    /// строится пакетно).
    fn update_bounds(&mut self, slot: Slot, update_tree: bool) {
        let n = self.at(slot);
        let geom = n.geometry();
        let local_bounds = match geom {
            Geometry::None => n
                .children
                .iter()
                .map(|&c| self.at(c))
                .filter(|c| c.props.visible)
                .filter_map(|c| c.local_bounds.map(|b| c.local.transform_rect_bbox(b)))
                .reduce(|a, b| a.union(b)),
            _ => geom.bounds(),
        };
        let outset = geom.stroke_outset(&n.props.strokes);
        let world_bounds =
            local_bounds.map(|b| n.world.transform_rect_bbox(b.inflate(outset, outset)));
        // Фрейм с обрезкой: дети не выходят за него, перебирать их не нужно.
        let subtree_bounds = if !n.props.visible {
            None
        } else if n.props.kind == NodeKind::Frame && n.props.clip_content {
            world_bounds
        } else {
            n.children
                .iter()
                .filter_map(|&c| self.at(c).subtree_bounds)
                .fold(world_bounds, |acc, b| Some(acc.map_or(b, |a| a.union(b))))
        };
        let old = n.world_bounds;
        let n = self.at_mut(slot);
        n.local_bounds = local_bounds;
        n.world_bounds = world_bounds;
        n.subtree_bounds = subtree_bounds;
        if update_tree && old != world_bounds {
            if let Some(b) = old {
                self.tree_removals.push((slot, b));
            }
            if let Some(b) = world_bounds {
                self.tree_inserts.push((slot, b));
            }
        }
    }

    /// Применяет накопленные изменения R-tree. Удаление по одной записи стоит
    /// ~1.3 мкс (поиск от корня), а один проход по всему дереву с выборкой по
    /// пометкам — ~0.3 мс на 100 000 записей, поэтому большие пачки удаляются проходом.
    fn flush_tree(&mut self) {
        let removals = std::mem::take(&mut self.tree_removals);
        if removals.len() > TREE_DRAIN_THRESHOLD {
            let mark = self.next_mark();
            for &(s, _) in &removals {
                self.marks[s as usize] = mark;
            }
            let marked = Marked {
                marks: &self.marks,
                mark,
            };
            self.rtree
                .drain_with_selection_function(marked)
                .for_each(drop);
        } else {
            for (s, b) in removals {
                self.rtree.remove(&entry(b, s));
            }
        }
        for (s, b) in std::mem::take(&mut self.tree_inserts) {
            self.rtree.insert(entry(b, s));
        }
    }

    /// Пересчитывает узлы `dirty`: мировую трансформацию и границы их поддеревьев,
    /// затем границы всех предков снизу вверх. `regrouped` — узлы, у которых
    /// поменялся только состав детей (ребёнок ушёл или удалён): им и их предкам
    /// пересчитываются границы. Каждый узел — не больше одного раза.
    fn recompute(&mut self, mut dirty: Vec<Slot>, regrouped: Vec<Slot>) {
        if dirty.is_empty() && regrouped.is_empty() {
            return;
        }
        dirty.sort_unstable_by_key(|&s| self.depth_of(s));
        dirty.dedup();
        let mark = self.next_mark();
        let mut subtree = Vec::new();
        let mut tops = Vec::new();
        for s in dirty {
            if self.marks[s as usize] == mark {
                continue;
            }
            tops.push(s);
            let start = subtree.len();
            subtree.push(s);
            let mut i = start;
            // Обход в ширину внутри поддерева: родитель всегда раньше детей.
            while i < subtree.len() {
                let cur = subtree[i];
                self.marks[cur as usize] = mark;
                subtree.extend(self.at(cur).children.iter().copied());
                i += 1;
            }
        }
        for &s in &subtree {
            self.update_world(s);
        }
        self.next_version += 1;
        let version = self.next_version;
        for &s in subtree.iter().rev() {
            self.update_bounds(s, true);
            self.at_mut(s).subtree_version = version;
        }
        // Предки: границы группы и поддерева зависят от детей.
        let mut ancestors = Vec::new();
        let starts: Vec<Option<Slot>> = tops
            .into_iter()
            .map(|s| self.at(s).parent)
            .chain(regrouped.into_iter().map(Some))
            .collect();
        for start in starts {
            let mut cur = start;
            while let Some(p) = cur {
                if self.marks[p as usize] == mark {
                    break;
                }
                self.marks[p as usize] = mark;
                ancestors.push(p);
                cur = self.at(p).parent;
            }
        }
        ancestors.sort_unstable_by_key(|&s| std::cmp::Reverse(self.at(s).depth));
        for s in ancestors {
            self.update_bounds(s, true);
            self.at_mut(s).subtree_version = version;
        }
    }

    /// Глубина по цепочке родителей: после перемещения поле `depth` ещё старое.
    fn depth_of(&self, slot: Slot) -> u32 {
        let mut d = 0;
        let mut cur = self.at(slot).parent;
        while let Some(p) = cur {
            d += 1;
            cur = self.at(p).parent;
        }
        d
    }

    /// Применяет изменения документа. Каждый узел из `changes` сверяется с текущим
    /// состоянием страницы: есть ли он, где стоит, какие у него свойства. Поэтому
    /// повторы, пересечения списков и порядок операций внутри транзакции не важны.
    pub fn apply(&mut self, changes: &Changes, page: &PageDoc) {
        if changes.is_empty() {
            return;
        }
        if self.try_apply(changes, page).is_none() {
            // Сцена разошлась с документом (например, построена по кэшу, который
            // документ отверг) — перестраиваем целиком, превью сохраняется.
            let preview: Vec<(NodeId, PropsPatch)> = self
                .preview
                .iter()
                .map(|(&s, (_, patch))| (self.at(s).id, patch.clone()))
                .collect();
            *self = Scene::from_page(page);
            self.set_preview(&preview);
        }
    }

    fn try_apply(&mut self, changes: &Changes, page: &PageDoc) -> Option<()> {
        let mut seen = HashSet::with_capacity(changes.len());
        // Где каждый узел стоит в документе сейчас: `None` — узла больше нет.
        let placed: Vec<(NodeId, Option<Placement>)> = changes
            .created
            .iter()
            .chain(&changes.deleted)
            .chain(&changes.moved)
            .copied()
            .filter(|id| seen.insert(*id))
            .map(|id| (id, page.placement(id)))
            .collect();

        // 1. Отцепить все затронутые узлы, потом удалить исчезнувшие. Сначала только
        // отцепить: узел, вынесенный из удаляемого фрейма, не должен уйти вместе с ним.
        // Списки детей, которые изменились, и первая изменённая позиция в каждом.
        let mut touched: HashMap<Option<Slot>, usize> = HashMap::new();
        let detached: Vec<Slot> = placed.iter().filter_map(|(id, _)| self.slot(*id)).collect();
        self.detach(&detached, &mut touched);
        for (id, place) in &placed {
            if place.is_none() {
                if let Some(s) = self.slot(*id) {
                    self.remove_subtree(s);
                }
            }
        }

        // 2. Создать недостающие узлы, затем прицепить к новым родителям. Сначала все
        // узлы: родитель, созданный в той же транзакции, должен уже иметь слот.
        let mut attach = Vec::with_capacity(placed.len());
        for (id, place) in placed {
            let Some((parent, position)) = place else {
                continue;
            };
            let slot = match self.slot(id) {
                Some(s) => s,
                None => self.alloc(id, page.props(id).ok()?),
            };
            attach.push((slot, parent, position));
        }
        let mut inserts: HashMap<Option<Slot>, Vec<(Option<usize>, Slot)>> = HashMap::new();
        let mut dirty = Vec::new();
        for (slot, parent, position) in attach {
            let parent = match parent {
                Some(p) => Some(self.slot(p)?),
                None => None,
            };
            self.at_mut(slot).parent = parent;
            inserts.entry(parent).or_default().push((position, slot));
            dirty.push(slot);
        }
        for (parent, list) in inserts {
            let first = match merge(self.list(parent), list) {
                Some((merged, first)) => {
                    *self.list_mut(parent) = merged;
                    first
                }
                // Позиции неизвестны (скрытые узлы на странице) или не сходятся со
                // сценой — список перечитывается из документа.
                None => {
                    let order = page.children(parent.map(|p| self.at(p).id));
                    let slots: Option<Vec<Slot>> = order.iter().map(|id| self.slot(*id)).collect();
                    *self.list_mut(parent) = slots?;
                    0
                }
            };
            let at = touched.entry(parent).or_insert(first);
            *at = (*at).min(first);
        }
        // Позиции в списке сдвигаются только начиная с первой изменённой.
        for (&parent, &from) in &touched {
            // Освобождённый слот мог достаться новому узлу; его список уже верен.
            if parent.is_some_and(|p| !self.alive(p)) {
                continue;
            }
            let list = match parent {
                Some(p) => &self.nodes.get(p).as_ref().expect("живой узел").children,
                None => &self.roots,
            };
            for (i, &s) in list.iter().enumerate().skip(from) {
                self.index[s as usize] = i as u32;
            }
        }

        // 3. Свойства. Созданные узлы прочитаны при создании; перемещённый узел,
        // сменивший и свойства, есть и в `changes.props`.
        for id in &changes.props {
            let Some(slot) = self.slot(*id) else { continue };
            let Ok(props) = page.props(*id) else { continue };
            self.set_doc_props(slot, props);
            dirty.push(slot);
        }
        let regrouped = touched
            .into_keys()
            .flatten()
            .filter(|p| self.alive(*p))
            .collect();
        self.recompute(dirty, regrouped);
        self.flush_tree();
        Some(())
    }

    /// Свойства из документа; превью жеста, если оно есть, накладывается поверх.
    fn set_doc_props(&mut self, slot: Slot, mut props: Props) {
        if let Some((doc, patch)) = self.preview.get_mut(&slot) {
            *doc = props.clone();
            patch.apply_to(&mut props);
        }
        shape::sanitize(&mut props);
        self.next_version += 1;
        let version = self.next_version;
        let n = self.at_mut(slot);
        if looks_different(&n.props, &props) {
            n.version = version;
        }
        n.props = props;
    }

    /// Убирает узлы из списков детей их родителей. Из одного списка — одним проходом.
    /// В `touched` отмечает первую изменённую позицию каждого списка.
    fn detach(&mut self, slots: &[Slot], touched: &mut HashMap<Option<Slot>, usize>) {
        let mut by_parent: HashMap<Option<Slot>, Vec<Slot>> = HashMap::new();
        for &s in slots {
            by_parent.entry(self.at(s).parent).or_default().push(s);
        }
        for (parent, gone) in by_parent {
            let first = gone
                .iter()
                .map(|&s| self.index[s as usize] as usize)
                .min()
                .unwrap_or(0);
            let at = touched.entry(parent).or_insert(first);
            *at = (*at).min(first);
            if let [one] = gone[..] {
                let list = self.list_mut(parent);
                if let Some(i) = list.iter().position(|&s| s == one) {
                    list.remove(i);
                }
            } else {
                let mark = self.next_mark();
                for &s in &gone {
                    self.marks[s as usize] = mark;
                }
                let marks = std::mem::take(&mut self.marks);
                self.list_mut(parent).retain(|&s| marks[s as usize] != mark);
                self.marks = marks;
            }
            for s in gone {
                self.at_mut(s).parent = None;
            }
        }
    }

    /// Освобождает узел и его поддерево (узел уже отцеплен от родителя).
    fn remove_subtree(&mut self, slot: Slot) {
        for s in self.preorder(&[slot]) {
            let n = self.nodes.get_mut(s).take().expect("живой узел сцены");
            if let Some(b) = n.world_bounds {
                self.tree_removals.push((s, b));
            }
            self.by_id.remove(&n.id);
            self.preview.remove(&s);
            self.free.push(s);
        }
    }

    /// Временные правки свойств для текущего жеста (перетаскивание, ресайз, поворот).
    /// Документ не меняется; сцена показывает узлы так, будто правка применена.
    /// Каждый вызов заменяет превью целиком: узлы, которых нет в новом списке,
    /// возвращаются к свойствам документа. Неизвестные узлы пропускаются.
    pub fn set_preview(&mut self, patches: &[(NodeId, PropsPatch)]) {
        let mut dirty = Vec::new();
        let new: HashSet<Slot> = patches
            .iter()
            .filter_map(|(id, _)| self.slot(*id))
            .collect();
        let stale: Vec<Slot> = self
            .preview
            .keys()
            .copied()
            .filter(|s| !new.contains(s))
            .collect();
        for s in stale {
            let (doc, _) = self.preview.remove(&s).expect("есть в превью");
            self.set_doc_props(s, doc);
            dirty.push(s);
        }
        for (id, patch) in patches {
            let Some(slot) = self.slot(*id) else { continue };
            let doc = match self.preview.remove(&slot) {
                Some((doc, _)) => doc,
                None => self.at(slot).props.clone(),
            };
            self.preview.insert(slot, (doc.clone(), patch.clone()));
            self.set_doc_props(slot, doc);
            dirty.push(slot);
        }
        self.recompute(dirty, Vec::new());
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
    /// R-tree без точной проверки формы. Для рамки выделения и отладки отсечения.
    pub fn query_rect(&self, rect: Rect) -> Vec<NodeId> {
        self.rtree
            .locate_in_envelope_intersecting(&aabb(rect))
            .map(|e| self.at(e.data).id)
            .collect()
    }

    /// Все узлы под точкой, верхний первым. `tolerance` — допуск в мировых единицах
    /// для тонких линий и обводок (4 экранных пикселя, делённые на зум).
    ///
    /// R-tree отбирает кандидатов по границам, затем каждый проверяется точно: форма
    /// с поворотом и обводкой, видимость узла и предков, обрезка фреймами-предками.
    /// Заблокированные узлы тоже возвращаются — правила выделения решает редактор.
    pub fn hit_test_all(&self, point: Point, tolerance: f64) -> Vec<NodeId> {
        let probe = Rect::from_center_size(point, (2.0 * tolerance, 2.0 * tolerance));
        let mut hits: Vec<(Vec<u32>, NodeId)> = self
            .rtree
            .locate_in_envelope_intersecting(&aabb(probe))
            .map(|e| e.data)
            .filter(|&s| self.hits(s, point, tolerance))
            .map(|s| (self.order_key(s), self.at(s).id))
            .collect();
        hits.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        hits.into_iter().map(|(_, id)| id).collect()
    }

    /// Верхний узел под точкой.
    pub fn hit_test(&self, point: Point, tolerance: f64) -> Option<NodeId> {
        self.hit_test_all(point, tolerance).into_iter().next()
    }

    fn hits(&self, slot: Slot, point: Point, tolerance: f64) -> bool {
        let n = self.at(slot);
        if n.props.kind == NodeKind::Group || !self.is_visible(slot) {
            return false;
        }
        // Предки-фреймы с обрезкой скрывают всё, что за их формой.
        let mut cur = n.parent;
        while let Some(p) = cur {
            let a = self.at(p);
            if a.props.kind == NodeKind::Frame
                && a.props.clip_content
                && !a.geometry().contains(a.world.inverse() * point)
            {
                return false;
            }
            cur = a.parent;
        }
        shape::hit(&n.props, n.world.inverse() * point, tolerance)
    }

    /// Ключ порядка отрисовки: позиции от корня до узла. Больше — выше на экране;
    /// потомок выше предка.
    pub fn order_key(&self, slot: Slot) -> Vec<u32> {
        let mut key = Vec::new();
        let mut cur = Some(slot);
        while let Some(s) = cur {
            key.push(self.index[s as usize]);
            cur = self.at(s).parent;
        }
        key.reverse();
        key
    }
}

/// Вставляет узлы в список детей на их итоговые позиции за один проход:
/// неперемещённые дети сохраняют порядок, вставленные встают точно на свои места.
/// Возвращает новый список и первую изменённую позицию; `None` — позиции
/// неизвестны или противоречат длине списка.
fn merge(old: &[Slot], mut inserted: Vec<(Option<usize>, Slot)>) -> Option<(Vec<Slot>, usize)> {
    let total = old.len() + inserted.len();
    inserted.sort_unstable_by_key(|(i, _)| *i);
    let mut out = Vec::with_capacity(total);
    let mut rest = old.iter().copied();
    let mut next = inserted.iter().peekable();
    let first = inserted.first()?.0?;
    for at in 0..total {
        match next.peek() {
            Some((Some(i), slot)) if *i == at => {
                out.push(*slot);
                next.next();
            }
            Some((Some(i), _)) if *i < at => return None,
            Some((None, _)) => return None,
            _ => out.push(rest.next()?),
        }
    }
    Some((out, first))
}

/// Меняет ли правка то, что рисуется в локальных координатах узла.
fn looks_different(a: &Props, b: &Props) -> bool {
    a.kind != b.kind
        || a.w != b.w
        || a.h != b.h
        || a.corner_radius != b.corner_radius
        || a.fills != b.fills
        || a.strokes != b.strokes
}

fn aabb(r: Rect) -> AABB<[f64; 2]> {
    AABB::from_corners([r.x0, r.y0], [r.x1, r.y1])
}

fn entry(r: Rect, slot: Slot) -> Entry {
    GeomWithData::new(Rectangle::from_corners([r.x0, r.y0], [r.x1, r.y1]), slot)
}

/// Внутреннее состояние для тестов: узлы в порядке обхода с родителями, позициями,
/// трансформациями и границами, плюс содержимое R-tree. Две сцены одной страницы,
/// построенные разными путями, должны давать одинаковый снимок.
#[doc(hidden)]
pub mod testing {
    use super::*;

    #[derive(Debug, PartialEq)]
    pub struct NodeState {
        pub id: NodeId,
        pub parent: Option<NodeId>,
        pub index: u32,
        pub depth: u32,
        pub props: Props,
        pub world: [f64; 6],
        pub local_bounds: Option<Rect>,
        pub world_bounds: Option<Rect>,
        pub subtree_bounds: Option<Rect>,
    }

    pub fn snapshot(scene: &Scene) -> (Vec<NodeState>, Vec<(String, [f64; 4])>) {
        let order = scene.preorder(scene.roots());
        let nodes = order
            .iter()
            .map(|&s| {
                let n = scene.at(s);
                NodeState {
                    id: n.id,
                    parent: n.parent.map(|p| scene.at(p).id),
                    index: scene.index[s as usize],
                    depth: n.depth,
                    props: n.props.clone(),
                    world: n.world.as_coeffs(),
                    local_bounds: n.local_bounds,
                    world_bounds: n.world_bounds,
                    subtree_bounds: n.subtree_bounds,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(order.len(), scene.len(), "все узлы достижимы от корней");
        let mut tree: Vec<(String, [f64; 4])> = scene
            .rtree
            .iter()
            .map(|e| {
                let (lo, hi) = (e.geom().lower(), e.geom().upper());
                (
                    scene.at(e.data).id.to_string(),
                    [lo[0], lo[1], hi[0], hi[1]],
                )
            })
            .collect();
        tree.sort_by(|a, b| a.partial_cmp(b).expect("границы конечны"));
        (nodes, tree)
    }
}
````

- [ ] **Step 5: Запустить тесты**

Run: `cargo test -p ferrum-scene --test scene`
Expected: PASS, `13 passed`.

Проверка, что тест пакетного обновления R-tree не проходит вхолостую: если временно заставить `Marked::should_unpack_leaf` возвращать `false`, `large_batches_match_rebuild` падает.

- [ ] **Step 6: Закоммитить**

```bash
git add crates/ferrum-scene
git commit -m "feat(scene): таблица сцены, R-tree, обновление по изменениям и превью"
```

---

### Task 4: Property-тест: сцена после изменений совпадает с построенной заново

**Files:**
- Test: `crates/ferrum-scene/tests/apply_props.rs`

**Interfaces:**
- Consumes: `Scene::{apply, from_page}`, `testing::snapshot`, `Document::{transact, undo, redo, take_changes}`.
- Produces: —

Случайные транзакции из одной–трёх правок (создание, перенос, перенос пачкой, удаление, геометрия, видимость и обрезка), вперемешку с отменой и повтором. После каждого шага снимок обновлённой сцены (узлы в порядке обхода, родители, позиции, трансформации, все границы и содержимое R-tree) сравнивается со сценой, построенной заново. Счётчики в конце не дают тесту пройти вхолостую: структурных шагов, шагов только со свойствами, отмен и повторов и откатов транзакций должно быть заметное число.

- [ ] **Step 1: Написать property-тест**

`crates/ferrum-scene/tests/apply_props.rs`:

````rust
//! Случайные транзакции, отмены и повторы: сцена, обновлённая по `Changes`,
//! после каждого шага совпадает со сценой, построенной заново по странице.

use std::cell::Cell;

use ferrum_doc::*;
use ferrum_scene::{testing::snapshot, Scene};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

#[derive(Debug, Clone)]
enum Edit {
    Create {
        parent: usize,
        kind: u8,
        index: usize,
    },
    Move {
        node: usize,
        parent: usize,
        index: usize,
    },
    MoveMany {
        nodes: (usize, usize),
        parent: usize,
        index: usize,
    },
    Delete {
        node: usize,
    },
    Geometry {
        node: usize,
        x: i16,
        w: u8,
        rotation: u8,
    },
    Toggle {
        node: usize,
        visible: bool,
        clip: bool,
    },
}

#[derive(Debug, Clone)]
enum Step {
    Tx(Vec<Edit>),
    Undo,
    Redo,
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        3 => (any::<usize>(), 0u8..6, 0usize..6)
            .prop_map(|(parent, kind, index)| Edit::Create { parent, kind, index }),
        2 => (any::<usize>(), any::<usize>(), 0usize..6)
            .prop_map(|(node, parent, index)| Edit::Move { node, parent, index }),
        1 => (any::<(usize, usize)>(), any::<usize>(), 0usize..6)
            .prop_map(|(nodes, parent, index)| Edit::MoveMany { nodes, parent, index }),
        1 => any::<usize>().prop_map(|node| Edit::Delete { node }),
        3 => (any::<usize>(), any::<i16>(), any::<u8>(), any::<u8>())
            .prop_map(|(node, x, w, rotation)| Edit::Geometry { node, x, w, rotation }),
        1 => (any::<usize>(), any::<bool>(), any::<bool>())
            .prop_map(|(node, visible, clip)| Edit::Toggle { node, visible, clip }),
    ]
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => prop::collection::vec(edit(), 1..4).prop_map(Step::Tx),
        1 => Just(Step::Undo),
        1 => Just(Step::Redo),
    ]
}

const KINDS: [NodeKind; 6] = [
    NodeKind::Frame,
    NodeKind::Group,
    NodeKind::Rect,
    NodeKind::Ellipse,
    NodeKind::Line,
    NodeKind::Image,
];

fn pick(alive: &[NodeId], i: usize) -> Option<NodeId> {
    (!alive.is_empty() && i % (alive.len() + 1) != alive.len())
        .then(|| alive[i % (alive.len() + 1)])
}

fn run(p: &Tx, e: &Edit) -> DocResult<()> {
    let alive = p.walk();
    match *e {
        Edit::Create {
            parent,
            kind,
            index,
        } => {
            let id = p.create_node(pick(&alive, parent), Some(index), KINDS[kind as usize])?;
            p.set_props(
                id,
                &PropsPatch {
                    x: Some(index as f64 * 13.0),
                    y: Some(kind as f64 * 7.0),
                    ..Default::default()
                },
            )
        }
        Edit::Move {
            node,
            parent,
            index,
        } => match pick(&alive, node) {
            Some(n) => p.move_node(n, pick(&alive, parent), index),
            None => Ok(()),
        },
        Edit::MoveMany {
            nodes: (a, b),
            parent,
            index,
        } => match pick(&alive, a) {
            Some(first) => {
                let ids: Vec<NodeId> = [Some(first), pick(&alive, b)]
                    .into_iter()
                    .flatten()
                    .collect();
                p.move_nodes(&ids, pick(&alive, parent), index)
            }
            None => Ok(()),
        },
        Edit::Delete { node } => match pick(&alive, node) {
            Some(n) => p.delete_nodes(&[n]),
            None => Ok(()),
        },
        Edit::Geometry {
            node,
            x,
            w,
            rotation,
        } => match pick(&alive, node) {
            Some(n) => p.set_props(
                n,
                &PropsPatch {
                    x: Some(x as f64),
                    w: Some(w as f64),
                    rotation: Some(rotation as f64 * 15.0),
                    ..Default::default()
                },
            ),
            None => Ok(()),
        },
        Edit::Toggle {
            node,
            visible,
            clip,
        } => match pick(&alive, node) {
            Some(n) => p.set_props(
                n,
                &PropsPatch {
                    visible: Some(visible),
                    clip_content: Some(clip),
                    ..Default::default()
                },
            ),
            None => Ok(()),
        },
    }
}

#[test]
fn scene_after_changes_matches_rebuild() {
    let structural = Cell::new(0usize);
    let props_only = Cell::new(0usize);
    let history = Cell::new(0usize);
    let failed_tx = Cell::new(0usize);
    let mut runner = TestRunner::new(Config {
        cases: 96,
        ..Config::default()
    });
    runner
        .run(&prop::collection::vec(step(), 1..40), |steps| {
            let mut doc = Document::new();
            let page = doc.pages()[0].0;
            let mut scene = Scene::new();
            for s in &steps {
                match s {
                    Step::Tx(edits) => {
                        let r = doc.transact(page, vec![], |p| {
                            edits.iter().try_for_each(|e| run(p, e)).map(|_| vec![])
                        });
                        if r.is_err() {
                            failed_tx.set(failed_tx.get() + 1);
                        }
                    }
                    Step::Undo => {
                        if doc.undo().unwrap().is_some() {
                            history.set(history.get() + 1);
                        }
                    }
                    Step::Redo => {
                        if doc.redo().unwrap().is_some() {
                            history.set(history.get() + 1);
                        }
                    }
                }
                let changes = doc.take_changes(page);
                if changes.created.len() + changes.deleted.len() + changes.moved.len() > 0 {
                    structural.set(structural.get() + 1);
                } else if !changes.is_empty() {
                    props_only.set(props_only.get() + 1);
                }
                let p = doc.page(page).unwrap();
                scene.apply(&changes, p);
                let fresh = Scene::from_page(p);
                prop_assert_eq!(snapshot(&scene), snapshot(&fresh), "после {:?}", s);
            }
            Ok(())
        })
        .unwrap();
    // Тест не должен проходить вхолостую: сцена обновлялась всеми путями.
    assert!(
        structural.get() > 300,
        "структурных шагов: {}",
        structural.get()
    );
    assert!(
        props_only.get() > 100,
        "шагов только со свойствами: {}",
        props_only.get()
    );
    assert!(history.get() > 100, "отмен и повторов: {}", history.get());
    assert!(
        failed_tx.get() > 0,
        "откатов транзакций: {}",
        failed_tx.get()
    );
}
````

- [ ] **Step 2: Запустить тест**

Run: `cargo test -p ferrum-scene --test apply_props`
Expected: PASS, `1 passed`, около 0.3 с в debug. В прототипе за прогон набиралось ~980 структурных шагов, ~240 шагов со свойствами, ~330 отмен и повторов, ~130 откатов — пороги в тесте взяты с запасом в 2–3 раза.

Проверка, что тест ловит ошибки: если временно убрать из `Scene::recompute` пересчёт границ родителей, у которых ушёл ребёнок (`.chain(regrouped.into_iter().map(Some))`), тест падает.

- [ ] **Step 3: Закоммитить**

```bash
git add crates/ferrum-scene/tests/apply_props.rs
git commit -m "test(scene): property-тест обновления сцены по изменениям документа"
```

---

### Task 5: Крейт `ferrum-render`: бэкенд, камера, изображения

Трейт `RenderBackend` — общее подмножество API `vello_cpu::RenderContext` и `vello_gpu::Scene`: матрица, краска (цвет или изображение), заливка пути и прямоугольника, обводка (только для оверлеев), обрезка путём, слой прозрачности. Обрезка — `push_clip_path` без отдельного слоя композиции: так дешевле.

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/ferrum-render/Cargo.toml`
- Create: `crates/ferrum-render/src/lib.rs`
- Create: `crates/ferrum-render/src/backend.rs`
- Create: `crates/ferrum-render/src/camera.rs`
- Create: `crates/ferrum-render/src/image.rs`
- Test: `crates/ferrum-render/tests/camera.rs`

**Interfaces:**
- Consumes: —
- Produces: `RenderBackend` (`size`, `set_transform`, `set_color`, `set_image(&ImagePaint)`, `fill_path(&BezPath, Fill)`, `fill_rect`, `stroke_path`, `push_clip`, `pop_clip`, `push_opacity_layer`, `pop_layer`, `supports_images` — по умолчанию `true`); `Rgba = AlphaColor<Srgb>`; `ImagePaint { pixmap: Arc<Pixmap>, transform, extend, alpha }`; `CpuBackend::{new, reset, render -> Pixmap}`; `Camera` с `new(viewport: Size, dpr)`, полями `center`, `viewport`, `dpr`, методами `zoom`, `set_zoom` (2%–25 600%), `zoom_at`, `pan_by`, `fit`, `world_to_screen`, `screen_to_world`, `world_to_screen_transform`, `view_transform` (мир → пиксели устройства), `device_scale`, `device_size`, `visible_world_rect`; `ImageProvider::image(hash) -> Option<Arc<Pixmap>>` (реализован для `HashMap<String, Arc<Pixmap>>`), `NoImages`, `decode_png` (проверяет размер по заголовку до декодирования, не больше `MAX_IMAGE_SIDE` = 8192); реэкспорт `Pixmap`.

- [ ] **Step 1: Добавить крейт и зависимости рендера**

`vello_cpu` подключается без своих текстовых возможностей: глифы рисуются контурами (Task 7). `parley` — без системных шрифтов. `skrifa` — той же версии, что тянет `parley` 0.12 (0.49 слишком новая).

`Cargo.toml`:

````toml
[workspace]
resolver = "2"
members = ["crates/ferrum-doc", "crates/ferrum-scene", "crates/ferrum-render"]

[workspace.package]
edition = "2021"
rust-version = "1.96"
license = "UNLICENSED"

[workspace.dependencies]
loro = "=1.16.2"
# loro 1.16.2 не собирается с loro-internal 1.17.x — фиксируем совместимую версию
loro-internal = "=1.16.2"
thiserror = "2.0.20"
blake3 = "1.8"
criterion = "0.8"
proptest = "1.12"
kurbo = "=0.13.1"
# 0.13.0 не годится: после bulk_load вставка падает с «This is a bug in rstar»
# (листья на разной глубине, georust/rstar#254 ещё не выпущен).
rstar = "=0.12.2"
# Рендер: vello_cpu сейчас, vello_gpu (бывший vello_hybrid) — на этапе 4. Без
# текстовых возможностей vello: глифы рисуются контурами из skrifa (см. text.rs).
vello_cpu = { version = "=0.3.0", default-features = false, features = ["std", "png", "u8_pipeline"] }
vello_gpu = { version = "=0.3.0", default-features = false, features = ["std", "wgpu"] }
# Без системных шрифтов: подписи только встроенным Onest, одинаково везде.
parley = { version = "=0.12.0", default-features = false, features = ["std"] }
# Та же версия, что тянет parley 0.12: контуры глифов для подписей.
skrifa = "=0.44.0"
````

`crates/ferrum-render/Cargo.toml`:

````toml
[package]
name = "ferrum-render"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[features]
# Бэкенд vello_gpu (WebGPU через wgpu). Только сборка и кодирование сцены;
# поверхности и растеризация на GPU — этап 4.
gpu = ["dep:vello_gpu"]

[dependencies]
ferrum-doc = { path = "../ferrum-doc" }
ferrum-scene = { path = "../ferrum-scene" }
kurbo.workspace = true
vello_cpu.workspace = true
vello_gpu = { workspace = true, optional = true }
parley.workspace = true
skrifa.workspace = true
thiserror.workspace = true
````

`crates/ferrum-render/src/lib.rs`:

````rust
//! Рендер Ferrum: камера, сборка кадра со списками отрисовки, отсечением и LOD,
//! оверлеи, экспорт PNG. Получает сцену, камеру и описание оверлеев; о Loro и
//! инструментах не знает.

mod backend;
mod camera;
mod image;

pub use backend::{CpuBackend, ImagePaint, RenderBackend, Rgba};
pub use camera::{Camera, MAX_ZOOM, MIN_ZOOM};
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use vello_cpu::Pixmap;
````

- [ ] **Step 2: Написать падающие тесты камеры**

`crates/ferrum-render/tests/camera.rs`:

````rust
use ferrum_render::{Camera, MAX_ZOOM, MIN_ZOOM};
use kurbo::{Point, Rect, Size, Vec2};

fn near(a: Point, b: Point) -> bool {
    (a - b).hypot() < 1e-9
}

#[test]
fn zoom_is_clamped_to_2_and_25600_percent() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_zoom(1000.0);
    assert_eq!(c.zoom(), MAX_ZOOM);
    c.set_zoom(0.0001);
    assert_eq!(c.zoom(), MIN_ZOOM);
    c.set_zoom(f64::NAN);
    assert_eq!(c.zoom(), 1.0);
    assert_eq!((MIN_ZOOM, MAX_ZOOM), (0.02, 256.0));
}

#[test]
fn screen_and_world_round_trip() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 2.0);
    c.center = Point::new(1000.0, -500.0);
    c.set_zoom(4.0);
    let world = Point::new(1010.0, -490.0);
    let screen = c.world_to_screen(world);
    assert!(near(screen, Point::new(440.0, 340.0)));
    assert!(near(c.screen_to_world(screen), world));
    // Матрица для содержимого — в пикселях устройства (dpr = 2).
    assert!(near(c.view_transform() * world, Point::new(880.0, 680.0)));
    assert_eq!(c.device_size(), (1600, 1200));
    assert_eq!(c.device_scale(), 8.0);
}

#[test]
fn zoom_at_keeps_point_under_cursor() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.center = Point::new(50.0, 50.0);
    let cursor = Point::new(600.0, 100.0);
    let before = c.screen_to_world(cursor);
    c.zoom_at(cursor, 3.0);
    assert_eq!(c.zoom(), 3.0);
    assert!(near(c.screen_to_world(cursor), before));
    // Упор в предел зума тоже не сдвигает точку под курсором.
    c.zoom_at(cursor, 1000.0);
    assert_eq!(c.zoom(), MAX_ZOOM);
    assert!((c.screen_to_world(cursor) - before).hypot() < 1e-6);
}

#[test]
fn pan_moves_content_with_cursor() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.set_zoom(2.0);
    let p = Point::new(10.0, 20.0);
    let before = c.world_to_screen(p);
    c.pan_by(Vec2::new(30.0, -10.0));
    assert!(near(c.world_to_screen(p), before + Vec2::new(30.0, -10.0)));
}

#[test]
fn fit_and_visible_rect() {
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.fit(Rect::new(0.0, 0.0, 1600.0, 600.0), 0.0);
    assert_eq!(c.zoom(), 0.5);
    let v = c.visible_world_rect();
    assert!((v.x0, v.x1, v.y0, v.y1) == (0.0, 1600.0, -300.0, 900.0));
}

#[test]
fn large_coordinates_stay_precise() {
    // Центр на 1e7: экранные координаты соседних точек мира различимы в f32.
    let mut c = Camera::new(Size::new(800.0, 600.0), 1.0);
    c.center = Point::new(1e7, 1e7);
    c.set_zoom(256.0);
    let a = c.view_transform() * Point::new(1e7 + 0.01, 1e7);
    let b = c.view_transform() * Point::new(1e7 + 0.02, 1e7);
    assert!(((b.x - a.x) as f32 - 2.56).abs() < 1e-3);
}
````

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-render --test camera`
Expected: ошибка компиляции — нет модулей `backend`, `camera`, `image`.

- [ ] **Step 4: Реализовать бэкенд, камеру и изображения**

`crates/ferrum-render/src/backend.rs`:

````rust
//! Общее подмножество API `vello_cpu::RenderContext` и `vello_gpu::Scene`, через
//! которое рендер рисует. Оба бэкенда — sparse strips: пути разбираются на полосы
//! при каждом вызове заливки, поэтому сборка кадра повторяет список отрисовки
//! узлов, а не склеивает готовые фрагменты.

use std::sync::Arc;

use kurbo::{Affine, BezPath, Rect, Stroke};
use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::peniko::{Extend, Fill, ImageQuality, ImageSampler};
use vello_cpu::{Image, ImageSource, Pixmap, RenderContext, Resources};

pub type Rgba = AlphaColor<Srgb>;

/// Изображение как краска.
#[derive(Clone)]
pub struct ImagePaint {
    pub pixmap: Arc<Pixmap>,
    /// Переводит пиксели изображения в координаты текущей трансформации.
    pub transform: Affine,
    /// `Repeat` — плитка, `Pad` — одно изображение.
    pub extend: Extend,
    pub alpha: f32,
}

pub trait RenderBackend {
    /// Размер области рисования в пикселях устройства.
    fn size(&self) -> (u16, u16);
    /// Трансформация из координат следующих путей в пиксели устройства.
    fn set_transform(&mut self, transform: Affine);
    fn set_color(&mut self, color: Rgba);
    fn set_image(&mut self, image: &ImagePaint);
    fn fill_path(&mut self, path: &BezPath, rule: Fill);
    fn fill_rect(&mut self, rect: &Rect);
    /// Обводка в координатах текущей трансформации — только для оверлеев: обводки
    /// узлов рисуются заливкой колец (см. `ferrum_scene::shape`).
    fn stroke_path(&mut self, path: &BezPath, stroke: &Stroke);
    /// Обрезка без отдельного слоя композиции — дешевле слоя.
    fn push_clip(&mut self, path: &BezPath);
    fn pop_clip(&mut self);
    fn push_opacity_layer(&mut self, alpha: f32);
    fn pop_layer(&mut self);
    /// Рисует ли бэкенд изображения из `Pixmap`. Без этого рендер не использует
    /// растровый кэш фреймов (он хранится как изображения).
    fn supports_images(&self) -> bool {
        true
    }
}

/// Растеризация на CPU через `vello_cpu`: эталонные тесты, экспорт PNG и запасной
/// рендер без GPU.
pub struct CpuBackend {
    ctx: RenderContext,
    resources: Resources,
}

impl CpuBackend {
    pub fn new(width: u16, height: u16) -> CpuBackend {
        CpuBackend {
            ctx: RenderContext::new(width, height),
            resources: Resources::new(),
        }
    }

    /// Очищает записанные команды перед новым кадром; размер меняется при ресайзе.
    pub fn reset(&mut self, width: u16, height: u16) {
        if (width, height) == self.size() {
            self.ctx.reset();
        } else {
            self.ctx.reset_and_resize(width, height);
        }
    }

    /// Растеризует записанный кадр в прозрачный `Pixmap`.
    pub fn render(&mut self) -> Pixmap {
        let (w, h) = self.size();
        let mut target = Pixmap::new(w, h);
        self.ctx.flush();
        self.ctx.render(&mut target, &mut self.resources);
        target
    }
}

impl RenderBackend for CpuBackend {
    fn size(&self) -> (u16, u16) {
        (self.ctx.width(), self.ctx.height())
    }

    fn set_transform(&mut self, transform: Affine) {
        self.ctx.set_transform(transform);
    }

    fn set_color(&mut self, color: Rgba) {
        self.ctx.reset_paint_transform();
        self.ctx.set_paint(color);
    }

    fn set_image(&mut self, image: &ImagePaint) {
        self.ctx.set_paint(Image {
            image: ImageSource::Pixmap(image.pixmap.clone()),
            sampler: ImageSampler {
                x_extend: image.extend,
                y_extend: image.extend,
                quality: ImageQuality::Medium,
                alpha: image.alpha,
            },
        });
        self.ctx.set_paint_transform(image.transform);
    }

    fn fill_path(&mut self, path: &BezPath, rule: Fill) {
        self.ctx.set_fill_rule(rule);
        self.ctx.fill_path(path);
    }

    fn fill_rect(&mut self, rect: &Rect) {
        self.ctx.fill_rect(rect);
    }

    fn stroke_path(&mut self, path: &BezPath, stroke: &Stroke) {
        self.ctx.set_stroke(stroke.clone());
        self.ctx.stroke_path(path);
    }

    fn push_clip(&mut self, path: &BezPath) {
        self.ctx.set_fill_rule(Fill::NonZero);
        self.ctx.push_clip_path(path);
    }

    fn pop_clip(&mut self) {
        self.ctx.pop_clip();
    }

    fn push_opacity_layer(&mut self, alpha: f32) {
        self.ctx.push_opacity_layer(alpha);
    }

    fn pop_layer(&mut self) {
        self.ctx.pop_layer();
    }
}
````

`crates/ferrum-render/src/camera.rs`:

````rust
//! Камера холста: центр в мировых координатах (`f64`), зум и размер вьюпорта.
//!
//! Точность: мировые координаты могут быть большими (1e7), а растеризатор работает в
//! `f32`. Поэтому итоговая матрица «мир → пиксели» собирается в `f64` относительно
//! центра камеры, и в `f32` переводятся уже экранные координаты (это делает vello при
//! разборе путей). Дрожания на больших координатах нет: в экранной системе числа малы.

use kurbo::{Affine, Point, Rect, Size, Vec2};

pub const MIN_ZOOM: f64 = 0.02;
pub const MAX_ZOOM: f64 = 256.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    /// Точка мира в центре вьюпорта.
    pub center: Point,
    zoom: f64,
    /// Размер вьюпорта в CSS-пикселях.
    pub viewport: Size,
    /// `devicePixelRatio`: пикселей устройства в одном CSS-пикселе.
    pub dpr: f64,
}

impl Camera {
    pub fn new(viewport: Size, dpr: f64) -> Camera {
        Camera {
            center: Point::ZERO,
            zoom: 1.0,
            viewport,
            dpr: if dpr.is_finite() && dpr > 0.0 {
                dpr
            } else {
                1.0
            },
        }
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    /// Зум ограничивается диапазоном 2%–25 600%.
    pub fn set_zoom(&mut self, zoom: f64) {
        self.zoom = if zoom.is_finite() {
            zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.0
        };
    }

    /// Зум к курсору: точка мира под `screen` остаётся на месте.
    pub fn zoom_at(&mut self, screen: Point, factor: f64) {
        let anchor = self.screen_to_world(screen);
        self.set_zoom(self.zoom * factor);
        self.center = anchor - (screen - self.screen_center()) / self.zoom;
    }

    /// Панорамирование на сдвиг в CSS-пикселях (содержимое едет вместе с курсором).
    pub fn pan_by(&mut self, screen_delta: Vec2) {
        self.center -= screen_delta / self.zoom;
    }

    /// Показать прямоугольник мира целиком с отступом в CSS-пикселях.
    pub fn fit(&mut self, world: Rect, padding: f64) {
        let avail = Size::new(
            (self.viewport.width - 2.0 * padding).max(1.0),
            (self.viewport.height - 2.0 * padding).max(1.0),
        );
        let w = world.width().max(1e-9);
        let h = world.height().max(1e-9);
        self.set_zoom((avail.width / w).min(avail.height / h));
        self.center = world.center();
    }

    fn screen_center(&self) -> Point {
        Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0)
    }

    pub fn world_to_screen(&self, world: Point) -> Point {
        self.screen_center() + (world - self.center) * self.zoom
    }

    pub fn screen_to_world(&self, screen: Point) -> Point {
        self.center + (screen - self.screen_center()) / self.zoom
    }

    /// Мир → CSS-пиксели.
    pub fn world_to_screen_transform(&self) -> Affine {
        Affine::translate(self.screen_center().to_vec2())
            * Affine::scale(self.zoom)
            * Affine::translate(-self.center.to_vec2())
    }

    /// Мир → пиксели устройства: матрица, с которой рисуется содержимое.
    pub fn view_transform(&self) -> Affine {
        Affine::scale(self.dpr) * self.world_to_screen_transform()
    }

    /// Пикселей устройства на единицу мира.
    pub fn device_scale(&self) -> f64 {
        self.zoom * self.dpr
    }

    /// Размер холста в пикселях устройства.
    pub fn device_size(&self) -> (u16, u16) {
        let px = |v: f64| (v * self.dpr).ceil().clamp(1.0, u16::MAX as f64) as u16;
        (px(self.viewport.width), px(self.viewport.height))
    }

    /// Видимая часть мира.
    pub fn visible_world_rect(&self) -> Rect {
        Rect::from_points(
            self.screen_to_world(Point::ZERO),
            self.screen_to_world(Point::new(self.viewport.width, self.viewport.height)),
        )
    }
}
````

`crates/ferrum-render/src/image.rs`:

````rust
//! Изображения для заливок. Рендер не декодирует байты документа сам: картинки
//! даёт поставщик по хэшу (в браузере их декодирует `createImageBitmap` в воркере,
//! в тестах и экспорте — `decode_png`).

use std::collections::HashMap;
use std::sync::Arc;

use vello_cpu::Pixmap;

/// Наибольшая сторона изображения: `Pixmap` хранит размеры в `u16`, а картинка из
/// файла не должна съесть память (16384² × 4 байта = 1 ГБ — уже слишком).
pub const MAX_IMAGE_SIDE: u16 = 8192;

pub trait ImageProvider {
    /// Готовое к отрисовке изображение (RGBA с премультипликацией) или `None`,
    /// если его ещё нет — тогда рисуется заглушка.
    fn image(&self, hash: &str) -> Option<Arc<Pixmap>>;
}

impl ImageProvider for HashMap<String, Arc<Pixmap>> {
    fn image(&self, hash: &str) -> Option<Arc<Pixmap>> {
        self.get(hash).cloned()
    }
}

/// Поставщик без изображений.
pub struct NoImages;

impl ImageProvider for NoImages {
    fn image(&self, _: &str) -> Option<Arc<Pixmap>> {
        None
    }
}

/// Декодирует PNG. Повреждённые и слишком большие картинки дают `None`.
///
/// Размер проверяется по заголовку до декодирования: иначе файл с размером
/// 65535×65535 в заголовке заставил бы декодер выделить 16 ГБ.
pub fn decode_png(bytes: &[u8]) -> Option<Pixmap> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let side = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().expect("4 байта"));
    let (w, h) = (side(16), side(20));
    let max = MAX_IMAGE_SIDE as u32;
    if w == 0 || h == 0 || w > max || h > max {
        return None;
    }
    Pixmap::from_png(std::io::Cursor::new(bytes)).ok()
}
````

- [ ] **Step 5: Закрепить зависимости рендера**

`parley_engine` требует `icu_* ^2.3`, а семейство `icu_*` 2.4.0 опубликовано 2026-10-22 и связано точными версиями друг с другом: по одной его не откатить. Поэтому сначала временно требуем 2.3.0 прямо в манифесте, обновляем и убираем строки — откат остаётся в `Cargo.lock`.

Выполнить:

```bash
# Новые зависимости попадают в Cargo.lock (уже закреплённые версии не меняются).
cargo fetch -q
# Семейство icu_* 2.4.0 связано точными версиями: временно требуем 2.3.0.
manifest=crates/ferrum-render/Cargo.toml
backup=$(mktemp)
cp "$manifest" "$backup"
printf '%s\n' 'icu_segmenter = "=2.3.0"' 'icu_properties = "=2.3.0"' \
  'icu_normalizer = "=2.3.0"' 'icu_locale_core = "=2.3.0"' >> "$manifest"
cargo update -q -p icu_segmenter -p icu_properties -p icu_normalizer -p icu_locale_core
cp "$backup" "$manifest"
pins="annotate-snippets@0.12 0.12.16
crc32fast@1 1.5.2
font-types@0.12 0.12.6
litrs@1 1.0.0
memmap2@0.9 0.9.11
tinystr@0.8 0.8.4
tokio@1 1.53.2
yoke@0.8 0.8.3
zerovec@0.11 0.11.8
zerovec-derive@0.11 0.11.6
icu_collections@2 2.3.0
icu_locale_core@2 2.3.0
icu_locale_fallback@2 2.3.0
icu_locale_fallback_data@2 2.3.0
icu_normalizer@2 2.3.0
icu_normalizer_data@2 2.3.0
icu_properties@2 2.3.0
icu_properties_data@2 2.3.0
icu_provider@2 2.3.1
icu_segmenter@2 2.3.0
icu_segmenter_data@2 2.3.0"
# Порядок важен не всегда: несколько проходов, пока все версии не встанут.
for _ in 1 2 3; do
  while read -r spec want; do
    cargo update -q -p "$spec" --precise "$want" 2>/dev/null || true
  done <<< "$pins"
done
while read -r spec want; do
  name=${spec%@*}
  grep -A1 "^name = \"$name\"$" Cargo.lock | grep -q "^version = \"$want\"$" \
    || { echo "не закреплён $name $want"; exit 1; }
done <<< "$pins"
```

- [ ] **Step 6: Запустить тесты**

Run: `cargo test -p ferrum-render --test camera`
Expected: PASS, `6 passed`. Первая сборка скачивает vello и parley и занимает несколько минут.

- [ ] **Step 7: Закоммитить**

```bash
git add Cargo.toml Cargo.lock crates/ferrum-render
git commit -m "feat(render): трейт бэкенда, бэкенд vello_cpu, камера, изображения"
```

---

### Task 6: Сборка кадра: списки отрисовки, отсечение, LOD, растровый кэш, экспорт PNG

Что показал прототип на странице из 100 000 узлов:
- Перестраивать контуры на каждом кадре дорого (~0.8 мкс на узел только наших расходов): список отрисовки хранит контуры формы и колец в локальных координатах. Контуры строятся при первом полном рисовании узла и выбрасываются, если узел не рисовался 300 кадров; мелкому узлу (LOD) список не нужен — только цвет.
- Самый частый случай — непрозрачная заливка и непрозрачная внутренняя обводка — рисуется двумя заливками (форма цветом обводки, сжатая форма цветом заливки) вместо заливки и кольца: на треть меньше работы vello.
- 100 000 прямоугольников LOD при отдалении стоят ~7 мс внутри vello плюс обход — больше 8 мс. Поэтому включён растровый кэш фреймов верхнего уровня (раздел 8.4 спеки): при масштабе устройства ≤ 0.25 контейнер верхнего уровня не больше 256 px рисуется из растра с плотностью — ближайшей степенью двойки не меньше масштаба. Растр годится, пока масштаб не упадёт вдвое, и устаревает по `subtree_version`. За кадр строится не больше 32 растров; остальные фреймы этого кадра рисуются узлами. Растр, экспорт и узлы внутри растра не кэшируют свои списки — иначе построение растров держало бы в памяти контуры всей страницы (в прототипе — 180 МБ).
- Отсечение — обходом дерева по `subtree_bounds`: поддерево вне вьюпорта отбрасывается целиком, порядок отрисовки сохраняется без сортировки. R-tree сцены служит hit-testing и запросам по рамке.

**Files:**
- Create: `crates/ferrum-render/src/display.rs`
- Create: `crates/ferrum-render/src/draw.rs`
- Modify: `crates/ferrum-render/src/lib.rs`
- Test: `crates/ferrum-render/tests/common/mod.rs`
- Test: `crates/ferrum-render/tests/render.rs`

**Interfaces:**
- Consumes: `Scene`, `SceneNode`, `shape::Geometry` из Task 3; бэкенд, камера, изображения из Task 5.
- Produces: `Renderer::{new, draw_scene(backend, scene, camera, images) -> FrameStats, export_png(scene, id, scale, images) -> Result<Vec<u8>, ExportError>, export_pixmap}`; `FrameStats { visited, culled, too_small, drawn, lod_rects, lists_built, rasters, raster_builds, clips, layers }`; `ExportError::{NotFound, BadScale, Empty, TooLarge(w, h), Encode}`; константы `LOD_SKIP_PX` (0.5), `LOD_RECT_PX` (3), `RASTER_MAX_PX` (256), `RASTER_MAX_SCALE` (0.25), `RASTER_BUILDS_PER_FRAME` (32), `EVICT_AFTER_FRAMES` (300), `MAX_EXPORT_SIDE` (16384).

- [ ] **Step 1: Помощники тестов**

`tests/common/mod.rs` — страница для наполнения, отрисовка на белом фоне, проверка пикселей и сравнение с эталонами (эталоны появятся в Task 8).

`crates/ferrum-render/tests/common/mod.rs`:

````rust
//! Помощники тестов рендера: сборка страницы, отрисовка, сравнение с эталонами.
#![allow(dead_code)]

use std::path::PathBuf;

use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{
    Camera, CpuBackend, ImageProvider, NoImages, Pixmap, RenderBackend, Renderer, Rgba,
};
use ferrum_scene::Scene;
use kurbo::{Affine, Point, Rect, Size};

/// Страница, которую тест наполняет узлами.
pub struct Page {
    pub doc: Document,
    pub page: PageId,
}

impl Page {
    pub fn new() -> Page {
        let doc = Document::new();
        let page = doc.pages()[0].0;
        Page { doc, page }
    }

    pub fn add(&mut self, parent: Option<NodeId>, kind: NodeKind, patch: PropsPatch) -> NodeId {
        self.doc
            .transact(self.page, vec![], |p| {
                let id = p.create_node(parent, None, kind)?;
                p.set_props(id, &patch)?;
                Ok(vec![id])
            })
            .unwrap()[0]
    }

    pub fn edit(&mut self, id: NodeId, patch: PropsPatch) {
        self.doc
            .transact(self.page, vec![], |p| {
                p.set_props(id, &patch).map(|_| vec![])
            })
            .unwrap();
    }

    pub fn scene(&mut self) -> Scene {
        self.doc.take_changes(self.page);
        Scene::from_page(self.doc.page(self.page).unwrap())
    }

    /// Применяет накопленные изменения к сцене.
    pub fn update(&mut self, scene: &mut Scene) {
        let changes = self.doc.take_changes(self.page);
        scene.apply(&changes, self.doc.page(self.page).unwrap());
    }
}

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

/// Камера с центром в середине холста `w × h` при зуме 1: мир совпадает с экраном.
pub fn camera(w: f64, h: f64) -> Camera {
    let mut c = Camera::new(Size::new(w, h), 1.0);
    c.center = Point::new(w / 2.0, h / 2.0);
    c
}

pub const WHITE: Rgba = Rgba::new([1.0, 1.0, 1.0, 1.0]);

/// Содержимое сцены на белом фоне, без оверлеев.
pub fn render(scene: &Scene, camera: &Camera) -> Pixmap {
    render_with(&mut Renderer::new(), scene, camera, &NoImages)
}

pub fn render_with(
    renderer: &mut Renderer,
    scene: &Scene,
    camera: &Camera,
    images: &dyn ImageProvider,
) -> Pixmap {
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(WHITE);
    backend.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
    renderer.draw_scene(&mut backend, scene, camera, images);
    backend.render()
}

/// Пиксель (с премультипликацией: на непрозрачном фоне совпадает с обычным цветом).
pub fn px(p: &Pixmap, x: u16, y: u16) -> [u8; 4] {
    let c = p.sample(x, y);
    [c.r, c.g, c.b, c.a]
}

/// Цвет пикселя совпадает с ожидаемым с точностью `tol` по каждому каналу.
#[track_caller]
pub fn assert_px(p: &Pixmap, x: u16, y: u16, rgb: [u8; 3], tol: u8) {
    let got = px(p, x, y);
    let ok = (0..3).all(|i| got[i].abs_diff(rgb[i]) <= tol);
    assert!(ok, "пиксель ({x}, {y}) = {got:?}, ожидался {rgb:?} ±{tol}");
}

/// Допуск сравнения с эталоном по каждому каналу: сглаживание краёв на разных
/// процессорах (NEON, AVX2, WASM SIMD) может отличаться на единицы.
pub const GOLDEN_TOLERANCE: u8 = 3;

/// Сравнивает картинку с эталоном `tests/golden/<name>.png`. С
/// `FERRUM_UPDATE_GOLDEN=1` эталон перезаписывается. Отличающийся результат
/// сохраняется рядом с временными файлами сборки для просмотра.
#[track_caller]
pub fn golden(name: &str, actual: &Pixmap) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"));
    // Через PNG и обратно: так эталон и результат одинаково теряют точность
    // полупрозрачных пикселей при снятии премультипликации.
    let png = actual.clone().into_png().expect("кодирование PNG");
    if std::env::var("FERRUM_UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().expect("каталог эталонов"))
            .expect("каталог эталонов");
        std::fs::write(&path, &png).expect("запись эталона");
        return;
    }
    let actual = Pixmap::from_png(std::io::Cursor::new(&png)).expect("чтение PNG");
    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "нет эталона {}: создайте его командой FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test golden",
            path.display()
        )
    });
    let expected = Pixmap::from_png(std::io::Cursor::new(bytes)).expect("эталон — PNG");
    assert_eq!(
        (expected.width(), expected.height()),
        (actual.width(), actual.height()),
        "размер {name}"
    );
    let mut worst = 0u8;
    let mut differing = 0usize;
    for (e, a) in expected.data().iter().zip(actual.data()) {
        let d = [
            e.r.abs_diff(a.r),
            e.g.abs_diff(a.g),
            e.b.abs_diff(a.b),
            e.a.abs_diff(a.a),
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        worst = worst.max(d);
        if d > GOLDEN_TOLERANCE {
            differing += 1;
        }
    }
    if differing > 0 {
        let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.actual.png"));
        std::fs::write(&out, &png).expect("запись результата");
        panic!(
            "{name}: {differing} пикселей отличаются от эталона больше чем на {GOLDEN_TOLERANCE} (максимум {worst}); результат: {}",
            out.display()
        );
    }
}
````

- [ ] **Step 2: Написать падающие тесты сборки кадра**

`crates/ferrum-render/tests/render.rs`:

````rust
mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch};
use ferrum_render::{
    decode_png, Camera, CpuBackend, ExportError, ImagePaint, NoImages, Pixmap, RenderBackend,
    Renderer, Rgba, LOD_RECT_PX, RASTER_BUILDS_PER_FRAME,
};
use ferrum_scene::Scene;
use kurbo::{Affine, BezPath, Point, Rect, Size, Stroke};
use vello_cpu::peniko::Fill as FillRule;

fn solid(r: u8, g: u8, b: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, 255))])
}

/// Сетка фреймов `cols × rows` по `kids` прямоугольников — уменьшенная копия
/// страницы из бенчмарка.
fn grid(cols: usize, rows: usize, kids: usize) -> (Page, Vec<ferrum_doc::NodeId>) {
    let mut p = Page::new();
    let mut frames = Vec::new();
    for i in 0..cols * rows {
        let f = p.add(
            None,
            NodeKind::Frame,
            PropsPatch {
                fills: solid(240, 240, 240),
                ..rect(
                    (i % cols) as f64 * 220.0,
                    (i / cols) as f64 * 220.0,
                    200.0,
                    200.0,
                )
            },
        );
        for k in 0..kids {
            p.add(
                Some(f),
                NodeKind::Rect,
                PropsPatch {
                    fills: solid((k * 40 % 256) as u8, 100, 200),
                    ..rect((k % 5) as f64 * 40.0, (k / 5) as f64 * 40.0, 30.0, 30.0)
                },
            );
        }
        frames.push(f);
    }
    (p, frames)
}

fn draw(renderer: &mut Renderer, scene: &Scene, camera: &Camera) -> ferrum_render::FrameStats {
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    renderer.draw_scene(&mut backend, scene, camera, &NoImages)
}

#[test]
fn display_lists_rebuild_only_for_changed_nodes() {
    let (mut p, frames) = grid(2, 1, 4);
    let mut scene = p.scene();
    let cam = camera(440.0, 220.0);
    let mut r = Renderer::new();
    let first = draw(&mut r, &scene, &cam);
    assert_eq!(first.drawn, 10);
    assert_eq!(first.lists_built, 10);
    // Камера сдвинулась — списки те же.
    let mut moved = cam.clone();
    moved.pan_by(kurbo::Vec2::new(5.0, 0.0));
    assert_eq!(draw(&mut r, &scene, &moved).lists_built, 0);
    // Перемещение узла тоже не трогает его список: он в локальных координатах.
    p.edit(frames[0], rect(3.0, 0.0, 200.0, 200.0));
    p.update(&mut scene);
    assert_eq!(draw(&mut r, &scene, &cam).lists_built, 0);
    // Смена цвета — пересобирается только этот узел.
    p.edit(
        frames[1],
        PropsPatch {
            fills: solid(1, 2, 3),
            ..Default::default()
        },
    );
    p.update(&mut scene);
    assert_eq!(draw(&mut r, &scene, &cam).lists_built, 1);
}

#[test]
fn offscreen_subtrees_are_culled_whole() {
    let (mut p, _) = grid(10, 10, 10);
    let scene = p.scene();
    let mut r = Renderer::new();
    // Вьюпорт на один фрейм: обход не заходит в остальные 99 поддеревьев.
    let stats = draw(&mut r, &scene, &camera(200.0, 200.0));
    assert_eq!(stats.drawn, 11);
    assert_eq!(stats.culled, 99);
    assert_eq!(stats.visited, 100 + 10);
}

#[test]
fn tiny_nodes_become_rects_and_specks_disappear() {
    let (mut p, _) = grid(2, 1, 10);
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(400.0, 200.0), 1.0);
    cam.center = Point::new(210.0, 100.0);
    // Дети 30 px: при зуме 0.08 — 2.4 px, прямоугольники LOD.
    cam.set_zoom(0.08);
    assert!(30.0 * cam.zoom() < LOD_RECT_PX);
    let mut r = Renderer::new();
    let mut backend = CpuBackend::new(400, 200);
    // Бэкенд без изображений: проверяем LOD узлов, а не растровый кэш.
    let stats = r.draw_scene(&mut NoRaster(&mut backend), &scene, &cam, &NoImages);
    assert_eq!(stats.lod_rects, 20);
    assert_eq!(stats.drawn, 2);
    // Узел 20 px при минимальном зуме 2% — 0.4 px: не рисуется вовсе.
    let mut tiny = Page::new();
    tiny.add(None, NodeKind::Rect, rect(0.0, 0.0, 20.0, 20.0));
    cam.set_zoom(0.0);
    cam.center = Point::ZERO;
    let stats = r.draw_scene(&mut NoRaster(&mut backend), &tiny.scene(), &cam, &NoImages);
    assert_eq!((stats.drawn, stats.lod_rects, stats.too_small), (0, 0, 1));
}

/// Обёртка, которая выдаёт бэкенд за не умеющий изображения.
struct NoRaster<'a>(&'a mut CpuBackend);

impl RenderBackend for NoRaster<'_> {
    fn size(&self) -> (u16, u16) {
        self.0.size()
    }
    fn set_transform(&mut self, t: Affine) {
        self.0.set_transform(t)
    }
    fn set_color(&mut self, c: Rgba) {
        self.0.set_color(c)
    }
    fn set_image(&mut self, i: &ImagePaint) {
        self.0.set_image(i)
    }
    fn fill_path(&mut self, p: &BezPath, r: FillRule) {
        self.0.fill_path(p, r)
    }
    fn fill_rect(&mut self, r: &Rect) {
        self.0.fill_rect(r)
    }
    fn stroke_path(&mut self, p: &BezPath, s: &Stroke) {
        self.0.stroke_path(p, s)
    }
    fn push_clip(&mut self, p: &BezPath) {
        self.0.push_clip(p)
    }
    fn pop_clip(&mut self) {
        self.0.pop_clip()
    }
    fn push_opacity_layer(&mut self, a: f32) {
        self.0.push_opacity_layer(a)
    }
    fn pop_layer(&mut self) {
        self.0.pop_layer()
    }
    fn supports_images(&self) -> bool {
        false
    }
}

#[test]
fn raster_cache_matches_live_drawing_and_follows_changes() {
    let (mut p, frames) = grid(8, 8, 10);
    let mut scene = p.scene();
    let mut cam = Camera::new(Size::new(400.0, 400.0), 1.0);
    cam.fit(scene.bounds().unwrap(), 10.0);
    let (w, h) = cam.device_size();
    let mut r = Renderer::new();

    let mut live = CpuBackend::new(w, h);
    r.draw_scene(&mut NoRaster(&mut live), &scene, &cam, &NoImages);
    let live = live.render();

    // Кэш строится не больше `RASTER_BUILDS_PER_FRAME` фреймов за кадр.
    let mut frames_to_warm = 0;
    let cached = loop {
        let mut b = CpuBackend::new(w, h);
        let stats = r.draw_scene(&mut b, &scene, &cam, &NoImages);
        assert!(stats.raster_builds <= RASTER_BUILDS_PER_FRAME);
        if stats.raster_builds == 0 {
            assert_eq!(stats.rasters, 64);
            break b.render();
        }
        frames_to_warm += 1;
    };
    assert_eq!(frames_to_warm, 2);
    // Растр с плотностью до 2× от нужной почти не отличается от рисования узлами.
    let diff: u64 = live
        .data()
        .iter()
        .zip(cached.data())
        .map(|(a, b)| {
            a.r.abs_diff(b.r)
                .max(a.g.abs_diff(b.g))
                .max(a.b.abs_diff(b.b)) as u64
        })
        .sum();
    let mean = diff as f64 / (w as f64 * h as f64);
    assert!(mean < 4.0, "среднее отличие {mean}");

    // Правка ребёнка перестраивает растр только его фрейма.
    let kid = scene.at(scene.slot(frames[5]).unwrap()).children[0];
    let kid = scene.at(kid).id;
    p.edit(
        kid,
        PropsPatch {
            fills: solid(0, 0, 0),
            ..Default::default()
        },
    );
    p.update(&mut scene);
    let stats = draw(&mut r, &scene, &cam);
    assert_eq!(stats.raster_builds, 1);
    // Немного приблизили — растры годятся (плотность с запасом до 2×).
    cam.set_zoom(cam.zoom() * 1.1);
    assert_eq!(draw(&mut r, &scene, &cam).raster_builds, 0);
    // Отдалили вдвое — растры перестраиваются в меньшей плотности.
    cam.set_zoom(cam.zoom() * 0.45);
    assert_eq!(draw(&mut r, &scene, &cam).raster_builds, 32);
}

#[test]
fn export_is_pixel_exact_at_each_scale() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(10, 20, 30),
            ..rect(100.0, 50.0, 393.0, 852.0)
        },
    );
    p.add(Some(f), NodeKind::Rect, rect(-50.0, 0.0, 100.0, 100.0));
    let scene = p.scene();
    let mut r = Renderer::new();
    for (scale, w, h) in [(1.0, 393, 852), (2.0, 786, 1704), (3.0, 1179, 2556)] {
        let png = r.export_png(&scene, f, scale, &NoImages).unwrap();
        // Размер из заголовка PNG.
        let pw = u32::from_be_bytes(png[16..20].try_into().unwrap());
        let ph = u32::from_be_bytes(png[20..24].try_into().unwrap());
        assert_eq!((pw, ph), (w, h), "@{scale}x");
    }
    let img = r.export_pixmap(&scene, f, 1.0, &NoImages).unwrap();
    // Ребёнок, выступающий за левый край, обрезан краем картинки; фон фрейма — в углу.
    assert_eq!(px(&img, 392, 851), [10, 20, 30, 255]);
    assert_eq!(px(&img, 10, 10), [217, 217, 217, 255]);
}

#[test]
fn export_errors_are_reported() {
    let mut p = Page::new();
    let big = p.add(None, NodeKind::Frame, rect(0.0, 0.0, 10_000.0, 10.0));
    let empty = p.add(None, NodeKind::Group, PropsPatch::default());
    let scene = p.scene();
    let mut r = Renderer::new();
    assert_eq!(
        r.export_png(&scene, big, 2.0, &NoImages),
        Err(ExportError::TooLarge(20_000, 20))
    );
    assert_eq!(
        r.export_png(&scene, big, 0.0, &NoImages),
        Err(ExportError::BadScale)
    );
    assert_eq!(
        r.export_png(&scene, empty, 1.0, &NoImages),
        Err(ExportError::Empty)
    );
    let other = Page::new().add(None, NodeKind::Rect, PropsPatch::default());
    assert_eq!(
        r.export_png(&scene, other, 1.0, &NoImages),
        Err(ExportError::NotFound)
    );
}

#[test]
fn export_ignores_ancestor_opacity() {
    let mut p = Page::new();
    let g = p.add(
        None,
        NodeKind::Group,
        PropsPatch {
            opacity: Some(0.2),
            ..Default::default()
        },
    );
    let leaf = p.add(
        Some(g),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let img = Renderer::new()
        .export_pixmap(&p.scene(), leaf, 1.0, &NoImages)
        .unwrap();
    assert_eq!(px(&img, 5, 5), [255, 0, 0, 255]);
}

#[test]
fn missing_image_draws_placeholder() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Image,
        PropsPatch {
            fills: Some(vec![Fill::image("нет-такого")]),
            ..rect(0.0, 0.0, 20.0, 20.0)
        },
    );
    let img = render(&p.scene(), &camera(20.0, 20.0));
    assert_px(&img, 10, 10, [217, 217, 217], 1);
    // Та же картинка, если поставщик её знает, рисуется ею.
    let mut red = Pixmap::new(1, 1);
    red.set_pixel(
        0,
        0,
        vello_cpu::color::PremulRgba8 {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
    );
    let images: HashMap<String, Arc<Pixmap>> = [("нет-такого".to_string(), Arc::new(red))].into();
    let img = render_with(
        &mut Renderer::new(),
        &p.scene(),
        &camera(20.0, 20.0),
        &images,
    );
    assert_px(&img, 10, 10, [255, 0, 0], 1);
}

#[test]
fn decode_png_rejects_garbage_and_huge_headers() {
    let mut img = Pixmap::new(3, 2);
    img.set_pixel(
        1,
        1,
        vello_cpu::color::PremulRgba8 {
            r: 9,
            g: 8,
            b: 7,
            a: 255,
        },
    );
    let png = img.into_png().unwrap();
    let back = decode_png(&png).unwrap();
    assert_eq!((back.width(), back.height()), (3, 2));
    assert!(decode_png(b"not a png").is_none());
    let mut huge = png.clone();
    huge[16..20].copy_from_slice(&60_000u32.to_be_bytes());
    assert!(
        decode_png(&huge).is_none(),
        "размер из заголовка проверяется до декодирования"
    );
}
````

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-render --test render`
Expected: ошибка компиляции — нет `Renderer`.

- [ ] **Step 4: Реализовать список отрисовки**

`crates/ferrum-render/src/display.rs`:

````rust
//! Список отрисовки узла: что рисовать в его локальных координатах. Строится из
//! свойств один раз и живёт, пока не изменилась версия узла в сцене; перемещение
//! узла или камеры его не трогает — меняется только матрица при воспроизведении.
//!
//! Контуры (`BezPath`) строятся при первом полном рисовании узла: мелкому узлу
//! (LOD) они не нужны, а скруглённый прямоугольник с кольцом обводки — около
//! 1.7 КБ, на 100 000 узлов это 170 МБ. Поэтому контуры давно не рисовавшихся
//! узлов выбрасываются (`Painter::evict`), а описание остаётся.

use ferrum_doc::{Color, FillKind, NodeKind, Props, ScaleMode, Stroke, StrokeAlign};
use ferrum_scene::shape::{self, Geometry};
use ferrum_scene::SceneNode;
use kurbo::BezPath;

use crate::backend::Rgba;

/// Цвет изображения для LOD и заглушки, пока картинка не загружена.
pub const IMAGE_PLACEHOLDER: Rgba = Rgba::new([0.85, 0.85, 0.85, 1.0]);

#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Solid(Rgba),
    Image {
        hash: String,
        mode: ScaleMode,
        alpha: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// Заливка формы узла.
    Fill(Paint),
    /// Обводка — кольцо, заливаемое по even-odd.
    Stroke(Stroke, Rgba),
    /// Самый частый случай — непрозрачная заливка и внутренняя непрозрачная
    /// обводка: форма цветом обводки, поверх — форма, сжатая на ширину обводки,
    /// цветом заливки. Два контура вместо трёх (заливка + кольцо), результат тот же.
    FilledInsideStroke {
        fill: Rgba,
        stroke: Rgba,
        width: f64,
    },
}

/// Контуры в локальных координатах: форма и по одному на элемент списка.
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    pub shape: BezPath,
    /// Кольцо обводки или сжатая форма; `None` — элементу хватает `shape`.
    pub items: Vec<Option<BezPath>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayList {
    pub version: u64,
    pub geometry: Geometry,
    /// Снизу вверх: заливки, затем обводки (как в Figma).
    pub items: Vec<Item>,
    /// Цвет, которым мелкий узел рисуется одним прямоугольником.
    pub lod_color: Option<Rgba>,
    pub paths: Option<Paths>,
    /// Номер кадра, в котором узел последний раз рисовался полностью.
    pub last_used: u64,
}

pub fn color(c: Color) -> Rgba {
    Rgba::from_rgba8(c.r, c.g, c.b, c.a)
}

/// Цвет узла для LOD-прямоугольника без построения списка: первая видимая заливка,
/// иначе первая обводка.
pub fn lod_color(p: &Props) -> Option<Rgba> {
    if !matches!(p.kind, NodeKind::Line | NodeKind::Group) {
        if let Some(f) = p.fills.iter().find(|f| f.visible && f.opacity > 0.0) {
            return Some(match f.kind {
                FillKind::Solid => color(f.color).multiply_alpha(f.opacity as f32),
                FillKind::Image => IMAGE_PLACEHOLDER.multiply_alpha(f.opacity as f32),
            });
        }
    }
    if p.kind == NodeKind::Group {
        return None;
    }
    p.strokes
        .iter()
        .find(|s| s.width > 0.0 && s.color.a > 0)
        .map(|s| color(s.color))
}

impl DisplayList {
    pub fn build(node: &SceneNode) -> DisplayList {
        let p: &Props = &node.props;
        let geometry = shape::geometry(p);
        let mut items = Vec::new();
        // У линии и группы своей заливки нет.
        let fillable = !matches!(p.kind, NodeKind::Line | NodeKind::Group);
        if fillable {
            for f in p.fills.iter().filter(|f| f.visible && f.opacity > 0.0) {
                let paint = match (f.kind, &f.image_hash) {
                    (FillKind::Solid, _) => {
                        Paint::Solid(color(f.color).multiply_alpha(f.opacity as f32))
                    }
                    (FillKind::Image, Some(hash)) => Paint::Image {
                        hash: hash.clone(),
                        mode: f.scale_mode,
                        alpha: f.opacity as f32,
                    },
                    (FillKind::Image, None) => continue,
                };
                items.push(Item::Fill(paint));
            }
        }
        if p.kind != NodeKind::Group {
            for s in p.strokes.iter().filter(|s| s.width > 0.0 && s.color.a > 0) {
                items.push(Item::Stroke(s.clone(), color(s.color)));
            }
        }
        if let [Item::Fill(Paint::Solid(fill)), Item::Stroke(s, stroke)] = &items[..] {
            if s.align == StrokeAlign::Inside && fill.components[3] >= 1.0 && s.color.a == 255 {
                items = vec![Item::FilledInsideStroke {
                    fill: *fill,
                    stroke: *stroke,
                    width: s.width,
                }];
            }
        }
        DisplayList {
            version: node.version,
            geometry,
            items,
            lod_color: lod_color(p),
            paths: None,
            last_used: 0,
        }
    }

    /// Контуры, построенные при первом обращении.
    pub fn paths(&mut self) -> &Paths {
        let geom = self.geometry;
        let items = &self.items;
        self.paths.get_or_insert_with(|| {
            let mut shape = BezPath::new();
            geom.fill_path_into(&mut shape);
            let items = items
                .iter()
                .map(|item| match item {
                    Item::Fill(_) => None,
                    Item::Stroke(s, _) => geom.stroke_ring(s),
                    Item::FilledInsideStroke { width, .. } => geom.inset_path(*width),
                })
                .collect();
            Paths { shape, items }
        })
    }
}
````

- [ ] **Step 5: Реализовать обход сцены**

`crates/ferrum-render/src/draw.rs`:

````rust
//! Сборка кадра: обход сцены с отсечением невидимого и LOD, растровый кэш
//! мелких фреймов верхнего уровня, воспроизведение списков отрисовки в бэкенд.

use std::collections::HashMap;
use std::sync::Arc;

use ferrum_doc::{NodeId, NodeKind, ScaleMode};
use ferrum_scene::shape::Geometry;
use ferrum_scene::{Scene, SceneNode, Slot};
use kurbo::{Affine, BezPath, Point, Rect};
use vello_cpu::peniko::{Extend, Fill};
use vello_cpu::Pixmap;

use crate::backend::{CpuBackend, ImagePaint, RenderBackend};
use crate::display::{self, DisplayList, Item, Paint, IMAGE_PLACEHOLDER};
use crate::image::ImageProvider;

/// Поддерево меньше полупикселя не рисуется вовсе.
pub const LOD_SKIP_PX: f64 = 0.5;
/// Узел меньше трёх пикселей рисуется одним прямоугольником цвета первой заливки:
/// скругления и обводки на таком размере не различимы, а разбор пути дороже.
pub const LOD_RECT_PX: f64 = 3.0;
/// Контейнер верхнего уровня не больше этого размера на экране рисуется из
/// растрового кэша: при сильном отдалении на странице тысячи фреймов по сотне
/// мелких узлов, и даже прямоугольники LOD не укладываются в кадр.
pub const RASTER_MAX_PX: f64 = 256.0;
/// Растровый кэш — только при сильном отдалении (пикселей устройства на единицу
/// мира): вблизи узлы рисуются дёшево, а любая правка перестраивала бы растр.
pub const RASTER_MAX_SCALE: f64 = 0.25;
/// Сколько растровых кэшей строить за кадр; остальные фреймы этого кадра рисуются
/// узлами, а кэш догоняет в следующих кадрах.
pub const RASTER_BUILDS_PER_FRAME: usize = 32;
/// Через сколько кадров без отрисовки выбрасываются контуры узла и растр фрейма.
pub const EVICT_AFTER_FRAMES: u64 = 300;

/// Что произошло за кадр — для тестов, бенчмарка и статистики в интерфейсе.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FrameStats {
    /// Узлы, до которых дошёл обход.
    pub visited: usize,
    /// Поддеревья, отброшенные как невидимые во вьюпорте.
    pub culled: usize,
    /// Поддеревья меньше `LOD_SKIP_PX`.
    pub too_small: usize,
    /// Узлы, нарисованные полностью.
    pub drawn: usize,
    /// Узлы, нарисованные прямоугольником LOD.
    pub lod_rects: usize,
    /// Списки отрисовки, построенные заново (новые или изменённые узлы).
    pub lists_built: usize,
    /// Контейнеры, нарисованные из растрового кэша, и построенные за кадр кэши.
    pub rasters: usize,
    pub raster_builds: usize,
    pub clips: usize,
    pub layers: usize,
}

/// Параметры обхода.
pub struct DrawContext<'a> {
    pub scene: &'a Scene,
    pub images: &'a dyn ImageProvider,
    /// Мир → пиксели устройства.
    pub transform: Affine,
    /// Пикселей устройства на единицу мира.
    pub scale: f64,
    /// Видимая часть мира; `None` — рисовать всё (экспорт).
    pub cull: Option<Rect>,
    pub lod: bool,
    /// Можно ли рисовать контейнеры из растрового кэша (бэкенд умеет изображения).
    pub raster: bool,
    /// Сохранять ли построенные списки отрисовки и контуры. При построении растра
    /// и экспорте узлы рисуются один раз: кэшировать их контуры значит держать
    /// в памяти всю страницу (сотни мегабайт), которую на экране никто не видит.
    pub retain: bool,
}

enum Visit {
    Enter(Slot),
    Exit { clip: bool, layer: bool },
}

/// Растр поддерева контейнера в мировых координатах.
struct Raster {
    id: NodeId,
    subtree_version: u64,
    /// Пикселей растра на единицу мира — степень двойки.
    scale: f64,
    origin: Point,
    pixmap: Arc<Pixmap>,
    last_used: u64,
}

/// Кэши между кадрами: списки отрисовки по слотам сцены, растры фреймов, общий
/// буфер для контуров обрезки.
#[derive(Default)]
pub struct Painter {
    lists: Vec<Option<DisplayList>>,
    rasters: HashMap<Slot, Raster>,
    path: BezPath,
    frame: u64,
    raster_budget: usize,
    /// Один бэкенд на все построения растров: создание `RenderContext` дороже
    /// самой отрисовки мелкого фрейма.
    raster_backend: Option<CpuBackend>,
    pub stats: FrameStats,
}

impl Painter {
    /// Начало кадра: сброс статистики, бюджет кэша, вытеснение старого.
    pub fn begin_frame(&mut self) {
        self.stats = FrameStats::default();
        self.frame += 1;
        self.raster_budget = RASTER_BUILDS_PER_FRAME;
        if self.frame.is_multiple_of(64) {
            self.evict();
        }
    }

    /// Выбрасывает контуры узлов и растры, не рисовавшиеся `EVICT_AFTER_FRAMES` кадров.
    fn evict(&mut self) {
        let frame = self.frame;
        for list in self.lists.iter_mut().flatten() {
            if list.paths.is_some() && list.last_used + EVICT_AFTER_FRAMES < frame {
                list.paths = None;
            }
        }
        self.rasters
            .retain(|_, r| r.last_used + EVICT_AFTER_FRAMES >= frame);
    }

    /// Рисует поддеревья `roots` снизу вверх. Обход без рекурсии: глубина дерева из
    /// файла не ограничена.
    pub fn draw(&mut self, backend: &mut impl RenderBackend, ctx: &DrawContext, roots: &[Slot]) {
        let scene = ctx.scene;
        let mut stack: Vec<Visit> = roots.iter().rev().map(|&s| Visit::Enter(s)).collect();
        while let Some(visit) = stack.pop() {
            let slot = match visit {
                Visit::Enter(slot) => slot,
                Visit::Exit { clip, layer } => {
                    if clip {
                        backend.pop_clip();
                    }
                    if layer {
                        backend.pop_layer();
                    }
                    continue;
                }
            };
            self.stats.visited += 1;
            let n = scene.at(slot);
            // Скрытый узел: границ поддерева нет.
            let Some(sub) = n.subtree_bounds else {
                continue;
            };
            if ctx.cull.is_some_and(|c| !overlaps(sub, c)) {
                self.stats.culled += 1;
                continue;
            }
            let px = max_side(sub) * ctx.scale;
            if ctx.lod && px < LOD_SKIP_PX {
                self.stats.too_small += 1;
                continue;
            }
            let has_children = !n.children.is_empty();
            if ctx.raster
                && ctx.scale <= RASTER_MAX_SCALE
                && n.parent.is_none()
                && has_children
                && px <= RASTER_MAX_PX
                && self.draw_raster(backend, ctx, slot, n, sub)
            {
                continue;
            }
            let opacity = n.props.opacity as f32;
            if opacity <= 0.0 {
                continue;
            }
            // Прозрачность контейнера — слоем: перекрытия детей не просвечивают друг
            // через друга. У листа прозрачность умножается в краску.
            let layer = has_children && opacity < 1.0;
            if layer {
                backend.push_opacity_layer(opacity);
                self.stats.layers += 1;
            }
            self.draw_own(backend, ctx, slot, n, if layer { 1.0 } else { opacity });
            if has_children {
                let clip =
                    n.props.kind == NodeKind::Frame && n.props.clip_content && needs_clip(scene, n);
                if clip {
                    backend.set_transform(ctx.transform * n.world);
                    self.path.truncate(0);
                    n.geometry().fill_path_into(&mut self.path);
                    backend.push_clip(&self.path);
                    self.stats.clips += 1;
                }
                stack.push(Visit::Exit { clip, layer });
                stack.extend(n.children.iter().rev().map(|&c| Visit::Enter(c)));
            }
        }
    }

    fn draw_own(
        &mut self,
        backend: &mut impl RenderBackend,
        ctx: &DrawContext,
        slot: Slot,
        n: &SceneNode,
        alpha: f32,
    ) {
        if n.props.kind == NodeKind::Group {
            return;
        }
        let Some(bounds) = n.world_bounds else { return };
        if ctx.cull.is_some_and(|c| !overlaps(bounds, c)) {
            return;
        }
        let px = max_side(bounds) * ctx.scale;
        if ctx.lod && px < LOD_SKIP_PX {
            return;
        }
        if ctx.lod && px < LOD_RECT_PX {
            // Мелкому узлу список отрисовки не нужен — только цвет.
            if let Some(c) = display::lod_color(&n.props) {
                backend.set_transform(ctx.transform);
                backend.set_color(c.multiply_alpha(alpha));
                backend.fill_rect(&bounds);
                self.stats.lod_rects += 1;
            }
            return;
        }
        let i = slot as usize;
        let cached = self
            .lists
            .get(i)
            .and_then(Option::as_ref)
            .is_some_and(|l| l.version == n.version && (ctx.retain || l.paths.is_some()));
        backend.set_transform(ctx.transform * n.world);
        self.stats.drawn += 1;
        if !cached {
            self.stats.lists_built += 1;
            let mut list = DisplayList::build(n);
            list.paths();
            replay(backend, &list, alpha, ctx.images);
            if ctx.retain {
                list.last_used = self.frame;
                if self.lists.len() <= i {
                    self.lists.resize(i + 1, None);
                }
                self.lists[i] = Some(list);
            }
            return;
        }
        let list = self.lists[i].as_mut().expect("проверено выше");
        list.last_used = self.frame;
        list.paths();
        replay(backend, list, alpha, ctx.images);
    }

    /// Рисует контейнер из растрового кэша, при необходимости строя кэш. `false` —
    /// кэша нет и бюджет кадра исчерпан (или растр был бы слишком большим): тогда
    /// контейнер рисуется узлами.
    ///
    /// Растр строится с плотностью — ближайшей степенью двойки не меньше текущего
    /// масштаба, и годится, пока масштаб не уменьшится вдвое: билинейная выборка с
    /// уменьшением до 2× не даёт заметной ряби, а плавный зум не перестраивает кэш
    /// на каждом кадре.
    fn draw_raster(
        &mut self,
        backend: &mut impl RenderBackend,
        ctx: &DrawContext,
        slot: Slot,
        n: &SceneNode,
        area: Rect,
    ) -> bool {
        let fresh = self.rasters.get(&slot).is_some_and(|r| {
            r.id == n.id
                && r.subtree_version == n.subtree_version
                && r.scale >= ctx.scale
                && r.scale < 2.0 * ctx.scale
        });
        if !fresh {
            if self.raster_budget == 0 {
                return false;
            }
            let scale = 2f64.powf(ctx.scale.log2().ceil());
            let (w, h) = (
                (area.width() * scale).ceil(),
                (area.height() * scale).ceil(),
            );
            if w > 2.0 * RASTER_MAX_PX || h > 2.0 * RASTER_MAX_PX {
                return false;
            }
            self.raster_budget -= 1;
            let (w, h) = (w.max(1.0) as u16, h.max(1.0) as u16);
            let mut cpu = match self.raster_backend.take() {
                Some(mut b) => {
                    b.reset(w, h);
                    b
                }
                None => CpuBackend::new(w, h),
            };
            let inner = DrawContext {
                scene: ctx.scene,
                images: ctx.images,
                transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
                scale,
                cull: None,
                lod: true,
                raster: false,
                retain: false,
            };
            let outer = self.stats;
            self.draw(&mut cpu, &inner, &[slot]);
            self.stats = FrameStats {
                raster_builds: outer.raster_builds + 1,
                lists_built: self.stats.lists_built,
                ..outer
            };
            self.rasters.insert(
                slot,
                Raster {
                    id: n.id,
                    subtree_version: n.subtree_version,
                    scale,
                    origin: area.origin(),
                    pixmap: Arc::new(cpu.render()),
                    last_used: 0,
                },
            );
            self.raster_backend = Some(cpu);
        }
        let r = self.rasters.get_mut(&slot).expect("есть или построен выше");
        r.last_used = self.frame;
        backend.set_transform(ctx.transform);
        backend.set_image(&ImagePaint {
            pixmap: r.pixmap.clone(),
            transform: Affine::translate(r.origin.to_vec2()) * Affine::scale(1.0 / r.scale),
            extend: Extend::Pad,
            alpha: 1.0,
        });
        backend.fill_rect(&Rect::from_origin_size(
            r.origin,
            (
                r.pixmap.width() as f64 / r.scale,
                r.pixmap.height() as f64 / r.scale,
            ),
        ));
        self.stats.rasters += 1;
        true
    }
}

/// Воспроизводит список отрисовки в локальных координатах узла.
/// Контуры списка уже построены (`DisplayList::paths`).
fn replay(
    backend: &mut impl RenderBackend,
    list: &DisplayList,
    alpha: f32,
    images: &dyn ImageProvider,
) {
    let geom = list.geometry;
    let paths = list
        .paths
        .as_ref()
        .expect("контуры построены до воспроизведения");
    for (item, item_path) in list.items.iter().zip(&paths.items) {
        match item {
            Item::Fill(Paint::Solid(c)) => {
                backend.set_color(c.multiply_alpha(alpha));
                fill_shape(backend, geom, &paths.shape);
            }
            Item::Fill(Paint::Image {
                hash,
                mode,
                alpha: a,
            }) => {
                let Some(area) = geom.bounds() else { continue };
                let Some(pixmap) = images.image(hash) else {
                    backend.set_color(IMAGE_PLACEHOLDER.multiply_alpha(a * alpha));
                    fill_shape(backend, geom, &paths.shape);
                    continue;
                };
                let (iw, ih) = (pixmap.width() as f64, pixmap.height() as f64);
                let (transform, extend, fit) = place_image(*mode, area, iw, ih);
                backend.set_image(&ImagePaint {
                    pixmap,
                    transform,
                    extend,
                    alpha: a * alpha,
                });
                match fit {
                    // «Вписать»: картинка целиком, поля прозрачные, форма обрезает углы.
                    Some(rect) => {
                        backend.push_clip(&paths.shape);
                        backend.fill_rect(&rect);
                        backend.pop_clip();
                    }
                    None => fill_shape(backend, geom, &paths.shape),
                }
            }
            Item::Stroke(_, c) => {
                if let Some(ring) = item_path {
                    backend.set_color(c.multiply_alpha(alpha));
                    backend.fill_path(ring, Fill::EvenOdd);
                }
            }
            Item::FilledInsideStroke {
                fill,
                stroke,
                width,
            } => {
                backend.set_color(stroke.multiply_alpha(alpha));
                fill_shape(backend, geom, &paths.shape);
                backend.set_color(fill.multiply_alpha(alpha));
                match geom {
                    Geometry::Rect { rect, radius } if radius <= 0.0 => {
                        let inner = rect.inset(-*width);
                        if inner.width() > 0.0 && inner.height() > 0.0 {
                            backend.fill_rect(&inner);
                        }
                    }
                    _ => {
                        if let Some(inner) = item_path {
                            backend.fill_path(inner, Fill::NonZero);
                        }
                    }
                }
            }
        }
    }
}

/// Заливка формы; прямоугольник без скругления — быстрым путём `fill_rect`.
fn fill_shape(backend: &mut impl RenderBackend, geom: Geometry, shape: &BezPath) {
    match geom {
        Geometry::Rect { rect, radius } if radius <= 0.0 => backend.fill_rect(&rect),
        _ if !shape.elements().is_empty() => backend.fill_path(shape, Fill::NonZero),
        _ => {}
    }
}

/// Размещение изображения `iw × ih` в области узла: матрица «пиксели картинки →
/// локальные координаты», режим краёв и, для «вписать», прямоугольник картинки.
pub fn place_image(
    mode: ScaleMode,
    area: Rect,
    iw: f64,
    ih: f64,
) -> (Affine, Extend, Option<Rect>) {
    let (aw, ah) = (area.width(), area.height());
    match mode {
        ScaleMode::Fill | ScaleMode::Fit => {
            let s = if mode == ScaleMode::Fill {
                (aw / iw).max(ah / ih)
            } else {
                (aw / iw).min(ah / ih)
            };
            let x = area.x0 + (aw - iw * s) / 2.0;
            let y = area.y0 + (ah - ih * s) / 2.0;
            let fit = (mode == ScaleMode::Fit).then(|| Rect::new(x, y, x + iw * s, y + ih * s));
            (
                Affine::translate((x, y)) * Affine::scale(s),
                Extend::Pad,
                fit,
            )
        }
        // Плитка в натуральном размере от левого верхнего угла.
        ScaleMode::Tile => (Affine::translate((area.x0, area.y0)), Extend::Repeat, None),
    }
}

/// Нужна ли фрейму обрезка детей. Обрезка не бесплатна, а в типичном макете дети
/// лежат внутри фрейма: если фрейм не повёрнут, без скругления и границы всех детей
/// внутри него, обрезать нечего.
fn needs_clip(scene: &Scene, n: &SceneNode) -> bool {
    let Geometry::Rect { rect, radius } = n.geometry() else {
        return true;
    };
    let c = n.world.as_coeffs();
    if radius > 0.0 || c[1] != 0.0 || c[2] != 0.0 {
        return true;
    }
    let frame = n.world.transform_rect_bbox(rect);
    n.children
        .iter()
        .filter_map(|&c| scene.at(c).subtree_bounds)
        .any(|b| b.x0 < frame.x0 || b.y0 < frame.y0 || b.x1 > frame.x1 || b.y1 > frame.y1)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && a.x1 >= b.x0 && a.y0 <= b.y1 && a.y1 >= b.y0
}

fn max_side(r: Rect) -> f64 {
    r.width().max(r.height())
}
````

- [ ] **Step 6: `Renderer` и экспорт**

Экспорт рисует только поддерево узла: прозрачность и обрезка предков не применяются. Область фрейма — его форма, как в Figma, поэтому 393×852 @3x даёт ровно 1179×2556.

`crates/ferrum-render/src/lib.rs`:

````rust
//! Рендер Ferrum: камера, сборка кадра со списками отрисовки, отсечением и LOD,
//! оверлеи, экспорт PNG. Получает сцену, камеру и описание оверлеев; о Loro и
//! инструментах не знает.

mod backend;
mod camera;
mod display;
mod draw;
mod image;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::Scene;
use kurbo::{Affine, Rect};

pub use backend::{CpuBackend, ImagePaint, RenderBackend, Rgba};
pub use camera::{Camera, MAX_ZOOM, MIN_ZOOM};
pub use draw::{
    FrameStats, EVICT_AFTER_FRAMES, LOD_RECT_PX, LOD_SKIP_PX, RASTER_BUILDS_PER_FRAME,
    RASTER_MAX_PX, RASTER_MAX_SCALE,
};
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use vello_cpu::Pixmap;

use draw::{DrawContext, Painter};

/// Наибольшая сторона экспортируемой картинки в пикселях.
pub const MAX_EXPORT_SIDE: u32 = 16384;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExportError {
    #[error("Узел для экспорта не найден")]
    NotFound,
    #[error("Масштаб экспорта должен быть положительным")]
    BadScale,
    #[error("Нечего экспортировать: у узла нулевой размер или он скрыт")]
    Empty,
    #[error(
        "Картинка {0}×{1} слишком большая: сторона не больше 16384 пикселей. Уменьшите масштаб"
    )]
    TooLarge(u32, u32),
    #[error("Не удалось закодировать PNG: {0}")]
    Encode(String),
}

/// Рендерер одного холста: кэши списков отрисовки и растров живут между кадрами.
#[derive(Default)]
pub struct Renderer {
    painter: Painter,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer::default()
    }

    /// Только содержимое сцены, без фона и оверлеев.
    pub fn draw_scene(
        &mut self,
        backend: &mut impl RenderBackend,
        scene: &Scene,
        camera: &Camera,
        images: &dyn ImageProvider,
    ) -> FrameStats {
        self.painter.begin_frame();
        let ctx = DrawContext {
            scene,
            images,
            transform: camera.view_transform(),
            scale: camera.device_scale(),
            cull: Some(camera.visible_world_rect()),
            lod: true,
            raster: backend.supports_images(),
            retain: true,
        };
        self.painter.draw(backend, &ctx, scene.roots());
        self.painter.stats
    }

    /// Экспорт узла в PNG в масштабе `scale` (1, 2, 3…). Область — форма фрейма
    /// (как в Figma: всё за его краем обрезается) или границы узла с обводкой и
    /// потомками для остальных. Размер — пиксель в пиксель: фрейм 393×852 при @3x
    /// даёт 1179×2556. Предки не влияют: их прозрачность и обрезка не применяются.
    pub fn export_png(
        &mut self,
        scene: &Scene,
        id: NodeId,
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Vec<u8>, ExportError> {
        let pixmap = self.export_pixmap(scene, id, scale, images)?;
        pixmap
            .into_png()
            .map_err(|e| ExportError::Encode(e.to_string()))
    }

    pub fn export_pixmap(
        &mut self,
        scene: &Scene,
        id: NodeId,
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Pixmap, ExportError> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(ExportError::BadScale);
        }
        let slot = scene.slot(id).ok_or(ExportError::NotFound)?;
        let n = scene.at(slot);
        let area = match (n.props.kind, n.local_bounds) {
            (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
            _ => match (n.world_bounds, n.subtree_bounds) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            },
        }
        .ok_or(ExportError::Empty)?;
        let w = (area.width() * scale).round();
        let h = (area.height() * scale).round();
        if w < 1.0 || h < 1.0 {
            return Err(ExportError::Empty);
        }
        if w > MAX_EXPORT_SIDE as f64 || h > MAX_EXPORT_SIDE as f64 {
            return Err(ExportError::TooLarge(w as u32, h as u32));
        }
        let mut backend = CpuBackend::new(w as u16, h as u16);
        let ctx = DrawContext {
            scene,
            images,
            transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
            scale,
            cull: None,
            lod: false,
            raster: false,
            retain: false,
        };
        self.painter.begin_frame();
        self.painter.draw(&mut backend, &ctx, &[slot]);
        Ok(backend.render())
    }
}
````

- [ ] **Step 7: Запустить тесты**

Run: `cargo test -p ferrum-render --test render`
Expected: PASS, `9 passed`.

- [ ] **Step 8: Закоммитить**

```bash
git add crates/ferrum-render
git commit -m "feat(render): списки отрисовки, отсечение, LOD, растровый кэш, экспорт PNG"
```

---

### Task 7: Подписи и оверлеи

Оверлеи рисуются поверх содержимого в экранных пикселях: пиксельная сетка (с зума 800%), названия фреймов верхнего уровня (усечённые с «…» до ширины фрейма, выделенный — цветом выделения), подсветка наведения по форме узла, рамка выделения мышью, линии привязки, рамка выделения с ручками 8 px и бейдж «W × H». Тонкие линии выравниваются по пикселям, иначе линия в 1 px размывается на две полупрозрачные строки.

Подписи раскладывает `parley` со встроенным шрифтом Onest, а глифы рисуются заливкой контуров из `skrifa`, а не `glyph_run` vello: тому в `vello_gpu` нужны ресурсы рендерера с GPU-устройством, а контуры одинаково рисуются любым бэкендом и в эталонных тестах. Бейдж — Onest с цифрами одинаковой ширины (`tnum`); моноширинный шрифт дизайн-системы не подключён (см. «Что этап 2 сознательно не делает»).

**Files:**
- Create: `crates/ferrum-render/assets/fonts/Onest-wght.ttf`
- Create: `crates/ferrum-render/assets/fonts/OFL.txt`
- Create: `crates/ferrum-render/src/text.rs`
- Create: `crates/ferrum-render/src/overlay.rs`
- Modify: `crates/ferrum-render/src/lib.rs`
- Modify: `crates/ferrum-render/tests/common/mod.rs`
- Test: `crates/ferrum-render/tests/overlay.rs`

**Interfaces:**
- Consumes: `Scene`, `Camera`, `RenderBackend`, `Painter` из Task 6.
- Produces: `Renderer::render(backend, scene, camera, overlay, images, background: Option<Rgba>) -> FrameStats`; `Overlay { theme, selection, hover, marquee, snap_lines, frame_labels, pixel_grid }` (по умолчанию — тёмная тема, подписи и сетка включены); `OverlayTheme::{dark, light}` с полями `select`, `on_select`, `component`, `handle_fill`, `frame_label`, `snap`, `pixel_grid`; `TextEngine::{new, label(text, size, weight, max_width) -> Option<Arc<Label>>}`; `Label { width, ascent, descent }` с `draw(backend, origin, color)`; `format_size`; константы `HANDLE_SIZE` (8), `LABEL_SIZE` (11), `PIXEL_GRID_MIN_ZOOM` (8), `ONEST`.

- [ ] **Step 1: Подключить шрифт Onest**

Шрифт и лицензия — из официального репозитория google/fonts, закреплённого коммитом; контрольные суммы проверяются.

Выполнить:

```bash
mkdir -p crates/ferrum-render/assets/fonts
base=https://raw.githubusercontent.com/google/fonts/4c1db3aec83c67dd223dd82a68c039cab30917a9/ofl/onest
curl -sSfL -o crates/ferrum-render/assets/fonts/Onest-wght.ttf "$base/Onest%5Bwght%5D.ttf"
curl -sSfL -o crates/ferrum-render/assets/fonts/OFL.txt "$base/OFL.txt"
echo "966c5c29b4755da84b6854d5c21dd4eaa2420225d0e9874de602de176d4a9f31  crates/ferrum-render/assets/fonts/Onest-wght.ttf
071195d8806e226faeee60259c28ca67b458227af5195a73f5cfcab06e3003bc  crates/ferrum-render/assets/fonts/OFL.txt" | shasum -a 256 -c
```

Ожидаемый вывод: обе строки `OK`. Файл шрифта — 193 056 байт, вариативный (ось `wght` 100–900).

- [ ] **Step 2: Написать падающие тесты оверлеев**

`crates/ferrum-render/tests/overlay.rs`:

````rust
mod common;

use common::*;
use ferrum_doc::NodeKind;
use ferrum_render::{format_size, Camera, Overlay, OverlayTheme, TextEngine, HANDLE_SIZE};
use kurbo::{Point, Size};

#[test]
fn labels_truncate_with_ellipsis() {
    let mut t = TextEngine::new();
    let full = t.label("Главный экран", 11.0, 400, 1000.0).unwrap();
    assert!(full.width > 40.0 && full.width < 120.0, "{}", full.width);
    assert!(full.ascent > 5.0 && full.descent > 1.0);
    let short = t.label("Главный экран", 11.0, 400, 40.0).unwrap();
    assert!(short.width <= 40.0 && short.width > 20.0, "{}", short.width);
    assert!(t.label("Главный экран", 11.0, 400, 2.0).is_none());
    assert!(t.label("", 11.0, 400, 100.0).is_none());
}

#[test]
fn size_badge_format() {
    assert_eq!(format_size(393.0), "393");
    assert_eq!(format_size(12.345), "12.35");
    assert_eq!(format_size(0.5), "0.5");
}

#[test]
fn overlay_with_dpr_scales_handles() {
    let mut p = Page::new();
    let id = p.add(None, NodeKind::Rect, rect(10.0, 10.0, 40.0, 20.0));
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(60.0, 40.0), 2.0);
    cam.center = Point::new(30.0, 20.0);
    let overlay = Overlay {
        selection: vec![id],
        frame_labels: false,
        ..Default::default()
    };
    let img = render_overlay(&scene, &cam, &overlay, WHITE);
    assert_eq!((img.width(), img.height()), (120, 80));
    // Ручка 8 CSS px = 16 пикселей устройства: белая середина в 5 px от угла.
    assert_px(&img, 25, 25, [255, 255, 255], 0);
}

#[test]
fn themes_follow_design_system() {
    let hex = |c: ferrum_render::Rgba| c.to_rgba8().to_u8_array();
    let dark = OverlayTheme::dark();
    let light = OverlayTheme::light();
    assert_eq!(hex(dark.select), [0x4a, 0xa3, 0xff, 255]);
    assert_eq!(hex(light.select), [0x17, 0x69, 0xe0, 255]);
    assert_eq!(hex(dark.on_select), [0x1a, 0x19, 0x18, 255]);
    assert_eq!(hex(light.on_select), [255, 255, 255, 255]);
    assert_eq!(hex(dark.component), [0xb4, 0x9c, 0xff, 255]);
    assert_eq!(hex(light.component), [0x6d, 0x44, 0xd9, 255]);
    assert_eq!(hex(dark.frame_label), [0xa9, 0xa3, 0x9c, 255]);
    assert_eq!(hex(light.frame_label), [0x65, 0x5f, 0x58, 255]);
    assert_eq!(hex(dark.handle_fill), [255, 255, 255, 255]);
    assert_eq!(HANDLE_SIZE, 8.0);
}
````

`crates/ferrum-render/tests/common/mod.rs`:

````rust
//! Помощники тестов рендера: сборка страницы, отрисовка, сравнение с эталонами.
#![allow(dead_code)]

use std::path::PathBuf;

use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{
    Camera, CpuBackend, ImageProvider, NoImages, Overlay, Pixmap, RenderBackend, Renderer, Rgba,
};
use ferrum_scene::Scene;
use kurbo::{Affine, Point, Rect, Size};

/// Страница, которую тест наполняет узлами.
pub struct Page {
    pub doc: Document,
    pub page: PageId,
}

impl Page {
    pub fn new() -> Page {
        let doc = Document::new();
        let page = doc.pages()[0].0;
        Page { doc, page }
    }

    pub fn add(&mut self, parent: Option<NodeId>, kind: NodeKind, patch: PropsPatch) -> NodeId {
        self.doc
            .transact(self.page, vec![], |p| {
                let id = p.create_node(parent, None, kind)?;
                p.set_props(id, &patch)?;
                Ok(vec![id])
            })
            .unwrap()[0]
    }

    pub fn edit(&mut self, id: NodeId, patch: PropsPatch) {
        self.doc
            .transact(self.page, vec![], |p| {
                p.set_props(id, &patch).map(|_| vec![])
            })
            .unwrap();
    }

    pub fn scene(&mut self) -> Scene {
        self.doc.take_changes(self.page);
        Scene::from_page(self.doc.page(self.page).unwrap())
    }

    /// Применяет накопленные изменения к сцене.
    pub fn update(&mut self, scene: &mut Scene) {
        let changes = self.doc.take_changes(self.page);
        scene.apply(&changes, self.doc.page(self.page).unwrap());
    }
}

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

/// Камера с центром в середине холста `w × h` при зуме 1: мир совпадает с экраном.
pub fn camera(w: f64, h: f64) -> Camera {
    let mut c = Camera::new(Size::new(w, h), 1.0);
    c.center = Point::new(w / 2.0, h / 2.0);
    c
}

pub const WHITE: Rgba = Rgba::new([1.0, 1.0, 1.0, 1.0]);

/// Содержимое сцены на белом фоне, без оверлеев.
pub fn render(scene: &Scene, camera: &Camera) -> Pixmap {
    render_with(&mut Renderer::new(), scene, camera, &NoImages)
}

pub fn render_with(
    renderer: &mut Renderer,
    scene: &Scene,
    camera: &Camera,
    images: &dyn ImageProvider,
) -> Pixmap {
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(WHITE);
    backend.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
    renderer.draw_scene(&mut backend, scene, camera, images);
    backend.render()
}

/// Кадр целиком: фон, содержимое, оверлеи.
pub fn render_overlay(
    scene: &Scene,
    camera: &Camera,
    overlay: &Overlay,
    background: Rgba,
) -> Pixmap {
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    Renderer::new().render(
        &mut backend,
        scene,
        camera,
        overlay,
        &NoImages,
        Some(background),
    );
    backend.render()
}

/// Пиксель (с премультипликацией: на непрозрачном фоне совпадает с обычным цветом).
pub fn px(p: &Pixmap, x: u16, y: u16) -> [u8; 4] {
    let c = p.sample(x, y);
    [c.r, c.g, c.b, c.a]
}

/// Цвет пикселя совпадает с ожидаемым с точностью `tol` по каждому каналу.
#[track_caller]
pub fn assert_px(p: &Pixmap, x: u16, y: u16, rgb: [u8; 3], tol: u8) {
    let got = px(p, x, y);
    let ok = (0..3).all(|i| got[i].abs_diff(rgb[i]) <= tol);
    assert!(ok, "пиксель ({x}, {y}) = {got:?}, ожидался {rgb:?} ±{tol}");
}

/// Допуск сравнения с эталоном по каждому каналу: сглаживание краёв на разных
/// процессорах (NEON, AVX2, WASM SIMD) может отличаться на единицы.
pub const GOLDEN_TOLERANCE: u8 = 3;

/// Сравнивает картинку с эталоном `tests/golden/<name>.png`. С
/// `FERRUM_UPDATE_GOLDEN=1` эталон перезаписывается. Отличающийся результат
/// сохраняется рядом с временными файлами сборки для просмотра.
#[track_caller]
pub fn golden(name: &str, actual: &Pixmap) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"));
    // Через PNG и обратно: так эталон и результат одинаково теряют точность
    // полупрозрачных пикселей при снятии премультипликации.
    let png = actual.clone().into_png().expect("кодирование PNG");
    if std::env::var("FERRUM_UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().expect("каталог эталонов"))
            .expect("каталог эталонов");
        std::fs::write(&path, &png).expect("запись эталона");
        return;
    }
    let actual = Pixmap::from_png(std::io::Cursor::new(&png)).expect("чтение PNG");
    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "нет эталона {}: создайте его командой FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test golden",
            path.display()
        )
    });
    let expected = Pixmap::from_png(std::io::Cursor::new(bytes)).expect("эталон — PNG");
    assert_eq!(
        (expected.width(), expected.height()),
        (actual.width(), actual.height()),
        "размер {name}"
    );
    let mut worst = 0u8;
    let mut differing = 0usize;
    for (e, a) in expected.data().iter().zip(actual.data()) {
        let d = [
            e.r.abs_diff(a.r),
            e.g.abs_diff(a.g),
            e.b.abs_diff(a.b),
            e.a.abs_diff(a.a),
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        worst = worst.max(d);
        if d > GOLDEN_TOLERANCE {
            differing += 1;
        }
    }
    if differing > 0 {
        let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.actual.png"));
        std::fs::write(&out, &png).expect("запись результата");
        panic!(
            "{name}: {differing} пикселей отличаются от эталона больше чем на {GOLDEN_TOLERANCE} (максимум {worst}); результат: {}",
            out.display()
        );
    }
}
````

- [ ] **Step 3: Убедиться, что тесты не компилируются**

Run: `cargo test -p ferrum-render --test overlay`
Expected: ошибка компиляции — нет `Overlay`, `TextEngine`, `Renderer::render`.

- [ ] **Step 4: Реализовать подписи**

`crates/ferrum-render/src/text.rs`:

````rust
//! Однострочные подписи оверлеев (названия фреймов, бейдж размера) встроенным
//! шрифтом Onest.
//!
//! Раскладка — `parley`; глифы рисуются заливкой своих контуров из `skrifa`, а не
//! через `glyph_run` vello: тому в `vello_gpu` нужны ресурсы рендерера с
//! GPU-устройством, а контуры одинаково рисуются любым бэкендом и в эталонных
//! тестах. Контуры кэшируются в единицах шрифта, раскладки — по тексту и стилю.

use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{Affine, BezPath, Point};
use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::{
    FontContext, FontFamily, FontWeight, Layout, LayoutContext, PositionedLayoutItem, StyleProperty,
};
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use vello_cpu::peniko::Fill;

use crate::backend::{RenderBackend, Rgba};

/// Onest (SIL Open Font License 1.1, лицензия — `assets/fonts/OFL.txt`).
pub const ONEST: &[u8] = include_bytes!("../assets/fonts/Onest-wght.ttf");

/// Сколько разных подписей держать в кэше раскладок; больше — кэш сбрасывается.
const LABEL_CACHE_LIMIT: usize = 512;

/// Разложенная подпись: глифы в координатах строки (ноль — левый край базовой линии).
pub struct Label {
    glyphs: Vec<(Arc<BezPath>, Point, f64)>,
    /// Ширина строки.
    pub width: f64,
    /// Подъём над базовой линией и спуск под ней.
    pub ascent: f64,
    pub descent: f64,
}

pub struct TextEngine {
    fonts: FontContext,
    layouts: LayoutContext<()>,
    family: String,
    outlines: HashMap<(u32, Vec<i16>), Arc<BezPath>>,
    labels: HashMap<(String, u32, u16, u32), Arc<Label>>,
}

impl Default for TextEngine {
    fn default() -> Self {
        TextEngine::new()
    }
}

impl TextEngine {
    pub fn new() -> TextEngine {
        // Без системных шрифтов: подпись выглядит одинаково везде, и в WASM нечего искать.
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        });
        let families = collection.register_fonts(Blob::new(Arc::new(ONEST)), None);
        let family = families
            .first()
            .and_then(|(id, _)| collection.family_name(*id))
            .unwrap_or("Onest")
            .to_string();
        TextEngine {
            fonts: FontContext {
                collection,
                source_cache: SourceCache::default(),
            },
            layouts: LayoutContext::new(),
            family,
            outlines: HashMap::new(),
            labels: HashMap::new(),
        }
    }

    /// Подпись, усечённая с многоточием до `max_width` (`None` — не помещается даже
    /// многоточие). Размер — в пикселях, в которых подпись будет нарисована.
    pub fn label(
        &mut self,
        text: &str,
        size: f64,
        weight: u16,
        max_width: f64,
    ) -> Option<Arc<Label>> {
        // Отрицательные, нулевые и NaN-размеры подписи не дают.
        let positive = |v: f64| v > 0.0;
        if text.is_empty() || !positive(max_width) || !positive(size) {
            return None;
        }
        let key = (
            text.to_string(),
            (size * 64.0) as u32,
            weight,
            max_width.clamp(0.0, 1e6) as u32,
        );
        if let Some(l) = self.labels.get(&key) {
            return Some(l.clone());
        }
        let full = self.layout(text, size, weight);
        let label = if full.width <= max_width {
            full
        } else {
            // Самый длинный префикс, который вместе с «…» помещается в ширину.
            let cuts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
            let mut shortened = |keep: usize| {
                let candidate = format!("{}…", text[..cuts[keep]].trim_end());
                self.layout(&candidate, size, weight)
            };
            let mut best = shortened(0);
            if best.width > max_width {
                return None;
            }
            let (mut lo, mut hi) = (0, cuts.len() - 1);
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                let candidate = shortened(mid);
                if candidate.width <= max_width {
                    best = candidate;
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            best
        };
        if self.labels.len() >= LABEL_CACHE_LIMIT {
            self.labels.clear();
        }
        let label = Arc::new(label);
        self.labels.insert(key, label.clone());
        Some(label)
    }

    fn layout(&mut self, text: &str, size: f64, weight: u16) -> Label {
        let mut builder = self
            .layouts
            .ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(&self.family)));
        builder.push_default(StyleProperty::FontSize(size as f32));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(weight as f32)));
        // Цифры одной ширины: значения в бейдже размера не прыгают при ресайзе.
        builder.push_default(StyleProperty::FontFeatures("\"tnum\" on".into()));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        let mut glyphs = Vec::new();
        let (mut ascent, mut descent) = (0.0f64, 0.0f64);
        let mut baseline = None;
        for line in layout.lines() {
            let line_baseline = *baseline.get_or_insert(line.metrics().baseline as f64);
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let r = run.run();
                let m = r.font_metrics();
                ascent = ascent.max(m.ascent as f64);
                descent = descent.max(m.descent.abs() as f64);
                let font = r.font();
                let Ok(font_ref) = FontRef::from_index(font.data.as_ref(), font.index) else {
                    continue;
                };
                let coords: Vec<i16> = r.normalized_coords().iter().map(|c| c.to_bits()).collect();
                let upem = font_ref
                    .metrics(Size::unscaled(), LocationRef::default())
                    .units_per_em
                    .max(1) as f64;
                let scale = r.font_size() as f64 / upem;
                for g in run.positioned_glyphs() {
                    let path = self.outline(&font_ref, g.id, &coords);
                    let at = Point::new(g.x as f64, g.y as f64 - line_baseline);
                    glyphs.push((path, at, scale));
                }
            }
        }
        Label {
            glyphs,
            width: layout.width() as f64,
            ascent,
            descent,
        }
    }

    /// Контур глифа в единицах шрифта (ось Y вверх), из кэша.
    fn outline(&mut self, font: &FontRef<'_>, glyph: u32, coords: &[i16]) -> Arc<BezPath> {
        let key = (glyph, coords.to_vec());
        if let Some(p) = self.outlines.get(&key) {
            return p.clone();
        }
        let location: Vec<NormalizedCoord> = coords
            .iter()
            .map(|c| NormalizedCoord::from_bits(*c))
            .collect();
        let mut pen = Pen(BezPath::new());
        if let Some(outline) = font.outline_glyphs().get(GlyphId::new(glyph)) {
            // Глиф с повреждённым контуром рисуется пустым, а не роняет кадр.
            let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::new(&location));
            if outline.draw(settings, &mut pen).is_err() {
                pen.0 = BezPath::new();
            }
        }
        let path = Arc::new(pen.0);
        self.outlines.insert(key, path.clone());
        path
    }
}

impl Label {
    /// Рисует подпись: `origin` — левый край базовой линии в пикселях устройства.
    pub fn draw(&self, backend: &mut impl RenderBackend, origin: Point, color: Rgba) {
        backend.set_color(color);
        for (path, at, scale) in &self.glyphs {
            backend.set_transform(
                Affine::translate(origin.to_vec2() + at.to_vec2())
                    * Affine::scale_non_uniform(*scale, -*scale),
            );
            backend.fill_path(path, Fill::NonZero);
        }
    }
}

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.quad_to((cx as f64, cy as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.0.curve_to(
            (c1x as f64, c1y as f64),
            (c2x as f64, c2y as f64),
            (x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
````

- [ ] **Step 5: Реализовать оверлеи**

`crates/ferrum-render/src/overlay.rs`:

````rust
//! Оверлеи поверх содержимого в экранных пикселях: пиксельная сетка, названия
//! фреймов, подсветка наведения, рамка выделения мышью, линии привязки, рамка
//! выделения с ручками и бейдж «W × H». Толщины и размеры задаются в CSS-пикселях
//! и умножаются на `devicePixelRatio`.

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::shape::Geometry;
use ferrum_scene::Scene;
use kurbo::{Affine, BezPath, Line, Point, Rect, RoundedRect, Shape, Stroke};
use vello_cpu::peniko::Fill;

use crate::backend::{RenderBackend, Rgba};
use crate::camera::Camera;
use crate::text::TextEngine;

/// Сторона квадратной ручки выделения.
pub const HANDLE_SIZE: f64 = 8.0;
/// Пиксельная сетка видна с зума 800%.
pub const PIXEL_GRID_MIN_ZOOM: f64 = 8.0;
/// Кегль подписей фреймов и бейджа.
pub const LABEL_SIZE: f64 = 11.0;

/// Цвета оверлеев из дизайн-системы Ferrum.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayTheme {
    /// Рамки выделения и наведения, ручки, фон бейджа.
    pub select: Rgba,
    /// Текст на фоне `select`.
    pub on_select: Rgba,
    /// Выделение компонента (подсистема 5).
    pub component: Rgba,
    pub handle_fill: Rgba,
    /// Названия фреймов (`ink-muted`).
    pub frame_label: Rgba,
    /// Линии привязки.
    pub snap: Rgba,
    pub pixel_grid: Rgba,
}

fn hex(rgb: u32) -> Rgba {
    Rgba::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

impl OverlayTheme {
    pub fn dark() -> OverlayTheme {
        OverlayTheme {
            select: hex(0x4aa3ff),
            on_select: hex(0x1a1918),
            component: hex(0xb49cff),
            handle_fill: hex(0xffffff),
            frame_label: hex(0xa9a39c),
            snap: hex(0xf24822),
            pixel_grid: Rgba::from_rgba8(255, 255, 255, 26),
        }
    }

    pub fn light() -> OverlayTheme {
        OverlayTheme {
            select: hex(0x1769e0),
            on_select: hex(0xffffff),
            component: hex(0x6d44d9),
            handle_fill: hex(0xffffff),
            frame_label: hex(0x655f58),
            snap: hex(0xf24822),
            pixel_grid: Rgba::from_rgba8(0, 0, 0, 20),
        }
    }
}

/// Что показать поверх содержимого. Заполняет редактор (этап 3).
#[derive(Debug, Clone)]
pub struct Overlay {
    pub theme: OverlayTheme,
    pub selection: Vec<NodeId>,
    pub hover: Option<NodeId>,
    /// Рамка выделения мышью в мировых координатах.
    pub marquee: Option<Rect>,
    /// Линии привязки в мировых координатах.
    pub snap_lines: Vec<Line>,
    /// Названия фреймов верхнего уровня.
    pub frame_labels: bool,
    /// Пиксельная сетка (рисуется только с `PIXEL_GRID_MIN_ZOOM`).
    pub pixel_grid: bool,
}

impl Default for Overlay {
    fn default() -> Self {
        Overlay {
            theme: OverlayTheme::dark(),
            selection: Vec::new(),
            hover: None,
            marquee: None,
            snap_lines: Vec::new(),
            frame_labels: true,
            pixel_grid: true,
        }
    }
}

/// Координата тонкой линии, выровненная по пикселям: линия нечётной толщины
/// проходит через центры пикселей, чётной — по их границам. Иначе линия в 1 px
/// размывается на две полупрозрачные строки.
fn crisp(v: f64, width: f64) -> f64 {
    if (width.round() as i64) % 2 == 1 {
        v.floor() + 0.5
    } else {
        v.round()
    }
}

fn crisp_point(p: Point, width: f64) -> Point {
    Point::new(crisp(p.x, width), crisp(p.y, width))
}

/// Размер для бейджа: целые без дробной части, остальное — до сотых.
pub fn format_size(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 {
        format!("{r:.0}")
    } else {
        format!("{r}")
    }
}

pub(crate) fn draw(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    scene: &Scene,
    camera: &Camera,
    overlay: &Overlay,
) {
    let dpr = camera.dpr;
    // Мир → пиксели устройства.
    let view = camera.view_transform();
    let theme = &overlay.theme;
    let hairline = Stroke::new(dpr);

    if overlay.pixel_grid && camera.zoom() >= PIXEL_GRID_MIN_ZOOM {
        pixel_grid(backend, camera, theme.pixel_grid);
    }
    if overlay.frame_labels {
        frame_labels(backend, text, scene, camera, overlay);
    }
    if let Some(id) = overlay.hover.filter(|h| !overlay.selection.contains(h)) {
        if let Some(outline) = node_outline(scene, id, view) {
            backend.set_transform(Affine::IDENTITY);
            backend.set_color(theme.select);
            backend.stroke_path(&outline, &Stroke::new(1.5 * dpr));
        }
    }
    if let Some(m) = overlay.marquee {
        let r = view.transform_rect_bbox(m);
        let r = Rect::from_points(
            crisp_point(r.origin(), dpr),
            crisp_point(Point::new(r.x1, r.y1), dpr),
        );
        backend.set_transform(Affine::IDENTITY);
        backend.set_color(theme.select.multiply_alpha(0.12));
        backend.fill_rect(&r);
        backend.set_color(theme.select);
        backend.stroke_path(&r.to_path(0.1), &hairline);
    }
    for line in &overlay.snap_lines {
        let mut path = BezPath::new();
        path.move_to(crisp_point(view * line.p0, dpr));
        path.line_to(crisp_point(view * line.p1, dpr));
        backend.set_transform(Affine::IDENTITY);
        backend.set_color(theme.snap);
        backend.stroke_path(&path, &hairline);
    }
    selection(backend, text, scene, camera, overlay);
}

/// Контур формы узла в пикселях устройства; у группы — рамка её границ.
fn node_outline(scene: &Scene, id: NodeId, view: Affine) -> Option<BezPath> {
    let n = scene.node(id)?;
    let to_device = view * n.world;
    let mut path = BezPath::new();
    match n.geometry() {
        Geometry::Line(len) => {
            path.move_to(to_device * Point::ZERO);
            path.line_to(to_device * Point::new(len, 0.0));
        }
        Geometry::None => path = to_device * n.local_bounds?.to_path(0.1),
        geom => {
            geom.fill_path_into(&mut path);
            path.apply_affine(to_device);
        }
    }
    Some(path)
}

fn pixel_grid(backend: &mut impl RenderBackend, camera: &Camera, color: Rgba) {
    let visible = camera.visible_world_rect();
    let view = camera.view_transform();
    let (w, h) = backend.size();
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(color);
    let mut x = visible.x0.floor();
    while x <= visible.x1 {
        let dx = (view * Point::new(x, 0.0)).x.round();
        backend.fill_rect(&Rect::new(dx, 0.0, dx + 1.0, h as f64));
        x += 1.0;
    }
    let mut y = visible.y0.floor();
    while y <= visible.y1 {
        let dy = (view * Point::new(0.0, y)).y.round();
        backend.fill_rect(&Rect::new(0.0, dy, w as f64, dy + 1.0));
        y += 1.0;
    }
}

/// Названия фреймов верхнего уровня над их левым верхним углом, усечённые до
/// ширины фрейма. Выделенный фрейм подписан цветом выделения.
fn frame_labels(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    scene: &Scene,
    camera: &Camera,
    overlay: &Overlay,
) {
    let dpr = camera.dpr;
    let view = camera.view_transform();
    let (w, h) = backend.size();
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64);
    for &slot in scene.roots() {
        let n = scene.at(slot);
        if n.props.kind != NodeKind::Frame || !n.props.visible {
            continue;
        }
        let Some(b) = n.world_bounds.map(|b| view.transform_rect_bbox(b)) else {
            continue;
        };
        let gap = 4.0 * dpr;
        if b.x1 < 0.0
            || b.x0 > screen.x1
            || b.y0 < 0.0
            || b.y0 - gap - LABEL_SIZE * dpr * 1.5 > screen.y1
        {
            continue;
        }
        let Some(label) = text.label(&n.props.name, LABEL_SIZE * dpr, 400, b.width()) else {
            continue;
        };
        let color = if overlay.selection.contains(&n.id) {
            overlay.theme.select
        } else {
            overlay.theme.frame_label
        };
        label.draw(backend, Point::new(b.x0, b.y0 - gap - label.descent), color);
    }
}

fn selection(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    scene: &Scene,
    camera: &Camera,
    overlay: &Overlay,
) {
    let dpr = camera.dpr;
    let view = camera.view_transform();
    let theme = &overlay.theme;
    let nodes: Vec<_> = overlay
        .selection
        .iter()
        .filter_map(|id| scene.node(*id))
        .filter(|n| n.local_bounds.is_some())
        .collect();
    if nodes.is_empty() {
        return;
    }
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    let hairline = Stroke::new(dpr);
    // Рамка каждого узла — его повёрнутый прямоугольник.
    let mut boxes = Vec::new();
    for n in &nodes {
        let to_device = view * n.world;
        let b = n.local_bounds.expect("отфильтровано выше");
        let c = to_device.as_coeffs();
        // Рамка неповёрнутого узла выравнивается по пикселям, повёрнутого — нет.
        let axis_aligned = c[1] == 0.0 && c[2] == 0.0;
        let quad = [
            Point::new(b.x0, b.y0),
            Point::new(b.x1, b.y0),
            Point::new(b.x1, b.y1),
            Point::new(b.x0, b.y1),
        ]
        .map(|p| {
            let d = to_device * p;
            if axis_aligned {
                crisp_point(d, dpr)
            } else {
                d
            }
        });
        let mut path = BezPath::new();
        path.move_to(quad[0]);
        for p in &quad[1..] {
            path.line_to(*p);
        }
        path.close_path();
        backend.stroke_path(&path, &hairline);
        boxes.push(quad);
    }

    // Ручки и бейдж: у одного узла — по его повёрнутой рамке, у нескольких — по
    // общей рамке на экране.
    let (quad, size, is_line) = if let [n] = nodes[..] {
        let b = n.local_bounds.expect("отфильтровано выше");
        (
            boxes[0],
            (b.width(), b.height()),
            n.props.kind == NodeKind::Line,
        )
    } else {
        let world = nodes
            .iter()
            .filter_map(|n| n.world_bounds)
            .reduce(|a, b| a.union(b))
            .expect("непустое выделение");
        let r = boxes
            .iter()
            .flat_map(|q| q.iter())
            .fold(Rect::from_points(boxes[0][0], boxes[0][0]), |r, p| {
                r.union_pt(*p)
            });
        let r = Rect::from_points(
            crisp_point(r.origin(), dpr),
            crisp_point(Point::new(r.x1, r.y1), dpr),
        );
        backend.stroke_path(&r.to_path(0.1), &hairline);
        let quad = [
            Point::new(r.x0, r.y0),
            Point::new(r.x1, r.y0),
            Point::new(r.x1, r.y1),
            Point::new(r.x0, r.y1),
        ];
        (quad, (world.width(), world.height()), false)
    };
    let handles: Vec<Point> = if is_line {
        vec![quad[0], quad[1]]
    } else {
        (0..4)
            .flat_map(|i| [quad[i], quad[i].midpoint(quad[(i + 1) % 4])])
            .collect()
    };
    let angle = (quad[1] - quad[0]).atan2();
    let half = HANDLE_SIZE * dpr / 2.0;
    let square = Rect::new(-half, -half, half, half).to_path(0.1);
    for h in handles {
        backend.set_transform(Affine::translate(h.to_vec2()) * Affine::rotate(angle));
        backend.set_color(theme.handle_fill);
        backend.fill_path(&square, Fill::NonZero);
        backend.set_color(theme.select);
        backend.stroke_path(&square, &hairline);
    }

    let caption = if is_line {
        format_size(size.0)
    } else {
        format!("{} × {}", format_size(size.0), format_size(size.1))
    };
    let Some(label) = text.label(&caption, LABEL_SIZE * dpr, 500, f64::MAX) else {
        return;
    };
    let bottom = quad.iter().fold(f64::MIN, |m, p| m.max(p.y));
    let center_x = quad.iter().map(|p| p.x).sum::<f64>() / 4.0;
    let (pad_x, pad_y) = (4.0 * dpr, 2.0 * dpr);
    let height = label.ascent + label.descent + 2.0 * pad_y;
    let pill = Rect::new(
        center_x - label.width / 2.0 - pad_x,
        bottom + 6.0 * dpr,
        center_x + label.width / 2.0 + pad_x,
        bottom + 6.0 * dpr + height,
    );
    backend.set_transform(Affine::IDENTITY);
    backend.set_color(theme.select);
    backend.fill_path(
        &RoundedRect::from_rect(pill, 2.0 * dpr).to_path(0.1),
        Fill::NonZero,
    );
    label.draw(
        backend,
        Point::new(pill.x0 + pad_x, pill.y0 + pad_y + label.ascent),
        theme.on_select,
    );
}
````

- [ ] **Step 6: Кадр с оверлеями в `Renderer`**

`crates/ferrum-render/src/lib.rs`:

````rust
//! Рендер Ferrum: камера, сборка кадра со списками отрисовки, отсечением и LOD,
//! оверлеи, экспорт PNG. Получает сцену, камеру и описание оверлеев; о Loro и
//! инструментах не знает.

mod backend;
mod camera;
mod display;
mod draw;
mod image;
mod overlay;
mod text;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::Scene;
use kurbo::{Affine, Rect};

pub use backend::{CpuBackend, ImagePaint, RenderBackend, Rgba};
pub use camera::{Camera, MAX_ZOOM, MIN_ZOOM};
pub use draw::{
    FrameStats, EVICT_AFTER_FRAMES, LOD_RECT_PX, LOD_SKIP_PX, RASTER_BUILDS_PER_FRAME,
    RASTER_MAX_PX, RASTER_MAX_SCALE,
};
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use overlay::{
    format_size, Overlay, OverlayTheme, HANDLE_SIZE, LABEL_SIZE, PIXEL_GRID_MIN_ZOOM,
};
pub use text::{Label, TextEngine, ONEST};
pub use vello_cpu::Pixmap;

use draw::{DrawContext, Painter};

/// Наибольшая сторона экспортируемой картинки в пикселях.
pub const MAX_EXPORT_SIDE: u32 = 16384;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExportError {
    #[error("Узел для экспорта не найден")]
    NotFound,
    #[error("Масштаб экспорта должен быть положительным")]
    BadScale,
    #[error("Нечего экспортировать: у узла нулевой размер или он скрыт")]
    Empty,
    #[error(
        "Картинка {0}×{1} слишком большая: сторона не больше 16384 пикселей. Уменьшите масштаб"
    )]
    TooLarge(u32, u32),
    #[error("Не удалось закодировать PNG: {0}")]
    Encode(String),
}

/// Рендерер одного холста: кэш списков отрисовки узлов и шрифтов живёт между
/// кадрами.
#[derive(Default)]
pub struct Renderer {
    painter: Painter,
    text: TextEngine,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer::default()
    }

    /// Рисует кадр: фон, содержимое, оверлеи.
    pub fn render(
        &mut self,
        backend: &mut impl RenderBackend,
        scene: &Scene,
        camera: &Camera,
        overlay: &Overlay,
        images: &dyn ImageProvider,
        background: Option<Rgba>,
    ) -> FrameStats {
        if let Some(bg) = background {
            let (w, h) = backend.size();
            backend.set_transform(Affine::IDENTITY);
            backend.set_color(bg);
            backend.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
        }
        let stats = self.draw_scene(backend, scene, camera, images);
        overlay::draw(backend, &mut self.text, scene, camera, overlay);
        stats
    }

    /// Только содержимое сцены, без фона и оверлеев.
    pub fn draw_scene(
        &mut self,
        backend: &mut impl RenderBackend,
        scene: &Scene,
        camera: &Camera,
        images: &dyn ImageProvider,
    ) -> FrameStats {
        self.painter.begin_frame();
        let ctx = DrawContext {
            scene,
            images,
            transform: camera.view_transform(),
            scale: camera.device_scale(),
            cull: Some(camera.visible_world_rect()),
            lod: true,
            raster: backend.supports_images(),
            retain: true,
        };
        self.painter.draw(backend, &ctx, scene.roots());
        self.painter.stats
    }

    /// Экспорт узла в PNG в масштабе `scale` (1, 2, 3…). Область — форма фрейма
    /// (как в Figma: всё за его краем обрезается) или границы узла с обводкой и
    /// потомками для остальных. Размер — пиксель в пиксель: фрейм 393×852 при @3x
    /// даёт 1179×2556. Предки не влияют: их прозрачность и обрезка не применяются.
    pub fn export_png(
        &mut self,
        scene: &Scene,
        id: NodeId,
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Vec<u8>, ExportError> {
        let pixmap = self.export_pixmap(scene, id, scale, images)?;
        pixmap
            .into_png()
            .map_err(|e| ExportError::Encode(e.to_string()))
    }

    pub fn export_pixmap(
        &mut self,
        scene: &Scene,
        id: NodeId,
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Pixmap, ExportError> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(ExportError::BadScale);
        }
        let slot = scene.slot(id).ok_or(ExportError::NotFound)?;
        let n = scene.at(slot);
        let area = match (n.props.kind, n.local_bounds) {
            (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
            _ => match (n.world_bounds, n.subtree_bounds) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            },
        }
        .ok_or(ExportError::Empty)?;
        let w = (area.width() * scale).round();
        let h = (area.height() * scale).round();
        if w < 1.0 || h < 1.0 {
            return Err(ExportError::Empty);
        }
        if w > MAX_EXPORT_SIDE as f64 || h > MAX_EXPORT_SIDE as f64 {
            return Err(ExportError::TooLarge(w as u32, h as u32));
        }
        let mut backend = CpuBackend::new(w as u16, h as u16);
        let ctx = DrawContext {
            scene,
            images,
            transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
            scale,
            cull: None,
            lod: false,
            raster: false,
            retain: false,
        };
        self.painter.begin_frame();
        self.painter.draw(&mut backend, &ctx, &[slot]);
        Ok(backend.render())
    }
}
````

- [ ] **Step 7: Запустить тесты**

Run: `cargo test -p ferrum-render --test overlay --test render`
Expected: PASS, `4 passed` и `9 passed`.

- [ ] **Step 8: Закоммитить**

```bash
git add crates/ferrum-render
git commit -m "feat(render): подписи Onest и оверлеи выделения, привязок, сетки"
```

---

### Task 8: Эталонные картинки

Каждый тест сначала проверяет смысл картинки по отдельным пикселям — так тест полезен и до появления эталона и ловит ошибку, даже если эталон перезаписали неверной картинкой. Затем картинка сравнивается с `tests/golden/<имя>.png` с допуском 3 по каждому каналу (сглаживание краёв на NEON, AVX2 и WASM SIMD может отличаться на единицы). Без эталона тест падает с подсказкой; `FERRUM_UPDATE_GOLDEN=1` перезаписывает эталоны. Отличающийся результат сохраняется во временный каталог сборки (`CARGO_TARGET_TMPDIR`) для просмотра.

Наборы: фигуры (скругление, эллипс, линия), выравнивание обводок, обрезка фрейма, прозрачность контейнера и листа, режимы заливки изображением, оверлеи, пиксельная сетка, подписи фреймов, повороты; плюс проверка, что узел на координатах 1e7 рисуется пиксель в пиксель как у начала координат.

**Files:**
- Test: `crates/ferrum-render/tests/golden.rs`
- Create: `crates/ferrum-render/tests/golden/*.png`

**Interfaces:**
- Consumes: `Renderer`, `Overlay`, помощники `tests/common`.
- Produces: —

- [ ] **Step 1: Написать тесты эталонов**

`crates/ferrum-render/tests/golden.rs`:

````rust
//! Эталонные картинки рендера через `vello_cpu`. Каждый тест сначала проверяет
//! смысл картинки по отдельным пикселям (тест полезен и без эталона), затем
//! сравнивает её с `tests/golden/<имя>.png` с допуском по каналам.
//! Пересоздать эталоны: `FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test golden`.

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch, ScaleMode, Stroke, StrokeAlign};
use ferrum_render::{Camera, Overlay, OverlayTheme, Pixmap, Renderer, Rgba};
use kurbo::{Line, Point, Rect, Size};
use vello_cpu::color::PremulRgba8;

fn solid(r: u8, g: u8, b: u8, a: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, a))])
}

fn stroke(width: f64, align: StrokeAlign) -> Option<Vec<Stroke>> {
    Some(vec![Stroke {
        color: Color::BLACK,
        width,
        align,
    }])
}

#[test]
fn shapes() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            corner_radius: Some(12.0),
            fills: solid(255, 0, 0, 255),
            ..rect(10.0, 10.0, 60.0, 40.0)
        },
    );
    p.add(
        None,
        NodeKind::Ellipse,
        PropsPatch {
            fills: solid(0, 0, 255, 255),
            ..rect(90.0, 10.0, 60.0, 40.0)
        },
    );
    p.add(
        None,
        NodeKind::Line,
        PropsPatch {
            strokes: stroke(3.0, StrokeAlign::Center),
            ..rect(10.0, 90.0, 140.0, 0.0)
        },
    );
    let img = render(&p.scene(), &camera(160.0, 120.0));
    assert_px(&img, 40, 30, [255, 0, 0], 0);
    assert_px(&img, 11, 11, [255, 255, 255], 0);
    assert_px(&img, 120, 30, [0, 0, 255], 0);
    assert_px(&img, 92, 12, [255, 255, 255], 0);
    assert_px(&img, 80, 90, [0, 0, 0], 0);
    assert_px(&img, 80, 94, [255, 255, 255], 0);
    golden("shapes", &img);
}

#[test]
fn stroke_alignment() {
    let mut p = Page::new();
    let aligns = [
        (15.0, StrokeAlign::Inside, 255),
        (65.0, StrokeAlign::Center, 255),
        (115.0, StrokeAlign::Outside, 255),
        // Полупрозрачная заливка: внутренняя обводка кольцом, а не слиянием с заливкой.
        (165.0, StrokeAlign::Inside, 128),
    ];
    for (x, align, alpha) in aligns {
        p.add(
            None,
            NodeKind::Rect,
            PropsPatch {
                fills: solid(200, 200, 200, alpha),
                strokes: stroke(6.0, align),
                ..rect(x, 15.0, 30.0, 30.0)
            },
        );
    }
    let img = render(&p.scene(), &camera(210.0, 60.0));
    // Внутренняя: у края внутри чёрное, снаружи фон, глубже заливка.
    assert_px(&img, 16, 30, [0, 0, 0], 0);
    assert_px(&img, 13, 30, [255, 255, 255], 0);
    assert_px(&img, 24, 30, [200, 200, 200], 0);
    // По центру: по 3 пикселя в обе стороны от края.
    assert_px(&img, 63, 30, [0, 0, 0], 0);
    assert_px(&img, 67, 30, [0, 0, 0], 0);
    assert_px(&img, 60, 30, [255, 255, 255], 0);
    assert_px(&img, 72, 30, [200, 200, 200], 0);
    // Снаружи: 6 пикселей за краем.
    assert_px(&img, 111, 30, [0, 0, 0], 0);
    assert_px(&img, 107, 30, [255, 255, 255], 0);
    assert_px(&img, 117, 30, [200, 200, 200], 0);
    // Кольцо поверх полупрозрачной заливки.
    assert_px(&img, 166, 30, [0, 0, 0], 0);
    assert_px(&img, 180, 30, [227, 227, 227], 2);
    golden("stroke_alignment", &img);
}

#[test]
fn frame_clip() {
    let mut p = Page::new();
    for (x, clip) in [(10.0, true), (90.0, false)] {
        let f = p.add(
            None,
            NodeKind::Frame,
            PropsPatch {
                clip_content: Some(clip),
                fills: solid(230, 230, 230, 255),
                ..rect(x, 10.0, 50.0, 50.0)
            },
        );
        p.add(
            Some(f),
            NodeKind::Rect,
            PropsPatch {
                fills: solid(0, 160, 0, 255),
                ..rect(20.0, 20.0, 40.0, 40.0)
            },
        );
    }
    let img = render(&p.scene(), &camera(160.0, 80.0));
    assert_px(&img, 40, 40, [0, 160, 0], 0);
    assert_px(&img, 65, 40, [255, 255, 255], 0);
    assert_px(&img, 145, 40, [0, 160, 0], 0);
    golden("frame_clip", &img);
}

#[test]
fn container_and_leaf_opacity() {
    let mut p = Page::new();
    let g = p.add(
        None,
        NodeKind::Group,
        PropsPatch {
            x: Some(10.0),
            y: Some(10.0),
            opacity: Some(0.5),
            ..Default::default()
        },
    );
    for (x, y) in [(0.0, 0.0), (20.0, 20.0)] {
        p.add(
            Some(g),
            NodeKind::Rect,
            PropsPatch {
                fills: solid(255, 0, 0, 255),
                ..rect(x, y, 40.0, 40.0)
            },
        );
    }
    for (x, y) in [(90.0, 10.0), (110.0, 30.0)] {
        p.add(
            None,
            NodeKind::Rect,
            PropsPatch {
                opacity: Some(0.5),
                fills: solid(255, 0, 0, 255),
                ..rect(x, y, 40.0, 40.0)
            },
        );
    }
    let img = render(&p.scene(), &camera(160.0, 80.0));
    // Группа — одним слоем: перекрытие не темнее остального.
    assert_px(&img, 15, 15, [255, 128, 128], 2);
    assert_px(&img, 40, 40, [255, 128, 128], 2);
    // Листья — каждый со своей прозрачностью: перекрытие темнее.
    assert_px(&img, 95, 15, [255, 128, 128], 2);
    assert_px(&img, 120, 40, [255, 64, 64], 2);
    golden("container_and_leaf_opacity", &img);
}

/// Картинка 4×2: левые два столбца красные, правые синие.
fn two_tone() -> Arc<Pixmap> {
    let mut img = Pixmap::new(4, 2);
    for y in 0..2 {
        for x in 0..4 {
            let (r, b) = if x < 2 { (255, 0) } else { (0, 255) };
            img.set_pixel(x, y, PremulRgba8 { r, g: 0, b, a: 255 });
        }
    }
    Arc::new(img)
}

#[test]
fn image_fill_modes() {
    let mut p = Page::new();
    for (x, mode) in [
        (10.0, ScaleMode::Fill),
        (80.0, ScaleMode::Fit),
        (150.0, ScaleMode::Tile),
    ] {
        p.add(
            None,
            NodeKind::Image,
            PropsPatch {
                fills: Some(vec![Fill {
                    scale_mode: mode,
                    ..Fill::image("two-tone")
                }]),
                ..rect(x, 10.0, 60.0, 40.0)
            },
        );
    }
    let images: HashMap<String, Arc<Pixmap>> = [("two-tone".to_string(), two_tone())].into();
    let mut r = Renderer::new();
    let img = render_with(&mut r, &p.scene(), &camera(220.0, 60.0), &images);
    // «Заполнить»: картинка 80×40 обрезана по бокам — половина красная, половина синяя.
    assert_px(&img, 15, 30, [255, 0, 0], 8);
    assert_px(&img, 65, 30, [0, 0, 255], 8);
    // «Вписать»: 60×30 по центру, сверху и снизу по 5 пикселей фона.
    assert_px(&img, 110, 12, [255, 255, 255], 0);
    assert_px(&img, 85, 30, [255, 0, 0], 8);
    assert_px(&img, 135, 30, [0, 0, 255], 8);
    // «Плитка»: картинка в натуральном размере повторяется каждые 4 пикселя.
    assert_px(&img, 150, 20, [255, 0, 0], 8);
    assert_px(&img, 153, 20, [0, 0, 255], 8);
    assert_px(&img, 154, 20, [255, 0, 0], 8);
    golden("image_fill_modes", &img);
}

#[test]
fn overlays() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Экран".into()),
            ..rect(20.0, 30.0, 140.0, 100.0)
        },
    );
    let sel = p.add(Some(f), NodeKind::Rect, rect(20.0, 20.0, 60.0, 40.0));
    let hover = p.add(Some(f), NodeKind::Ellipse, rect(100.0, 30.0, 30.0, 30.0));
    let scene = p.scene();
    let overlay = Overlay {
        theme: OverlayTheme::light(),
        selection: vec![sel],
        hover: Some(hover),
        marquee: Some(Rect::new(170.0, 100.0, 230.0, 150.0)),
        snap_lines: vec![Line::new((0.0, 50.0), (240.0, 50.0))],
        ..Default::default()
    };
    let img = render_overlay(&scene, &camera(240.0, 160.0), &overlay, WHITE);
    // Ручка в углу выделения: белая середина.
    assert_px(&img, 40, 50, [255, 255, 255], 30);
    // Линия привязки красная.
    let snap = px(&img, 5, 50);
    assert!(snap[0] > 200 && snap[1] < 140, "{snap:?}");
    // Рамка выделения мышью подкрашена цветом выделения.
    let m = px(&img, 200, 125);
    assert!(m[2] > m[0] && m[0] < 250, "{m:?}");
    // Бейдж «60 × 40» под выделением — синяя плашка.
    let blue = (50..90)
        .flat_map(|x| (96..114).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let c = px(&img, x, y);
            c[2] > 180 && c[0] < 60
        })
        .count();
    assert!(blue > 100, "пикселей бейджа: {blue}");
    golden("overlays", &img);
}

#[test]
fn pixel_grid() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0, 255),
            ..rect(2.0, 2.0, 3.0, 2.0)
        },
    );
    let mut cam = Camera::new(Size::new(120.0, 80.0), 1.0);
    cam.center = Point::new(6.0, 4.0);
    cam.set_zoom(10.0);
    let overlay = Overlay {
        theme: OverlayTheme::light(),
        frame_labels: false,
        ..Default::default()
    };
    let img = render_overlay(&p.scene(), &cam, &overlay, WHITE);
    assert_px(&img, 10, 35, [235, 235, 235], 1);
    assert_px(&img, 15, 35, [255, 255, 255], 0);
    assert_px(&img, 35, 25, [255, 0, 0], 0);
    // Ниже 800% сетки нет.
    cam.set_zoom(4.0);
    let plain = render_overlay(&p.scene(), &cam, &overlay, WHITE);
    assert!(plain.data().iter().all(|c| c.r == 255 || c.g == 0));
    golden("pixel_grid", &img);
}

#[test]
fn frame_labels() {
    let mut p = Page::new();
    p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Главный экран".into()),
            ..rect(20.0, 40.0, 120.0, 60.0)
        },
    );
    p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Очень длинное название фрейма".into()),
            ..rect(160.0, 40.0, 60.0, 60.0)
        },
    );
    let bg = Rgba::from_rgba8(0x1e, 0x1e, 0x1e, 255);
    let overlay = Overlay {
        pixel_grid: false,
        ..Default::default()
    };
    let img = render_overlay(&p.scene(), &camera(260.0, 120.0), &overlay, bg);
    let lit = |x0: u16, x1: u16| {
        (x0..x1)
            .flat_map(|x| (24..38).map(move |y| (x, y)))
            .filter(|&(x, y)| px(&img, x, y)[0] > 0x40)
            .count()
    };
    assert!(lit(20, 140) > 40, "подпись первого фрейма");
    assert!(lit(160, 221) > 20, "усечённая подпись второго фрейма");
    assert_eq!(lit(221, 260), 0, "подпись не выходит за ширину фрейма");
    golden("frame_labels", &img);
}

#[test]
fn rotation() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            rotation: Some(30.0),
            fills: solid(220, 220, 220, 255),
            ..rect(60.0, 60.0, 80.0, 60.0)
        },
    );
    p.add(
        Some(f),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 140, 0, 255),
            ..rect(10.0, 10.0, 90.0, 20.0)
        },
    );
    p.add(
        None,
        NodeKind::Ellipse,
        PropsPatch {
            rotation: Some(-45.0),
            fills: solid(0, 170, 80, 255),
            ..rect(20.0, 130.0, 60.0, 24.0)
        },
    );
    let img = render(&p.scene(), &camera(180.0, 180.0));
    // Точка внутри повёрнутого прямоугольника: (30, 20) в координатах фрейма.
    let t = kurbo::Affine::translate((60.0, 60.0)) * kurbo::Affine::rotate(-30f64.to_radians());
    let inside = t * Point::new(30.0, 20.0);
    assert_px(&img, inside.x as u16, inside.y as u16, [255, 140, 0], 0);
    golden("rotation", &img);
}

#[test]
fn far_coordinates_render_like_origin() {
    let draw = |origin: f64| {
        let mut p = Page::new();
        p.add(
            None,
            NodeKind::Rect,
            PropsPatch {
                corner_radius: Some(6.0),
                fills: solid(255, 0, 0, 255),
                strokes: stroke(1.0, StrokeAlign::Center),
                ..rect(origin + 10.0, origin + 10.0, 40.0, 30.0)
            },
        );
        let mut cam = Camera::new(Size::new(240.0, 160.0), 1.0);
        cam.center = Point::new(origin + 30.0, origin + 25.0);
        cam.set_zoom(4.0);
        render(&p.scene(), &cam)
    };
    let near = draw(0.0);
    let far = draw(1.0e7);
    assert!(
        near.data() == far.data(),
        "на 1e7 рисуется так же, как у начала координат"
    );
}
````

- [ ] **Step 2: Убедиться, что без эталонов тесты падают**

Run: `cargo test -p ferrum-render --test golden`
Expected: FAIL — 9 тестов падают с «нет эталона …», `far_coordinates_render_like_origin` проходит (у него нет эталона, только сравнение двух картинок).

- [ ] **Step 3: Создать эталоны и просмотреть их**

Выполнить:

```bash
FERRUM_UPDATE_GOLDEN=1 cargo test -p ferrum-render --test golden
ls crates/ferrum-render/tests/golden/*.png | wc -l
```

Ожидаемый вывод: тесты проходят, файлов — 9. Откройте каждый PNG и сверьте с описанием в тесте: красный скруглённый прямоугольник, синий эллипс и чёрная линия; четыре квадрата с обводкой внутри, по центру, снаружи и кольцом поверх полупрозрачной заливки; левый фрейм обрезает зелёный квадрат, правый нет; перекрытие в группе с прозрачностью не темнее остального, у отдельных листьев — темнее; «заполнить», «вписать» (полосы фона сверху и снизу) и «плитка»; фрейм «Экран» с выделенным прямоугольником, ручками и бейджем «60 × 40», голубой обводкой наведения у круга, рамкой выделения мышью и красной линией привязки; сетка поверх красного прямоугольника; подписи «Главный экран» и «Очень дл…» на тёмном фоне; повёрнутый на 30° фрейм с обрезанной оранжевой полосой и зелёный эллипс, повёрнутый по часовой стрелке.

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p ferrum-render --test golden`
Expected: PASS, `10 passed`.

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-render/tests/golden.rs crates/ferrum-render/tests/golden
git commit -m "test(render): эталонные картинки через vello_cpu"
```

---

### Task 9: Бэкенд `vello_gpu` за фичей `gpu`

`vello_gpu` принимает изображения только загруженными в атлас, а атлас живёт в ресурсах рендерера, которому нужно GPU-устройство. Поэтому на этом этапе `GpuBackend` рисует вместо изображений заглушку и сообщает `supports_images() == false` — рендер тогда не использует растровый кэш. Загрузка изображений и растров в текстуры — этап 4.

**Files:**
- Create: `crates/ferrum-render/src/gpu.rs`
- Modify: `crates/ferrum-render/src/lib.rs`
- Test: `crates/ferrum-render/tests/gpu.rs`

**Interfaces:**
- Consumes: `RenderBackend`, `Renderer`.
- Produces: `GpuBackend { scene: vello_gpu::Scene }` с `new`, `reset` (только с фичей `gpu`).

- [ ] **Step 1: Написать падающий тест**

`crates/ferrum-render/tests/gpu.rs`:

````rust
//! Бэкенд `vello_gpu` собирается и принимает тот же кадр, что и CPU.
//! Запуск: `cargo test -p ferrum-render --features gpu --test gpu`.
#![cfg(feature = "gpu")]

mod common;

use common::*;
use ferrum_doc::{Fill, NodeKind, PropsPatch};
use ferrum_render::{Camera, GpuBackend, NoImages, Overlay, RenderBackend, Renderer};
use kurbo::{Point, Size};

#[test]
fn frame_encodes_into_vello_gpu_scene() {
    let mut p = Page::new();
    let f = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            name: Some("Экран".into()),
            opacity: Some(0.5),
            rotation: Some(10.0),
            ..rect(10.0, 10.0, 200.0, 150.0)
        },
    );
    let kid = p.add(Some(f), NodeKind::Ellipse, rect(150.0, 100.0, 100.0, 80.0));
    p.add(
        Some(f),
        NodeKind::Image,
        PropsPatch {
            fills: Some(vec![Fill::image("нет")]),
            ..rect(10.0, 10.0, 50.0, 50.0)
        },
    );
    let scene = p.scene();
    let mut cam = Camera::new(Size::new(320.0, 240.0), 2.0);
    cam.center = Point::new(110.0, 90.0);
    let overlay = Overlay {
        selection: vec![kid],
        hover: Some(f),
        ..Default::default()
    };
    let (w, h) = cam.device_size();
    let mut backend = GpuBackend::new(w, h);
    assert!(!backend.supports_images());
    let stats =
        Renderer::new().render(&mut backend, &scene, &cam, &overlay, &NoImages, Some(WHITE));
    // Слой прозрачности и обрезка повёрнутого фрейма закодированы и закрыты.
    assert_eq!((stats.layers, stats.clips, stats.drawn), (1, 1, 3));
    assert_eq!(backend.size(), (640, 480));
    // Сильное отдаление: растрового кэша у GPU-бэкенда пока нет — узлы рисуются LOD.
    cam.set_zoom(0.02);
    backend.reset(w, h);
    let stats = Renderer::new().render(&mut backend, &scene, &cam, &overlay, &NoImages, None);
    assert_eq!(stats.rasters, 0);
}
````

- [ ] **Step 2: Убедиться, что тест не компилируется**

Run: `cargo test -p ferrum-render --features gpu --test gpu`
Expected: ошибка компиляции — нет `GpuBackend`.

- [ ] **Step 3: Реализовать бэкенд**

`crates/ferrum-render/src/gpu.rs`:

````rust
//! Бэкенд `vello_gpu`: кадр кодируется в `vello_gpu::Scene` тем же кодом, что и
//! для CPU. Поверхность, устройство wgpu/WebGL и растеризация — этап 4.

use kurbo::{Affine, BezPath, Rect, Stroke};
use vello_cpu::peniko::Fill;

use crate::backend::{ImagePaint, RenderBackend, Rgba};

/// Цвет-заглушка вместо изображений. `vello_gpu` принимает картинки только
/// загруженными в атлас (`ImageSource::OpaqueId`), а атлас живёт в `Resources`
/// рендерера, которому нужно GPU-устройство. Загрузка изображений — этап 4.
const IMAGE_PLACEHOLDER: Rgba = Rgba::new([0.9, 0.9, 0.9, 1.0]);

pub struct GpuBackend {
    pub scene: vello_gpu::Scene,
}

impl GpuBackend {
    pub fn new(width: u16, height: u16) -> GpuBackend {
        GpuBackend {
            scene: vello_gpu::Scene::new(width, height),
        }
    }

    pub fn reset(&mut self, width: u16, height: u16) {
        self.scene.reset_and_resize(width, height);
    }
}

impl RenderBackend for GpuBackend {
    fn size(&self) -> (u16, u16) {
        (self.scene.width(), self.scene.height())
    }

    fn set_transform(&mut self, transform: Affine) {
        self.scene.set_transform(transform);
    }

    fn set_color(&mut self, color: Rgba) {
        self.scene.reset_paint_transform();
        self.scene.set_paint(color);
    }

    fn set_image(&mut self, image: &ImagePaint) {
        self.set_color(IMAGE_PLACEHOLDER.multiply_alpha(image.alpha));
    }

    fn fill_path(&mut self, path: &BezPath, rule: Fill) {
        self.scene.set_fill_rule(rule);
        self.scene.fill_path(path);
    }

    fn fill_rect(&mut self, rect: &Rect) {
        self.scene.fill_rect(rect);
    }

    fn stroke_path(&mut self, path: &BezPath, stroke: &Stroke) {
        self.scene.set_stroke(stroke.clone());
        self.scene.stroke_path(path);
    }

    fn push_clip(&mut self, path: &BezPath) {
        self.scene.set_fill_rule(Fill::NonZero);
        self.scene.push_clip_path(path);
    }

    fn pop_clip(&mut self) {
        self.scene.pop_clip();
    }

    fn push_opacity_layer(&mut self, alpha: f32) {
        self.scene.push_opacity_layer(alpha);
    }

    fn pop_layer(&mut self) {
        self.scene.pop_layer();
    }

    fn supports_images(&self) -> bool {
        false
    }
}
````

`crates/ferrum-render/src/lib.rs`:

````rust
//! Рендер Ferrum: камера, сборка кадра со списками отрисовки, отсечением и LOD,
//! оверлеи, экспорт PNG. Получает сцену, камеру и описание оверлеев; о Loro и
//! инструментах не знает.

mod backend;
mod camera;
mod display;
mod draw;
#[cfg(feature = "gpu")]
mod gpu;
mod image;
mod overlay;
mod text;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::Scene;
use kurbo::{Affine, Rect};

pub use backend::{CpuBackend, ImagePaint, RenderBackend, Rgba};
pub use camera::{Camera, MAX_ZOOM, MIN_ZOOM};
pub use draw::{
    FrameStats, EVICT_AFTER_FRAMES, LOD_RECT_PX, LOD_SKIP_PX, RASTER_BUILDS_PER_FRAME,
    RASTER_MAX_PX, RASTER_MAX_SCALE,
};
#[cfg(feature = "gpu")]
pub use gpu::GpuBackend;
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
pub use overlay::{
    format_size, Overlay, OverlayTheme, HANDLE_SIZE, LABEL_SIZE, PIXEL_GRID_MIN_ZOOM,
};
pub use text::{Label, TextEngine, ONEST};
pub use vello_cpu::Pixmap;

use draw::{DrawContext, Painter};

/// Наибольшая сторона экспортируемой картинки в пикселях.
pub const MAX_EXPORT_SIDE: u32 = 16384;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ExportError {
    #[error("Узел для экспорта не найден")]
    NotFound,
    #[error("Масштаб экспорта должен быть положительным")]
    BadScale,
    #[error("Нечего экспортировать: у узла нулевой размер или он скрыт")]
    Empty,
    #[error(
        "Картинка {0}×{1} слишком большая: сторона не больше 16384 пикселей. Уменьшите масштаб"
    )]
    TooLarge(u32, u32),
    #[error("Не удалось закодировать PNG: {0}")]
    Encode(String),
}

/// Рендерер одного холста: кэш списков отрисовки узлов и шрифтов живёт между
/// кадрами.
#[derive(Default)]
pub struct Renderer {
    painter: Painter,
    text: TextEngine,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer::default()
    }

    /// Рисует кадр: фон, содержимое, оверлеи.
    pub fn render(
        &mut self,
        backend: &mut impl RenderBackend,
        scene: &Scene,
        camera: &Camera,
        overlay: &Overlay,
        images: &dyn ImageProvider,
        background: Option<Rgba>,
    ) -> FrameStats {
        if let Some(bg) = background {
            let (w, h) = backend.size();
            backend.set_transform(Affine::IDENTITY);
            backend.set_color(bg);
            backend.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
        }
        let stats = self.draw_scene(backend, scene, camera, images);
        overlay::draw(backend, &mut self.text, scene, camera, overlay);
        stats
    }

    /// Только содержимое сцены, без фона и оверлеев.
    pub fn draw_scene(
        &mut self,
        backend: &mut impl RenderBackend,
        scene: &Scene,
        camera: &Camera,
        images: &dyn ImageProvider,
    ) -> FrameStats {
        self.painter.begin_frame();
        let ctx = DrawContext {
            scene,
            images,
            transform: camera.view_transform(),
            scale: camera.device_scale(),
            cull: Some(camera.visible_world_rect()),
            lod: true,
            raster: backend.supports_images(),
            retain: true,
        };
        self.painter.draw(backend, &ctx, scene.roots());
        self.painter.stats
    }

    /// Экспорт узла в PNG в масштабе `scale` (1, 2, 3…). Область — форма фрейма
    /// (как в Figma: всё за его краем обрезается) или границы узла с обводкой и
    /// потомками для остальных. Размер — пиксель в пиксель: фрейм 393×852 при @3x
    /// даёт 1179×2556. Предки не влияют: их прозрачность и обрезка не применяются.
    pub fn export_png(
        &mut self,
        scene: &Scene,
        id: NodeId,
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Vec<u8>, ExportError> {
        let pixmap = self.export_pixmap(scene, id, scale, images)?;
        pixmap
            .into_png()
            .map_err(|e| ExportError::Encode(e.to_string()))
    }

    pub fn export_pixmap(
        &mut self,
        scene: &Scene,
        id: NodeId,
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Pixmap, ExportError> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(ExportError::BadScale);
        }
        let slot = scene.slot(id).ok_or(ExportError::NotFound)?;
        let n = scene.at(slot);
        let area = match (n.props.kind, n.local_bounds) {
            (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
            _ => match (n.world_bounds, n.subtree_bounds) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            },
        }
        .ok_or(ExportError::Empty)?;
        let w = (area.width() * scale).round();
        let h = (area.height() * scale).round();
        if w < 1.0 || h < 1.0 {
            return Err(ExportError::Empty);
        }
        if w > MAX_EXPORT_SIDE as f64 || h > MAX_EXPORT_SIDE as f64 {
            return Err(ExportError::TooLarge(w as u32, h as u32));
        }
        let mut backend = CpuBackend::new(w as u16, h as u16);
        let ctx = DrawContext {
            scene,
            images,
            transform: Affine::scale(scale) * Affine::translate(-area.origin().to_vec2()),
            scale,
            cull: None,
            lod: false,
            raster: false,
            retain: false,
        };
        self.painter.begin_frame();
        self.painter.draw(&mut backend, &ctx, &[slot]);
        Ok(backend.render())
    }
}
````

- [ ] **Step 4: Запустить тест и линтер с фичей**

Run: `cargo test -p ferrum-render --features gpu --test gpu`
Expected: PASS, `1 passed`. Сборка wgpu добавляет пару минут.

Run: `cargo clippy -p ferrum-render --all-targets --features gpu -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-render
git commit -m "feat(render): бэкенд vello_gpu за фичей gpu"
```

---

### Task 10: Контрольный бенчмарк и отчёт

**Files:**
- Create: `crates/ferrum-render/examples/render_report.rs`
- Create: `docs/benchmarks/2026-10-24-ferrum-render.md`

**Interfaces:**
- Consumes: всё выше; `ferrum_doc::fixture::fill`, `container::{encode, decode}`.
- Produces: —

Страница из 100 000 узлов (фреймы по 100: фрейм + 99 фигур) сохраняется в файл и открывается: сцена строится из кэша сцены. Замеры — медиана нескольких прогонов: один прогон на ноутбуке шумит на десятки процентов. Правки 1000 узлов берут фигуры почти из каждого фрейма страницы (худший случай для обновления предков), отдельно — 10 фреймов вместе с детьми. После всех правок и отмен сцена сверяется с построенной заново. Растровый кэш прогревается до замера всей страницы; число кадров прогрева и самый долгий из них печатаются.

- [ ] **Step 1: Добавить контрольный бенчмарк**

`crates/ferrum-render/examples/render_report.rs`:

````rust
//! Контрольный бенчмарк сцены и рендера:
//! `cargo run --release -p ferrum-render --example render_report`.
//! Сверяет замеры с целями этапа 2 и печатает таблицу PASS/FAIL (код выхода 1 при провале).

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ferrum_doc::container::{decode, encode};
use ferrum_doc::fixture::fill;
use ferrum_doc::{Document, NodeId, NodeKind, PageId, PropsPatch};
use ferrum_render::{Camera, CpuBackend, NoImages, Renderer};
use ferrum_scene::Scene;
use kurbo::{Point, Rect, Size};

/// Считает живые байты в куче — для памяти кэша отрисовки.
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

const NODES: usize = 100_000;
const VIEWPORT: Size = Size::new(1920.0, 1080.0);

/// Медиана нескольких замеров: один прогон на ноутбуке шумит на десятки процентов.
fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
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

    fn info(&mut self, metric: &str, value: String) {
        self.0.push(Row {
            metric: metric.to_string(),
            value,
            limit: "—",
            pass: None,
        });
    }
}

/// Применяет транзакцию и замеряет `Scene::apply` её изменений, затем отмены.
fn apply_and_undo(
    doc: &mut Document,
    page: PageId,
    scene: &mut Scene,
    edit: impl Fn(&ferrum_doc::Tx) -> ferrum_doc::DocResult<()>,
) -> (Duration, Duration) {
    doc.transact(page, vec![], |p| edit(p).map(|_| vec![]))
        .unwrap();
    let changes = doc.take_changes(page);
    let t = Instant::now();
    scene.apply(&changes, doc.page(page).unwrap());
    let forward = t.elapsed();
    doc.undo().unwrap();
    let changes = doc.take_changes(page);
    let t = Instant::now();
    scene.apply(&changes, doc.page(page).unwrap());
    (forward, t.elapsed())
}

/// Кадр после прогрева кэша: медиана нескольких сборок в бэкенд без растеризации.
fn frame(
    renderer: &mut Renderer,
    backend: &mut CpuBackend,
    scene: &Scene,
    camera: &Camera,
) -> (Duration, ferrum_render::FrameStats) {
    let (w, h) = camera.device_size();
    let mut stats = Default::default();
    let runs = (0..7)
        .map(|_| {
            backend.reset(w, h);
            let t = Instant::now();
            stats = renderer.draw_scene(backend, scene, camera, &NoImages);
            t.elapsed()
        })
        .collect();
    (median(runs), stats)
}

fn main() {
    let mut report = Report(Vec::new());

    // Документ из 100 000 узлов (фреймы по 100: фрейм + 99 фигур), сохранённый в файл.
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids: Vec<NodeId> = doc.transact(page, vec![], |p| fill(p, NODES)).unwrap();
    let bytes = encode(&doc).unwrap();
    let mut doc = decode(&bytes).unwrap().document;
    let rows = doc.cached_scene(page).unwrap().to_vec();
    assert_eq!(rows.len(), NODES);

    let build = median(
        (0..5)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(Scene::from_rows(&rows));
                t.elapsed()
            })
            .collect(),
    );
    report.gate(
        "Построение сцены из кэша сцены (100 000)",
        build,
        100.0,
        "≤ 100 мс",
    );
    let mut scene = Scene::from_rows(&rows);
    // Фоновая загрузка Loro (этап 1) до правок: она не входит в замеры сцены.
    assert_eq!(doc.page(page).unwrap().walk().len(), NODES);

    // 1000 фигур, разбросанных по всей странице (по одной почти из каждого фрейма).
    // Фреймы — каждый сотый узел генератора; их сдвиг двигает и 99 детей.
    let spread: Vec<NodeId> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 100 != 0)
        .map(|(_, id)| *id)
        .step_by(99)
        .take(1000)
        .collect();
    assert_eq!(spread.len(), 1000);
    let frames: Vec<NodeId> = ids.iter().step_by(100).take(10).copied().collect();
    let (props, undo_props) = apply_and_undo(&mut doc, page, &mut scene, |p| {
        for (i, id) in spread.iter().enumerate() {
            p.set_props(
                *id,
                &PropsPatch {
                    x: Some(i as f64),
                    y: Some(7.0),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    });
    report.gate("Scene::apply: сдвиг 1000 узлов", props, 5.0, "≤ 5 мс");
    report.gate(
        "Scene::apply: undo сдвига 1000 узлов",
        undo_props,
        5.0,
        "≤ 5 мс",
    );

    let (frames_moved, undo_frames) = apply_and_undo(&mut doc, page, &mut scene, |p| {
        for id in &frames {
            p.set_props(
                *id,
                &PropsPatch {
                    x: Some(-5000.0),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    });
    report.gate(
        "Scene::apply: сдвиг 10 фреймов (1000 узлов с детьми)",
        frames_moved,
        5.0,
        "≤ 5 мс",
    );
    report.gate(
        "Scene::apply: undo сдвига 10 фреймов",
        undo_frames,
        5.0,
        "≤ 5 мс",
    );

    let (group, undo_group) = apply_and_undo(&mut doc, page, &mut scene, |p| {
        let frame = p.create_node(None, None, NodeKind::Frame)?;
        p.move_nodes(&spread, Some(frame), 0)
    });
    report.gate("Scene::apply: 1000 узлов во фрейм", group, 5.0, "≤ 5 мс");
    report.gate(
        "Scene::apply: undo переноса 1000 узлов",
        undo_group,
        5.0,
        "≤ 5 мс",
    );

    let (delete, undo_delete) =
        apply_and_undo(&mut doc, page, &mut scene, |p| p.delete_nodes(&spread));
    report.gate("Scene::apply: удаление 1000 узлов", delete, 5.0, "≤ 5 мс");
    report.gate(
        "Scene::apply: undo удаления 1000 узлов",
        undo_delete,
        5.0,
        "≤ 5 мс",
    );

    // Превью перетаскивания 1000 узлов: каждый кадр жеста — новое смещение.
    let preview = median(
        (0..7)
            .map(|k| {
                let patches: Vec<(NodeId, PropsPatch)> = spread
                    .iter()
                    .map(|id| {
                        (
                            *id,
                            PropsPatch {
                                x: Some(k as f64 * 3.0),
                                ..Default::default()
                            },
                        )
                    })
                    .collect();
                let t = Instant::now();
                scene.set_preview(&patches);
                t.elapsed()
            })
            .collect(),
    );
    scene.clear_preview();
    report.gate(
        "Превью перетаскивания 1000 узлов (кадр)",
        preview,
        5.0,
        "≤ 5 мс",
    );
    assert_eq!(
        ferrum_scene::testing::snapshot(&scene),
        ferrum_scene::testing::snapshot(&Scene::from_page(doc.page(page).unwrap())),
        "сцена после правок и отмен совпадает с построенной заново"
    );

    // Hit-test: точки по всей странице, допуск 4 экранных пикселя при зуме 1.
    let bounds = scene.bounds().unwrap();
    let points: Vec<Point> = (0..10_000u64)
        .map(|i| {
            let fx = (i.wrapping_mul(7919) % 10_007) as f64 / 10_007.0;
            let fy = (i.wrapping_mul(104_729) % 10_009) as f64 / 10_009.0;
            Point::new(
                bounds.x0 + fx * bounds.width(),
                bounds.y0 + fy * bounds.height(),
            )
        })
        .collect();
    let t = Instant::now();
    let hits = points
        .iter()
        .filter(|p| scene.hit_test(**p, 4.0).is_some())
        .count();
    let hit = t.elapsed() / points.len() as u32;
    assert!(hits > points.len() / 2, "точки попадают в узлы: {hits}");
    report.gate("Hit-test (среднее)", hit, 0.1, "≤ 0.1 мс");

    let mut renderer = Renderer::new();

    // Типичный вьюпорт: около 2000 видимых узлов.
    let mut camera = Camera::new(VIEWPORT, 1.0);
    camera.set_zoom(0.25);
    camera.center = Point::new(3500.0, 2000.0);
    let (w, h) = camera.device_size();
    let mut backend = CpuBackend::new(w, h);
    let before = heap_mb();
    let t = Instant::now();
    renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
    let cold = t.elapsed();
    let (typical, stats) = frame(&mut renderer, &mut backend, &scene, &camera);
    let visible = scene.query_rect(camera.visible_world_rect());
    let t = Instant::now();
    let query = scene.query_rect(camera.visible_world_rect()).len();
    let query_time = t.elapsed();
    assert_eq!(query, visible.len());
    report.info(
        "Типичный вьюпорт: видимых узлов (R-tree / нарисовано)",
        format!("{} / {}", visible.len(), stats.drawn + stats.lod_rects),
    );
    report.info("Типичный вьюпорт: запрос R-tree", ms(query_time));
    report.info("Типичный вьюпорт: первый кадр (списки строятся)", ms(cold));
    report.gate(
        "Типичный вьюпорт: отсечение + списки в бэкенд",
        typical,
        4.0,
        "≤ 4 мс",
    );
    let t = Instant::now();
    std::hint::black_box(backend.render());
    report.info(
        "Типичный вьюпорт: растеризация CPU 1920×1080",
        ms(t.elapsed()),
    );

    // Вся страница при сильном отдалении: LOD и растровый кэш фреймов. Кэш
    // строится по `RASTER_BUILDS_PER_FRAME` фреймов за кадр; замер — после прогрева.
    camera.fit(scene.bounds().unwrap(), 0.0);
    let mut warmup = Vec::new();
    loop {
        backend.reset(w, h);
        let t = Instant::now();
        let s = renderer.draw_scene(&mut backend, &scene, &camera, &NoImages);
        warmup.push(t.elapsed());
        if s.raster_builds == 0 {
            break;
        }
    }
    report.info(
        "Вся страница: прогрев кэша (кадров / самый долгий кадр)",
        format!(
            "{} / {}",
            warmup.len() - 1,
            ms(*warmup.iter().max().unwrap())
        ),
    );
    let (whole, stats) = frame(&mut renderer, &mut backend, &scene, &camera);
    report.info(
        "Вся страница: зум / из растрового кэша / узлами / LOD-прямоугольников",
        format!(
            "{:.3} / {} / {} / {}",
            camera.zoom(),
            stats.rasters,
            stats.drawn,
            stats.lod_rects
        ),
    );
    report.gate("Вся страница с LOD: сборка кадра", whole, 8.0, "≤ 8 мс");
    let t = Instant::now();
    std::hint::black_box(backend.render());
    report.info("Вся страница: растеризация CPU 1920×1080", ms(t.elapsed()));
    report.info(
        "Память кэшей рендера (списки и контуры видимых узлов, растры фреймов)",
        format!("{:.1} МБ", heap_mb() - before),
    );

    // Экспорт фрейма @3x — справочно.
    let first_frame = scene.at(scene.roots()[0]).id;
    let t = Instant::now();
    let png = renderer
        .export_png(&scene, first_frame, 3.0, &NoImages)
        .unwrap();
    report.info(
        "Экспорт фрейма 1440×1024 @3x в PNG",
        format!("{} ({} КБ)", ms(t.elapsed()), png.len() / 1024),
    );
    let _ = Rect::ZERO;

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

- [ ] **Step 2: Проверить форматирование, линтер и все тесты**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: без ошибок и предупреждений; `ferrum-doc` — модульные 21, `changes` 5, `container` 20, `document` 11, `invariants` 1, `page` 10, `types` 5; `ferrum-scene` — `shape` 8, `scene` 13, `apply_props` 1; `ferrum-render` — `camera` 6, `render` 9, `overlay` 4, `golden` 10.

- [ ] **Step 3: Запустить контрольные бенчмарки**

Run: `set -o pipefail; cargo run --release -p ferrum-render --example render_report | tee /tmp/ferrum-render-bench.md`
Expected: код выхода 0, все строки с целью — `PASS`. Ориентиры прототипа (Apple Silicon, 10 ядер): построение сцены ~37 мс; `apply` 1000 узлов 0.9–2.4 мс; превью 1000 узлов ~1.1 мс; hit-test ~0.5 мкс; типичный вьюпорт (1700 узлов) ~2.8 мс сборки и ~2.1 мс растеризации; вся страница — 32 кадра прогрева (самый долгий ~16 мс), затем ~0.2 мс сборки и ~4.5 мс растеризации.

Run: `cargo run --release -p ferrum-doc --example bench_report`
Expected: код выхода 0 — изменения Task 1 не замедлили модель документа.

Если какая-то строка `FAIL`: не меняйте цель. Сравните значение с ориентиром, найдите регрессию и исправьте код.

- [ ] **Step 4: Записать отчёт**

Создать `docs/benchmarks/2026-10-24-ferrum-render.md`: заголовок `# Бенчмарк ferrum-scene и ferrum-render`, строка с машиной (`sysctl -n machdep.cpu.brand_string` на macOS) и версией Rust (`rustc --version`), затем таблица из `/tmp/ferrum-render-bench.md` без изменений.

Выполнить:

```bash
mkdir -p docs/benchmarks
{
  echo "# Бенчмарк ferrum-scene и ferrum-render"
  echo
  echo "Машина: $(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m), $(rustc --version)."
  echo
  cat /tmp/ferrum-render-bench.md
} > docs/benchmarks/2026-10-24-ferrum-render.md
```

- [ ] **Step 5: Закоммитить**

```bash
git add crates/ferrum-render/examples/render_report.rs docs/benchmarks/2026-10-24-ferrum-render.md
git commit -m "bench(render): контрольный бенчмарк сцены и рендера и отчёт"
```

---

## Что этап 2 сознательно не делает

- Растеризация на GPU, поверхности, OffscreenCanvas, выбор WebGPU → WebGL2 → CPU, загрузка изображений и растров фреймов в текстуры `vello_gpu` — этап 4. Сейчас `GpuBackend` только кодирует кадр, а изображения у него — заглушка.
- Декодирование изображений кроме PNG (JPEG, WebP; `createImageBitmap` в воркере), уменьшенные копии под зум и бюджет памяти изображений с вытеснением — этап 4. Рендер получает готовые `Pixmap` от поставщика.
- Многопоточная растеризация `vello_cpu` и WASM-потоки — по результатам бенчмарка этапа 4.
- Правила выделения (верхний уровень во фрейме, двойной клик вглубь, заблокированные узлы), привязки и расстояния между объектами — этап 3; сцена отдаёт все узлы под точкой, верхний первым, вместе с заблокированными.
- Превью меняет только свойства узлов; перенос в другой фрейм во время перетаскивания показывается после отпускания (одной транзакцией).
- Моноширинный шрифт бейджа размера: бейдж набран Onest с цифрами одинаковой ширины; второй шрифт добавляется вместе с панелями интерфейса.
- Хинтинг глифов и кэш глифов в атласе: подписи — это десятки глифов на кадр, контуры кэшируются в единицах шрифта.
- Сцена из кэша сцены, который `PageDoc::load` потом отверг (хэш совпал, а структура нет), перестраивается вызывающим кодом через `Scene::from_page` после загрузки — это делает редактор на этапе 3.
- Повёрнутые подписи фреймов и ручки поворота за углами — этап 3 (инструмент «Перемещение»).
