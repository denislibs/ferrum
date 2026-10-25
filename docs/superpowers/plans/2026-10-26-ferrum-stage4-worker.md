# Ferrum, подсистема 1, этап 4: worker и обмен данными — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ядро Ferrum работает в браузере: крейт `ferrum-wasm` (wasm-bindgen, `wasm32-unknown-unknown`, один поток) в выделенном Web Worker с OffscreenCanvas владеет редактором, документом, сценой и рендером; основной поток только пересылает ввод и команды через `Transport` (SharedArrayBuffer + postMessage на изолированной странице, только postMessage — без изоляции) и читает живое состояние. Рендер выбирается при старте: `vello_gpu` на WebGPU → `vello_gpu` на WebGL2 → `vello_cpu` с `putImageData`; картинки документа и растры фреймов живут в атласе GPU по ключу. Стенд `apps/web` (Vite + TypeScript) показывает холст на всё окно с отладочной плашкой; E2E-тесты Playwright проверяют транспорт, бэкенды (пиксели GPU сверяются с `vello_cpu`), ввод, отмену, ресайз и бюджет основного потока на странице из 100 000 узлов.

**Architecture:** Основной поток кладёт события ввода записями по 64 байта в кольцевой буфер общей памяти, в `requestAnimationFrame` увеличивает счётчик кадров и будит worker `Atomics.notify`, а из блока живого состояния (seqlock) читает выделение, камеру и статистику кадра. Worker ждёт кадр в `Atomics.waitAsync`, забирает накопленные записи, отдаёт их `Core` (ядро без браузера: редактор, рендерер, картинки документа), рисует кадр, только если изменились сцена, камера или оверлей или прошлый кадр неполный, и отправляет события. Без изоляции тот же формат записей уходит в сообщении кадра, а живое состояние — в ответном сообщении. Команды и события — сообщения, типы описаны в Rust (`protocol.rs`) и сгенерированы `tsify`; байты передаются transferable-буферами. `WasmCore` (только wasm32) держит `Core` и поверхность: WebGPU (wgpu 30), WebGL2 (`WebGlRenderer` vello_gpu на OffscreenCanvas) или CPU. Атлас изображений `vello_gpu` ведётся зеркальным кэшем выделений (`vello_common::image_cache`) с той же конфигурацией: зеркало заранее знает, поместится ли картинка, — wgpu-рендерер vello при нехватке места паникует.

**Tech Stack:** Rust 1.96 (`wasm32-unknown-unknown`, SIMD128), wasm-bindgen 0.2.129 (и `wasm-bindgen-cli` той же версии), js-sys/web-sys 0.3.106, wasm-bindgen-futures 0.4.79, wgpu 30.0.1 (`webgpu`), vello_gpu 0.3.0 (`wgpu`, `webgl`), vello_common 0.3.0, tsify 0.5.8 (`js`), serde-wasm-bindgen 0.6.5, serde_bytes 0.11.19, console_error_panic_hook 0.1.7, wasm-bindgen-test 0.3.79; Node 24, Vite 8.3.4, TypeScript 6.0.3, @playwright/test 1.64.0 (Chromium 156 headless shell), @types/node 24.19.1.

**Спека:** `docs/superpowers/specs/2026-10-23-ferrum-core-design.md`, разделы 3, 4.1–4.3, 8.1–8.2, 8.5, 11, 12.

## Global Constraints

- Rust toolchain `1.96.0` с целью `wasm32-unknown-unknown` и компонентом `llvm-tools` (ставит `rust-toolchain.toml` из Task 1). Зависимости Rust — точными версиями (`=x.y.z`), опубликованными не позже 2026-10-10 (≥ 14 дней до начала этапа). Новые записи `Cargo.lock` этапа — только `ferrum-wasm` и: `async-trait 0.1.92`, `console_error_panic_hook 0.1.7`, `minicov 0.3.8`, `serde-wasm-bindgen 0.6.5`, `serde_bytes 0.11.19`, `serde_derive_internals 0.29.1`, `termcolor 1.4.1`, `tsify 0.5.8`, `tsify-macros 0.5.8`, `wasm-bindgen-test 0.3.79`, `wasm-bindgen-test-macro 0.3.79`, `wasm-bindgen-test-shared 0.2.129` (остальное — wasm-bindgen, wgpu, vello_gpu — уже было в lock). Если cargo выбрал другую версию, вернуть её: `cargo update -p <крейт> --precise <версия>`.
- npm-пакеты — точными версиями; `apps/web/.npmrc` задаёт `save-exact=true` и `before=2026-10-10`, поэтому и зависимости зависимостей не новее этой даты; `package-lock.json` коммитится.
- Основной поток тонкий (спека 4.2): ничего, что зависит от размера документа. Обработчик события ввода — O(1): переложить поля события в запись. Бюджет — ≤ 4 мс за кадр на все обработчики и кадр основного потока; длинных задач (`longtask` > 50 мс) нет ни при работе, ни при открытии и сохранении файла на 100 000 узлов.
- Всё, что пересекает границу потоков, — не доверенное: записи кольцевого буфера, счётчики общей памяти, команды. Разбор не паникует; непонятная запись пропускается и считается, неверная команда даёт событие `error`, неудачное открытие файла оставляет текущий документ.
- Кадр рисуется, только если изменились сцена, камера или оверлей (`EditorEvent::NeedsRedraw`) или `FrameStats::needs_another_frame`. Время рендера в wasm — только через трейт `Clock` (`performance.now()`): `std::time::Instant` в wasm32 паникует.
- Программный GPU (SwiftShader, запасной адаптер WebGPU) при автоматическом выборе не берётся — он медленнее `vello_cpu`; явно запрошенный бэкенд (`?backend=webgl`) берётся и программным.
- Тесты не проходят вхолостую: пиксели GPU читаются и сверяются с `vello_cpu`, у бэкенда проверяется, что картинка пришла из атласа, у перфтеста — что зум дошёл до всей страницы и кадры рисовались.
- Тексты ошибок и событий — на русском, для пользователя. Комментарии — на русском, объясняют «почему».
- Каталог сборки (`CARGO_TARGET_DIR`) у каждой копии репозитория свой; скрипт `apps/web/scripts/build-wasm.mjs` его учитывает.
- Цели (100 000 узлов, 1280 × 720, dpr 1, headless Chromium на Apple M1 Pro): обработчики основного потока ≤ 4 мс за кадр, длинных задач 0; p95 кадра worker'а ≤ 16,7 мс для `vello_cpu` и WebGL2 на настоящем GPU (WebGPU — справочно: исполнение на GPU асинхронное и во время кадра не входит).

---

## Этапы подсистемы 1

Подсистема 1 делится на шесть этапов (таблица — в плане этапа 1, `docs/superpowers/plans/2026-10-23-ferrum-stage1-document-model.md`). Этот документ — план этапа 4:

| Этап | Результат | Разделы спеки |
|---|---|---|
| 1. Модель документа (готов) | `ferrum-doc` | 3, 5 |
| 2. Сцена и рендер (готов) | `ferrum-scene`, `ferrum-render` | 6, 8 |
| 3. Редактор без браузера (готов) | `ferrum-editor` | 7 |
| **4. Worker и обмен данными** (этот план) | `ferrum-wasm`, Web Worker + OffscreenCanvas, `Transport` (SAB + postMessage, запасной режим), цикл кадров, запасные рендеры; холст в браузере | 4.2–4.3, 8.1–8.2 |
| 5. Интерфейс | `apps/web` на SolidJS поверх `Core` | 4.4, 9 |
| 6. Файлы и десктоп | `FileStore`, воркер ввода-вывода, автосохранение, Tauri, E2E | 10, 11, 12 |

---

## Структура файлов этапа 4

| Файл | Ответственность |
|---|---|
| `rust-toolchain.toml`, `.cargo/config.toml`, `scripts/llvm-ar` | цель wasm32, llvm-ar для `psm`, SIMD128, запуск wasm-тестов в Node |
| `crates/ferrum-render/src/clock.rs` | трейт `Clock`, `StdClock`; часы рендера для предела времени растров |
| `crates/ferrum-render/src/{backend,draw}.rs` | `ImageKey` у картинок: хэш blake3 или номер построения растра |
| `crates/ferrum-render/src/gpu.rs` | `GpuBackend<D: GpuDevice>`: атлас изображений с зеркальным кэшем, вытеснение, уменьшение больших картинок |
| `crates/ferrum-render/src/lib.rs` | `set_clock`, экспорт нескольких узлов одной картинкой |
| `crates/ferrum-doc/src/document.rs` | `clear_history` — документ-пример без шага «создать всё» |
| `crates/ferrum-editor/src/editor.rs` | `selection_version` — для живого состояния |
| `crates/ferrum-wasm/src/layout.rs` | раскладка общей памяти, генерация `layout.gen.ts` |
| `crates/ferrum-wasm/src/input.rs` | записи кольцевого буфера: кодирование и разбор |
| `crates/ferrum-wasm/src/live.rs` | блок живого состояния |
| `crates/ferrum-wasm/src/protocol.rs` | команды и события (serde + tsify) |
| `crates/ferrum-wasm/src/images.rs` | картинки документа: декодирование с бюджетом, кэш с пределом |
| `crates/ferrum-wasm/src/core.rs` | `Core`: ввод, команды, события, решение о кадре, живое состояние |
| `crates/ferrum-wasm/src/web/` | `WasmCore`, поверхности WebGPU/WebGL2/CPU, `PerformanceClock` (только wasm32) |
| `crates/ferrum-wasm/tests/` | нативные тесты; `web.rs` — wasm-bindgen-test в Node |
| `apps/web/src/core/` | `Core` (TS), `Transport` (SAB и postMessage), кодирование ввода, сообщения, `layout.gen.ts` |
| `apps/web/src/worker/core.worker.ts` | worker: цикл кадров, кольцо, живое состояние, события |
| `apps/web/src/{main,hud}.ts`, `index.html` | стенд: холст на всё окно и плашка |
| `apps/web/e2e/` | Playwright: стенд, пиксели, производительность |

Все команды запускаются из корня репозитория. Перед Task 1 закоммитить нечего; ветка — `feat/stage4-worker`.

---

### Task 1: Часы рендера и сборка под wasm32

Растровый кэш фреймов (этап 2) ограничен по времени на кадр, а `std::time::Instant::now` в `wasm32-unknown-unknown` паникует. Сейчас в wasm предел просто выключен (`now()` возвращает `None`). Рендер получает часы трейтом `Clock`: нативно — `StdClock` (по умолчанию), в worker'е — `performance.now()` (Task 7). Без часов остаётся счётный предел `RASTER_BUILDS_PER_FRAME`. Трейт заодно делает предел проверяемым: в тесте часы «тикают» ровно на заданное время за каждое построение растра.

Сборка под wasm32 требует двух настроек. Цель и `llvm-tools` ставит `rust-toolchain.toml`. Крейт `psm` (его тянет `pest` → `loro-internal`) упаковывает готовый объектный файл wasm через `ar`; системный `ar` macOS не понимает объекты wasm, архив выходит без таблицы символов, и rustc падает с `LLVM error: section too large`. Поэтому `.cargo/config.toml` направляет `AR_wasm32_unknown_unknown` на `llvm-ar` из тулчейна (обёртка `scripts/llvm-ar`). На Linux системный `ar` справляется, обёртка тоже работает.

**Files:**
- Modify: `rust-toolchain.toml`
- Create: `.cargo/config.toml`
- Create: `scripts/llvm-ar`
- Create: `crates/ferrum-render/src/clock.rs`
- Modify: `crates/ferrum-render/src/draw.rs`
- Modify: `crates/ferrum-render/src/lib.rs`
- Test: `crates/ferrum-render/tests/clock.rs`

**Interfaces:**
- Consumes: `Painter` и `Renderer` этапа 2.
- Produces: `ferrum_render::{Clock, StdClock (не wasm32), default_clock}`; `Clock::now_ms(&self) -> f64`; `Renderer::set_clock(Option<Box<dyn Clock>>)`. Предел времени растров работает только при заданных часах.

- [ ] **Step 1: Цель wasm32 и llvm-ar**

`rust-toolchain.toml` (целиком):

````toml
[toolchain]
channel = "1.96.0"
components = ["rustfmt", "clippy", "llvm-tools"]
# Ядро в браузере — wasm32 (этап 4).
targets = ["wasm32-unknown-unknown"]
````

`.cargo/config.toml`:

````toml
# `psm` (его тянет pest → loro-internal) при сборке под wasm32 упаковывает готовый
# объектный файл через `ar`. Системный `ar` macOS не понимает объекты wasm (архив
# без таблицы символов, rustc падает с «section too large»), поэтому для wasm32
# берётся llvm-ar из компонента llvm-tools тулчейна (rust-toolchain.toml).
[env]
AR_wasm32_unknown_unknown = { value = "scripts/llvm-ar", relative = true }
````

`scripts/llvm-ar`:

````bash
#!/usr/bin/env bash
# llvm-ar из компонента llvm-tools текущего тулчейна (см. .cargo/config.toml).
set -euo pipefail
sysroot="$(rustc --print sysroot)"
host="$(rustc -vV | sed -n 's/^host: //p')"
exec "$sysroot/lib/rustlib/$host/bin/llvm-ar" "$@"
````

Run: `chmod +x scripts/llvm-ar`
Expected: без вывода.

Run: `cargo build --target wasm32-unknown-unknown -p ferrum-editor`
Expected: rustup ставит цель и llvm-tools, сборка проходит (`Finished`). Без `.cargo/config.toml` на macOS — `LLVM error: section too large` в `psm`.

- [ ] **Step 2: Написать падающий тест часов**

`crates/ferrum-render/tests/clock.rs`:

````rust
//! Часы рендера: предел времени на растры фреймов за кадр считается по часам,
//! которые дал вызывающий; без часов остаётся счётный предел.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use common::*;
use ferrum_doc::{Color, Fill, NodeKind, PropsPatch};
use ferrum_render::{
    default_clock, Camera, Clock, CpuBackend, NoImages, Renderer, StdClock, RASTER_BUILDS_PER_FRAME,
};
use kurbo::Size;

/// Часы, которые сдвигаются на `step` миллисекунд при каждом чтении: между
/// чтениями до и после построения растра проходит ровно `step`.
struct FakeClock {
    now: Rc<Cell<f64>>,
    step: f64,
}

impl Clock for FakeClock {
    fn now_ms(&self) -> f64 {
        let t = self.now.get();
        self.now.set(t + self.step);
        t
    }
}

/// 64 фрейма по 10 прямоугольников при сильном отдалении: все рисуются из растров.
fn zoomed_out_grid() -> (Page, Camera) {
    let mut p = Page::new();
    for i in 0..64 {
        let f = p.add(
            None,
            NodeKind::Frame,
            PropsPatch {
                fills: Some(vec![Fill::solid(Color::rgba(240, 240, 240, 255))]),
                ..rect((i % 8) as f64 * 220.0, (i / 8) as f64 * 220.0, 200.0, 200.0)
            },
        );
        for k in 0..10 {
            p.add(
                Some(f),
                NodeKind::Rect,
                PropsPatch {
                    fills: Some(vec![Fill::solid(Color::rgba(20, 120, 200, 255))]),
                    ..rect(10.0 + k as f64 * 18.0, 10.0, 12.0, 12.0)
                },
            );
        }
    }
    let mut cam = Camera::new(Size::new(400.0, 400.0), 1.0);
    let bounds = p.scene().bounds().unwrap();
    cam.fit(bounds, 10.0);
    (p, cam)
}

fn builds_per_frame(r: &mut Renderer, p: &mut Page, cam: &Camera, frames: usize) -> Vec<usize> {
    let scene = p.scene();
    let (w, h) = cam.device_size();
    (0..frames)
        .map(|_| {
            let mut b = CpuBackend::new(w, h);
            r.draw_scene(&mut b, &scene, cam, &NoImages).raster_builds
        })
        .collect()
}

#[test]
fn raster_budget_follows_the_given_clock() {
    let (mut p, cam) = zoomed_out_grid();
    let now = Rc::new(Cell::new(0.0));
    let mut r = Renderer::new();
    r.set_raster_build_time(Some(Duration::from_millis(4)));
    // Каждое построение «стоит» 1 мс: в кадр с пределом 4 мс входит ровно 4.
    r.set_clock(Some(Box::new(FakeClock {
        now: now.clone(),
        step: 1.0,
    })));
    let builds = builds_per_frame(&mut r, &mut p, &cam, 3);
    assert_eq!(builds, [4, 4, 4]);
    assert!(now.get() >= 12.0, "часы читались: {}", now.get());
}

#[test]
fn slow_clock_still_builds_one_raster_per_frame() {
    let (mut p, cam) = zoomed_out_grid();
    let mut r = Renderer::new();
    // Одно построение «стоит» 100 мс — больше предела, но кэш всё равно догоняет.
    r.set_clock(Some(Box::new(FakeClock {
        now: Rc::new(Cell::new(0.0)),
        step: 100.0,
    })));
    assert_eq!(builds_per_frame(&mut r, &mut p, &cam, 2), [1, 1]);
}

#[test]
fn without_clock_only_the_count_limit_applies() {
    let (mut p, cam) = zoomed_out_grid();
    let mut r = Renderer::new();
    r.set_raster_build_time(Some(Duration::from_millis(4)));
    r.set_clock(None);
    // Как в wasm32 без часов: 64 растра — по 32 за кадр, затем всё готово.
    let builds = builds_per_frame(&mut r, &mut p, &cam, 3);
    assert_eq!(
        builds,
        [RASTER_BUILDS_PER_FRAME, RASTER_BUILDS_PER_FRAME, 0]
    );
}

#[test]
fn std_clock_is_monotonic_and_default_on_native() {
    let c = StdClock::new();
    let a = c.now_ms();
    std::thread::sleep(Duration::from_millis(2));
    let b = c.now_ms();
    assert!(b - a >= 1.5, "{a} → {b}");
    assert!(default_clock().is_some());
}
````

Run: `cargo test -p ferrum-render --test clock`
Expected: FAIL — не компилируется: `unresolved imports` `default_clock`, `Clock`, `StdClock`.

- [ ] **Step 3: Трейт `Clock` в рендере**

Часы — поле `Painter`; построение растра читает их до и после. Без часов предел времени не действует (`can_build_raster`).

`crates/ferrum-render/src/clock.rs`:

````rust
//! Часы рендера. Растровый кэш фреймов ограничен по времени на кадр, а в
//! `wasm32-unknown-unknown` `std::time::Instant::now` паникует. Поэтому время даёт
//! тот, кто запускает рендер: нативно — `StdClock`, в worker'е — `performance.now()`
//! (крейт `ferrum-wasm`). Без часов остаётся только счётный предел растров за кадр.

/// Монотонные часы в миллисекундах.
pub trait Clock {
    fn now_ms(&self) -> f64;
}

/// Часы `std::time::Instant` — для нативных сборок (тесты, бенчмарки, десктоп).
#[cfg(not(target_arch = "wasm32"))]
pub struct StdClock(std::time::Instant);

#[cfg(not(target_arch = "wasm32"))]
impl StdClock {
    pub fn new() -> StdClock {
        StdClock(std::time::Instant::now())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for StdClock {
    fn default() -> StdClock {
        StdClock::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Clock for StdClock {
    fn now_ms(&self) -> f64 {
        self.0.elapsed().as_secs_f64() * 1000.0
    }
}

/// Часы по умолчанию: `StdClock` нативно, нет часов в wasm32 (их ставит
/// `Renderer::set_clock`).
pub fn default_clock() -> Option<Box<dyn Clock>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Some(Box::new(StdClock::new()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
use std::sync::Arc;
use std::time::{Duration, Instant};

````

на:

````rust
use std::sync::Arc;
use std::time::Duration;

````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
use crate::backend::{CpuBackend, ImagePaint, RenderBackend};
use crate::display::{self, DisplayList, Item, Paint, IMAGE_PLACEHOLDER};
````

на:

````rust
use crate::backend::{CpuBackend, ImagePaint, RenderBackend};
use crate::clock::{default_clock, Clock};
use crate::display::{self, DisplayList, Item, Paint, IMAGE_PLACEHOLDER};
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
    raster_time_spent: Duration,
    /// Один бэкенд на все построения растров: создание `RenderContext` дороже
````

на:

````rust
    raster_time_spent: Duration,
    /// Часы для предела по времени; без них — только счётный предел.
    clock: Option<Box<dyn Clock>>,
    /// Один бэкенд на все построения растров: создание `RenderContext` дороже
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
            raster_time_spent: Duration::ZERO,
            raster_backend: None,
````

на:

````rust
            raster_time_spent: Duration::ZERO,
            clock: default_clock(),
            raster_backend: None,
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        self.raster_time = budget;
    }
````

на:

````rust
        self.raster_time = budget;
    }

    /// Часы для предела времени на растры (`None` — только счётный предел).
    pub fn set_clock(&mut self, clock: Option<Box<dyn Clock>>) {
        self.clock = clock;
    }
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
            self.raster_budget -= 1;
            let started = now();
            let (w, h) = (w.max(1.0) as u16, h.max(1.0) as u16);
````

на:

````rust
            self.raster_budget -= 1;
            let started = self.clock.as_ref().map(|c| c.now_ms());
            let (w, h) = (w.max(1.0) as u16, h.max(1.0) as u16);
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
            self.raster_backend = Some(cpu);
            if let Some(t) = started {
                self.raster_time_spent += t.elapsed();
            }
````

на:

````rust
            self.raster_backend = Some(cpu);
            if let (Some(t), Some(c)) = (started, self.clock.as_ref()) {
                let ms = (c.now_ms() - t).max(0.0);
                self.raster_time_spent += Duration::from_secs_f64(ms / 1000.0);
            }
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        let first = self.stats.raster_builds == 0;
        first || self.raster_time.is_none_or(|t| self.raster_time_spent < t)
    }
````

на:

````rust
        let first = self.stats.raster_builds == 0;
        // Без часов время не измерить: остаётся счётный предел.
        let timed = self.raster_time.filter(|_| self.clock.is_some());
        first || timed.is_none_or(|t| self.raster_time_spent < t)
    }
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
    r.last_used + 1 < frame
}

/// Текущее время, если платформа его даёт. В `wasm32-unknown-unknown`
/// `Instant::now` паникует; там предел по времени не работает, остаётся счётный,
/// пока часы (`performance.now`) не придут из `ferrum-wasm`.
fn now() -> Option<Instant> {
    (!cfg!(target_arch = "wasm32")).then(Instant::now)
}
````

на:

````rust
    r.last_used + 1 < frame
}
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
mod camera;
mod display;
````

на:

````rust
mod camera;
mod clock;
mod display;
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
pub use camera::{Camera, MAX_DEVICE_PIXELS, MAX_DEVICE_SIDE, MAX_ZOOM, MIN_ZOOM};
pub use draw::{
````

на:

````rust
pub use camera::{Camera, MAX_DEVICE_PIXELS, MAX_DEVICE_SIDE, MAX_ZOOM, MIN_ZOOM};
#[cfg(not(target_arch = "wasm32"))]
pub use clock::StdClock;
pub use clock::{default_clock, Clock};
pub use draw::{
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
        self.painter.set_raster_time(budget);
    }
````

на:

````rust
        self.painter.set_raster_time(budget);
    }

    /// Часы для предела времени на растры. По умолчанию нативно — `StdClock`, в
    /// wasm32 часов нет (там `Instant` паникует) и работает только счётный предел,
    /// пока `ferrum-wasm` не поставит `performance.now()`.
    pub fn set_clock(&mut self, clock: Option<Box<dyn Clock>>) {
        self.painter.set_clock(clock);
    }
````

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p ferrum-render --test clock`
Expected: PASS, `4 passed`.

Run: `cargo test -p ferrum-render`
Expected: PASS — наборы этапа 2 без изменений.

Run: `cargo clippy --target wasm32-unknown-unknown -p ferrum-editor -p ferrum-render -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 5: Закоммитить**

```bash
git add rust-toolchain.toml .cargo/config.toml scripts/llvm-ar crates/ferrum-render
git commit -m "feat(render): часы рендера трейтом Clock и сборка под wasm32"
```

---

### Task 2: Атлас изображений GPU

`vello_gpu` рисует изображения только из своего атласа (`ImageSource::OpaqueId`), а атлас живёт в ресурсах рендерера, которому нужно устройство. На этапе 2 `GpuBackend` рисовал вместо картинок заглушку и выключал растровый кэш фреймов (`supports_images() == false`). Теперь бэкенд обобщён по устройству (`GpuBackend<D: GpuDevice>`): картинка загружается в атлас при первом использовании и дальше рисуется по номеру.

Ключ картинки — `ImageKey`: у картинки документа — хэш blake3 её байтов (та же картинка, заново декодированная, не грузится второй раз), у растра фрейма — номер построения (перестроенный растр не путается с прежним). Вытесняются картинки, не рисовавшиеся `EVICT_AFTER_FRAMES` кадров, и — когда места нет — давно не рисовавшиеся, но никогда не нарисованные в текущем кадре: сцена кадра ссылается на картинки по номеру, освобождённый номер дал бы чужие пиксели. Если места нет и вытеснять нечего, рисуется заглушка (`AtlasStats::misses`). Картинка больше страницы атласа уменьшается на CPU один раз при загрузке, матрица краски растягивает её обратно.

Место выделяется зеркальным кэшем `vello_common::image_cache::ImageCache` с той же конфигурацией (`atlas_config`), что получает рендерер устройства. Алгоритм выделения детерминирован, поэтому зеркало заранее знает, поместится ли картинка: wgpu-рендерер vello при нехватке места паникует (`upload_image` разворачивает ошибку), а паника в wasm роняет весь worker. Номер, выданный устройством, сверяется с зеркальным; расхождение ломает атлас (дальше — заглушки и `atlas_error`), но не worker. `vello_common` — та же версия 0.3.0, что тянут `vello_cpu` и `vello_gpu`.

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/ferrum-render/Cargo.toml`
- Modify: `crates/ferrum-render/src/backend.rs`
- Modify: `crates/ferrum-render/src/draw.rs`
- Modify: `crates/ferrum-render/src/gpu.rs`
- Modify: `crates/ferrum-render/src/lib.rs`
- Test: `crates/ferrum-render/tests/gpu.rs`
- Modify: `crates/ferrum-render/tests/backend.rs`

**Interfaces:**
- Consumes: `RenderBackend`, `ImagePaint`, `Painter` этапа 2; `vello_common::{image_cache::ImageCache, multi_atlas::AtlasConfig, paint::ImageId}`.
- Produces: `ImageKey { Hash([u8; 32]), Raster(u64) }` с `for_hash(&str)` и `next_raster()`; поле `ImagePaint::key`; трейт `GpuDevice { upload(&Pixmap) -> Result<ImageId, String>, destroy(ImageId) }`; `GpuBackend<D>::{new(w, h, device, AtlasConfig), reset, device, device_mut, parts, atlas_stats, atlas_error}`; `AtlasStats { images, bytes, uploads, evictions, misses, downscaled }`; `atlas_config(max_texture_side)`, `GPU_ATLAS_SIDE = 4096`, `GPU_ATLAS_PAGES = 4`; реэкспорт `AtlasConfig`, `ImageId` (фича `gpu`). `GpuBackend::supports_images()` — `true`.

- [ ] **Step 1: Зависимость `vello_common`**

В `Cargo.toml` заменить:

````toml
vello_gpu = { version = "=0.3.0", default-features = false, features = ["std", "wgpu"] }
# Без системных шрифтов: подписи только встроенным Onest, одинаково везде.
````

на:

````toml
vello_gpu = { version = "=0.3.0", default-features = false, features = ["std", "wgpu"] }
# Атлас изображений vello_gpu: зеркальный кэш выделений (см. ferrum-render/src/gpu.rs).
# Та же версия, что тянут vello_cpu и vello_gpu.
vello_common = { version = "=0.3.0", default-features = false, features = ["std"] }
# Без системных шрифтов: подписи только встроенным Onest, одинаково везде.
````

В `crates/ferrum-render/Cargo.toml` заменить:

````toml
[features]
# Бэкенд vello_gpu (WebGPU через wgpu). Только сборка и кодирование сцены;
# поверхности и растеризация на GPU — этап 4.
gpu = ["dep:vello_gpu"]

````

на:

````toml
[features]
# Бэкенд vello_gpu: кодирование сцены и атлас изображений. Устройство и
# поверхность (WebGPU, WebGL2) даёт ferrum-wasm через трейт `GpuDevice`.
gpu = ["dep:vello_gpu", "dep:vello_common"]

````

В `crates/ferrum-render/Cargo.toml` заменить:

````toml
vello_gpu = { workspace = true, optional = true }
parley.workspace = true
````

на:

````toml
vello_gpu = { workspace = true, optional = true }
vello_common = { workspace = true, optional = true }
blake3.workspace = true
parley.workspace = true
````

- [ ] **Step 2: Написать падающие тесты**

`FakeDevice` выделяет место своим кэшем той же конфигурации — так же, как атлас настоящего рендерера, — и записывает загрузки и освобождения; `skew` имитирует устройство, разошедшееся с зеркалом.

В `crates/ferrum-render/tests/gpu.rs` заменить:

````rust
//! Бэкенд `vello_gpu` принимает тот же кадр, что и CPU: журнал вызовов совпадает.
//! Запуск: `cargo test -p ferrum-render --features gpu --test gpu`.
````

на:

````rust
//! Бэкенд `vello_gpu` принимает тот же кадр, что и CPU (журнал вызовов совпадает),
//! и держит картинки в атласе по ключу: загрузка при первом использовании,
//! вытеснение давно не рисовавшихся, заглушка, когда места нет.
//! Запуск: `cargo test -p ferrum-render --features gpu --test gpu`.
````

В `crates/ferrum-render/tests/gpu.rs` заменить:

````rust
use ferrum_render::{
    Camera, CpuBackend, GpuBackend, ImagePaint, NoImages, Overlay, Pixmap, RenderBackend, Renderer,
    Rgba,
};
use kurbo::{Affine, BezPath, Circle, Point, Rect, Shape, Size, Stroke};
// Оба бэкенда используют один и тот же `peniko` (см. `cargo tree -d`).
use vello_cpu::peniko::{Extend, Fill as FillRule};

````

на:

````rust
use ferrum_render::{
    atlas_config, AtlasConfig, Camera, CpuBackend, GpuBackend, GpuDevice, ImageId, ImageKey,
    ImagePaint, NoImages, Overlay, Pixmap, RenderBackend, Renderer, Rgba, EVICT_AFTER_FRAMES,
};
use kurbo::{Affine, BezPath, Point, Rect, Size, Stroke};
use vello_common::image_cache::ImageCache;
// Оба бэкенда используют один и тот же `peniko` (см. `cargo tree -d`).
use vello_cpu::peniko::{Extend, Fill as FillRule};

/// Устройство без GPU: выделяет место своим кэшем той же конфигурации — так же,
/// как атлас настоящего рендерера, — и записывает загрузки и освобождения.
struct FakeDevice {
    atlas: ImageCache,
    log: Vec<String>,
    /// Сдвиг номеров: имитирует устройство, разошедшееся с зеркалом.
    skew: u32,
}

impl FakeDevice {
    fn new(config: AtlasConfig) -> FakeDevice {
        FakeDevice {
            atlas: ImageCache::new_with_config(config),
            log: vec![],
            skew: 0,
        }
    }
}

impl GpuDevice for FakeDevice {
    fn upload(&mut self, pixmap: &Pixmap) -> Result<ImageId, String> {
        let id = self
            .atlas
            .allocate(pixmap.width(), pixmap.height(), 0)
            .map_err(|e| e.to_string())?;
        self.log
            .push(format!("upload {}x{}", pixmap.width(), pixmap.height()));
        Ok(ImageId::new(id.as_u32() + self.skew))
    }

    fn destroy(&mut self, id: ImageId) {
        self.atlas.deallocate(id);
        self.log.push(format!("destroy {}", id.as_u32()));
    }
}

fn gpu(w: u16, h: u16, config: AtlasConfig) -> GpuBackend<FakeDevice> {
    GpuBackend::new(w, h, FakeDevice::new(config), config)
}

/// Атлас из одной страницы `side × side`.
fn tiny_atlas(side: u16) -> AtlasConfig {
    AtlasConfig {
        max_atlases: 1,
        atlas_size: (side, side),
        ..atlas_config(4096)
    }
}

fn paint(key: ImageKey, w: u16, h: u16) -> ImagePaint {
    ImagePaint {
        key,
        pixmap: Arc::new(Pixmap::new(w, h)),
        transform: Affine::IDENTITY,
        extend: Extend::Pad,
        alpha: 1.0,
    }
}

/// Рисует картинку прямоугольником и сообщает, попала ли в сцену картинка (а не
/// заглушка-цвет).
fn draw_image(b: &mut GpuBackend<FakeDevice>, p: &ImagePaint) -> bool {
    b.set_image(p);
    b.fill_rect(&Rect::new(0.0, 0.0, 10.0, 10.0));
    format!("{:?}", b.scene.paint()).contains("Image")
}

````

В `crates/ferrum-render/tests/gpu.rs` заменить:

````rust
    let (w, h) = cam.device_size();
    let mut gpu = Recording::new(GpuBackend::new(w, h));
    let mut cpu = Recording::new(CpuBackend::new(w, h));
    assert!(!gpu.supports_images());
    let gpu_stats =
        Renderer::new().render(&mut gpu, &scene, &cam, &overlay, &NoImages, Some(WHITE));
    // CPU без растрового кэша, чтобы кадры были сравнимы.
    struct NoRaster(Recording<CpuBackend>);
````

на:

````rust
    let (w, h) = cam.device_size();
    let mut gpu = Recording::new(gpu(w, h, atlas_config(8192)));
    let mut cpu = Recording::new(CpuBackend::new(w, h));
    // С атласом GPU-бэкенд умеет изображения, а значит и растровый кэш фреймов.
    assert!(gpu.supports_images());
    let gpu_stats =
        Renderer::new().render(&mut gpu, &scene, &cam, &overlay, &NoImages, Some(WHITE));
    // CPU без растрового кэша: при этом зуме растров нет и у GPU, кадры сравнимы.
    struct NoRaster(Recording<CpuBackend>);
````

В `crates/ferrum-render/tests/gpu.rs` заменить:

````rust
    for extra in [
        (|b: &mut GpuBackend| b.scene.pop_layer()) as fn(&mut GpuBackend),
        |b| b.scene.pop_clip(),
````

на:

````rust
    for extra in [
        (|b: &mut GpuBackend<FakeDevice>| b.scene.pop_layer()) as fn(&mut GpuBackend<FakeDevice>),
        |b| b.scene.pop_clip(),
````

В `crates/ferrum-render/tests/gpu.rs` заменить:

````rust

    // Сильное отдаление: растрового кэша у GPU-бэкенда нет — узлы рисуются LOD.
    cam.set_zoom(0.02);
    gpu.inner.reset(w, h);
    gpu.log.clear();
    let stats = Renderer::new().render(&mut gpu, &scene, &cam, &overlay, &NoImages, None);
    assert_eq!(stats.rasters, 0);
    assert!(stats.lod_rects > 0, "LOD-путь не сработал: {stats:?}");
    assert!(gpu.count("fill_rect") + gpu.count("fill_path") > 0);
    assert_eq!((gpu.clips, gpu.layers), (0, 0));
````

на:

````rust

    // Сильное отдаление: фрейм рисуется из растра, растр загружается в атлас.
    cam.set_zoom(0.02);
    gpu.inner.reset(w, h);
    gpu.log.clear();
    let stats = Renderer::new().render(&mut gpu, &scene, &cam, &overlay, &NoImages, None);
    assert_eq!((stats.rasters, stats.raster_builds), (1, 1), "{stats:?}");
    assert_eq!(gpu.count("set_image"), 1);
    assert_eq!(gpu.inner.atlas_stats().uploads, 1);
    assert_eq!(
        gpu.inner.device().log.len(),
        1,
        "{:?}",
        gpu.inner.device().log
    );
    assert_eq!((gpu.clips, gpu.layers), (0, 0));
````

В `crates/ferrum-render/tests/gpu.rs` заменить:

````rust
#[test]
fn image_placeholder_is_accepted() {
    let mut gpu = Recording::new(GpuBackend::new(64, 64));
    let paint = ImagePaint {
        pixmap: Arc::new(Pixmap::new(4, 4)),
        transform: Affine::IDENTITY,
        extend: Extend::Pad,
        alpha: 0.5,
    };
    gpu.set_image(&paint);
    gpu.fill_path(
        &Circle::new((32.0, 32.0), 20.0).to_path(0.1),
        FillRule::NonZero,
    );
    // Заглушка: серая краска с учётом альфы, изображения в сцене нет.
    let placeholder = format!("{:?}", gpu.inner.scene.paint());
    assert!(!placeholder.contains("Image"), "{placeholder}");
    gpu.set_color(Rgba::new([1.0, 0.0, 0.0, 1.0]));
    let red = format!("{:?}", gpu.inner.scene.paint());
    assert_ne!(red, placeholder, "set_color не сменил краску");
    let stroke = Stroke::new(3.0);
    gpu.stroke_path(&Circle::new((32.0, 32.0), 10.0).to_path(0.1), &stroke);
    assert_eq!(gpu.inner.scene.stroke().width, 3.0);
    gpu.fill_rect(&Rect::new(0.0, 0.0, 10.0, 10.0));
    // Заглушка идёт через `set_color`, отдельного вызова изображения нет.
    assert_eq!(
        gpu.log,
        [
            "set_image",
            "fill_path",
            "set_color",
            "stroke_path",
            "fill_rect"
        ]
    );
}
````

на:

````rust
#[test]
fn image_is_uploaded_once_per_key() {
    let mut b = gpu(64, 64, atlas_config(8192));
    let a = paint(ImageKey::for_hash(&"ab".repeat(32)), 8, 8);
    for _ in 0..3 {
        b.begin_frame(64, 64);
        assert!(draw_image(&mut b, &a));
        // Та же картинка второй раз в кадре — без новой загрузки.
        assert!(draw_image(&mut b, &a));
    }
    // Та же картинка, другой `Arc` (заново декодирована) — ключ тот же.
    let again = paint(a.key, 8, 8);
    assert!(draw_image(&mut b, &again));
    // Другой ключ — другая картинка, даже с теми же пикселями.
    let other = paint(ImageKey::for_hash(&"cd".repeat(32)), 8, 8);
    assert!(draw_image(&mut b, &other));
    assert_eq!(b.device().log, ["upload 8x8", "upload 8x8"]);
    let s = b.atlas_stats();
    assert_eq!((s.images, s.bytes, s.uploads), (2, 2 * 8 * 8 * 4, 2));
}

#[test]
fn hash_keys_are_blake3_bytes_and_raster_keys_are_unique() {
    let hex = blake3::hash(b"png").to_hex().to_string();
    assert_eq!(
        ImageKey::for_hash(&hex),
        ImageKey::Hash(*blake3::hash(b"png").as_bytes())
    );
    // Повреждённый хэш из файла тоже даёт устойчивый ключ.
    assert_eq!(ImageKey::for_hash("не хэш"), ImageKey::for_hash("не хэш"));
    assert_ne!(ImageKey::for_hash("не хэш"), ImageKey::for_hash(&hex));
    assert_ne!(ImageKey::next_raster(), ImageKey::next_raster());
}

#[test]
fn full_atlas_evicts_only_images_not_drawn_this_frame() {
    // Одна страница 64×64: две картинки 40×40 вместе не помещаются.
    let mut b = gpu(64, 64, tiny_atlas(64));
    let a = paint(ImageKey::Raster(1), 40, 40);
    let c = paint(ImageKey::Raster(2), 40, 40);
    b.begin_frame(64, 64);
    assert!(draw_image(&mut b, &a));
    // В этом же кадре `a` уже в сцене — вытеснять её нельзя: заглушка.
    assert!(!draw_image(&mut b, &c));
    assert_eq!(b.atlas_stats().misses, 1);
    // Следующий кадр без `a`: она вытесняется ради `c`.
    b.begin_frame(64, 64);
    assert!(draw_image(&mut b, &c));
    assert_eq!(
        b.device().log,
        ["upload 40x40", "destroy 0", "upload 40x40"],
        "{:?}",
        b.device().log
    );
    let s = b.atlas_stats();
    assert_eq!((s.images, s.evictions, s.misses), (1, 1, 1));
}

#[test]
fn images_unused_for_long_are_evicted() {
    let mut b = gpu(64, 64, atlas_config(8192));
    let a = paint(ImageKey::Raster(7), 16, 16);
    b.begin_frame(64, 64);
    assert!(draw_image(&mut b, &a));
    for _ in 0..EVICT_AFTER_FRAMES + 64 {
        b.begin_frame(64, 64);
    }
    assert_eq!(b.atlas_stats().images, 0);
    assert_eq!(b.device().log, ["upload 16x16", "destroy 0"]);
}

#[test]
fn image_larger_than_atlas_page_is_downscaled() {
    let mut b = gpu(64, 64, tiny_atlas(64));
    let mut big = paint(ImageKey::Raster(3), 256, 128);
    big.transform = Affine::translate((5.0, 0.0));
    b.begin_frame(64, 64);
    assert!(draw_image(&mut b, &big));
    assert_eq!(b.device().log, ["upload 64x32"]);
    assert_eq!(b.atlas_stats().downscaled, 1);
    // Матрица краски растягивает уменьшенную копию обратно в размер исходной.
    let state = format!("{:?}", b.scene.save_current_state());
    let expected = format!(
        "{:?}",
        Affine::translate((5.0, 0.0)) * Affine::scale_non_uniform(4.0, 4.0)
    );
    assert!(
        state.contains(&expected),
        "нет матрицы {expected} в {state}"
    );
}

#[test]
fn device_ids_out_of_step_with_mirror_break_the_atlas() {
    let config = atlas_config(8192);
    let mut dev = FakeDevice::new(config);
    dev.skew = 1;
    let mut b = GpuBackend::new(64, 64, dev, config);
    b.begin_frame(64, 64);
    assert!(!draw_image(&mut b, &paint(ImageKey::Raster(1), 8, 8)));
    assert!(b.atlas_error().is_some_and(|e| e.contains("рассогласован")));
    // Номер устройства освобождён, дальше — только заглушки, без загрузок.
    assert!(!draw_image(&mut b, &paint(ImageKey::Raster(2), 8, 8)));
    assert_eq!(b.device().log, ["upload 8x8", "destroy 1"]);
}
````

В `crates/ferrum-render/tests/backend.rs` заменить:

````rust

use ferrum_render::{Camera, CpuBackend, ImagePaint, Pixmap, RenderBackend, Rgba};
use kurbo::{Affine, Point, Rect, Shape, Size};
````

на:

````rust

use ferrum_render::{Camera, CpuBackend, ImageKey, ImagePaint, Pixmap, RenderBackend, Rgba};
use kurbo::{Affine, Point, Rect, Shape, Size};
````

В `crates/ferrum-render/tests/backend.rs` заменить:

````rust
    b.set_image(&ImagePaint {
        pixmap: Arc::new(img),
````

на:

````rust
    b.set_image(&ImagePaint {
        key: ImageKey::Raster(1),
        pixmap: Arc::new(img),
````

Run: `cargo test -p ferrum-render --features gpu --test gpu`
Expected: FAIL — не компилируется: нет `ImageKey`, `GpuDevice`, `atlas_config`, у `ImagePaint` нет поля `key`.

- [ ] **Step 3: Ключи картинок и атлас**

В `crates/ferrum-render/src/backend.rs` заменить:

````rust

use std::sync::Arc;
````

на:

````rust

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
````

В `crates/ferrum-render/src/backend.rs` заменить:

````rust

/// Изображение как краска.
#[derive(Clone)]
pub struct ImagePaint {
    pub pixmap: Arc<Pixmap>,
````

на:

````rust

/// Ключ изображения в кэшах бэкенда (атлас GPU): одна и та же картинка — один
/// ключ, сколько бы раз и где её ни рисовали.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageKey {
    /// Картинка документа: blake3 её байтов (тот же хэш, что в `ImageStore`).
    Hash([u8; 32]),
    /// Растр фрейма из растрового кэша: у каждого построения свой номер, поэтому
    /// перестроенный растр не путается с прежним.
    Raster(u64),
}

impl ImageKey {
    /// Ключ картинки документа по её хэшу (hex). Строка не из 64 hex-символов
    /// (повреждённый файл) не теряется: ключом становится blake3 самой строки.
    pub fn for_hash(hash: &str) -> ImageKey {
        let bytes = match blake3::Hash::from_hex(hash) {
            Ok(h) => *h.as_bytes(),
            Err(_) => *blake3::hash(hash.as_bytes()).as_bytes(),
        };
        ImageKey::Hash(bytes)
    }

    /// Новый ключ растра: номера не повторяются во всём процессе, даже если
    /// рендереров несколько.
    pub fn next_raster() -> ImageKey {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        ImageKey::Raster(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// Изображение как краска.
#[derive(Clone)]
pub struct ImagePaint {
    pub key: ImageKey,
    pub pixmap: Arc<Pixmap>,
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust

use crate::backend::{CpuBackend, ImagePaint, RenderBackend};
use crate::clock::{default_clock, Clock};
````

на:

````rust

use crate::backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend};
use crate::clock::{default_clock, Clock};
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
    pixmap: Arc<Pixmap>,
    last_used: u64,
````

на:

````rust
    pixmap: Arc<Pixmap>,
    /// Ключ этого построения для кэшей бэкенда (атлас GPU).
    key: ImageKey,
    last_used: u64,
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
                pixmap: Arc::new(cpu.render()),
                last_used: self.frame,
````

на:

````rust
                pixmap: Arc::new(cpu.render()),
                key: ImageKey::next_raster(),
                last_used: self.frame,
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
        backend.set_image(&ImagePaint {
            pixmap: r.pixmap.clone(),
````

на:

````rust
        backend.set_image(&ImagePaint {
            key: r.key,
            pixmap: r.pixmap.clone(),
````

В `crates/ferrum-render/src/draw.rs` заменить:

````rust
                backend.set_image(&ImagePaint {
                    pixmap,
````

на:

````rust
                backend.set_image(&ImagePaint {
                    key: ImageKey::for_hash(hash),
                    pixmap,
````

`crates/ferrum-render/src/gpu.rs` (целиком):

````rust
//! Бэкенд `vello_gpu`: кадр кодируется в `vello_gpu::Scene` тем же кодом, что и
//! для CPU. Устройство и поверхность (WebGPU или WebGL2) даёт `ferrum-wasm` через
//! трейт `GpuDevice`; здесь — кодирование и атлас изображений.
//!
//! `vello_gpu` рисует изображения только из своего атласа (`ImageSource::OpaqueId`).
//! Картинки документа и растры фреймов загружаются туда по ключу (`ImageKey`) при
//! первом использовании и вытесняются, когда давно не рисовались или когда место
//! нужно новой картинке. Вытесняется только то, что не рисовалось в текущем кадре:
//! сцена кадра ссылается на картинки по номеру, и освобождённый номер в ней дал бы
//! чужие пиксели.
//!
//! Место в атласе выделяется зеркальным кэшем (`vello_common::image_cache`) с той же
//! конфигурацией, что у рендерера устройства. Алгоритм выделения детерминирован,
//! поэтому зеркало заранее знает, поместится ли картинка: `vello_gpu` с wgpu при
//! нехватке места паникует (`upload_image` разворачивает ошибку), а паника в wasm
//! роняет весь worker. Номер, выданный устройством, сверяется с зеркальным; при
//! расхождении атлас считается сломанным и картинки рисуются заглушкой.

use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{Affine, BezPath, Rect, Stroke};
use vello_common::image_cache::ImageCache;
use vello_common::multi_atlas::{AllocationStrategy, AtlasConfig};
use vello_common::paint::{Image, ImageId, ImageSource};
// vello_cpu и vello_gpu используют один и тот же `peniko` (проверено `cargo tree -d`).
use vello_cpu::peniko::{Extend, Fill, ImageQuality, ImageSampler};
use vello_cpu::Pixmap;

use crate::backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::display::IMAGE_PLACEHOLDER;
use crate::draw::EVICT_AFTER_FRAMES;

/// Сторона страницы атласа, если устройство позволяет (WebGL2 гарантирует только
/// 2048, WebGPU — 8192).
pub const GPU_ATLAS_SIDE: u16 = 4096;
/// Страниц атласа не больше: 4 × 4096² × 4 байта = 256 МБ видеопамяти в худшем
/// случае.
pub const GPU_ATLAS_PAGES: usize = 4;

/// Устройство, в атлас которого загружаются картинки (`vello_gpu::Renderer` с wgpu
/// или `WebGlRenderer`).
pub trait GpuDevice {
    /// Загружает картинку в атлас и возвращает её номер. Атлас устройства выделяет
    /// место тем же алгоритмом и с той же конфигурацией (`atlas_config`), что и
    /// зеркало `GpuBackend`, поэтому номер обязан совпасть с зеркальным.
    fn upload(&mut self, pixmap: &Pixmap) -> Result<ImageId, String>;
    /// Освобождает место картинки в атласе.
    fn destroy(&mut self, id: ImageId);
}

/// Конфигурация атласа для устройства с наибольшей стороной текстуры
/// `max_texture_side`. Та же конфигурация передаётся рендереру устройства
/// (`RenderSettings::memory_settings`) и зеркалу.
pub fn atlas_config(max_texture_side: u32) -> AtlasConfig {
    let side = max_texture_side.clamp(1, GPU_ATLAS_SIDE as u32) as u16;
    AtlasConfig {
        initial_atlas_count: 0,
        max_atlases: GPU_ATLAS_PAGES,
        atlas_size: (side, side),
        auto_grow: true,
        allocation_strategy: AllocationStrategy::FirstFit,
    }
}

/// Состояние атласа — для статистики в интерфейсе и тестов.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AtlasStats {
    /// Картинок в атласе и их байт.
    pub images: usize,
    pub bytes: usize,
    /// Загрузок, вытеснений и картинок, которым не хватило места (нарисованы
    /// заглушкой), за всё время.
    pub uploads: u64,
    pub evictions: u64,
    pub misses: u64,
    /// Картинок больше страницы атласа, уменьшенных перед загрузкой.
    pub downscaled: u64,
}

struct Entry {
    id: ImageId,
    /// Размер исходной картинки и загруженной (меньше, если не влезла в страницу).
    source: (u16, u16),
    uploaded: (u16, u16),
    last_used: u64,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.uploaded.0 as usize * self.uploaded.1 as usize * 4
    }
}

pub struct GpuBackend<D: GpuDevice> {
    pub scene: vello_gpu::Scene,
    device: D,
    mirror: ImageCache,
    side: u16,
    entries: HashMap<ImageKey, Entry>,
    frame: u64,
    stats: AtlasStats,
    broken: Option<String>,
}

impl<D: GpuDevice> GpuBackend<D> {
    /// Бэкенд кадра `width × height` для устройства с атласом `atlas` — той же
    /// конфигурацией, что получил рендерер устройства.
    pub fn new(width: u16, height: u16, device: D, atlas: AtlasConfig) -> GpuBackend<D> {
        GpuBackend {
            scene: vello_gpu::Scene::new(width, height),
            device,
            side: atlas.atlas_size.0.min(atlas.atlas_size.1),
            mirror: ImageCache::new_with_config(atlas),
            entries: HashMap::new(),
            frame: 0,
            stats: AtlasStats::default(),
            broken: None,
        }
    }

    pub fn reset(&mut self, width: u16, height: u16) {
        self.scene.reset_and_resize(width, height);
    }

    pub fn device(&self) -> &D {
        &self.device
    }

    pub fn device_mut(&mut self) -> &mut D {
        &mut self.device
    }

    /// Сцена кадра и устройство вместе — чтобы отдать сцену рендереру устройства.
    pub fn parts(&mut self) -> (&vello_gpu::Scene, &mut D) {
        (&self.scene, &mut self.device)
    }

    pub fn atlas_stats(&self) -> AtlasStats {
        AtlasStats {
            images: self.entries.len(),
            bytes: self.entries.values().map(Entry::bytes).sum(),
            ..self.stats
        }
    }

    /// Ошибка атласа, после которой картинки рисуются заглушкой.
    pub fn atlas_error(&self) -> Option<&str> {
        self.broken.as_deref()
    }

    /// Номер картинки в атласе (загружает при первом использовании) и множители
    /// уменьшения по осям. `None` — места нет или атлас сломан.
    fn image_id(&mut self, image: &ImagePaint) -> Option<(ImageId, f64, f64)> {
        if self.broken.is_some() {
            return None;
        }
        let size = (image.pixmap.width(), image.pixmap.height());
        if let Some(e) = self.entries.get_mut(&image.key) {
            if e.source == size {
                e.last_used = self.frame;
                return Some(scaled(e));
            }
            // Под тем же ключом картинка другого размера (перекодирована): заменить.
            let old = self.entries.remove(&image.key).expect("запись есть");
            self.free(old.id);
        }
        let (fitted, uploaded) = fit_to_side(&image.pixmap, self.side);
        if fitted.is_some() {
            self.stats.downscaled += 1;
        }
        let id = loop {
            match self.mirror.allocate(uploaded.0, uploaded.1, 0) {
                Ok(id) => break id,
                Err(_) => {
                    if !self.evict_one() {
                        self.stats.misses += 1;
                        return None;
                    }
                }
            }
        };
        let pixmap: &Pixmap = fitted.as_ref().unwrap_or(&image.pixmap);
        match self.device.upload(pixmap) {
            Ok(real) if real == id => {}
            Ok(real) => {
                self.device.destroy(real);
                self.mirror.deallocate(id);
                self.broken = Some(format!(
                    "Атлас изображений GPU рассогласован (номер {} вместо {})",
                    real.as_u32(),
                    id.as_u32()
                ));
                return None;
            }
            Err(e) => {
                self.mirror.deallocate(id);
                self.broken = Some(format!("Не удалось загрузить изображение в GPU: {e}"));
                return None;
            }
        }
        self.stats.uploads += 1;
        let entry = Entry {
            id,
            source: size,
            uploaded,
            last_used: self.frame,
        };
        let out = scaled(&entry);
        self.entries.insert(image.key, entry);
        Some(out)
    }

    /// Вытесняет картинку, дольше всех не рисовавшуюся, кроме нарисованных в этом
    /// кадре. `false` — вытеснять нечего.
    fn evict_one(&mut self) -> bool {
        let frame = self.frame;
        let oldest = self
            .entries
            .iter()
            .filter(|(_, e)| e.last_used < frame)
            .min_by_key(|(_, e)| e.last_used)
            .map(|(k, _)| *k);
        let Some(key) = oldest else { return false };
        let e = self.entries.remove(&key).expect("запись есть");
        self.free(e.id);
        self.stats.evictions += 1;
        true
    }

    fn free(&mut self, id: ImageId) {
        self.mirror.deallocate(id);
        self.device.destroy(id);
    }

    /// Выбрасывает картинки, не рисовавшиеся `EVICT_AFTER_FRAMES` кадров (например,
    /// растры фреймов, перестроенные под новыми ключами).
    fn evict_aged(&mut self) {
        let frame = self.frame;
        let aged: Vec<ImageKey> = self
            .entries
            .iter()
            .filter(|(_, e)| e.last_used + EVICT_AFTER_FRAMES < frame)
            .map(|(k, _)| *k)
            .collect();
        for key in aged {
            let e = self.entries.remove(&key).expect("запись есть");
            self.free(e.id);
            self.stats.evictions += 1;
        }
    }
}

/// Номер и множители уменьшения записи.
fn scaled(e: &Entry) -> (ImageId, f64, f64) {
    (
        e.id,
        e.uploaded.0 as f64 / e.source.0 as f64,
        e.uploaded.1 as f64 / e.source.1 as f64,
    )
}

/// Картинка больше страницы атласа уменьшается с сохранением пропорций (на CPU,
/// один раз при загрузке). Возвращает уменьшенную копию, если она понадобилась, и
/// размер загружаемой картинки.
fn fit_to_side(pixmap: &Arc<Pixmap>, side: u16) -> (Option<Pixmap>, (u16, u16)) {
    let (w, h) = (pixmap.width(), pixmap.height());
    if w <= side && h <= side {
        return (None, (w, h));
    }
    let k = side as f64 / w.max(h) as f64;
    let nw = ((w as f64 * k).floor() as u16).clamp(1, side);
    let nh = ((h as f64 * k).floor() as u16).clamp(1, side);
    let mut cpu = CpuBackend::new(nw, nh);
    cpu.begin_frame(nw, nh);
    cpu.set_transform(Affine::IDENTITY);
    cpu.set_image(&ImagePaint {
        key: ImageKey::Raster(0),
        pixmap: pixmap.clone(),
        transform: Affine::scale_non_uniform(nw as f64 / w as f64, nh as f64 / h as f64),
        extend: Extend::Pad,
        alpha: 1.0,
    });
    cpu.fill_rect(&Rect::new(0.0, 0.0, nw as f64, nh as f64));
    (Some(cpu.render()), (nw, nh))
}

impl<D: GpuDevice> RenderBackend for GpuBackend<D> {
    fn begin_frame(&mut self, width: u16, height: u16) {
        self.reset(width, height);
        self.frame += 1;
        if self.frame.is_multiple_of(64) {
            self.evict_aged();
        }
    }

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
        let Some((id, sx, sy)) = self.image_id(image) else {
            self.set_color(IMAGE_PLACEHOLDER.multiply_alpha(image.alpha));
            return;
        };
        self.scene.set_paint(Image {
            image: ImageSource::opaque_id(id),
            sampler: ImageSampler {
                x_extend: image.extend,
                y_extend: image.extend,
                quality: ImageQuality::Medium,
                alpha: image.alpha,
            },
        });
        // Пиксель уменьшенной копии — `1/s` пикселей исходной картинки.
        self.scene
            .set_paint_transform(image.transform * Affine::scale_non_uniform(1.0 / sx, 1.0 / sy));
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
}
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust

pub use backend::{CpuBackend, ImagePaint, RenderBackend, Rgba};
pub use camera::{Camera, MAX_DEVICE_PIXELS, MAX_DEVICE_SIDE, MAX_ZOOM, MIN_ZOOM};
````

на:

````rust

pub use backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend, Rgba};
pub use camera::{Camera, MAX_DEVICE_PIXELS, MAX_DEVICE_SIDE, MAX_ZOOM, MIN_ZOOM};
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
#[cfg(feature = "gpu")]
pub use gpu::GpuBackend;
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
````

на:

````rust
#[cfg(feature = "gpu")]
pub use gpu::{atlas_config, AtlasStats, GpuBackend, GpuDevice, GPU_ATLAS_PAGES, GPU_ATLAS_SIDE};
pub use image::{decode_png, ImageProvider, NoImages, MAX_IMAGE_SIDE};
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
pub use text::{Label, TextEngine, ONEST};
pub use vello_cpu::Pixmap;
````

на:

````rust
pub use text::{Label, TextEngine, ONEST};
#[cfg(feature = "gpu")]
pub use vello_common::{multi_atlas::AtlasConfig, paint::ImageId};
pub use vello_cpu::Pixmap;
````

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p ferrum-render --features gpu --test gpu`
Expected: PASS, `7 passed`.

Run: `cargo test -p ferrum-render --features gpu`
Expected: PASS — все наборы рендера.

Run: `cargo clippy -p ferrum-render --all-targets --features gpu -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 5: Закоммитить**

```bash
git add Cargo.toml Cargo.lock crates/ferrum-render
git commit -m "feat(render): атлас изображений vello_gpu с зеркальным кэшем выделений"
```

---

### Task 3: Экспорт нескольких узлов одной картинкой

«Скопировать как PNG» (`EditorEvent::ExportPng(ids)`) передаёт выделение целиком; склейка — здесь. Область — объединение областей узлов (у фрейма — его форма, у остальных — границы с обводкой и потомками, как у `export_png`). Узлы рисуются в порядке отрисовки сцены (`Scene::order_key`), а не выделения; узел, чей предок тоже экспортируется, пропускается — иначе он лёг бы поверх второй раз (полупрозрачный — темнее). Неизвестные и скрытые узлы пропускаются; если не осталось ни одного известного — `NotFound`, ни одного видимого — `Empty`. `export_png` и `export_pixmap` теперь частные случаи.

**Files:**
- Modify: `crates/ferrum-render/src/lib.rs`
- Test: `crates/ferrum-render/tests/export_many.rs`

**Interfaces:**
- Consumes: `Renderer::export_*` этапа 2, `Scene::order_key`.
- Produces: `Renderer::export_png_nodes(scene, &[NodeId], scale, images)`, `Renderer::export_pixmap_nodes(…)`.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-render/tests/export_many.rs`:

````rust
//! Экспорт нескольких узлов одной картинкой («Скопировать как PNG» для выделения).

mod common;

use common::*;
use ferrum_doc::{Color, Fill, NodeId, NodeKind, PropsPatch};
use ferrum_render::{ExportError, NoImages, Renderer};

fn solid(r: u8, g: u8, b: u8, a: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, a))])
}

#[test]
fn union_of_areas_in_paint_order() {
    let mut p = Page::new();
    let red = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0, 255),
            ..rect(0.0, 0.0, 20.0, 20.0)
        },
    );
    let blue = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 0, 255, 255),
            ..rect(10.0, 10.0, 30.0, 10.0)
        },
    );
    let scene = p.scene();
    // Порядок выделения обратный порядку отрисовки: синий всё равно сверху.
    let img = Renderer::new()
        .export_pixmap_nodes(&scene, &[blue, red], 2.0, &NoImages)
        .unwrap();
    assert_eq!((img.width(), img.height()), (80, 40));
    assert_eq!(px(&img, 5, 5), [255, 0, 0, 255]);
    assert_eq!(px(&img, 30, 30), [0, 0, 255, 255], "синий над красным");
    assert_eq!(px(&img, 70, 30), [0, 0, 255, 255]);
    // Вне обоих узлов — прозрачно.
    assert_eq!(px(&img, 70, 5), [0, 0, 0, 0]);
    let png = Renderer::new()
        .export_png_nodes(&scene, &[red, blue], 1.0, &NoImages)
        .unwrap();
    assert_eq!(&png[1..4], b"PNG");
}

#[test]
fn child_of_exported_parent_is_drawn_once() {
    let mut p = Page::new();
    let g = p.add(None, NodeKind::Group, PropsPatch::default());
    let leaf = p.add(
        Some(g),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0, 128),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let scene = p.scene();
    let once = Renderer::new()
        .export_pixmap_nodes(&scene, &[g], 1.0, &NoImages)
        .unwrap();
    let both = Renderer::new()
        .export_pixmap_nodes(&scene, &[leaf, g, leaf], 1.0, &NoImages)
        .unwrap();
    assert_eq!(px(&once, 5, 5)[3], 128);
    assert_eq!(px(&both, 5, 5), px(&once, 5, 5), "ребёнок нарисован дважды");
}

#[test]
fn unknown_and_hidden_nodes_are_skipped() {
    let mut p = Page::new();
    let shown = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: solid(0, 255, 0, 255),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let hidden = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            visible: Some(false),
            fills: solid(0, 0, 0, 255),
            ..rect(100.0, 100.0, 10.0, 10.0)
        },
    );
    let scene = p.scene();
    let ghost: NodeId = format!("1@{}", u64::MAX - 1).parse().unwrap();
    let img = Renderer::new()
        .export_pixmap_nodes(&scene, &[ghost, hidden, shown], 1.0, &NoImages)
        .unwrap();
    // Скрытый узел не расширил область.
    assert_eq!((img.width(), img.height()), (10, 10));
    let mut r = Renderer::new();
    assert_eq!(
        r.export_png_nodes(&scene, &[ghost], 1.0, &NoImages),
        Err(ExportError::NotFound)
    );
    assert_eq!(
        r.export_png_nodes(&scene, &[hidden], 1.0, &NoImages),
        Err(ExportError::Empty)
    );
    assert_eq!(
        r.export_png_nodes(&scene, &[], 1.0, &NoImages),
        Err(ExportError::NotFound)
    );
}
````

Run: `cargo test -p ferrum-render --test export_many`
Expected: FAIL — не компилируется: нет методов `export_pixmap_nodes`, `export_png_nodes`.

- [ ] **Step 2: Экспорт нескольких узлов**

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    ) -> Result<Vec<u8>, ExportError> {
        let pixmap = self.export_pixmap(scene, id, scale, images)?;
        pixmap
````

на:

````rust
    ) -> Result<Vec<u8>, ExportError> {
        self.export_png_nodes(scene, &[id], scale, images)
    }

    /// Экспорт нескольких узлов одной картинкой («Скопировать как PNG» для
    /// выделения): область — объединение областей узлов (как у `export_png`), узлы
    /// рисуются в порядке отрисовки сцены, а не выделения. Неизвестные и скрытые
    /// узлы пропускаются; если не осталось ни одного — ошибка.
    pub fn export_png_nodes(
        &mut self,
        scene: &Scene,
        ids: &[NodeId],
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Vec<u8>, ExportError> {
        let pixmap = self.export_pixmap_nodes(scene, ids, scale, images)?;
        pixmap
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
    ) -> Result<Pixmap, ExportError> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(ExportError::BadScale);
        }
        let slot = scene.slot(id).ok_or(ExportError::NotFound)?;
        let n = scene.at(slot);
        // Скрытый узел (и узел без видимого содержимого) картинки не даёт.
        if n.subtree_bounds.is_none() {
            return Err(ExportError::Empty);
        }
        let area = match (n.props.kind, n.local_bounds) {
            (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
            _ => match (n.world_bounds, n.subtree_bounds) {
                (Some(a), Some(b)) => Some(a.union(b)),
                (a, b) => a.or(b),
            },
        }
        .ok_or(ExportError::Empty)?;
        let w = (area.width() * scale).round();
````

на:

````rust
    ) -> Result<Pixmap, ExportError> {
        self.export_pixmap_nodes(scene, &[id], scale, images)
    }

    pub fn export_pixmap_nodes(
        &mut self,
        scene: &Scene,
        ids: &[NodeId],
        scale: f64,
        images: &dyn ImageProvider,
    ) -> Result<Pixmap, ExportError> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(ExportError::BadScale);
        }
        let mut slots = Vec::with_capacity(ids.len());
        let mut area: Option<Rect> = None;
        let mut known = false;
        for id in ids {
            let Some(slot) = scene.slot(*id) else {
                continue;
            };
            known = true;
            let n = scene.at(slot);
            // Скрытый узел (и узел без видимого содержимого) картинки не даёт.
            if n.subtree_bounds.is_none() {
                continue;
            }
            let own = match (n.props.kind, n.local_bounds) {
                (NodeKind::Frame, Some(b)) => Some(n.world.transform_rect_bbox(b)),
                _ => match (n.world_bounds, n.subtree_bounds) {
                    (Some(a), Some(b)) => Some(a.union(b)),
                    (a, b) => a.or(b),
                },
            };
            if let Some(own) = own {
                area = Some(area.map_or(own, |a| a.union(own)));
                slots.push(slot);
            }
        }
        if !known {
            return Err(ExportError::NotFound);
        }
        let area = area.ok_or(ExportError::Empty)?;
        // Узел, чей предок тоже экспортируется, уже нарисован вместе с предком:
        // второй раз он лёг бы поверх (полупрозрачный — темнее).
        let chosen: std::collections::HashSet<_> = slots.iter().copied().collect();
        slots.retain(|&s| {
            let mut cur = scene.at(s).parent;
            while let Some(p) = cur {
                if chosen.contains(&p) {
                    return false;
                }
                cur = scene.at(p).parent;
            }
            true
        });
        // Порядок отрисовки: ключ — позиции от корня, без повторов.
        slots.sort_by_cached_key(|s| scene.order_key(*s));
        slots.dedup();
        let w = (area.width() * scale).round();
````

В `crates/ferrum-render/src/lib.rs` заменить:

````rust
        painter.begin_frame();
        painter.draw(&mut backend, &ctx, &[slot]);
        Ok(backend.render())
````

на:

````rust
        painter.begin_frame();
        painter.draw(&mut backend, &ctx, &slots);
        Ok(backend.render())
````

- [ ] **Step 3: Запустить тесты**

Run: `cargo test -p ferrum-render --test export_many`
Expected: PASS, `3 passed`.

Run: `cargo test -p ferrum-render --test render`
Expected: PASS, `25 passed` — экспорт одного узла прежний.

- [ ] **Step 4: Закоммитить**

```bash
git add crates/ferrum-render
git commit -m "feat(render): экспорт нескольких узлов одной картинкой"
```

---

### Task 4: Крейт `ferrum-wasm`: общая память, записи ввода, живое состояние

Раскладка SharedArrayBuffer описана в Rust и генерируется в TypeScript (`apps/web/src/core/layout.gen.ts`; тест сверяет файл, `FERRUM_UPDATE_LAYOUT=1` пишет его заново):

```text
0               64                320                            65 856
| Int32 × 16    | Float64 × 32    | записи ввода × 1024 по 64 байта |
| управление    | живое состояние | кольцевой буфер                 |
```

Управление — счётчики `Atomics`: кадр (основной поток увеличивает его в `requestAnimationFrame`), позиции записи и чтения кольца, версия живого состояния, выброшенные события. Кольцо — один писатель (основной поток) и один читатель (worker): запись кладётся в ячейку `write % 1024`, и только потом публикуется `write + 1`; счётчики сравниваются по модулю 2³². Живое состояние пишется под seqlock: нечётная версия — идёт запись.

Запись ввода — 64 байта little-endian: вид, кнопка (как `PointerEvent.button`), биты модификаторов (Shift, Alt, Ctrl, Meta, AltGraph — только настоящее состояние, повтор), до четырёх `f64` (точка, время, дельты колеса, множитель щипка, размер холста и dpr) или у клавиш — время, `code` и `key` UTF-8 до 24 байт. Отдельный вид — размер холста (`ResizeObserver` и dpr): так он идёт тем же путём, что ввод, в обоих режимах транспорта. Разбор не паникует ни на каких байтах: неизвестный вид или кнопка, строка не UTF-8, пустой `code`, недопустимый размер — запись пропускается и считается. Нечисловые координаты отбрасывает сам редактор.

Живое состояние — 29 ячеек `f64` из макроса: кадры, время кадра (последнее, медиана и p95 по 120 кадрам), fps, бэкенд, выделение (число и значения панели «Положение»), камера, холст, жест, история, узлы, принятые и пропущенные записи, атлас.

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/ferrum-wasm/Cargo.toml`
- Create: `crates/ferrum-wasm/src/lib.rs`
- Create: `crates/ferrum-wasm/src/layout.rs`
- Create: `crates/ferrum-wasm/src/input.rs`
- Create: `crates/ferrum-wasm/src/live.rs`
- Create: `apps/web/src/core/layout.gen.ts`
- Test: `crates/ferrum-wasm/tests/input.rs`
- Test: `crates/ferrum-wasm/tests/layout.rs`

**Interfaces:**
- Consumes: `ferrum_editor::{Input, PointerEvent, WheelEvent, KeyEvent, Modifiers, PointerButton}`.
- Produces: `layout::{RECORD_SIZE, RING_CAPACITY, CTRL_BYTES, LIVE_OFFSET, LIVE_SLOTS, RING_OFFSET, SAB_BYTES, ctrl::*, kind::*, flag::*, field::*, BACKENDS, typescript()}`; `input::{Record { Input(Input), Resize { width, height, dpr } }, encode, decode, decode_all, Decoded { records, skipped }, MAX_VIEWPORT_SIDE, MAX_DPR}`; `live::{LiveState, LiveField}` с `to_slots`/`from_slots`, `ts_name`/`js_name`. Workspace-зависимости этапа (wasm-bindgen, js-sys, web-sys, wgpu, tsify, serde-wasm-bindgen, serde_bytes, console_error_panic_hook, wasm-bindgen-test) объявлены сразу.

- [ ] **Step 1: Крейт в workspace**

В `Cargo.toml` заменить:

````toml
    "crates/ferrum-editor",
]
````

на:

````toml
    "crates/ferrum-editor",
    "crates/ferrum-wasm",
]
````

В `Cargo.toml` заменить:

````toml
serde_json = "=1.0.152"
````

на:

````toml
serde_json = "=1.0.152"
# Worker и обмен данными (этап 4). Версии wasm-bindgen, js-sys, web-sys и wgpu — те,
# что уже в Cargo.lock (их тянут vello_gpu и wgpu); все опубликованы до 2026-10-10.
wasm-bindgen = "=0.2.129"
wasm-bindgen-futures = "=0.4.79"
js-sys = "=0.3.106"
web-sys = "=0.3.106"
wgpu = { version = "=30.0.1", default-features = false, features = ["std", "wgsl", "webgpu"] }
# Только объявления TypeScript (без JSON и serde-wasm-bindgen внутри tsify).
tsify = { version = "=0.5.8", default-features = false, features = ["wasm-bindgen"] }
serde-wasm-bindgen = "=0.6.5"
serde_bytes = "=0.11.19"
console_error_panic_hook = "=0.1.7"
wasm-bindgen-test = "=0.3.79"
````

`crates/ferrum-wasm/Cargo.toml`:

````toml
[package]
name = "ferrum-wasm"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[lib]
# cdylib — модуль для браузера (wasm-bindgen), rlib — нативные тесты.
crate-type = ["cdylib", "rlib"]

[dependencies]
ferrum-doc = { path = "../ferrum-doc" }
ferrum-editor = { path = "../ferrum-editor" }
kurbo.workspace = true

# proptest под wasm32 не собирается (getrandom без бэкенда): нативные тесты.
[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]
proptest.workspace = true
````

- [ ] **Step 2: Написать падающие тесты**

Тесты нативные (`#![cfg(not(target_arch = "wasm32"))]`): proptest под wasm32 не собирается.

`crates/ferrum-wasm/tests/input.rs`:

````rust
//! Записи кольцевого буфера ввода: каждое событие переживает кодирование и разбор,
//! а любые байты разбираются без паники.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, KeyEvent, Modifiers, PointerButton, PointerEvent, WheelEvent};
use ferrum_wasm::input::{decode, decode_all, encode, Record};
use ferrum_wasm::layout::{field, kind, RECORD_SIZE};
use kurbo::{Point, Vec2};
use proptest::prelude::*;

fn all_kinds() -> Vec<Record> {
    let mods = Modifiers {
        shift: true,
        alt: false,
        ctrl: true,
        meta: true,
    };
    let p = |x: f64, y: f64, button| PointerEvent {
        point: Point::new(x, y),
        button,
        modifiers: mods,
        time_ms: 1234.5,
    };
    let key = KeyEvent {
        code: "BracketLeft".into(),
        key: "х".into(),
        modifiers: Modifiers::ALT,
        repeat: true,
        alt_graph: true,
    };
    vec![
        Record::Input(Input::PointerDown(p(10.25, -3.5, PointerButton::Primary))),
        Record::Input(Input::PointerMove(p(1e7, 2e-3, PointerButton::Primary))),
        Record::Input(Input::PointerUp(p(0.0, 0.0, PointerButton::Secondary))),
        Record::Input(Input::PointerDown(p(5.0, 5.0, PointerButton::Middle))),
        Record::Input(Input::PointerCancel),
        Record::Input(Input::Wheel(WheelEvent {
            point: Point::new(100.0, 200.0),
            delta: Vec2::new(-4.0, 120.0),
            modifiers: Modifiers::CTRL,
        })),
        Record::Input(Input::Pinch {
            point: Point::new(3.0, 4.0),
            scale: 1.25,
        }),
        Record::Input(Input::KeyDown(key.clone())),
        Record::Input(Input::KeyUp(KeyEvent {
            repeat: false,
            alt_graph: false,
            ..key
        })),
        Record::Input(Input::Blur),
        Record::Resize {
            width: 1280.0,
            height: 720.0,
            dpr: 2.0,
        },
    ]
}

#[test]
fn every_kind_round_trips() {
    for rec in all_kinds() {
        let bytes = encode(&rec);
        assert_eq!(decode(&bytes), Some(rec.clone()), "{rec:?}");
    }
    // Пачкой — в том же порядке.
    let bytes: Vec<u8> = all_kinds().iter().flat_map(encode).collect();
    let d = decode_all(&bytes);
    assert_eq!((d.records, d.skipped), (all_kinds(), 0));
}

#[test]
fn layout_of_a_pointer_record_is_fixed() {
    // Те же байты пишет TypeScript: поля на своих местах, little-endian.
    let bytes = encode(&Record::Input(Input::PointerDown(PointerEvent {
        point: Point::new(1.5, 2.5),
        button: PointerButton::Secondary,
        modifiers: Modifiers::SHIFT,
        time_ms: 99.0,
    })));
    assert_eq!(bytes.len(), RECORD_SIZE);
    assert_eq!(bytes[field::KIND], kind::POINTER_DOWN);
    assert_eq!(bytes[field::BUTTON], 2);
    assert_eq!(bytes[field::FLAGS], 1);
    assert_eq!(&bytes[field::A..field::A + 8], &1.5f64.to_le_bytes());
    assert_eq!(&bytes[field::B..field::B + 8], &2.5f64.to_le_bytes());
    assert_eq!(&bytes[field::C..field::C + 8], &99.0f64.to_le_bytes());
}

#[test]
fn long_key_strings_are_clipped_on_char_boundary() {
    let rec = Record::Input(Input::KeyDown(KeyEvent {
        code: "LaunchApplication1".into(),
        key: "ыыыыыыыыыыыыыы".into(), // 28 байт: обрезается до 12 символов (24 байта)
        modifiers: Modifiers::NONE,
        repeat: false,
        alt_graph: false,
    }));
    let Some(Record::Input(Input::KeyDown(k))) = decode(&encode(&rec)) else {
        panic!("клавиша не разобралась");
    };
    assert_eq!(k.code, "LaunchApplication1");
    assert_eq!(k.key, "ы".repeat(12));
}

#[test]
fn malformed_records_are_skipped() {
    let good = encode(&Record::Input(Input::Blur));
    let mut unknown = good;
    unknown[field::KIND] = 200;
    let mut button = encode(&all_kinds()[0]);
    button[field::BUTTON] = 7;
    let mut text = encode(&all_kinds()[7]);
    text[field::CODE_LEN] = 25;
    let mut utf8 = encode(&all_kinds()[7]);
    utf8[field::CODE] = 0xff;
    let mut empty_code = encode(&all_kinds()[7]);
    empty_code[field::CODE_LEN] = 0;
    let mut bad_size = encode(&all_kinds()[10]);
    bad_size[field::C..field::C + 8].copy_from_slice(&f64::NAN.to_le_bytes());
    let mut zero_size = encode(&all_kinds()[10]);
    zero_size[field::A..field::A + 8].copy_from_slice(&0f64.to_le_bytes());
    let mut bytes = Vec::new();
    for rec in [
        unknown, button, text, utf8, empty_code, bad_size, zero_size, good,
    ] {
        bytes.extend_from_slice(&rec);
    }
    bytes.extend_from_slice(&[1, 2, 3]); // неполный хвост
    let d = decode_all(&bytes);
    assert_eq!(d.records, [Record::Input(Input::Blur)]);
    assert_eq!(d.skipped, 8);
    assert_eq!(decode(&good[..RECORD_SIZE - 1]), None);
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let d = decode_all(&bytes);
        prop_assert!(d.records.len() + d.skipped >= bytes.len() / RECORD_SIZE);
    }

    #[test]
    fn arbitrary_records_never_panic(half in proptest::array::uniform32(any::<u8>()), kind in 0u8..12) {
        // Правдоподобный вид записи и мусор во всех остальных байтах.
        let mut rec = [0u8; RECORD_SIZE];
        rec[..32].copy_from_slice(&half);
        rec[32..].copy_from_slice(&half);
        rec[field::KIND] = kind;
        let _ = decode(&rec);
    }
}
````

`crates/ferrum-wasm/tests/layout.rs`:

````rust
//! Раскладка общей памяти: области не пересекаются и выровнены, TypeScript-файл
//! основного потока совпадает со сгенерированным.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_wasm::layout::*;
use ferrum_wasm::live::{LiveField, LiveState};

#[test]
fn regions_are_aligned_and_disjoint() {
    assert_eq!(CTRL_BYTES % 8, 0);
    assert_eq!(LIVE_OFFSET, CTRL_BYTES);
    assert_eq!(LIVE_OFFSET % 8, 0, "Float64Array требует выравнивания 8");
    assert_eq!(RING_OFFSET, LIVE_OFFSET + LIVE_SLOTS * 8);
    assert_eq!(SAB_BYTES, RING_OFFSET + RING_CAPACITY * RECORD_SIZE);
    assert!(RING_CAPACITY.is_power_of_two());
    let used = [
        ctrl::FRAME,
        ctrl::INPUT_WRITE,
        ctrl::INPUT_READ,
        ctrl::LIVE_SEQ,
        ctrl::INPUT_DROPPED,
    ];
    let mut sorted = used.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), used.len(), "ячейки управления совпадают");
    assert!(used.iter().all(|&c| c < ctrl::SLOTS));
    // Поля записи не налезают друг на друга и на строки (остальное проверяет
    // компилятор в `layout.rs`).
    assert_eq!(field::CODE + field::TEXT_MAX, field::KEY);
    assert_eq!(field::KEY + field::TEXT_MAX, RECORD_SIZE);
}

#[test]
fn live_state_round_trips_through_slots() {
    assert!(LiveField::ALL.len() <= LIVE_SLOTS);
    for (i, f) in LiveField::ALL.iter().enumerate() {
        assert_eq!(*f as usize, i);
    }
    let s = LiveState {
        frames: 42.0,
        selection_x: -12.5,
        zoom: 0.25,
        atlas_misses: 3.0,
        ..Default::default()
    };
    let slots = s.to_slots();
    assert_eq!(slots[LiveField::FRAMES as usize], 42.0);
    assert_eq!(slots[LiveField::ATLAS_MISSES as usize], 3.0);
    assert_eq!(LiveState::from_slots(&slots), s);
    // Короткий массив (старый основной поток) — нули, без паники.
    assert_eq!(LiveState::from_slots(&slots[..2]).zoom, 0.0);
}

#[test]
fn typescript_layout_is_up_to_date() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/src/core/layout.gen.ts");
    let expected = typescript();
    if std::env::var_os("FERRUM_UPDATE_LAYOUT").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &expected).unwrap();
    }
    let actual = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        actual == expected,
        "{} устарел: FERRUM_UPDATE_LAYOUT=1 cargo test -p ferrum-wasm --test layout",
        path.display()
    );
    assert!(expected.contains("export const SAB_BYTES = 65856;"));
    assert!(expected.contains("SELECTION_X: 7,"));
}
````

Run: `cargo test -p ferrum-wasm`
Expected: FAIL — `failed to load manifest for workspace member … crates/ferrum-wasm`: у крейта ещё нет `src/lib.rs`.

- [ ] **Step 3: Раскладка, записи, живое состояние**

`crates/ferrum-wasm/src/lib.rs`:

````rust
//! Ядро Ferrum в Web Worker: обмен данными с основным потоком и цикл кадров.
//!
//! Нативная часть (тесты обычным `cargo test`): раскладка общей памяти
//! (`layout`), записи кольцевого буфера ввода (`input`) и блок живого состояния
//! (`live`). Основной поток пишет ввод в кольцевой буфер SharedArrayBuffer (или
//! в буфер сообщения, если страница не изолирована), worker разбирает его в начале
//! кадра и отдаёт редактору по порядку.

pub mod input;
pub mod layout;
pub mod live;
````

`crates/ferrum-wasm/src/layout.rs`:

````rust
//! Раскладка общей памяти (SharedArrayBuffer) между основным потоком и worker'ом.
//!
//! ```text
//! 0               CTRL_BYTES        RING_OFFSET                       SAB_BYTES
//! | Int32 × 16    | Float64 × 32    | записи ввода × RING_CAPACITY     |
//! | управление    | живое состояние | по RECORD_SIZE байт              |
//! ```
//!
//! Управляющий блок — счётчики `Atomics`: кадр (основной поток увеличивает его в
//! `requestAnimationFrame` и будит worker `Atomics.notify`), позиции записи и чтения
//! кольцевого буфера, версия живого состояния. Кольцевой буфер — один писатель
//! (основной поток) и один читатель (worker): писатель кладёт запись в ячейку
//! `write % RING_CAPACITY` и только потом публикует `write + 1` через
//! `Atomics.store`; читатель забирает всё до `write` и публикует `read`. Счётчики
//! растут монотонно и сравниваются по модулю 2³², поэтому переполнение не страшно.
//!
//! Живое состояние пишет worker под счётчиком-«замком» (seqlock): нечётное
//! значение — идёт запись, читатель повторяет чтение, пока значение до и после не
//! совпадёт и не будет чётным.
//!
//! TypeScript-константы для основного потока генерируются отсюда
//! (`typescript()`), файл `apps/web/src/core/layout.gen.ts` сверяет тест.

use crate::live::LiveField;

/// Размер одной записи ввода в байтах.
pub const RECORD_SIZE: usize = 64;
/// Сколько записей помещается в кольцевой буфер (степень двойки). 1024 события —
/// около 16 кадров по 60 coalesced-движений указателя: столько worker не отстаёт
/// даже при открытии большого документа.
pub const RING_CAPACITY: usize = 1024;

/// Ячейки управляющего блока (индексы `Int32Array`).
pub mod ctrl {
    /// Счётчик кадров основного потока (`requestAnimationFrame`).
    pub const FRAME: usize = 0;
    /// Сколько записей ввода положено (писатель — основной поток).
    pub const INPUT_WRITE: usize = 1;
    /// Сколько записей ввода забрано (писатель — worker).
    pub const INPUT_READ: usize = 2;
    /// Версия живого состояния (seqlock: нечётная — идёт запись).
    pub const LIVE_SEQ: usize = 3;
    /// Сколько событий основной поток не смог положить: буфер был полон и
    /// очередь ожидания переполнилась.
    pub const INPUT_DROPPED: usize = 4;
    /// Ячеек в блоке.
    pub const SLOTS: usize = 16;
}

/// Байт управляющего блока.
pub const CTRL_BYTES: usize = ctrl::SLOTS * 4;
/// Начало живого состояния (`Float64Array`).
pub const LIVE_OFFSET: usize = CTRL_BYTES;
/// Ячеек живого состояния (с запасом под новые поля).
pub const LIVE_SLOTS: usize = 32;
/// Начало кольцевого буфера.
pub const RING_OFFSET: usize = LIVE_OFFSET + LIVE_SLOTS * 8;
/// Размер SharedArrayBuffer.
pub const SAB_BYTES: usize = RING_OFFSET + RING_CAPACITY * RECORD_SIZE;

/// Вид записи ввода (байт 0).
pub mod kind {
    pub const POINTER_DOWN: u8 = 1;
    pub const POINTER_MOVE: u8 = 2;
    pub const POINTER_UP: u8 = 3;
    pub const POINTER_CANCEL: u8 = 4;
    pub const WHEEL: u8 = 5;
    pub const PINCH: u8 = 6;
    pub const KEY_DOWN: u8 = 7;
    pub const KEY_UP: u8 = 8;
    pub const BLUR: u8 = 9;
    /// Размер холста: CSS-пиксели и `devicePixelRatio`.
    pub const RESIZE: u8 = 10;
}

/// Биты модификаторов (байт 2).
pub mod flag {
    pub const SHIFT: u8 = 1;
    pub const ALT: u8 = 2;
    pub const CTRL: u8 = 4;
    pub const META: u8 = 8;
    /// `KeyboardEvent.getModifierState("AltGraph")` — только настоящее состояние.
    pub const ALT_GRAPH: u8 = 16;
    pub const REPEAT: u8 = 32;
}

/// Смещения полей записи (little-endian).
///
/// | вид | a (8) | b (16) | c (24) | d (32) |
/// |---|---|---|---|---|
/// | указатель | x | y | время, мс | — |
/// | колесо | x | y | Δx | Δy |
/// | щипок | x | y | множитель | — |
/// | размер | ширина | высота | dpr | — |
///
/// У клавиш: время в `a`, `code` (UTF-8, до `TEXT_MAX` байт) с байта `CODE`,
/// `key` — с байта `KEY`; длины — в байтах `CODE_LEN` и `KEY_LEN`.
pub mod field {
    pub const KIND: usize = 0;
    /// Кнопка как `PointerEvent.button`: 0 — основная, 1 — средняя, 2 — правая.
    pub const BUTTON: usize = 1;
    pub const FLAGS: usize = 2;
    pub const CODE_LEN: usize = 3;
    pub const KEY_LEN: usize = 4;
    pub const A: usize = 8;
    pub const B: usize = 16;
    pub const C: usize = 24;
    pub const D: usize = 32;
    pub const CODE: usize = 16;
    pub const KEY: usize = 40;
    pub const TEXT_MAX: usize = 24;
}

// Раскладку проверяет компилятор: области выровнены, поля записи не налезают
// друг на друга.
const _: () = {
    assert!(LIVE_OFFSET.is_multiple_of(8));
    assert!(RING_OFFSET.is_multiple_of(8));
    assert!(RING_CAPACITY.is_power_of_two());
    assert!(field::KEY_LEN < field::A);
    assert!(field::D + 8 <= RECORD_SIZE);
    assert!(field::CODE + field::TEXT_MAX == field::KEY);
    assert!(field::KEY + field::TEXT_MAX == RECORD_SIZE);
};

/// Коды бэкендов рендера в живом состоянии.
pub const BACKENDS: [&str; 4] = ["none", "webgpu", "webgl", "cpu"];

/// Содержимое `apps/web/src/core/layout.gen.ts`.
pub fn typescript() -> String {
    let mut out = String::new();
    out.push_str("// Сгенерировано из crates/ferrum-wasm/src/layout.rs — не править руками.\n");
    out.push_str("// Обновить: FERRUM_UPDATE_LAYOUT=1 cargo test -p ferrum-wasm --test layout\n\n");
    for (name, value) in [
        ("RECORD_SIZE", RECORD_SIZE),
        ("RING_CAPACITY", RING_CAPACITY),
        ("CTRL_BYTES", CTRL_BYTES),
        ("LIVE_OFFSET", LIVE_OFFSET),
        ("LIVE_SLOTS", LIVE_SLOTS),
        ("RING_OFFSET", RING_OFFSET),
        ("SAB_BYTES", SAB_BYTES),
    ] {
        out.push_str(&format!("export const {name} = {value};\n"));
    }
    let block = |out: &mut String, name: &str, items: &[(&str, usize)]| {
        out.push_str(&format!("\nexport const {name} = {{\n"));
        for (k, v) in items {
            out.push_str(&format!("  {k}: {v},\n"));
        }
        out.push_str("} as const;\n");
    };
    block(
        &mut out,
        "Ctrl",
        &[
            ("FRAME", ctrl::FRAME),
            ("INPUT_WRITE", ctrl::INPUT_WRITE),
            ("INPUT_READ", ctrl::INPUT_READ),
            ("LIVE_SEQ", ctrl::LIVE_SEQ),
            ("INPUT_DROPPED", ctrl::INPUT_DROPPED),
        ],
    );
    block(
        &mut out,
        "Kind",
        &[
            ("POINTER_DOWN", kind::POINTER_DOWN as usize),
            ("POINTER_MOVE", kind::POINTER_MOVE as usize),
            ("POINTER_UP", kind::POINTER_UP as usize),
            ("POINTER_CANCEL", kind::POINTER_CANCEL as usize),
            ("WHEEL", kind::WHEEL as usize),
            ("PINCH", kind::PINCH as usize),
            ("KEY_DOWN", kind::KEY_DOWN as usize),
            ("KEY_UP", kind::KEY_UP as usize),
            ("BLUR", kind::BLUR as usize),
            ("RESIZE", kind::RESIZE as usize),
        ],
    );
    block(
        &mut out,
        "Flag",
        &[
            ("SHIFT", flag::SHIFT as usize),
            ("ALT", flag::ALT as usize),
            ("CTRL", flag::CTRL as usize),
            ("META", flag::META as usize),
            ("ALT_GRAPH", flag::ALT_GRAPH as usize),
            ("REPEAT", flag::REPEAT as usize),
        ],
    );
    block(
        &mut out,
        "Field",
        &[
            ("KIND", field::KIND),
            ("BUTTON", field::BUTTON),
            ("FLAGS", field::FLAGS),
            ("CODE_LEN", field::CODE_LEN),
            ("KEY_LEN", field::KEY_LEN),
            ("A", field::A),
            ("B", field::B),
            ("C", field::C),
            ("D", field::D),
            ("CODE", field::CODE),
            ("KEY", field::KEY),
            ("TEXT_MAX", field::TEXT_MAX),
        ],
    );
    let live: Vec<(&str, usize)> = LiveField::ALL
        .iter()
        .map(|f| (f.ts_name(), *f as usize))
        .collect();
    block(&mut out, "Live", &live);
    out.push_str("\nexport const LIVE_NAMES = [\n");
    for f in LiveField::ALL {
        out.push_str(&format!("  '{}',\n", f.js_name()));
    }
    out.push_str("] as const;\n");
    out.push_str("\nexport const BACKENDS = [");
    let names: Vec<String> = BACKENDS.iter().map(|b| format!("'{b}'")).collect();
    out.push_str(&names.join(", "));
    out.push_str("] as const;\n");
    out
}
````

`crates/ferrum-wasm/src/input.rs`:

````rust
//! Записи кольцевого буфера ввода: кодирование и разбор. Основной поток пишет те же
//! байты из TypeScript (`apps/web/src/core/input.ts`), worker разбирает их здесь.
//!
//! Данные из общей памяти не доверенные: другой поток мог записать что угодно.
//! Разбор не паникует ни на каких байтах — непонятная запись пропускается
//! целиком и учитывается в `Decoded::skipped`.

use ferrum_editor::{Input, KeyEvent, Modifiers, PointerButton, PointerEvent, WheelEvent};
use kurbo::{Point, Vec2};

use crate::layout::{field, flag, kind, RECORD_SIZE};

/// Наибольшая сторона холста в CSS-пикселях, которую принимает запись размера.
pub const MAX_VIEWPORT_SIDE: f64 = 100_000.0;
/// Наибольший `devicePixelRatio`.
pub const MAX_DPR: f64 = 16.0;

/// Одна запись буфера: событие редактора или новый размер холста.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Input(Input),
    /// Размер холста в CSS-пикселях и `devicePixelRatio` (`ResizeObserver`).
    Resize {
        width: f64,
        height: f64,
        dpr: f64,
    },
}

/// Результат разбора пачки записей.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Decoded {
    pub records: Vec<Record>,
    /// Пропущено непонятных записей (и неполный хвост, если длина не кратна
    /// `RECORD_SIZE`).
    pub skipped: usize,
}

fn put_f64(out: &mut [u8; RECORD_SIZE], at: usize, v: f64) {
    out[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn get_f64(rec: &[u8], at: usize) -> f64 {
    f64::from_le_bytes(rec[at..at + 8].try_into().expect("8 байт"))
}

fn mods_flags(m: Modifiers) -> u8 {
    let mut f = 0;
    if m.shift {
        f |= flag::SHIFT;
    }
    if m.alt {
        f |= flag::ALT;
    }
    if m.ctrl {
        f |= flag::CTRL;
    }
    if m.meta {
        f |= flag::META;
    }
    f
}

fn flags_mods(f: u8) -> Modifiers {
    Modifiers {
        shift: f & flag::SHIFT != 0,
        alt: f & flag::ALT != 0,
        ctrl: f & flag::CTRL != 0,
        meta: f & flag::META != 0,
    }
}

/// Обрезает строку до `TEXT_MAX` байт по границе символа.
fn clip(s: &str) -> &str {
    if s.len() <= field::TEXT_MAX {
        return s;
    }
    let mut end = field::TEXT_MAX;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn put_text(out: &mut [u8; RECORD_SIZE], len_at: usize, at: usize, s: &str) {
    let s = clip(s);
    out[len_at] = s.len() as u8;
    out[at..at + s.len()].copy_from_slice(s.as_bytes());
}

fn get_text(rec: &[u8], len_at: usize, at: usize) -> Option<String> {
    let len = rec[len_at] as usize;
    if len > field::TEXT_MAX {
        return None;
    }
    std::str::from_utf8(&rec[at..at + len])
        .ok()
        .map(str::to_string)
}

fn button_code(b: PointerButton) -> u8 {
    match b {
        PointerButton::Primary => 0,
        PointerButton::Middle => 1,
        PointerButton::Secondary => 2,
    }
}

/// Кодирует запись. Строки клавиш длиннее `TEXT_MAX` байт обрезаются по границе
/// символа (настоящие `KeyboardEvent.code` не длиннее 20 символов ASCII).
pub fn encode(record: &Record) -> [u8; RECORD_SIZE] {
    let mut out = [0u8; RECORD_SIZE];
    let pointer = |out: &mut [u8; RECORD_SIZE], k: u8, e: &PointerEvent| {
        out[field::KIND] = k;
        out[field::BUTTON] = button_code(e.button);
        out[field::FLAGS] = mods_flags(e.modifiers);
        put_f64(out, field::A, e.point.x);
        put_f64(out, field::B, e.point.y);
        put_f64(out, field::C, e.time_ms);
    };
    let key = |out: &mut [u8; RECORD_SIZE], k: u8, e: &KeyEvent| {
        out[field::KIND] = k;
        let mut f = mods_flags(e.modifiers);
        if e.alt_graph {
            f |= flag::ALT_GRAPH;
        }
        if e.repeat {
            f |= flag::REPEAT;
        }
        out[field::FLAGS] = f;
        put_text(out, field::CODE_LEN, field::CODE, &e.code);
        put_text(out, field::KEY_LEN, field::KEY, &e.key);
    };
    match record {
        Record::Input(Input::PointerDown(e)) => pointer(&mut out, kind::POINTER_DOWN, e),
        Record::Input(Input::PointerMove(e)) => pointer(&mut out, kind::POINTER_MOVE, e),
        Record::Input(Input::PointerUp(e)) => pointer(&mut out, kind::POINTER_UP, e),
        Record::Input(Input::PointerCancel) => out[field::KIND] = kind::POINTER_CANCEL,
        Record::Input(Input::Wheel(e)) => {
            out[field::KIND] = kind::WHEEL;
            out[field::FLAGS] = mods_flags(e.modifiers);
            put_f64(&mut out, field::A, e.point.x);
            put_f64(&mut out, field::B, e.point.y);
            put_f64(&mut out, field::C, e.delta.x);
            put_f64(&mut out, field::D, e.delta.y);
        }
        Record::Input(Input::Pinch { point, scale }) => {
            out[field::KIND] = kind::PINCH;
            put_f64(&mut out, field::A, point.x);
            put_f64(&mut out, field::B, point.y);
            put_f64(&mut out, field::C, *scale);
        }
        Record::Input(Input::KeyDown(e)) => key(&mut out, kind::KEY_DOWN, e),
        Record::Input(Input::KeyUp(e)) => key(&mut out, kind::KEY_UP, e),
        Record::Input(Input::Blur) => out[field::KIND] = kind::BLUR,
        Record::Resize { width, height, dpr } => {
            out[field::KIND] = kind::RESIZE;
            put_f64(&mut out, field::A, *width);
            put_f64(&mut out, field::B, *height);
            put_f64(&mut out, field::C, *dpr);
        }
    }
    out
}

/// Разбирает одну запись. `None` — запись непонятна (неизвестный вид, кнопка,
/// строка не UTF-8, недопустимый размер) или длина не `RECORD_SIZE`.
///
/// Нечисловые координаты указателя не отбрасываются здесь: их отбрасывает сам
/// редактор (`Editor::handle`), одинаково для любого источника событий.
pub fn decode(rec: &[u8]) -> Option<Record> {
    if rec.len() != RECORD_SIZE {
        return None;
    }
    let flags = rec[field::FLAGS];
    let modifiers = flags_mods(flags);
    let point = || Point::new(get_f64(rec, field::A), get_f64(rec, field::B));
    let pointer = |button: bool| -> Option<PointerEvent> {
        let button = if button {
            match rec[field::BUTTON] {
                0 => PointerButton::Primary,
                1 => PointerButton::Middle,
                2 => PointerButton::Secondary,
                _ => return None,
            }
        } else {
            PointerButton::Primary
        };
        Some(PointerEvent {
            point: point(),
            button,
            modifiers,
            time_ms: get_f64(rec, field::C),
        })
    };
    let key = || -> Option<KeyEvent> {
        let code = get_text(rec, field::CODE_LEN, field::CODE)?;
        if code.is_empty() {
            return None;
        }
        Some(KeyEvent {
            code,
            key: get_text(rec, field::KEY_LEN, field::KEY)?,
            modifiers,
            repeat: flags & flag::REPEAT != 0,
            alt_graph: flags & flag::ALT_GRAPH != 0,
        })
    };
    let input = match rec[field::KIND] {
        kind::POINTER_DOWN => Input::PointerDown(pointer(true)?),
        kind::POINTER_MOVE => Input::PointerMove(pointer(false)?),
        kind::POINTER_UP => Input::PointerUp(pointer(true)?),
        kind::POINTER_CANCEL => Input::PointerCancel,
        kind::WHEEL => Input::Wheel(WheelEvent {
            point: point(),
            delta: Vec2::new(get_f64(rec, field::C), get_f64(rec, field::D)),
            modifiers,
        }),
        kind::PINCH => Input::Pinch {
            point: point(),
            scale: get_f64(rec, field::C),
        },
        kind::KEY_DOWN => Input::KeyDown(key()?),
        kind::KEY_UP => Input::KeyUp(key()?),
        kind::BLUR => Input::Blur,
        kind::RESIZE => {
            let (width, height, dpr) = (
                get_f64(rec, field::A),
                get_f64(rec, field::B),
                get_f64(rec, field::C),
            );
            let side = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_VIEWPORT_SIDE;
            if !(side(width) && side(height) && dpr.is_finite() && dpr > 0.0 && dpr <= MAX_DPR) {
                return None;
            }
            return Some(Record::Resize { width, height, dpr });
        }
        _ => return None,
    };
    Some(Record::Input(input))
}

/// Разбирает подряд идущие записи. Неполный хвост считается одной пропущенной
/// записью.
pub fn decode_all(bytes: &[u8]) -> Decoded {
    let mut out = Decoded::default();
    let chunks = bytes.chunks_exact(RECORD_SIZE);
    if !chunks.remainder().is_empty() {
        out.skipped += 1;
    }
    for rec in chunks {
        match decode(rec) {
            Some(r) => out.records.push(r),
            None => out.skipped += 1,
        }
    }
    out
}
````

`crates/ferrum-wasm/src/live.rs`:

````rust
//! Блок живого состояния: то, что основной поток читает в каждом
//! `requestAnimationFrame` без сообщений — значения выделения во время
//! перетаскивания, камера, статистика кадра. Все поля — `f64` (одна ячейка
//! `Float64Array`); логические — 0 или 1, бэкенд — индекс в `layout::BACKENDS`.

use crate::layout::LIVE_SLOTS;

macro_rules! live_fields {
    ($($(#[doc = $doc:literal])* $name:ident => $variant:ident, $ts:literal;)+) => {
        /// Поле живого состояния; значение — номер ячейки. Имена — как у констант
        /// TypeScript (`Live.FRAME_MS`), чтобы их было легко искать в обоих языках.
        #[allow(non_camel_case_types)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum LiveField {
            $($(#[doc = $doc])* $variant,)+
        }

        impl LiveField {
            pub const ALL: &'static [LiveField] = &[$(LiveField::$variant,)+];

            /// Имя константы в TypeScript (`Live.FRAME_MS`).
            pub fn ts_name(self) -> &'static str {
                match self {
                    $(LiveField::$variant => stringify!($variant),)+
                }
            }

            /// Имя поля объекта в TypeScript (`live.frameMs`).
            pub fn js_name(self) -> &'static str {
                match self {
                    $(LiveField::$variant => $ts,)+
                }
            }
        }

        #[derive(Debug, Clone, Copy, Default, PartialEq)]
        pub struct LiveState {
            $($(#[doc = $doc])* pub $name: f64,)+
        }

        impl LiveState {
            /// Значения по ячейкам; лишние ячейки — нули.
            pub fn to_slots(&self) -> [f64; LIVE_SLOTS] {
                let mut out = [0.0; LIVE_SLOTS];
                $(out[LiveField::$variant as usize] = self.$name;)+
                out
            }

            /// Обратно из ячеек; недостающие ячейки — нули.
            pub fn from_slots(slots: &[f64]) -> LiveState {
                let get = |f: LiveField| slots.get(f as usize).copied().unwrap_or(0.0);
                LiveState {
                    $($name: get(LiveField::$variant),)+
                }
            }
        }
    };
}

live_fields! {
    /// Нарисовано кадров с начала работы.
    frames => FRAMES, "frames";
    /// Время последнего кадра в worker'е (рисование и отправка в GPU), мс.
    frame_ms => FRAME_MS, "frameMs";
    /// Медиана и 95-й процентиль времени кадра по последним 120 кадрам, мс.
    frame_p50 => FRAME_P50, "frameP50";
    frame_p95 => FRAME_P95, "frameP95";
    /// Нарисовано кадров за последнюю секунду.
    fps => FPS, "fps";
    /// Активный бэкенд рендера (`layout::BACKENDS`).
    backend => BACKEND, "backend";
    /// Выделение: число узлов и значения панели «Положение» — у одного узла его
    /// x, y (у родителя), ширина, высота и поворот; у нескольких — общая рамка
    /// в мире.
    selection_count => SELECTION_COUNT, "selectionCount";
    selection_x => SELECTION_X, "selectionX";
    selection_y => SELECTION_Y, "selectionY";
    selection_w => SELECTION_W, "selectionW";
    selection_h => SELECTION_H, "selectionH";
    selection_rotation => SELECTION_ROTATION, "selectionRotation";
    /// Камера: центр в мире и зум.
    camera_x => CAMERA_X, "cameraX";
    camera_y => CAMERA_Y, "cameraY";
    zoom => ZOOM, "zoom";
    /// Холст в CSS-пикселях и `devicePixelRatio`, который применил worker.
    viewport_w => VIEWPORT_W, "viewportW";
    viewport_h => VIEWPORT_H, "viewportH";
    dpr => DPR, "dpr";
    /// Размер холста в пикселях устройства.
    device_w => DEVICE_W, "deviceW";
    device_h => DEVICE_H, "deviceH";
    /// Идёт жест (кнопка нажата).
    busy => BUSY, "busy";
    /// Шагов истории, созданных редактором; можно ли отменить и повторить.
    steps => STEPS, "steps";
    can_undo => CAN_UNDO, "canUndo";
    can_redo => CAN_REDO, "canRedo";
    /// Узлов в сцене текущей страницы.
    nodes => NODES, "nodes";
    /// Записей ввода принято и пропущено (непонятных).
    inputs => INPUTS, "inputs";
    skipped => SKIPPED, "skipped";
    /// Картинок в атласе GPU и картинок, которым не хватило места.
    atlas_images => ATLAS_IMAGES, "atlasImages";
    atlas_misses => ATLAS_MISSES, "atlasMisses";
}
````

- [ ] **Step 4: Константы для TypeScript**

Файл генерируется тестом; ниже — ожидаемое содержимое, тест сверяет его байт в байт.

`apps/web/src/core/layout.gen.ts`:

````ts
// Сгенерировано из crates/ferrum-wasm/src/layout.rs — не править руками.
// Обновить: FERRUM_UPDATE_LAYOUT=1 cargo test -p ferrum-wasm --test layout

export const RECORD_SIZE = 64;
export const RING_CAPACITY = 1024;
export const CTRL_BYTES = 64;
export const LIVE_OFFSET = 64;
export const LIVE_SLOTS = 32;
export const RING_OFFSET = 320;
export const SAB_BYTES = 65856;

export const Ctrl = {
  FRAME: 0,
  INPUT_WRITE: 1,
  INPUT_READ: 2,
  LIVE_SEQ: 3,
  INPUT_DROPPED: 4,
} as const;

export const Kind = {
  POINTER_DOWN: 1,
  POINTER_MOVE: 2,
  POINTER_UP: 3,
  POINTER_CANCEL: 4,
  WHEEL: 5,
  PINCH: 6,
  KEY_DOWN: 7,
  KEY_UP: 8,
  BLUR: 9,
  RESIZE: 10,
} as const;

export const Flag = {
  SHIFT: 1,
  ALT: 2,
  CTRL: 4,
  META: 8,
  ALT_GRAPH: 16,
  REPEAT: 32,
} as const;

export const Field = {
  KIND: 0,
  BUTTON: 1,
  FLAGS: 2,
  CODE_LEN: 3,
  KEY_LEN: 4,
  A: 8,
  B: 16,
  C: 24,
  D: 32,
  CODE: 16,
  KEY: 40,
  TEXT_MAX: 24,
} as const;

export const Live = {
  FRAMES: 0,
  FRAME_MS: 1,
  FRAME_P50: 2,
  FRAME_P95: 3,
  FPS: 4,
  BACKEND: 5,
  SELECTION_COUNT: 6,
  SELECTION_X: 7,
  SELECTION_Y: 8,
  SELECTION_W: 9,
  SELECTION_H: 10,
  SELECTION_ROTATION: 11,
  CAMERA_X: 12,
  CAMERA_Y: 13,
  ZOOM: 14,
  VIEWPORT_W: 15,
  VIEWPORT_H: 16,
  DPR: 17,
  DEVICE_W: 18,
  DEVICE_H: 19,
  BUSY: 20,
  STEPS: 21,
  CAN_UNDO: 22,
  CAN_REDO: 23,
  NODES: 24,
  INPUTS: 25,
  SKIPPED: 26,
  ATLAS_IMAGES: 27,
  ATLAS_MISSES: 28,
} as const;

export const LIVE_NAMES = [
  'frames',
  'frameMs',
  'frameP50',
  'frameP95',
  'fps',
  'backend',
  'selectionCount',
  'selectionX',
  'selectionY',
  'selectionW',
  'selectionH',
  'selectionRotation',
  'cameraX',
  'cameraY',
  'zoom',
  'viewportW',
  'viewportH',
  'dpr',
  'deviceW',
  'deviceH',
  'busy',
  'steps',
  'canUndo',
  'canRedo',
  'nodes',
  'inputs',
  'skipped',
  'atlasImages',
  'atlasMisses',
] as const;

export const BACKENDS = ['none', 'webgpu', 'webgl', 'cpu'] as const;
````

Run: `cargo test -p ferrum-wasm --test layout`
Expected: PASS, `3 passed` (при расхождении: `FERRUM_UPDATE_LAYOUT=1 cargo test -p ferrum-wasm --test layout` и сверить с текстом выше).

Run: `cargo test -p ferrum-wasm --test input`
Expected: PASS, `6 passed` (2 из них — proptest).

Run: `cargo clippy -p ferrum-wasm --all-targets -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 5: Закоммитить**

```bash
git add Cargo.toml Cargo.lock crates/ferrum-wasm apps/web/src/core/layout.gen.ts
git commit -m "feat(wasm): раскладка общей памяти, записи ввода и живое состояние"
```

---

### Task 5: Протокол: команды и события

Команды основного потока и события worker'а — enum'ы с `serde` (`tag = "type"`, имена в camelCase) и `tsify`: при сборке wasm-bindgen TypeScript-типы `Command`, `CoreEvent`, `Tool`, `Theme`, `CursorKind`, `Backend` попадают в `ferrum_wasm.d.ts`. Событие называется `CoreEvent`, чтобы в TypeScript не путать его с DOM `Event`. Байты — `serde_bytes::ByteBuf`: serde-wasm-bindgen отдаёт их `Uint8Array`, буфер которого worker передаёт как transferable. `deny_unknown_fields`: команда с лишним полем — ошибка, а не тихо проигнорированное поле. Выделение в событии — число и не больше 1000 идентификаторов: панели получают из worker'а готовые срезы.

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/ferrum-wasm/Cargo.toml`
- Modify: `crates/ferrum-wasm/src/lib.rs`
- Create: `crates/ferrum-wasm/src/protocol.rs`
- Test: `crates/ferrum-wasm/tests/protocol.rs`

**Interfaces:**
- Consumes: `layout::BACKENDS`.
- Produces: `protocol::{Command { Open, Demo, Execute, SetTool, SetTheme, Paste, PlaceImage, Save, ReadPixels }, CoreEvent { Ready, Opened, Selection, Tool, Cursor, Document, Page, Clipboard, ClipboardPng, PasteRequested, RequestImage, ToggleUi, Saved, Pixels, Error }, Tool, Theme, CursorKind, Backend { Webgpu, Webgl, Cpu } с code/name/parse, MAX_DEMO_NODES = 500 000, SELECTION_IDS_MAX = 1000}`.

- [ ] **Step 1: Зависимости**

`tsify` — с фичей `js`: типы соответствуют serde-wasm-bindgen (без serde_json внутри).

В `Cargo.toml` заменить:

````toml
wgpu = { version = "=30.0.1", default-features = false, features = ["std", "wgsl", "webgpu"] }
# Только объявления TypeScript (без JSON и serde-wasm-bindgen внутри tsify).
tsify = { version = "=0.5.8", default-features = false, features = ["wasm-bindgen"] }
serde-wasm-bindgen = "=0.6.5"
````

на:

````toml
wgpu = { version = "=30.0.1", default-features = false, features = ["std", "wgsl", "webgpu"] }
# Типы TypeScript для serde-wasm-bindgen (фича `js`, без serde_json внутри tsify).
tsify = { version = "=0.5.8", default-features = false, features = ["js"] }
serde-wasm-bindgen = "=0.6.5"
````

В `crates/ferrum-wasm/Cargo.toml` заменить:

````toml
kurbo.workspace = true

# proptest под wasm32 не собирается (getrandom без бэкенда): нативные тесты.
[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]
proptest.workspace = true
````

на:

````toml
kurbo.workspace = true
serde.workspace = true
serde_bytes.workspace = true
tsify.workspace = true
wasm-bindgen.workspace = true

# proptest под wasm32 не собирается (getrandom без бэкенда): нативные тесты.
[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]
proptest.workspace = true
serde_json.workspace = true
````

- [ ] **Step 2: Написать падающие тесты**

`crates/ferrum-wasm/tests/protocol.rs`:

````rust
//! Сообщения worker'а: каждая команда и каждое событие переживают сериализацию,
//! имена полей — как ждёт TypeScript, неверные команды дают ошибку, а не панику.
//! (Тот же serde, что использует serde-wasm-bindgen в браузере; разбор настоящих
//! JS-объектов проверяет wasm-тест `tests/web.rs`.)
#![cfg(not(target_arch = "wasm32"))]

use ferrum_wasm::protocol::*;
use serde_bytes::ByteBuf;
use serde_json::json;

fn bytes(b: &[u8]) -> ByteBuf {
    ByteBuf::from(b.to_vec())
}

fn commands() -> Vec<Command> {
    vec![
        Command::Open {
            bytes: bytes(b"FERRUM"),
        },
        Command::Demo { nodes: 100_000 },
        Command::Execute {
            command: "edit.undo".into(),
        },
        Command::SetTool {
            tool: Tool::Ellipse,
        },
        Command::SetTheme {
            theme: Theme::Light,
        },
        Command::Paste { bytes: bytes(&[1]) },
        Command::PlaceImage {
            bytes: bytes(&[0x89, b'P']),
        },
        Command::Save,
        Command::ReadPixels { reference: true },
    ]
}

fn events() -> Vec<CoreEvent> {
    vec![
        CoreEvent::Ready {
            backend: Backend::Webgl,
            fallbacks: vec!["webgpu: нет адаптера".into()],
        },
        CoreEvent::Opened {
            pages: 2,
            nodes: 10,
            warnings: vec![],
        },
        CoreEvent::Selection {
            count: 2,
            ids: vec!["1@2".into(), "3@4".into()],
        },
        CoreEvent::Tool { tool: Tool::Hand },
        CoreEvent::Cursor {
            cursor: CursorKind::Resize,
            angle: 45.0,
        },
        CoreEvent::Document {
            can_undo: true,
            can_redo: false,
            steps: 3.0,
        },
        CoreEvent::Page { id: "p1".into() },
        CoreEvent::Clipboard { bytes: bytes(b"x") },
        CoreEvent::ClipboardPng { bytes: bytes(b"p") },
        CoreEvent::PasteRequested,
        CoreEvent::RequestImage,
        CoreEvent::ToggleUi,
        CoreEvent::Saved { bytes: bytes(b"s") },
        CoreEvent::Pixels {
            backend: Backend::Cpu,
            width: 1,
            height: 1,
            rgba: bytes(&[1, 2, 3, 4]),
        },
        CoreEvent::Error {
            message: "ой".into(),
        },
    ]
}

#[test]
fn every_message_round_trips() {
    for c in commands() {
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(serde_json::from_value::<Command>(v).unwrap(), c);
    }
    for e in events() {
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(serde_json::from_value::<CoreEvent>(v).unwrap(), e);
    }
}

#[test]
fn field_names_match_typescript() {
    assert_eq!(
        serde_json::to_value(Command::SetTool { tool: Tool::Rect }).unwrap(),
        json!({"type": "setTool", "tool": "rect"})
    );
    assert_eq!(
        serde_json::to_value(Command::ReadPixels { reference: false }).unwrap(),
        json!({"type": "readPixels", "reference": false})
    );
    assert_eq!(
        serde_json::to_value(&events()[5]).unwrap(),
        json!({"type": "document", "canUndo": true, "canRedo": false, "steps": 3.0})
    );
    assert_eq!(
        serde_json::to_value(&events()[8]).unwrap()["type"],
        json!("clipboardPng")
    );
    assert_eq!(
        serde_json::to_value(Command::Save).unwrap(),
        json!({"type": "save"})
    );
}

#[test]
fn bad_commands_are_errors_not_panics() {
    for bad in [
        json!({"type": "format"}),
        json!({"type": "demo"}),
        json!({"type": "demo", "nodes": -1}),
        json!({"type": "demo", "nodes": 1, "extra": 1}),
        json!({"type": "setTool", "tool": "pen"}),
        json!({"tool": "rect"}),
        json!("save"),
        json!(null),
    ] {
        assert!(
            serde_json::from_value::<Command>(bad.clone()).is_err(),
            "принято: {bad}"
        );
    }
}

#[test]
fn backend_codes_match_layout() {
    for b in [Backend::Webgpu, Backend::Webgl, Backend::Cpu] {
        assert_eq!(ferrum_wasm::layout::BACKENDS[b.code() as usize], b.name());
        assert_eq!(Backend::parse(b.name()), Some(b));
    }
    assert_eq!(Backend::parse("vulkan"), None);
}
````

Run: `cargo test -p ferrum-wasm --test protocol`
Expected: FAIL — не компилируется: нет модуля `protocol`.

- [ ] **Step 3: Типы сообщений**

В `crates/ferrum-wasm/src/lib.rs` заменить:

````rust
pub mod live;
````

на:

````rust
pub mod live;
pub mod protocol;
````

`crates/ferrum-wasm/src/protocol.rs`:

````rust
//! Сообщения между основным потоком и worker'ом. Типы описаны здесь, TypeScript-
//! типы генерирует `tsify` (они попадают в `ferrum_wasm.d.ts` при сборке
//! wasm-bindgen). Через `postMessage` они идут как обычные объекты
//! (serde-wasm-bindgen), байты — `Uint8Array`, буфер которых передаётся как
//! transferable.
//!
//! Команды от основного потока не доверенные: неизвестная команда, лишние или
//! неверные поля дают событие `Error`, а не панику worker'а.

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use tsify::Tsify;

/// Наибольшая страница-пример: столько узлов генерирует `Demo`.
pub const MAX_DEMO_NODES: u32 = 500_000;
/// Сколько идентификаторов выделения уходит в событии `Selection`: панели
/// получают из worker'а готовые срезы, полный список в основной поток не нужен.
pub const SELECTION_IDS_MAX: usize = 1000;

/// Инструмент.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    Move,
    Frame,
    Rect,
    Ellipse,
    Line,
    Hand,
    Image,
}

/// Тема оверлеев.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    Dark,
    Light,
}

/// CSS-курсор над холстом. У ресайза и поворота — угол на экране в градусах.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum CursorKind {
    Default,
    Crosshair,
    Grab,
    Grabbing,
    Duplicate,
    Resize,
    Rotate,
}

/// Бэкенд рендера (`layout::BACKENDS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Backend {
    Webgpu,
    Webgl,
    Cpu,
}

impl Backend {
    /// Индекс в `layout::BACKENDS` (живое состояние).
    pub fn code(self) -> f64 {
        match self {
            Backend::Webgpu => 1.0,
            Backend::Webgl => 2.0,
            Backend::Cpu => 3.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Backend::Webgpu => "webgpu",
            Backend::Webgl => "webgl",
            Backend::Cpu => "cpu",
        }
    }

    pub fn parse(s: &str) -> Option<Backend> {
        match s {
            "webgpu" => Some(Backend::Webgpu),
            "webgl" => Some(Backend::Webgl),
            "cpu" => Some(Backend::Cpu),
            _ => None,
        }
    }
}

/// Команда основного потока worker'у.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Command {
    /// Открыть файл `.ferrum`.
    Open {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Новый документ — страница-пример из `nodes` узлов (фреймы по 100 узлов).
    Demo {
        nodes: u32,
    },
    /// Команда редактора по ключу таблицы команд (`edit.undo`, `tool.rect`…).
    Execute {
        command: String,
    },
    SetTool {
        tool: Tool,
    },
    SetTheme {
        theme: Theme,
    },
    /// Вставить содержимое буфера обмена (формат `web application/x-ferrum`).
    Paste {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Разместить изображение (PNG): следующий клик ставит его на холст.
    PlaceImage {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Сохранить документ: ответ — событие `Saved`.
    Save,
    /// Прочитать пиксели текущего кадра (для тестов): ответ — `Pixels`.
    /// `reference` — тот же кадр, нарисованный `vello_cpu`, для сравнения с GPU.
    ReadPixels {
        reference: bool,
    },
}

/// Событие worker'а основному потоку.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CoreEvent {
    /// Ядро запущено: бэкенд рендера и почему не подошли более быстрые.
    Ready {
        backend: Backend,
        fallbacks: Vec<String>,
    },
    /// Документ открыт; предупреждения — о частях повреждённого файла.
    Opened {
        pages: u32,
        nodes: u32,
        warnings: Vec<String>,
    },
    /// Выделение: число узлов и первые `SELECTION_IDS_MAX` идентификаторов.
    Selection {
        count: u32,
        ids: Vec<String>,
    },
    Tool {
        tool: Tool,
    },
    Cursor {
        cursor: CursorKind,
        angle: f64,
    },
    /// Документ изменён правкой, отменой или повтором.
    Document {
        can_undo: bool,
        can_redo: bool,
        steps: f64,
    },
    Page {
        id: String,
    },
    /// Байты для системного буфера обмена (`web application/x-ferrum`).
    Clipboard {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// PNG выделения для буфера обмена («Скопировать как PNG»).
    ClipboardPng {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Пора прочитать системный буфер и прислать `Paste`.
    PasteRequested,
    /// Выбрать файл изображения и прислать `PlaceImage`.
    RequestImage,
    ToggleUi,
    /// Сохранённый файл `.ferrum`.
    Saved {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Пиксели кадра RGBA, строки сверху вниз.
    Pixels {
        backend: Backend,
        width: u32,
        height: u32,
        #[tsify(type = "Uint8Array")]
        rgba: ByteBuf,
    },
    /// Сообщение об ошибке для пользователя.
    Error {
        message: String,
    },
}
````

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p ferrum-wasm --test protocol`
Expected: PASS, `4 passed`.

Run: `cargo clippy -p ferrum-wasm --all-targets -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 5: Закоммитить**

```bash
git add Cargo.toml Cargo.lock crates/ferrum-wasm
git commit -m "feat(wasm): команды и события протокола (serde, tsify)"
```

---

### Task 6: Ядро кадра `Core`

`Core` — всё, что worker делает с документом, без браузера и потому с нативными тестами: редактор, рендерер, картинки документа. Ввод — `apply_input(&[u8])`: записи разбираются и по порядку уходят в `Editor::handle`, размер холста — в `set_viewport`; события редактора переводятся в события протокола один раз за пачку (курсор — только последний, документ — одно событие). Кадр — `draw(&mut impl RenderBackend)`: рисует и запоминает, нужен ли следующий (`needs_another_frame`); `wants_frame()` — флаг от `NeedsRedraw` и неполного кадра. Бэкенд и часы даёт вызывающий: в браузере — поверхность и `performance.now()` (Task 7), в тестах — `CpuBackend`.

`Demo { nodes }` строит страницу генератором бенчмарков (фреймы 1440 × 1024 по 99 фигур) и забывает историю (`Document::clear_history`, новая функция этапа 1): иначе первая же отмена стирала бы весь пример. Камера — на первом фрейме при зуме 50 %. `Open` при ошибке оставляет текущий документ. `PlaceImage` принимает PNG: декодирует (заодно проверяет), кладёт байты в документ и картинку в кэш, включает инструмент размещения. «Скопировать как PNG» экспортирует выделение через `export_png_nodes`.

Картинки документа (`DocImages`) декодируются по первому запросу рендера, не больше 2 за кадр (`needs_another_frame` догоняет остальные), и живут в кэше с пределом 256 МБ; вытесняются давно не рисовавшиеся. Картинка без байтов или неразборчивая заменяется постоянной заглушкой 1 × 1 — иначе её «отсутствие» требовало бы кадра вечно.

Рамка выделения для живого состояния пересчитывается только при смене сцены (`change_counter`) или выделения (`Editor::selection_version`, новая функция этапа 3): панорамирование с 100 000 выделенными не обходит их на каждом кадре. У одного узла — его x, y (у родителя), ширина, высота и поворот (как в панели «Положение»), у нескольких — общая рамка в мире.

**Files:**
- Modify: `crates/ferrum-doc/src/document.rs`
- Modify: `crates/ferrum-editor/src/editor.rs`
- Modify: `crates/ferrum-wasm/Cargo.toml`
- Modify: `crates/ferrum-wasm/src/lib.rs`
- Create: `crates/ferrum-wasm/src/images.rs`
- Create: `crates/ferrum-wasm/src/core.rs`
- Test: `crates/ferrum-wasm/tests/core.rs`
- Test: `crates/ferrum-wasm/tests/images.rs`

**Interfaces:**
- Consumes: `Editor` (этап 3), `Renderer::{render, export_png_nodes}`, `container::{encode, decode}`, `fixture::fill`, `decode_png`, `SelectionBox::of`; `input::decode_all`, `protocol`, `live`.
- Produces: `Document::clear_history()`, `Editor::selection_version()`; `images::{DocImages, Provider, IMAGE_CACHE_BYTES, DECODES_PER_FRAME}`; `core::{Core, FRAME_WINDOW, CANVAS_DARK, CANVAS_LIGHT, COPY_PNG_SCALE}` с методами `new(platform, viewport, dpr, backend)`, `editor`, `renderer_mut`, `backend`, `set_backend`, `device_size`, `open`, `demo`, `apply_input`, `handle`, `command`, `take_pixel_request`, `wants_frame`, `background`, `draw`, `frame_done(now_ms, frame_ms)`, `set_atlas`, `frame_percentiles`, `take_events`, `live`.

- [ ] **Step 1: Написать падающие тесты**

`crates/ferrum-wasm/tests/core.rs`:

````rust
//! Ядро worker'а без браузера: ввод из кольцевого буфера доходит до редактора,
//! кадр рисуется только когда нужен, команды и события протокола, живое
//! состояние. Рендер — `vello_cpu`, как запасной бэкенд в браузере.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_render::{CpuBackend, Pixmap};
use ferrum_wasm::core::{Core, CANVAS_DARK};
use ferrum_wasm::input::{encode, Record};
use ferrum_wasm::protocol::{Backend, Command, CoreEvent, Tool};
use kurbo::{Point, Size};
use serde_bytes::ByteBuf;

const VIEW: Size = Size::new(800.0, 600.0);

/// Ядро со страницей-примером: камера на первом фрейме, зум 0.5.
fn demo(nodes: u32) -> Core {
    let mut core = Core::new(Platform::Mac, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes });
    core
}

/// Экранная точка мира при камере примера (центр (720, 512), зум 0.5).
fn screen(x: f64, y: f64) -> Point {
    Point::new((x - 720.0) * 0.5 + 400.0, (y - 512.0) * 0.5 + 300.0)
}

fn frame(core: &mut Core) -> Pixmap {
    let (w, h) = core.device_size();
    let mut b = CpuBackend::new(w, h);
    core.draw(&mut b);
    b.render()
}

fn px(p: &Pixmap, at: Point) -> [u8; 4] {
    let i = (at.y as usize * p.width() as usize + at.x as usize) * 4;
    p.data_as_u8_slice()[i..i + 4].try_into().unwrap()
}

fn near(a: [u8; 4], b: [u8; 3]) -> bool {
    (0..3).all(|i| a[i].abs_diff(b[i]) <= 3)
}

fn pointer(at: Point, t: f64) -> PointerEvent {
    PointerEvent {
        point: at,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    }
}

/// Перетаскивание записями кольцевого буфера, как их пишет основной поток.
fn drag_records(from: Point, by: (f64, f64), t: f64) -> Vec<u8> {
    let mut recs = vec![Record::Input(Input::PointerDown(pointer(from, t)))];
    for i in 1..=10 {
        let k = i as f64 / 10.0;
        let p = Point::new(from.x + by.0 * k, from.y + by.1 * k);
        recs.push(Record::Input(Input::PointerMove(pointer(p, t + i as f64))));
    }
    let to = Point::new(from.x + by.0, from.y + by.1);
    recs.push(Record::Input(Input::PointerUp(pointer(to, t + 20.0))));
    recs.iter().flat_map(encode).collect()
}

#[test]
fn demo_page_is_drawn_once_and_shows_the_document() {
    let mut core = demo(1000);
    let ev = core.take_events();
    assert!(
        ev.contains(&CoreEvent::Opened {
            pages: 1,
            nodes: 1000,
            warnings: vec![]
        }),
        "{ev:?}"
    );
    // История примера пуста: отмена не стирает страницу.
    assert!(ev.contains(&CoreEvent::Document {
        can_undo: false,
        can_redo: false,
        steps: 0.0
    }));
    assert!(core.wants_frame());
    let img = frame(&mut core);
    assert_eq!((img.width(), img.height()), (800, 600));
    // Прямоугольник «Слой 1» первого фрейма: (140, 0) – (260, 80), заливка (1, 120, 200).
    assert!(near(px(&img, screen(200.0, 40.0)), [1, 120, 200]));
    // Вне фреймов — фон холста.
    let bg = CANVAS_DARK.to_rgba8();
    assert!(near(px(&img, Point::new(5.0, 5.0)), [bg.r, bg.g, bg.b]));
    // Ничего не изменилось — кадр не нужен.
    assert!(!core.wants_frame());
    let live = core.live();
    assert_eq!((live.frames, live.nodes, live.zoom), (1.0, 1000.0, 0.5));
    assert_eq!(
        (live.device_w, live.device_h, live.backend),
        (800.0, 600.0, 3.0)
    );
}

#[test]
fn drag_from_ring_records_moves_the_node_and_undo_returns_it() {
    let mut core = demo(1000);
    frame(&mut core);
    core.take_events();
    let n = core.apply_input(&drag_records(screen(200.0, 40.0), (50.0, 30.0), 1000.0));
    assert_eq!(n, 12);
    let ev = core.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::Selection { count: 1, .. })),
        "{ev:?}"
    );
    assert!(ev.contains(&CoreEvent::Document {
        can_undo: true,
        can_redo: false,
        steps: 1.0
    }));
    // Один шаг отмены — одно событие документа, хоть изменений и много.
    assert_eq!(
        ev.iter()
            .filter(|e| matches!(e, CoreEvent::Document { .. }))
            .count(),
        1
    );
    assert!(core.wants_frame());
    let img = frame(&mut core);
    let live = core.live();
    // Сдвиг на (50, 30) CSS-пикселей при зуме 0.5 — (100, 60) в мире.
    assert_eq!(
        (live.selection_count, live.selection_x, live.selection_y),
        (1.0, 240.0, 60.0)
    );
    assert_eq!((live.selection_w, live.selection_h), (120.0, 80.0));
    assert_eq!((live.steps, live.can_undo, live.inputs), (1.0, 1.0, 12.0));
    // На новом месте — прямоугольник.
    assert!(near(px(&img, screen(300.0, 100.0)), [1, 120, 200]));

    core.command(Command::Execute {
        command: "edit.undo".into(),
    });
    frame(&mut core);
    let live = core.live();
    assert_eq!((live.selection_x, live.selection_y), (140.0, 0.0));
    assert_eq!((live.can_undo, live.can_redo), (0.0, 1.0));
}

#[test]
fn resize_record_changes_device_size_and_requests_a_frame() {
    let mut core = demo(100);
    frame(&mut core);
    let rec = encode(&Record::Resize {
        width: 640.0,
        height: 480.0,
        dpr: 2.0,
    });
    core.apply_input(&rec);
    assert!(core.wants_frame());
    assert_eq!(core.device_size(), (1280, 960));
    let img = frame(&mut core);
    assert_eq!((img.width(), img.height()), (1280, 960));
    assert_eq!(core.live().dpr, 2.0);
}

#[test]
fn garbage_input_is_counted_and_ignored() {
    let mut core = demo(100);
    frame(&mut core);
    let mut bytes = vec![0xffu8; 64 * 3];
    bytes.extend_from_slice(&encode(&Record::Input(Input::Blur)));
    bytes.extend_from_slice(&[1, 2]);
    assert_eq!(core.apply_input(&bytes), 1);
    let live = core.live();
    assert_eq!((live.inputs, live.skipped), (1.0, 4.0));
    // Нечисловые координаты отбрасывает редактор: документ цел.
    core.handle(Input::PointerDown(pointer(Point::new(f64::NAN, 1.0), 0.0)));
    assert_eq!(core.live().nodes, 100.0);
}

#[test]
fn bad_commands_report_errors_and_keep_the_document() {
    let mut core = demo(100);
    core.take_events();
    core.command(Command::Execute {
        command: "edit.format_disk".into(),
    });
    core.command(Command::Open {
        bytes: ByteBuf::from(b"not a ferrum file".to_vec()),
    });
    core.command(Command::Demo { nodes: 10_000_000 });
    core.command(Command::Paste {
        bytes: ByteBuf::from(vec![1, 2, 3]),
    });
    core.command(Command::PlaceImage {
        bytes: ByteBuf::from(vec![0; 10]),
    });
    let errors: Vec<CoreEvent> = core
        .take_events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::Error { .. }))
        .collect();
    assert_eq!(errors.len(), 5, "{errors:?}");
    assert_eq!(core.live().nodes, 100.0);
}

#[test]
fn save_and_open_round_trip() {
    let mut core = demo(300);
    core.command(Command::Save);
    let Some(CoreEvent::Saved { bytes }) = core
        .take_events()
        .into_iter()
        .find(|e| matches!(e, CoreEvent::Saved { .. }))
    else {
        panic!("нет события Saved");
    };
    let mut other = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    other.command(Command::Open { bytes });
    assert!(other.take_events().contains(&CoreEvent::Opened {
        pages: 1,
        nodes: 300,
        warnings: vec![]
    }));
}

#[test]
fn copy_as_png_exports_the_selection() {
    let mut core = demo(100);
    frame(&mut core);
    // Клик по «Слою 1» выделяет его.
    let at = screen(200.0, 40.0);
    core.handle(Input::PointerDown(pointer(at, 0.0)));
    core.handle(Input::PointerUp(pointer(at, 10.0)));
    core.take_events();
    core.command(Command::Execute {
        command: "edit.copy_png".into(),
    });
    let png = core.take_events().into_iter().find_map(|e| match e {
        CoreEvent::ClipboardPng { bytes } => Some(bytes),
        _ => None,
    });
    let png = png.expect("нет PNG");
    let img = ferrum_render::decode_png(&png).expect("PNG не читается");
    assert_eq!((img.width(), img.height()), (120, 80));
}

#[test]
fn placed_image_is_drawn() {
    let mut core = demo(100);
    frame(&mut core);
    let mut src = Pixmap::new(40, 20);
    for p in src.data_as_u8_slice_mut().chunks_exact_mut(4) {
        p.copy_from_slice(&[255, 0, 0, 255]);
    }
    let png = src.into_png().unwrap();
    core.command(Command::PlaceImage {
        bytes: ByteBuf::from(png),
    });
    // Клик в пустом месте ставит картинку в исходном размере.
    let at = screen(720.0, 1100.0);
    core.handle(Input::PointerDown(pointer(at, 0.0)));
    core.handle(Input::PointerUp(pointer(at, 10.0)));
    let ev = core.take_events();
    assert!(ev.contains(&CoreEvent::Tool { tool: Tool::Move }), "{ev:?}");
    let img = frame(&mut core);
    let live = core.live();
    assert_eq!((live.selection_w, live.selection_h), (40.0, 20.0));
    let centre = screen(
        live.selection_x + live.selection_w / 2.0,
        live.selection_y + live.selection_h / 2.0,
    );
    assert!(
        near(px(&img, centre), [255, 0, 0]),
        "{:?}",
        px(&img, centre)
    );
    assert!(!core.wants_frame(), "картинка уже декодирована");
}

#[test]
fn zoomed_out_page_keeps_drawing_until_rasters_are_warm() {
    let mut core = demo(20_000);
    // Счётный предел растров за кадр: число кадров не зависит от скорости машины.
    core.renderer_mut().set_raster_build_time(None);
    frame(&mut core);
    core.command(Command::Execute {
        command: "view.fit_all".into(),
    });
    let mut frames = 0;
    let mut builds = 0;
    while core.wants_frame() {
        let (w, h) = core.device_size();
        builds += core.draw(&mut CpuBackend::new(w, h)).raster_builds;
        frames += 1;
        assert!(frames < 100, "кадры не кончаются");
    }
    // Растры строятся не больше 32 за кадр: прогрев занимает несколько кадров,
    // после него кадр больше не нужен.
    assert!(
        frames >= 2 && builds > 32,
        "кадров {frames}, растров {builds}"
    );
}

#[test]
fn frame_statistics() {
    let mut core = demo(100);
    for i in 0..10 {
        core.frame_done(i as f64 * 200.0, i as f64);
    }
    let live = core.live();
    assert_eq!(live.frame_ms, 9.0);
    assert_eq!((live.frame_p50, live.frame_p95), (5.0, 9.0));
    // За последнюю секунду (800..1800 мс) — 6 кадров.
    assert_eq!(live.fps, 6.0);
}
````

`crates/ferrum-wasm/tests/images.rs`:

````rust
//! Картинки документа в worker'е: декодирование по запросу с бюджетом на кадр,
//! постоянная заглушка для неразборчивых и предел памяти.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_doc::ImageStore;
use ferrum_render::{ImageProvider, Pixmap};
use ferrum_wasm::images::{DocImages, DECODES_PER_FRAME};

fn png(w: u16, h: u16, rgba: [u8; 4]) -> Vec<u8> {
    let mut p = Pixmap::new(w, h);
    for px in p.data_as_u8_slice_mut().chunks_exact_mut(4) {
        px.copy_from_slice(&rgba);
    }
    p.into_png().unwrap()
}

#[test]
fn decodes_at_most_a_few_images_per_frame() {
    let mut store = ImageStore::default();
    let hashes: Vec<String> = (0..5)
        .map(|i| store.add(png(4, 4, [i * 40, 0, 0, 255])))
        .collect();
    let images = DocImages::default();
    images.begin_frame();
    let p = images.provider(&store);
    let got: Vec<bool> = hashes.iter().map(|h| p.image(h).is_some()).collect();
    assert_eq!(DECODES_PER_FRAME, 2);
    assert_eq!(got, [true, true, false, false, false]);
    // Уже декодированные — из кэша, без бюджета.
    assert!(p.image(&hashes[0]).is_some());
    images.begin_frame();
    let p = images.provider(&store);
    let got: Vec<bool> = hashes.iter().map(|h| p.image(h).is_some()).collect();
    assert_eq!(got, [true, true, true, true, false]);
    assert_eq!(images.stats().2, 4);
    let img = p.image(&hashes[1]).unwrap();
    assert_eq!(&img.data_as_u8_slice()[..4], &[40, 0, 0, 255]);
}

#[test]
fn missing_and_broken_images_get_a_permanent_placeholder() {
    let mut store = ImageStore::default();
    let broken = store.add(b"not a png".to_vec());
    let images = DocImages::default();
    images.begin_frame();
    let p = images.provider(&store);
    // Байтов нет: заглушка сразу, без бюджета декодирования.
    let a = p.image("нет-такого").expect("заглушка");
    // Неразборчивая картинка: заглушка, и второй раз её не декодируют.
    let b = p.image(&broken).expect("заглушка");
    assert_eq!((a.width(), a.height(), b.width()), (1, 1, 1));
    images.begin_frame();
    assert!(images.provider(&store).image(&broken).is_some());
    assert_eq!(images.stats().2, 1, "неразборчивая декодировалась снова");
}

#[test]
fn memory_limit_evicts_images_not_used_this_frame() {
    let mut store = ImageStore::default();
    let a = store.add(png(16, 16, [1, 0, 0, 255]));
    let b = store.add(png(16, 16, [2, 0, 0, 255]));
    // Предел — одна картинка 16×16.
    let images = DocImages::with_limit(16 * 16 * 4);
    images.begin_frame();
    let p = images.provider(&store);
    p.image(&a).unwrap();
    // В этом же кадре `a` нужна: обе остаются, предел временно превышен.
    p.image(&b).unwrap();
    assert_eq!(images.stats().0, 2);
    images.begin_frame();
    let p = images.provider(&store);
    p.image(&b).unwrap();
    // `a` в этом кадре не рисовалась — вытесняется, когда понадобится место.
    images.insert("c", Pixmap::new(16, 16));
    assert_eq!(images.stats().0, 2);
    assert!(p.image(&b).is_some());
    let (n, bytes, decoded) = images.stats();
    assert_eq!((n, bytes, decoded), (2, 2 * 16 * 16 * 4, 2));
    // Вытеснена именно `a`: её снова приходится декодировать.
    images.begin_frame();
    images.provider(&store).image(&a).unwrap();
    assert_eq!(images.stats().2, 3);
}
````

Run: `cargo test -p ferrum-wasm --test core --test images`
Expected: FAIL — не компилируется: `unresolved import` `ferrum_wasm::core`, `ferrum_wasm::images`, `ferrum_render` (зависимость добавится в Step 3).

- [ ] **Step 2: История примера и версия выделения**

В `crates/ferrum-doc/src/document.rs` заменить:

````rust

    pub fn can_undo(&self) -> bool {
````

на:

````rust

    /// Забывает историю отмены и повтора: документ, собранный программно (пример,
    /// шаблон), открывается как только что прочитанный файл — без шага «создать всё».
    pub fn clear_history(&mut self) {
        self.history = History::default();
    }

    pub fn can_undo(&self) -> bool {
````

В `crates/ferrum-editor/src/editor.rs` заменить:

````rust
        &self.selection
    }
````

на:

````rust
        &self.selection
    }

    /// Версия выделения: растёт при каждом изменении. По ней worker не пересчитывает
    /// рамку выделения в живом состоянии, пока выделение и сцена те же.
    pub fn selection_version(&self) -> u64 {
        self.selection.version()
    }
````

- [ ] **Step 3: Картинки документа и ядро кадра**

В `crates/ferrum-wasm/Cargo.toml` заменить:

````toml
ferrum-editor = { path = "../ferrum-editor" }
kurbo.workspace = true
````

на:

````toml
ferrum-editor = { path = "../ferrum-editor" }
ferrum-render = { path = "../ferrum-render" }
ferrum-scene = { path = "../ferrum-scene" }
kurbo.workspace = true
````

`crates/ferrum-wasm/src/lib.rs` (целиком):

````rust
//! Ядро Ferrum в Web Worker: обмен данными с основным потоком и цикл кадров.
//!
//! Нативная часть (тесты обычным `cargo test`): раскладка общей памяти
//! (`layout`), записи кольцевого буфера ввода (`input`), блок живого состояния
//! (`live`), сообщения (`protocol`), картинки документа (`images`) и ядро кадра
//! (`core`). Основной поток пишет ввод в кольцевой буфер SharedArrayBuffer (или
//! в буфер сообщения, если страница не изолирована), worker разбирает его в начале
//! кадра и отдаёт редактору по порядку.

pub mod core;
pub mod images;
pub mod input;
pub mod layout;
pub mod live;
pub mod protocol;
````

`crates/ferrum-wasm/src/images.rs`:

````rust
//! Картинки документа для рендера: декодируются в worker'е по первому запросу и
//! живут в кэше с пределом памяти. Рендер спрашивает картинку по хэшу на каждом
//! кадре; если её ещё нет, рисуется заглушка и кадр помечается неполным — следующий
//! кадр декодирует дальше. Так открытие страницы с сотней картинок не останавливает
//! worker на секунды: за кадр декодируется не больше `DECODES_PER_FRAME`.
//!
//! Картинка без байтов в документе или неразборчивая заменяется постоянной
//! заглушкой (1×1 цвета заглушки рендера): иначе её «отсутствие» требовало бы
//! нового кадра вечно.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use ferrum_doc::ImageStore;
use ferrum_render::{decode_png, ImageProvider, Pixmap};

/// Предел памяти декодированных картинок.
pub const IMAGE_CACHE_BYTES: usize = 256 * 1024 * 1024;
/// Сколько картинок декодировать за кадр.
pub const DECODES_PER_FRAME: usize = 2;

struct Entry {
    pixmap: Arc<Pixmap>,
    last_used: u64,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.pixmap.width() as usize * self.pixmap.height() as usize * 4
    }
}

struct Inner {
    map: HashMap<String, Entry>,
    bytes: usize,
    limit: usize,
    frame: u64,
    decodes_left: usize,
    decoded: u64,
    broken: Arc<Pixmap>,
}

pub struct DocImages {
    inner: RefCell<Inner>,
}

impl Default for DocImages {
    fn default() -> DocImages {
        DocImages::with_limit(IMAGE_CACHE_BYTES)
    }
}

impl DocImages {
    pub fn with_limit(limit: usize) -> DocImages {
        // Цвет заглушки рендера (0.85 серого), непрозрачный, с премультипликацией.
        let mut broken = Pixmap::new(1, 1);
        broken
            .data_as_u8_slice_mut()
            .copy_from_slice(&[217, 217, 217, 255]);
        DocImages {
            inner: RefCell::new(Inner {
                map: HashMap::new(),
                bytes: 0,
                limit,
                frame: 0,
                decodes_left: DECODES_PER_FRAME,
                decoded: 0,
                broken: Arc::new(broken),
            }),
        }
    }

    /// Начало кадра: новый бюджет декодирования.
    pub fn begin_frame(&self) {
        let mut i = self.inner.borrow_mut();
        i.frame += 1;
        i.decodes_left = DECODES_PER_FRAME;
    }

    /// Кладёт уже декодированную картинку (размещение изображения).
    pub fn insert(&self, hash: &str, pixmap: Pixmap) {
        let mut i = self.inner.borrow_mut();
        let frame = i.frame;
        i.put(hash, Arc::new(pixmap), frame);
    }

    /// Поставщик для рендера: картинки из байтов `store`.
    pub fn provider<'a>(&'a self, store: &'a ImageStore) -> Provider<'a> {
        Provider {
            images: self,
            store,
        }
    }

    /// Картинок в кэше, их байт и сколько всего декодировано.
    pub fn stats(&self) -> (usize, usize, u64) {
        let i = self.inner.borrow();
        (i.map.len(), i.bytes, i.decoded)
    }
}

impl Inner {
    fn put(&mut self, hash: &str, pixmap: Arc<Pixmap>, frame: u64) {
        let entry = Entry {
            pixmap,
            last_used: frame,
        };
        self.bytes += entry.bytes();
        if let Some(old) = self.map.insert(hash.to_string(), entry) {
            self.bytes -= old.bytes();
        }
        // Предел: вытесняются давно не рисовавшиеся, кроме нужных в этом кадре.
        while self.bytes > self.limit {
            let oldest = self
                .map
                .iter()
                .filter(|(_, e)| e.last_used < frame)
                .min_by_key(|(_, e)| e.last_used)
                .map(|(k, _)| k.clone());
            let Some(key) = oldest else { break };
            let e = self.map.remove(&key).expect("запись есть");
            self.bytes -= e.bytes();
        }
    }
}

pub struct Provider<'a> {
    images: &'a DocImages,
    store: &'a ImageStore,
}

impl ImageProvider for Provider<'_> {
    fn image(&self, hash: &str) -> Option<Arc<Pixmap>> {
        let mut i = self.images.inner.borrow_mut();
        let frame = i.frame;
        if let Some(e) = i.map.get_mut(hash) {
            e.last_used = frame;
            return Some(e.pixmap.clone());
        }
        let Some(bytes) = self.store.get(hash) else {
            // Байтов нет (файл открыт без части картинок): постоянная заглушка.
            return Some(i.broken.clone());
        };
        if i.decodes_left == 0 {
            return None;
        }
        i.decodes_left -= 1;
        i.decoded += 1;
        let pixmap = match decode_png(bytes) {
            Some(p) => Arc::new(p),
            None => i.broken.clone(),
        };
        i.put(hash, pixmap.clone(), frame);
        Some(pixmap)
    }
}
````

`crates/ferrum-wasm/src/core.rs`:

````rust
//! Ядро worker'а без браузера: редактор, рендерер и картинки документа, ввод из
//! кольцевого буфера, команды и события протокола, решение «нужен ли кадр» и живое
//! состояние. Бэкенд рендера и часы даёт вызывающий: в браузере — поверхность
//! WebGPU, WebGL2 или CPU и `performance.now()` (модуль `web`), в тестах —
//! `CpuBackend`.
//!
//! Кадр рисуется только когда изменились сцена, камера или оверлей (событие
//! редактора `NeedsRedraw`) или прошлый кадр был неполным
//! (`FrameStats::needs_another_frame`: растры фреймов не догнали, картинки ещё
//! декодируются).

use std::collections::VecDeque;

use ferrum_doc::{container, fixture, Document, NodeId};
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::{CommandId, Cursor, Editor, EditorEvent, Input, Platform, ToolKind};
use ferrum_render::{decode_png, FrameStats, OverlayTheme, RenderBackend, Renderer, Rgba};
use kurbo::{Point, Size};
use serde_bytes::ByteBuf;

use crate::images::DocImages;
use crate::input::{decode_all, Record};
use crate::live::LiveState;
use crate::protocol::{
    Backend, Command, CursorKind, CoreEvent, Theme, Tool, MAX_DEMO_NODES, SELECTION_IDS_MAX,
};

/// Сколько последних кадров учитывают медиана и 95-й процентиль времени кадра.
pub const FRAME_WINDOW: usize = 120;
/// Фон холста (до цвета фона страницы в документе): тёмная и светлая темы.
pub const CANVAS_DARK: Rgba = Rgba::new([0.141, 0.137, 0.133, 1.0]);
pub const CANVAS_LIGHT: Rgba = Rgba::new([0.949, 0.941, 0.929, 1.0]);
/// Масштаб «Скопировать как PNG».
pub const COPY_PNG_SCALE: f64 = 1.0;

/// Значения выделения для живого состояния и ключ, при котором они посчитаны.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SelectionInfo {
    key: (u64, u64, usize),
    count: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rotation: f64,
}

pub struct Core {
    editor: Editor,
    renderer: Renderer,
    images: DocImages,
    backend: Backend,
    platform: Platform,
    theme: Theme,
    dirty: bool,
    events: Vec<CoreEvent>,
    /// Запрошены пиксели следующего кадра (`ReadPixels`): `Some(reference)`.
    pixels: Option<bool>,
    inputs: u64,
    skipped: u64,
    frames: u64,
    frame_ms: f64,
    times: VecDeque<f64>,
    stamps: VecDeque<f64>,
    selection: Option<SelectionInfo>,
    atlas: (usize, u64),
}

fn tool_kind(t: Tool) -> ToolKind {
    match t {
        Tool::Move => ToolKind::Move,
        Tool::Frame => ToolKind::Frame,
        Tool::Rect => ToolKind::Rect,
        Tool::Ellipse => ToolKind::Ellipse,
        Tool::Line => ToolKind::Line,
        Tool::Hand => ToolKind::Hand,
        Tool::Image => ToolKind::Image,
    }
}

fn tool(k: ToolKind) -> Tool {
    match k {
        ToolKind::Move => Tool::Move,
        ToolKind::Frame => Tool::Frame,
        ToolKind::Rect => Tool::Rect,
        ToolKind::Ellipse => Tool::Ellipse,
        ToolKind::Line => Tool::Line,
        ToolKind::Hand => Tool::Hand,
        ToolKind::Image => Tool::Image,
    }
}

fn cursor(c: Cursor) -> (CursorKind, f64) {
    match c {
        Cursor::Default => (CursorKind::Default, 0.0),
        Cursor::Crosshair => (CursorKind::Crosshair, 0.0),
        Cursor::Grab => (CursorKind::Grab, 0.0),
        Cursor::Grabbing => (CursorKind::Grabbing, 0.0),
        Cursor::Duplicate => (CursorKind::Duplicate, 0.0),
        Cursor::Resize { angle } => (CursorKind::Resize, angle),
        Cursor::Rotate { angle } => (CursorKind::Rotate, angle),
    }
}

fn overlay_theme(t: Theme) -> OverlayTheme {
    match t {
        Theme::Dark => OverlayTheme::dark(),
        Theme::Light => OverlayTheme::light(),
    }
}

impl Core {
    /// Ядро с пустым документом для холста `viewport` (CSS-пиксели) и `dpr`.
    pub fn new(platform: Platform, viewport: Size, dpr: f64, backend: Backend) -> Core {
        let editor = Editor::new(Document::new(), None, viewport, dpr, platform)
            .expect("в новом документе есть страница");
        Core {
            editor,
            renderer: Renderer::new(),
            images: DocImages::default(),
            backend,
            platform,
            theme: Theme::Dark,
            dirty: true,
            events: Vec::new(),
            pixels: None,
            inputs: 0,
            skipped: 0,
            frames: 0,
            frame_ms: 0.0,
            times: VecDeque::with_capacity(FRAME_WINDOW),
            stamps: VecDeque::new(),
            selection: None,
            atlas: (0, 0),
        }
    }

    pub fn editor(&self) -> &Editor {
        &self.editor
    }

    pub fn renderer_mut(&mut self) -> &mut Renderer {
        &mut self.renderer
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    /// Бэкенд сменился (запасной путь после ошибки GPU): кадр нужен заново.
    pub fn set_backend(&mut self, backend: Backend) {
        self.backend = backend;
        self.dirty = true;
    }

    /// Размер кадра в пикселях устройства.
    pub fn device_size(&self) -> (u16, u16) {
        self.editor.camera().device_size()
    }

    /// Заменяет документ: новый редактор с тем же холстом и темой.
    fn replace_document(&mut self, doc: Document) -> Result<(), String> {
        let cam = self.editor.camera();
        let (viewport, dpr) = (cam.viewport(), cam.dpr());
        let mut editor =
            Editor::new(doc, None, viewport, dpr, self.platform).map_err(|e| e.to_string())?;
        editor.set_theme(overlay_theme(self.theme));
        editor.take_events();
        self.editor = editor;
        self.images = DocImages::default();
        self.selection = None;
        self.dirty = true;
        Ok(())
    }

    fn push_opened(&mut self, warnings: Vec<String>) {
        let pages = self.editor.document().pages().len() as u32;
        let nodes = self.editor.scene().len() as u32;
        self.events.push(CoreEvent::Opened {
            pages,
            nodes,
            warnings,
        });
        self.push_document();
        self.events.push(CoreEvent::Selection {
            count: 0,
            ids: vec![],
        });
        self.events.push(CoreEvent::Tool {
            tool: tool(self.editor.tool_kind()),
        });
    }

    fn push_document(&mut self) {
        let doc = self.editor.document();
        self.events.push(CoreEvent::Document {
            can_undo: doc.can_undo(),
            can_redo: doc.can_redo(),
            steps: self.editor.step_count() as f64,
        });
    }

    fn error(&mut self, message: impl Into<String>) {
        self.events.push(CoreEvent::Error {
            message: message.into(),
        });
    }

    /// Открывает файл `.ferrum`. При ошибке текущий документ остаётся как был.
    pub fn open(&mut self, bytes: &[u8]) {
        match container::decode(bytes) {
            Ok(opened) => match self.replace_document(opened.document) {
                Ok(()) => self.push_opened(opened.warnings),
                Err(e) => self.error(e),
            },
            Err(e) => self.error(format!("Не удалось открыть файл: {e}")),
        }
    }

    /// Новый документ — страница-пример из `nodes` узлов (генератор бенчмарков:
    /// фреймы 1440 × 1024 по 99 фигур). Камера — на первом фрейме при зуме 50 %.
    pub fn demo(&mut self, nodes: u32) {
        if nodes > MAX_DEMO_NODES {
            self.error(format!("Пример не больше {MAX_DEMO_NODES} узлов"));
            return;
        }
        let mut doc = Document::new();
        let page = doc.pages()[0].0;
        if let Err(e) = doc.transact(page, vec![], |p| {
            fixture::fill(p, nodes as usize)?;
            Ok(vec![])
        }) {
            self.error(e.to_string());
            return;
        }
        doc.clear_history();
        if let Err(e) = self.replace_document(doc) {
            self.error(e);
            return;
        }
        let mut cam = self.editor.camera().clone();
        cam.set_zoom(0.5);
        cam.set_center(Point::new(720.0, 512.0));
        self.editor.set_camera(cam);
        self.editor.take_events();
        self.push_opened(vec![]);
    }

    /// Разбирает записи кольцевого буфера и отдаёт события редактору по порядку.
    /// Возвращает число принятых записей.
    pub fn apply_input(&mut self, bytes: &[u8]) -> usize {
        let decoded = decode_all(bytes);
        self.skipped += decoded.skipped as u64;
        let n = decoded.records.len();
        for rec in decoded.records {
            match rec {
                Record::Input(input) => self.editor.handle(input),
                Record::Resize { width, height, dpr } => {
                    self.editor.set_viewport(Size::new(width, height), dpr);
                    self.dirty = true;
                }
            }
        }
        self.inputs += n as u64;
        self.drain_editor();
        n
    }

    /// Одно событие ввода (тесты и бенчмарк без кодирования).
    pub fn handle(&mut self, input: Input) {
        self.editor.handle(input);
        self.inputs += 1;
        self.drain_editor();
    }

    /// Команда основного потока.
    pub fn command(&mut self, cmd: Command) {
        match cmd {
            Command::Open { bytes } => self.open(&bytes),
            Command::Demo { nodes } => self.demo(nodes),
            Command::Execute { command } => match CommandId::from_key(&command) {
                Some(id) => self.editor.execute(id),
                None => self.error(format!("Неизвестная команда «{command}»")),
            },
            Command::SetTool { tool } => self.editor.set_tool(tool_kind(tool)),
            Command::SetTheme { theme } => {
                self.theme = theme;
                self.editor.set_theme(overlay_theme(theme));
            }
            Command::Paste { bytes } => {
                // Ошибку вставки редактор сам отдаёт событием `Error`.
                let _ = self.editor.paste(&bytes);
            }
            Command::PlaceImage { bytes } => self.place_image(bytes.into_vec()),
            Command::Save => match container::encode(self.editor.document()) {
                Ok(bytes) => self.events.push(CoreEvent::Saved {
                    bytes: ByteBuf::from(bytes),
                }),
                Err(e) => self.error(format!("Не удалось сохранить: {e}")),
            },
            Command::ReadPixels { reference } => {
                self.pixels = Some(reference);
                self.dirty = true;
            }
        }
        self.drain_editor();
    }

    fn place_image(&mut self, bytes: Vec<u8>) {
        let Some(pixmap) = decode_png(&bytes) else {
            self.error("Не удалось прочитать изображение: нужен PNG не больше 8192 px");
            return;
        };
        let (w, h) = (pixmap.width() as f64, pixmap.height() as f64);
        let hash = self.editor.add_image(bytes);
        self.images.insert(&hash, pixmap);
        self.editor.place_image(&hash, w, h);
    }

    /// Запрос пикселей следующего кадра: `Some(reference)` один раз.
    pub fn take_pixel_request(&mut self) -> Option<bool> {
        self.pixels.take()
    }

    /// Переводит события редактора в события протокола.
    fn drain_editor(&mut self) {
        let mut document = false;
        for e in self.editor.take_events() {
            match e {
                EditorEvent::NeedsRedraw => self.dirty = true,
                EditorEvent::DocumentChanged => document = true,
                EditorEvent::SelectionChanged(ids) => self.events.push(CoreEvent::Selection {
                    count: ids.len() as u32,
                    ids: ids
                        .iter()
                        .take(SELECTION_IDS_MAX)
                        .map(NodeId::to_string)
                        .collect(),
                }),
                EditorEvent::ToolChanged(k) => self.events.push(CoreEvent::Tool { tool: tool(k) }),
                EditorEvent::CursorChanged(c) => {
                    // Наведение меняет курсор часто: в пачке важен только последний.
                    self.events.retain(|e| !matches!(e, CoreEvent::Cursor { .. }));
                    let (cursor, angle) = cursor(c);
                    self.events.push(CoreEvent::Cursor { cursor, angle });
                }
                EditorEvent::PageChanged(p) => self.events.push(CoreEvent::Page { id: p.to_string() }),
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
                    bytes: ByteBuf::from(bytes),
                }),
                EditorEvent::PasteRequested => self.events.push(CoreEvent::PasteRequested),
                EditorEvent::ExportPng(ids) => self.copy_png(&ids),
                EditorEvent::RequestImage => self.events.push(CoreEvent::RequestImage),
                EditorEvent::ToggleUi => self.events.push(CoreEvent::ToggleUi),
                EditorEvent::Error(message) => self.events.push(CoreEvent::Error { message }),
            }
        }
        if document {
            self.events.retain(|e| !matches!(e, CoreEvent::Document { .. }));
            self.push_document();
        }
    }

    fn copy_png(&mut self, ids: &[NodeId]) {
        let provider = self.images.provider(&self.editor.document().images);
        let result =
            self.renderer
                .export_png_nodes(self.editor.scene(), ids, COPY_PNG_SCALE, &provider);
        match result {
            Ok(png) => self.events.push(CoreEvent::ClipboardPng {
                bytes: ByteBuf::from(png),
            }),
            Err(e) => self.error(e.to_string()),
        }
    }

    /// Нужен ли кадр.
    pub fn wants_frame(&self) -> bool {
        self.dirty
    }

    pub fn background(&self) -> Rgba {
        match self.theme {
            Theme::Dark => CANVAS_DARK,
            Theme::Light => CANVAS_LIGHT,
        }
    }

    /// Рисует кадр в `backend`. Кадр нужен снова, только если этот неполный.
    pub fn draw(&mut self, backend: &mut impl RenderBackend) -> FrameStats {
        self.images.begin_frame();
        let overlay = self.editor.overlay();
        let provider = self.images.provider(&self.editor.document().images);
        let stats = self.renderer.render(
            backend,
            self.editor.scene(),
            self.editor.camera(),
            &overlay,
            &provider,
            Some(self.background()),
        );
        self.update_selection(&overlay.selection);
        self.dirty = stats.needs_another_frame;
        self.frames += 1;
        stats
    }

    /// Рамка выделения для живого состояния. Пересчитывается только при смене
    /// сцены или выделения: панорамирование с 100 000 выделенными не обходит их на
    /// каждом кадре.
    fn update_selection(&mut self, ids: &[NodeId]) {
        let scene = self.editor.scene();
        let key = (
            scene.change_counter(),
            self.editor.selection_version(),
            ids.len(),
        );
        if self.selection.is_some_and(|s| s.key == key) {
            return;
        }
        let mut info = SelectionInfo {
            key,
            count: ids.len() as f64,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            rotation: 0.0,
        };
        if let [one] = ids {
            if let Some(n) = scene.node(*one) {
                info.x = n.props.x;
                info.y = n.props.y;
                info.w = n.props.w;
                info.h = n.props.h;
                info.rotation = n.props.rotation;
            }
        } else if let Some(b) = SelectionBox::of(scene, ids) {
            let r = b.to_world.transform_rect_bbox(b.rect);
            (info.x, info.y, info.w, info.h) = (r.x0, r.y0, r.width(), r.height());
        }
        self.selection = Some(info);
    }

    /// Время кадра (рисование и отправка в GPU), измеренное вызывающим, и момент
    /// его окончания — для статистики.
    pub fn frame_done(&mut self, now_ms: f64, frame_ms: f64) {
        self.frame_ms = frame_ms;
        if self.times.len() == FRAME_WINDOW {
            self.times.pop_front();
        }
        self.times.push_back(frame_ms);
        self.stamps.push_back(now_ms);
        while self.stamps.front().is_some_and(|t| now_ms - t > 1000.0) {
            self.stamps.pop_front();
        }
    }

    /// Состояние атласа GPU: картинок и промахов (заглушек из-за нехватки места).
    pub fn set_atlas(&mut self, images: usize, misses: u64) {
        self.atlas = (images, misses);
    }

    /// Медиана и 95-й процентиль времени кадра по окну.
    pub fn frame_percentiles(&self) -> (f64, f64) {
        if self.times.is_empty() {
            return (0.0, 0.0);
        }
        let mut t: Vec<f64> = self.times.iter().copied().collect();
        t.sort_by(f64::total_cmp);
        let at = |q: f64| t[((t.len() - 1) as f64 * q).round() as usize];
        (at(0.5), at(0.95))
    }

    pub fn take_events(&mut self) -> Vec<CoreEvent> {
        std::mem::take(&mut self.events)
    }

    /// Живое состояние для основного потока.
    pub fn live(&self) -> LiveState {
        let cam = self.editor.camera();
        let (dw, dh) = cam.device_size();
        let (p50, p95) = self.frame_percentiles();
        let doc = self.editor.document();
        let sel = self.selection.unwrap_or(SelectionInfo {
            key: (0, 0, 0),
            count: 0.0,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            rotation: 0.0,
        });
        LiveState {
            frames: self.frames as f64,
            frame_ms: self.frame_ms,
            frame_p50: p50,
            frame_p95: p95,
            fps: self.stamps.len() as f64,
            backend: self.backend.code(),
            selection_count: sel.count,
            selection_x: sel.x,
            selection_y: sel.y,
            selection_w: sel.w,
            selection_h: sel.h,
            selection_rotation: sel.rotation,
            camera_x: cam.center().x,
            camera_y: cam.center().y,
            zoom: cam.zoom(),
            viewport_w: cam.viewport().width,
            viewport_h: cam.viewport().height,
            dpr: cam.dpr(),
            device_w: dw as f64,
            device_h: dh as f64,
            busy: if self.editor.is_busy() { 1.0 } else { 0.0 },
            steps: self.editor.step_count() as f64,
            can_undo: if doc.can_undo() { 1.0 } else { 0.0 },
            can_redo: if doc.can_redo() { 1.0 } else { 0.0 },
            nodes: self.editor.scene().len() as f64,
            inputs: self.inputs as f64,
            skipped: self.skipped as f64,
            atlas_images: self.atlas.0 as f64,
            atlas_misses: self.atlas.1 as f64,
        }
    }
}
````

- [ ] **Step 4: Запустить тесты**

Run: `cargo test -p ferrum-wasm --test core`
Expected: PASS, `10 passed`.

Run: `cargo test -p ferrum-wasm --test images`
Expected: PASS, `3 passed`.

Run: `cargo test -p ferrum-doc -p ferrum-editor`
Expected: PASS — наборы этапов 1 и 3 без изменений.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: без предупреждений.

- [ ] **Step 5: Закоммитить**

```bash
git add Cargo.lock crates/ferrum-doc crates/ferrum-editor crates/ferrum-wasm
git commit -m "feat(wasm): ядро кадра Core — ввод, команды, события и картинки документа"
```

---

### Task 7: `WasmCore`: поверхности WebGPU, WebGL2 и CPU

Браузерная часть крейта (модуль `web`, только wasm32). `WasmCore::create` выбирает бэкенд по порядку WebGPU → WebGL2 → CPU (или запрошенный и CPU как запасной) и сообщает причины отказов в событии `ready`. У холста контекст выбирается один раз: после `getContext("webgpu")` получить `webgl2` или `2d` уже нельзя. Поэтому каждый вариант проверяется без холста: WebGPU — адаптер и устройство до `create_surface`, WebGL2 — пробный OffscreenCanvas 1 × 1 с теми же параметрами контекста (без сглаживания, без программного рендера, глубина). При автоматическом выборе программный GPU не берётся: headless Chromium без GPU даёт WebGL2 на SwiftShader (`failIfMajorPerformanceCaveat` его не отсекает), а там кадры выдаются с задержкой и медленнее `vello_cpu`; имя рендерера читается через `WEBGL_debug_renderer_info`, у WebGPU — `DeviceType::Cpu` запасного адаптера.

- **WebGPU:** wgpu 30 (`BROWSER_WEBGPU`), `SurfaceTarget::OffscreenCanvas`, формат `Rgba8Unorm`, `COPY_SRC` для чтения пикселей, пределы адаптера (не умолчания WebGPU — иначе холст шире 8192 px не настроить). Загрузка картинки — `queue.write_texture`, а такие записи выполняются раньше команд следующего `submit`; поэтому очистка места при вытеснении отправляется своим кодировщиком сразу, иначе она стерла бы картинку, загруженную на это место после неё (проверено E2E-тестом вытеснения в Task 9). Потеря устройства и ошибки вне вызовов приходят обработчиками wgpu (`Send + Sync`, поэтому `Arc<Mutex>`).
- **WebGL2:** `WebGlRenderer` vello_gpu принимает `HtmlCanvasElement`, но берёт у него только `getContext`; вызов в wasm-bindgen идёт по имени метода, поэтому OffscreenCanvas подходит (`unchecked_ref`). Пиксели читаются сразу после рисования (без `preserveDrawingBuffer` буфер очищается), строки переворачиваются.
- **CPU:** `vello_cpu` → `ImageData` → `putImageData` в контекст 2D без альфы. Фон кадра непрозрачный, поэтому RGBA с премультипликацией совпадает с обычным.

`ReadPixels { reference }` отдаёт пиксели кадра (у WebGPU — асинхронно, через `map_async`, с размером на момент снимка) и, если `reference`, тот же кадр, нарисованный `vello_cpu` отдельным рендерером (`Core::draw_reference`), — основа E2E-сверки бэкендов. Необслуженный запрос (кадр пропущен, ждём прошлое чтение) возвращается в очередь. Часы рендера — `performance.now()` (`PerformanceClock`). Сборка под wasm32 включает SIMD128 (`.cargo/config.toml`): он есть во всех браузерах с OffscreenCanvas в worker'е и ускоряет `vello` на 35–45 %. Тесты кода, которому нужен wasm32, идут wasm-bindgen-test в Node.

**Files:**
- Modify: `.cargo/config.toml`
- Modify: `crates/ferrum-wasm/Cargo.toml`
- Modify: `crates/ferrum-wasm/src/lib.rs`
- Modify: `crates/ferrum-wasm/src/core.rs`
- Create: `crates/ferrum-wasm/src/web/mod.rs`
- Create: `crates/ferrum-wasm/src/web/clock.rs`
- Create: `crates/ferrum-wasm/src/web/surface.rs`
- Test: `crates/ferrum-wasm/tests/web.rs`

**Interfaces:**
- Consumes: `Core` (Task 6), `GpuBackend`/`GpuDevice`/`atlas_config` (Task 2), `Clock` (Task 1), `protocol`.
- Produces: `Core::{draw_reference, request_frame, push_event}`; `web::{WasmCore, PerformanceClock, parse_command, events_to_js, start}`. Для JS: `WasmCore.create(canvas, platform, width, height, dpr, force?, atlasSide?) → Promise<WasmCore>`, `backend()`, `input(Uint8Array) → number`, `command(Command)`, `frame() → boolean`, `live() → Float64Array`, `takeEvents() → CoreEvent[]`.

- [ ] **Step 1: wasm-bindgen-cli той же версии**

Run: `cargo install wasm-bindgen-cli --version 0.2.129 --locked`
Expected: ставит `wasm-bindgen` и `wasm-bindgen-test-runner` в `~/.cargo/bin` (пропустить, если `wasm-bindgen --version` уже `wasm-bindgen 0.2.129`).

- [ ] **Step 2: Запуск wasm-тестов и SIMD128; зависимости wasm32**

`.cargo/config.toml` (целиком):

````toml
# `psm` (его тянет pest → loro-internal) при сборке под wasm32 упаковывает готовый
# объектный файл через `ar`. Системный `ar` macOS не понимает объекты wasm (архив
# без таблицы символов, rustc падает с «section too large»), поэтому для wasm32
# берётся llvm-ar из компонента llvm-tools тулчейна (rust-toolchain.toml).
[env]
AR_wasm32_unknown_unknown = { value = "scripts/llvm-ar", relative = true }

# `cargo test --target wasm32-unknown-unknown` запускает тесты wasm-bindgen-test в
# Node (wasm-bindgen-cli той же версии, что крейт wasm-bindgen, 0.2.129).
[target.wasm32-unknown-unknown]
runner = "wasm-bindgen-test-runner"
# SIMD128 есть во всех браузерах с OffscreenCanvas в worker'е (Chrome 91+,
# Firefox 89+, Safari 16.4+): vello разбирает полосы и растеризует на CPU быстрее.
rustflags = ["-C", "target-feature=+simd128"]
````

В `crates/ferrum-wasm/Cargo.toml` заменить:

````toml
ferrum-render = { path = "../ferrum-render" }
ferrum-scene = { path = "../ferrum-scene" }
kurbo.workspace = true
````

на:

````toml
ferrum-render = { path = "../ferrum-render" }
kurbo.workspace = true
````

В `crates/ferrum-wasm/Cargo.toml` заменить:

````toml

# proptest под wasm32 не собирается (getrandom без бэкенда): нативные тесты.
[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]
proptest.workspace = true
serde_json.workspace = true
````

на:

````toml

# Браузерная часть (модуль `web`): только для wasm32, нативные тесты её не собирают.
[target.'cfg(target_arch = "wasm32")'.dependencies]
ferrum-render = { path = "../ferrum-render", features = ["gpu"] }
wasm-bindgen-futures.workspace = true
js-sys.workspace = true
web-sys = { workspace = true, features = [
    "OffscreenCanvas",
    "OffscreenCanvasRenderingContext2d",
    "ImageData",
    "HtmlCanvasElement",
    "WebGl2RenderingContext",
    "Performance",
] }
wgpu.workspace = true
vello_gpu = { workspace = true, features = ["webgl"] }
vello_common.workspace = true
serde-wasm-bindgen.workspace = true
console_error_panic_hook.workspace = true

# proptest под wasm32 не собирается (getrandom без бэкенда): нативные тесты.
[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]
proptest.workspace = true
serde_json.workspace = true

[target.'cfg(target_arch = "wasm32")'.dev-dependencies]
wasm-bindgen-test.workspace = true
````

- [ ] **Step 3: Написать падающие wasm-тесты**

`crates/ferrum-wasm/tests/web.rs`:

````rust
//! Код, которому нужен wasm32: часы `performance.now()`, разбор команд из
//! JS-объектов и события в JS, ядро кадра без `std::time::Instant` (он в wasm32
//! паникует). Запуск в Node:
//! `cargo test --target wasm32-unknown-unknown -p ferrum-wasm --test web`.
#![cfg(target_arch = "wasm32")]

use ferrum_editor::{Input, Modifiers, Platform, PointerButton, PointerEvent};
use ferrum_render::{Clock, CpuBackend};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::{Backend, Command, CoreEvent};
use ferrum_wasm::web::{events_to_js, parse_command, PerformanceClock};
use js_sys::{Object, Reflect, Uint8Array};
use kurbo::{Point, Size};
use serde_bytes::ByteBuf;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;

fn obj(pairs: &[(&str, JsValue)]) -> JsValue {
    let o = Object::new();
    for (k, v) in pairs {
        Reflect::set(&o, &(*k).into(), v).unwrap();
    }
    o.into()
}

#[wasm_bindgen_test]
fn performance_clock_runs() {
    let c = PerformanceClock::new().expect("в Node есть performance");
    let a = c.now_ms();
    let mut x = 0u64;
    for i in 0..2_000_000u64 {
        x = x.wrapping_add(i * i);
    }
    let b = c.now_ms();
    assert!(x > 0 && b >= a && b - a < 10_000.0, "{a} → {b}");
}

#[wasm_bindgen_test]
fn commands_parse_from_js_objects() {
    let bytes = Uint8Array::from(&[1u8, 2, 3][..]);
    let open = obj(&[("type", "open".into()), ("bytes", bytes.into())]);
    assert_eq!(
        parse_command(open),
        Ok(Command::Open {
            bytes: ByteBuf::from(vec![1, 2, 3])
        })
    );
    let demo = obj(&[("type", "demo".into()), ("nodes", 100.into())]);
    assert_eq!(parse_command(demo), Ok(Command::Demo { nodes: 100 }));
    for bad in [
        obj(&[("type", "demo".into()), ("nodes", "много".into())]),
        obj(&[("type", "erase".into())]),
        JsValue::from_str("save"),
        JsValue::NULL,
    ] {
        let e = parse_command(bad).unwrap_err();
        assert!(e.starts_with("Неверная команда"), "{e}");
    }
}

#[wasm_bindgen_test]
fn events_become_js_objects_with_byte_arrays() {
    let js = events_to_js(&[
        CoreEvent::Clipboard {
            bytes: ByteBuf::from(vec![7, 8]),
        },
        CoreEvent::Document {
            can_undo: true,
            can_redo: false,
            steps: 2.0,
        },
    ]);
    let arr: js_sys::Array = js.dyn_into().unwrap();
    assert_eq!(arr.length(), 2);
    let clip = arr.get(0);
    assert_eq!(Reflect::get(&clip, &"type".into()).unwrap(), "clipboard");
    let bytes = Reflect::get(&clip, &"bytes".into()).unwrap();
    let bytes: Uint8Array = bytes.dyn_into().expect("байты — Uint8Array");
    assert_eq!(bytes.to_vec(), [7, 8]);
    let doc = arr.get(1);
    assert_eq!(Reflect::get(&doc, &"canUndo".into()).unwrap(), true);
}

#[wasm_bindgen_test]
fn core_frame_runs_in_wasm_without_instant() {
    let mut core = Core::new(Platform::Linux, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    core.renderer_mut()
        .set_clock(PerformanceClock::new().map(|c| Box::new(c) as _));
    core.command(Command::Demo { nodes: 2000 });
    let (w, h) = core.device_size();
    let mut b = CpuBackend::new(w, h);
    core.draw(&mut b);
    // Перетаскивание и отмена — те же пути, что в браузере.
    let p = |x: f64, y: f64, t: f64| PointerEvent {
        point: Point::new(x, y),
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
        time_ms: t,
    };
    // Камера примера: центр (720, 512), зум 0.5, холст 400 × 300.
    let screen = |x: f64, y: f64| ((x - 720.0) * 0.5 + 200.0, (y - 512.0) * 0.5 + 150.0);
    // «Слой 25» первого фрейма: (700, 200) – (820, 280).
    let (sx, sy) = screen(760.0, 240.0);
    core.handle(Input::PointerDown(p(sx, sy, 0.0)));
    core.handle(Input::PointerMove(p(sx + 20.0, sy + 10.0, 5.0)));
    core.handle(Input::PointerUp(p(sx + 20.0, sy + 10.0, 9.0)));
    core.draw(&mut b);
    let live = core.live();
    assert_eq!(
        (live.steps, live.selection_x, live.selection_y),
        (1.0, 740.0, 220.0)
    );
    // Отдаление: растры фреймов с бюджетом по часам `performance.now()`.
    core.command(Command::Execute {
        command: "view.fit_all".into(),
    });
    for _ in 0..3 {
        core.draw(&mut b);
    }
    let pm = b.render();
    assert_eq!((pm.width(), pm.height()), (400, 300));
}
````

Run: `cargo test --target wasm32-unknown-unknown -p ferrum-wasm --test web`
Expected: FAIL — не компилируется: нет модуля `web`.

- [ ] **Step 4: Эталонный кадр и события обёртки в `Core`**

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
use crate::protocol::{
    Backend, Command, CursorKind, CoreEvent, Theme, Tool, MAX_DEMO_NODES, SELECTION_IDS_MAX,
};
````

на:

````rust
use crate::protocol::{
    Backend, Command, CoreEvent, CursorKind, Theme, Tool, MAX_DEMO_NODES, SELECTION_IDS_MAX,
};
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
                    // Наведение меняет курсор часто: в пачке важен только последний.
                    self.events.retain(|e| !matches!(e, CoreEvent::Cursor { .. }));
                    let (cursor, angle) = cursor(c);
                    self.events.push(CoreEvent::Cursor { cursor, angle });
                }
                EditorEvent::PageChanged(p) => self.events.push(CoreEvent::Page { id: p.to_string() }),
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
````

на:

````rust
                    // Наведение меняет курсор часто: в пачке важен только последний.
                    self.events
                        .retain(|e| !matches!(e, CoreEvent::Cursor { .. }));
                    let (cursor, angle) = cursor(c);
                    self.events.push(CoreEvent::Cursor { cursor, angle });
                }
                EditorEvent::PageChanged(p) => {
                    self.events.push(CoreEvent::Page { id: p.to_string() })
                }
                EditorEvent::Clipboard(bytes) => self.events.push(CoreEvent::Clipboard {
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        if document {
            self.events.retain(|e| !matches!(e, CoreEvent::Document { .. }));
            self.push_document();
````

на:

````rust
        if document {
            self.events
                .retain(|e| !matches!(e, CoreEvent::Document { .. }));
            self.push_document();
````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
            Err(e) => self.error(e.to_string()),
        }
    }

````

на:

````rust
            Err(e) => self.error(e.to_string()),
        }
    }

    /// Кадр не дошёл до экрана (поверхность пропустила его): нарисовать снова.
    pub fn request_frame(&mut self) {
        self.dirty = true;
    }

    /// Событие от вызывающего (готовность, пиксели, ошибки поверхности) — в общую
    /// очередь с событиями редактора.
    pub fn push_event(&mut self, e: CoreEvent) {
        self.events.push(e);
    }

````

В `crates/ferrum-wasm/src/core.rs` заменить:

````rust
        stats
    }
````

на:

````rust
        stats
    }

    /// Тот же кадр, нарисованный `vello_cpu` отдельным рендерером, — эталон для
    /// сравнения с GPU в тестах (`ReadPixels { reference: true }`). Свой рендерер:
    /// кэши и счётчики кадров экрана не трогаются. Без растрового кэша и без
    /// предела времени: эталон не зависит от истории кадров.
    pub fn draw_reference(&self) -> ferrum_render::Pixmap {
        let (w, h) = self.device_size();
        let mut backend = ferrum_render::CpuBackend::new(w, h);
        let mut renderer = Renderer::with_raster_cache_bytes(0);
        renderer.set_raster_build_time(None);
        let provider = self.images.provider(&self.editor.document().images);
        renderer.render(
            &mut backend,
            self.editor.scene(),
            self.editor.camera(),
            &self.editor.overlay(),
            &provider,
            Some(self.background()),
        );
        backend.render()
    }
````

- [ ] **Step 5: `WasmCore` и поверхности**

В `crates/ferrum-wasm/src/lib.rs` заменить:

````rust
//! кадра и отдаёт редактору по порядку.

````

на:

````rust
//! кадра и отдаёт редактору по порядку.
//!
//! Браузерная часть (`web`, только wasm32): `WasmCore` для worker'а —
//! поверхность WebGPU, WebGL2 или CPU на OffscreenCanvas, часы `performance.now()`
//! и привязки wasm-bindgen.

````

В `crates/ferrum-wasm/src/lib.rs` заменить:

````rust
pub mod protocol;
````

на:

````rust
pub mod protocol;
#[cfg(target_arch = "wasm32")]
pub mod web;
````

`crates/ferrum-wasm/src/web/mod.rs`:

````rust
//! Привязки wasm-bindgen для worker'а: `WasmCore` владеет ядром (`Core`) и
//! поверхностью рендера. Цикл кадров — в `apps/web/src/worker/core.worker.ts`: в
//! начале кадра он передаёт накопленные записи ввода (`input`), затем вызывает
//! `frame`, пишет `live()` в общую память и отправляет `takeEvents()`.

mod clock;
mod surface;

use ferrum_editor::Platform;
use kurbo::Size;
use serde::Serialize;
use serde_bytes::ByteBuf;
use wasm_bindgen::prelude::*;
use web_sys::OffscreenCanvas;

use crate::core::Core;
use crate::protocol::{Backend, Command, CoreEvent};
pub use clock::PerformanceClock;
use surface::{Pixels, Readback, Surface};

#[wasm_bindgen(start)]
pub fn start() {
    // Паника в Rust — в консоль worker'а с текстом и стеком, а не «unreachable».
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct WasmCore {
    core: Core,
    surface: Surface,
    clock: Option<PerformanceClock>,
    /// Поверхность больше не рисует (потеря устройства или контекста).
    dead: bool,
    /// Ждём пиксели из GPU (WebGPU копирует их асинхронно).
    awaiting: bool,
    /// Об ошибке атласа уже сообщили (она остаётся до перезапуска).
    atlas_reported: bool,
}

/// Команда из JS-объекта. Неверная форма — текст ошибки, не паника.
pub fn parse_command(cmd: JsValue) -> Result<Command, String> {
    serde_wasm_bindgen::from_value::<Command>(cmd).map_err(|e| format!("Неверная команда: {e}"))
}

/// События — массив JS-объектов; байты — `Uint8Array`.
pub fn events_to_js(events: &[CoreEvent]) -> JsValue {
    let ser = serde_wasm_bindgen::Serializer::new();
    events.serialize(&ser).unwrap_or_else(|e| {
        // Сериализация своих же типов не падает; на всякий случай — без паники.
        let err = CoreEvent::Error {
            message: format!("Не удалось передать события: {e}"),
        };
        js_sys::Array::of1(&err.serialize(&ser).unwrap_or(JsValue::NULL)).into()
    })
}

fn parse_platform(p: &str) -> Platform {
    match p {
        "mac" => Platform::Mac,
        "windows" => Platform::Windows,
        _ => Platform::Linux,
    }
}

#[wasm_bindgen]
impl WasmCore {
    /// Запускает ядро на холсте `width × height` CSS-пикселей: выбирает бэкенд по
    /// порядку WebGPU → WebGL2 → CPU (или `force`, а CPU — запасной), причины
    /// отказов — в событии `ready`. `atlas_side` — только для тестов: атлас
    /// изображений GPU из одной страницы этой стороны (вытеснение сразу).
    pub async fn create(
        canvas: OffscreenCanvas,
        platform: String,
        width: f64,
        height: f64,
        dpr: f64,
        force: Option<String>,
        atlas_side: Option<u16>,
    ) -> Result<WasmCore, JsError> {
        let mut core = Core::new(
            parse_platform(&platform),
            Size::new(width.max(1.0), height.max(1.0)),
            dpr,
            Backend::Cpu,
        );
        core.renderer_mut()
            .set_clock(PerformanceClock::new().map(|c| Box::new(c) as _));
        let forced = force.as_deref().and_then(Backend::parse);
        let order: Vec<Backend> = match forced {
            Some(Backend::Cpu) => vec![Backend::Cpu],
            Some(b) => vec![b, Backend::Cpu],
            None => vec![Backend::Webgpu, Backend::Webgl, Backend::Cpu],
        };
        let (w, h) = core.device_size();
        let mut fallbacks = Vec::new();
        let mut chosen = None;
        for b in order {
            // Программный GPU — только если его попросили явно.
            match Surface::create(b, &canvas, w, h, forced == Some(b), atlas_side).await {
                Ok(s) => {
                    chosen = Some(s);
                    break;
                }
                Err(e) => fallbacks.push(format!("{}: {e}", b.name())),
            }
        }
        let surface = chosen.ok_or_else(|| {
            JsError::new(&format!(
                "Нет ни одного бэкенда рендера: {}",
                fallbacks.join("; ")
            ))
        })?;
        core.set_backend(surface.kind());
        core.push_event(CoreEvent::Ready {
            backend: surface.kind(),
            fallbacks,
        });
        Ok(WasmCore {
            core,
            surface,
            clock: PerformanceClock::new(),
            dead: false,
            awaiting: false,
            atlas_reported: false,
        })
    }

    /// Активный бэкенд: `webgpu`, `webgl` или `cpu`.
    pub fn backend(&self) -> String {
        self.surface.kind().name().to_string()
    }

    /// Записи кольцевого буфера ввода (кратно `RECORD_SIZE` байт). Возвращает
    /// число принятых.
    pub fn input(&mut self, records: &[u8]) -> u32 {
        self.core.apply_input(records) as u32
    }

    /// Команда основного потока. Объект не той формы — событие `error`, не паника.
    pub fn command(&mut self, #[wasm_bindgen(unchecked_param_type = "Command")] cmd: JsValue) {
        match parse_command(cmd) {
            Ok(c) => self.core.command(c),
            Err(message) => self.core.push_event(CoreEvent::Error { message }),
        }
    }

    fn now(&self) -> f64 {
        use ferrum_render::Clock;
        self.clock.as_ref().map_or(0.0, |c| c.now_ms())
    }

    fn pixels(&mut self, backend: Backend, p: Pixels) {
        self.core.push_event(CoreEvent::Pixels {
            backend,
            width: p.width as u32,
            height: p.height as u32,
            rgba: ByteBuf::from(p.rgba),
        });
    }

    /// Начало кадра после ввода: рисует кадр, если он нужен. `true` — нарисован.
    pub fn frame(&mut self) -> bool {
        if let Some(r) = self.surface.poll_readback() {
            self.awaiting = false;
            match r {
                Ok(p) => self.pixels(self.surface.kind(), p),
                Err(message) => self.core.push_event(CoreEvent::Error { message }),
            }
        }
        let (fatal, soft) = self.surface.take_errors();
        if let Some(message) = soft {
            self.core.push_event(CoreEvent::Error { message });
        }
        if let Some(message) = fatal {
            self.dead = true;
            self.core.push_event(CoreEvent::Error { message });
        }
        if !self.atlas_reported {
            if let Some(message) = self.surface.atlas_error().map(str::to_string) {
                self.atlas_reported = true;
                self.core.push_event(CoreEvent::Error { message });
            }
        }
        if self.dead {
            if self.core.take_pixel_request().is_some() {
                self.core.push_event(CoreEvent::Error {
                    message: "Пиксели не прочитать: поверхность не рисует".into(),
                });
            }
            return false;
        }
        if !self.core.wants_frame() {
            return false;
        }
        let start = self.now();
        let read = self.core.take_pixel_request();
        // Пока прошлое чтение из GPU не закончилось, новое ждёт следующего кадра.
        let read_now = read.is_some() && !self.awaiting;
        match &mut self.surface {
            Surface::WebGpu(s) => {
                self.core.draw(&mut s.backend);
            }
            Surface::WebGl(s) => {
                self.core.draw(&mut s.backend);
            }
            Surface::Cpu(s) => {
                self.core.draw(&mut s.backend);
            }
        }
        let served = match self.surface.present(read_now) {
            Ok(Some(Readback::Ready(p))) => {
                self.pixels(self.surface.kind(), p);
                true
            }
            Ok(Some(Readback::Pending)) => {
                self.awaiting = true;
                true
            }
            Ok(None) => false,
            Err(_) => {
                self.core.request_frame();
                false
            }
        };
        match read {
            // Запрос не обслужен (кадр пропущен или ждём прошлое чтение): вернуть.
            Some(reference) if !served => self.core.command(Command::ReadPixels { reference }),
            Some(true) => {
                let r = self.core.draw_reference();
                self.pixels(
                    Backend::Cpu,
                    Pixels {
                        width: r.width(),
                        height: r.height(),
                        rgba: r.data_as_u8_slice().to_vec(),
                    },
                );
            }
            _ => {}
        }
        let end = self.now();
        self.core.frame_done(end, end - start);
        let (images, misses) = self.surface.atlas();
        self.core.set_atlas(images, misses);
        true
    }

    /// Живое состояние (ячейки `Live` из `layout.gen.ts`).
    pub fn live(&self) -> Vec<f64> {
        self.core.live().to_slots().to_vec()
    }

    /// События с прошлого вызова. Байты — `Uint8Array`: их буферы worker передаёт
    /// основному потоку как transferable.
    #[wasm_bindgen(js_name = takeEvents, unchecked_return_type = "CoreEvent[]")]
    pub fn take_events(&mut self) -> JsValue {
        events_to_js(&self.core.take_events())
    }
}
````

`crates/ferrum-wasm/src/web/clock.rs`:

````rust
//! Часы `performance.now()` для рендера в wasm32: `std::time::Instant` там
//! паникует. `performance` есть и в окне, и в worker'е, и в Node (тесты).

use ferrum_render::Clock;
use wasm_bindgen::JsCast;
use web_sys::Performance;

pub struct PerformanceClock(Performance);

impl PerformanceClock {
    /// `None`, если в глобальном объекте нет `performance`.
    pub fn new() -> Option<PerformanceClock> {
        let global = js_sys::global();
        let perf = js_sys::Reflect::get(&global, &"performance".into()).ok()?;
        perf.dyn_into::<Performance>().ok().map(PerformanceClock)
    }
}

impl Clock for PerformanceClock {
    fn now_ms(&self) -> f64 {
        self.0.now()
    }
}
````

`crates/ferrum-wasm/src/web/surface.rs`:

````rust
//! Поверхности рендера на OffscreenCanvas: `vello_gpu` на WebGPU (wgpu), `vello_gpu`
//! на WebGL2 и `vello_cpu` с выводом через `putImageData`. Выбор — при старте, по
//! порядку, первый подошедший.
//!
//! У холста контекст выбирается один раз: после `getContext("webgpu")` получить
//! `webgl2` или `2d` уже нельзя. Поэтому каждый вариант сначала проверяется без
//! холста (адаптер WebGPU, пробный OffscreenCanvas 1×1 для WebGL2), и только потом
//! создаётся контекст настоящего холста.
//!
//! При автоматическом выборе программный GPU не годится: SwiftShader (WebGL2 без
//! видеокарты, им же рисует headless Chromium) и запасной адаптер WebGPU медленнее
//! `vello_cpu` и к тому же тормозят выдачу кадров. Явно запрошенный бэкенд
//! (`force`) берётся и программным.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use ferrum_render::{
    atlas_config, CpuBackend, GpuBackend, GpuDevice, ImageId, Pixmap, RenderBackend,
};
use vello_gpu::{MemorySettings, RenderSettings, RenderSize};
use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{
    HtmlCanvasElement, ImageData, OffscreenCanvas, OffscreenCanvasRenderingContext2d,
    WebGl2RenderingContext,
};

use crate::protocol::Backend;

/// Пиксели кадра: готовые (CPU, WebGL2) или ещё копируются из GPU (WebGPU).
pub enum Readback {
    Ready(Pixels),
    Pending,
}

/// RGBA кадра, строки сверху вниз, и его размер в момент чтения.
pub struct Pixels {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

/// Атлас изображений: по умолчанию `atlas_config` устройства; `atlas_side` (тесты)
/// — одна страница указанной стороны, чтобы вытеснение случалось сразу.
fn atlas_for(max_texture_side: u32, atlas_side: Option<u16>) -> ferrum_render::AtlasConfig {
    let base = atlas_config(max_texture_side);
    match atlas_side {
        Some(side) => ferrum_render::AtlasConfig {
            max_atlases: 1,
            atlas_size: (
                side.min(base.atlas_size.0).max(1),
                side.min(base.atlas_size.1).max(1),
            ),
            ..base
        },
        None => base,
    }
}

/// Устройство wgpu и рендерер `vello_gpu` для атласа изображений `GpuBackend`.
pub struct WgpuDevice {
    renderer: vello_gpu::Renderer,
    resources: vello_gpu::Resources,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Команды загрузки картинок этого кадра: уходят в GPU вместе с кадром.
    encoder: Option<wgpu::CommandEncoder>,
}

impl WgpuDevice {
    fn take_encoder(&mut self) -> wgpu::CommandEncoder {
        self.encoder.take().unwrap_or_else(|| {
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        })
    }
}

impl GpuDevice for WgpuDevice {
    fn upload(&mut self, pixmap: &Pixmap) -> Result<ImageId, String> {
        let mut enc = self.take_encoder();
        // Место заранее проверено зеркалом `GpuBackend`: здесь выделение не падает.
        let id = self.renderer.upload_image(
            &mut self.resources,
            &self.device,
            &self.queue,
            &mut enc,
            pixmap,
        );
        self.encoder = Some(enc);
        Ok(id)
    }

    fn destroy(&mut self, id: ImageId) {
        // Очистка места — проход в своём кодировщике, отправленный сразу. Загрузка
        // картинки идёт `queue.write_texture`, а такие записи выполняются раньше
        // команд следующего `submit`: оставь очистку в общем кодировщике кадра — она
        // стёрла бы картинку, загруженную на это же место после вытеснения.
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.renderer
            .destroy_image(&mut self.resources, &mut enc, id);
        self.queue.submit([enc.finish()]);
    }
}

/// Рендерер `vello_gpu` на WebGL2.
pub struct WebGlDevice {
    renderer: vello_gpu::WebGlRenderer,
    resources: vello_gpu::Resources,
}

impl GpuDevice for WebGlDevice {
    fn upload(&mut self, pixmap: &Pixmap) -> Result<ImageId, String> {
        self.renderer
            .upload_image(&mut self.resources, pixmap)
            .map_err(|e| format!("{e:?}"))
    }

    fn destroy(&mut self, id: ImageId) {
        let _ = self.renderer.destroy_image(&mut self.resources, id);
    }
}

/// Ошибки устройства WebGPU, пришедшие вне вызовов (потеря устройства, ошибки
/// проверки).
#[derive(Default)]
struct GpuErrors {
    lost: Option<String>,
    uncaptured: Option<String>,
}

pub struct WebGpuSurface {
    canvas: OffscreenCanvas,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pub backend: GpuBackend<WgpuDevice>,
    /// `Arc<Mutex>`: wgpu требует от обработчиков `Send + Sync`, хотя в wasm без
    /// потоков они вызываются в том же потоке.
    errors: Arc<Mutex<GpuErrors>>,
    readback: Option<PendingRead>,
}

/// Копия кадра для `ReadPixels`: буфер, байт в строке (кратно 256) и результат
/// `map_async`, когда он придёт.
struct PendingRead {
    buffer: wgpu::Buffer,
    row: u32,
    /// Размер кадра, который копируется: к ответу холст мог уже смениться.
    width: u16,
    height: u16,
    done: Rc<RefCell<Option<bool>>>,
}

pub struct WebGlSurface {
    canvas: OffscreenCanvas,
    pub backend: GpuBackend<WebGlDevice>,
}

pub struct CpuSurface {
    canvas: OffscreenCanvas,
    ctx: OffscreenCanvasRenderingContext2d,
    pub backend: CpuBackend,
}

pub enum Surface {
    WebGpu(Box<WebGpuSurface>),
    WebGl(Box<WebGlSurface>),
    Cpu(Box<CpuSurface>),
}

fn resize_canvas(canvas: &OffscreenCanvas, w: u16, h: u16) {
    if canvas.width() != w as u32 {
        canvas.set_width(w as u32);
    }
    if canvas.height() != h as u32 {
        canvas.set_height(h as u32);
    }
}

fn js_err(e: JsValue) -> String {
    e.as_string()
        .or_else(|| js_sys::Error::from(e).message().as_string())
        .unwrap_or_else(|| "ошибка JavaScript".into())
}

/// Параметры контекста WebGL2 — те же, что ставит `WebGlRenderer` (без сглаживания,
/// без программного рендера, с буфером глубины).
fn webgl_options() -> JsValue {
    let o = js_sys::Object::new();
    for (k, v) in [
        ("antialias", false),
        ("failIfMajorPerformanceCaveat", true),
        ("depth", true),
    ] {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v.into());
    }
    o.into()
}

impl Surface {
    /// Создаёт поверхность бэкенда `backend` на холсте размера `w × h` пикселей
    /// устройства. Ошибка — причина, по которой бэкенд не подошёл.
    pub async fn create(
        backend: Backend,
        canvas: &OffscreenCanvas,
        w: u16,
        h: u16,
        software_ok: bool,
        atlas_side: Option<u16>,
    ) -> Result<Surface, String> {
        resize_canvas(canvas, w, h);
        match backend {
            Backend::Webgpu => Ok(Surface::WebGpu(Box::new(
                WebGpuSurface::create(canvas, w, h, software_ok, atlas_side).await?,
            ))),
            Backend::Webgl => Ok(Surface::WebGl(Box::new(WebGlSurface::create(
                canvas,
                w,
                h,
                software_ok,
                atlas_side,
            )?))),
            Backend::Cpu => Ok(Surface::Cpu(Box::new(CpuSurface::create(canvas, w, h)?))),
        }
    }

    pub fn kind(&self) -> Backend {
        match self {
            Surface::WebGpu(_) => Backend::Webgpu,
            Surface::WebGl(_) => Backend::Webgl,
            Surface::Cpu(_) => Backend::Cpu,
        }
    }

    /// Ошибка, после которой поверхность больше не рисует (потеря устройства или
    /// контекста), и ошибка WebGPU, пришедшая вне вызовов (каждая — один раз).
    pub fn take_errors(&mut self) -> (Option<String>, Option<String>) {
        match self {
            Surface::WebGpu(s) => {
                let mut e = s.errors.lock().unwrap_or_else(|p| p.into_inner());
                (e.lost.take(), e.uncaptured.take())
            }
            Surface::WebGl(s) => {
                let gl: &WebGl2RenderingContext = s.backend.device().renderer.gl_context();
                let lost = gl
                    .is_context_lost()
                    .then(|| "Контекст WebGL2 потерян".to_string());
                (lost, None)
            }
            Surface::Cpu(_) => (None, None),
        }
    }

    /// Ошибка атласа изображений: после неё картинки рисуются заглушкой.
    pub fn atlas_error(&self) -> Option<&str> {
        match self {
            Surface::WebGpu(s) => s.backend.atlas_error(),
            Surface::WebGl(s) => s.backend.atlas_error(),
            Surface::Cpu(_) => None,
        }
    }

    /// Картинок в атласе GPU и картинок, которым не хватило места.
    pub fn atlas(&self) -> (usize, u64) {
        let s = match self {
            Surface::WebGpu(s) => s.backend.atlas_stats(),
            Surface::WebGl(s) => s.backend.atlas_stats(),
            Surface::Cpu(_) => return (0, 0),
        };
        (s.images, s.misses)
    }

    /// Отправляет нарисованный кадр на холст. `read` — заодно прочитать пиксели.
    pub fn present(&mut self, read: bool) -> Result<Option<Readback>, String> {
        match self {
            Surface::WebGpu(s) => s.present(read),
            Surface::WebGl(s) => s.present(read),
            Surface::Cpu(s) => s.present(read),
        }
    }

    /// Пиксели, скопированные из GPU, если копия готова (WebGPU).
    pub fn poll_readback(&mut self) -> Option<Result<Pixels, String>> {
        match self {
            Surface::WebGpu(s) => s.poll_readback(),
            _ => None,
        }
    }
}

impl WebGpuSurface {
    async fn create(
        canvas: &OffscreenCanvas,
        w: u16,
        h: u16,
        software_ok: bool,
        atlas_side: Option<u16>,
    ) -> Result<WebGpuSurface, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        // Адаптер и устройство — до контекста холста: без них холст свободен для
        // WebGL2 или 2D.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| format!("нет адаптера ({e})"))?;
        if !software_ok && adapter.get_info().device_type == wgpu::DeviceType::Cpu {
            return Err("только программный адаптер".into());
        }
        // Пределы адаптера, а не умолчания WebGPU: иначе холст шире 8192 пикселей
        // устройства (широкий экран с dpr 2) не настроить.
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: adapter.limits(),
                ..wgpu::DeviceDescriptor::default()
            })
            .await
            .map_err(|e| format!("нет устройства ({e})"))?;
        let errors = Arc::new(Mutex::new(GpuErrors::default()));
        {
            let e = errors.clone();
            device.set_device_lost_callback(move |reason, msg| {
                let mut e = e.lock().unwrap_or_else(|p| p.into_inner());
                e.lost = Some(format!("Устройство WebGPU потеряно ({reason:?}): {msg}"));
            });
            let e = errors.clone();
            device.on_uncaptured_error(Arc::new(move |err| {
                let mut e = e.lock().unwrap_or_else(|p| p.into_inner());
                if e.uncaptured.is_none() {
                    e.uncaptured = Some(format!("Ошибка WebGPU: {err}"));
                }
            }));
        }
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .map_err(|e| format!("нет поверхности ({e})"))?;
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let config = wgpu::SurfaceConfiguration {
            // COPY_SRC — для чтения пикселей в тестах (`ReadPixels`).
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width: w as u32,
            height: h as u32,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        let atlas = atlas_for(device.limits().max_texture_dimension_2d, atlas_side);
        let settings = RenderSettings {
            memory_settings: MemorySettings {
                image_atlas_config: atlas,
                ..MemorySettings::default()
            },
            ..RenderSettings::default()
        };
        let (renderer, resources) = vello_gpu::Renderer::new_with(
            &device,
            &vello_gpu::RenderTargetConfig {
                format,
                width: w,
                height: h,
            },
            settings,
        );
        let dev = WgpuDevice {
            renderer,
            resources,
            device,
            queue,
            encoder: None,
        };
        Ok(WebGpuSurface {
            canvas: canvas.clone(),
            surface,
            config,
            backend: GpuBackend::new(w, h, dev, atlas),
            errors,
            readback: None,
        })
    }

    fn present(&mut self, read: bool) -> Result<Option<Readback>, String> {
        let (w, h) = self.backend.size();
        if (w as u32, h as u32) != (self.config.width, self.config.height) {
            resize_canvas(&self.canvas, w, h);
            self.config.width = w as u32;
            self.config.height = h as u32;
            self.surface
                .configure(&self.backend.device().device, &self.config);
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                // Конфигурация устарела (смена размера, вкладка в фоне): настроить
                // заново, кадр нарисуется следующим.
                self.surface
                    .configure(&self.backend.device().device, &self.config);
                return Err(format!("кадр пропущен: {other:?}"));
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (scene, dev) = self.backend.parts();
        let mut enc = dev.take_encoder();
        dev.renderer
            .render(
                scene,
                &mut dev.resources,
                &dev.device,
                &dev.queue,
                &mut enc,
                &RenderSize {
                    width: w,
                    height: h,
                },
                &view,
                None,
                &vello_gpu::TextureBindings::new(),
                vello_gpu::TargetInit::Clear(vello_gpu::ClearSettings::default()),
            )
            .map_err(|e| format!("{e:?}"))?;
        let mut pending = None;
        if read {
            let row = (w as u32 * 4).next_multiple_of(256);
            let buffer = dev.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("ferrum readback"),
                size: row as u64 * h as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &frame.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(row),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d {
                    width: w as u32,
                    height: h as u32,
                    depth_or_array_layers: 1,
                },
            );
            pending = Some((buffer, row));
        }
        dev.queue.submit([enc.finish()]);
        dev.queue.present(frame);
        if let Some((buffer, row)) = pending {
            let done = Rc::new(RefCell::new(None));
            let flag = done.clone();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                *flag.borrow_mut() = Some(r.is_ok());
            });
            self.readback = Some(PendingRead {
                buffer,
                row,
                width: w,
                height: h,
                done,
            });
            return Ok(Some(Readback::Pending));
        }
        Ok(None)
    }

    fn poll_readback(&mut self) -> Option<Result<Pixels, String>> {
        let ready = self.readback.as_ref()?.done.borrow().is_some();
        if !ready {
            return None;
        }
        let PendingRead {
            buffer,
            row,
            width,
            height,
            done,
        } = self.readback.take()?;
        if *done.borrow() != Some(true) {
            return Some(Err("не удалось прочитать пиксели из GPU".into()));
        }
        let (w, h) = (width as usize, height as usize);
        let data = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| format!("{e:?}"));
        let out = data.map(|data| {
            let mut out = Vec::with_capacity(w * h * 4);
            for r in data.chunks_exact(row as usize).take(h) {
                out.extend_from_slice(&r[..w * 4]);
            }
            Pixels {
                width,
                height,
                rgba: out,
            }
        });
        buffer.unmap();
        Some(out)
    }
}

/// Имя программного рендерера WebGL2, если контекст рисует без видеокарты.
fn software_renderer(gl: &WebGl2RenderingContext) -> Option<String> {
    // UNMASKED_RENDERER_WEBGL из WEBGL_debug_renderer_info; без расширения —
    // обычный RENDERER (Chrome отдаёт там то же имя).
    const UNMASKED_RENDERER: u32 = 0x9246;
    let param = match gl.get_extension("WEBGL_debug_renderer_info") {
        Ok(Some(_)) => UNMASKED_RENDERER,
        _ => WebGl2RenderingContext::RENDERER,
    };
    let name = gl.get_parameter(param).ok()?.as_string()?;
    let lower = name.to_lowercase();
    [
        "swiftshader",
        "llvmpipe",
        "softpipe",
        "software",
        "basic render",
    ]
    .iter()
    .any(|s| lower.contains(s))
    .then_some(name)
}

impl WebGlSurface {
    fn create(
        canvas: &OffscreenCanvas,
        w: u16,
        h: u16,
        software_ok: bool,
        atlas_side: Option<u16>,
    ) -> Result<WebGlSurface, String> {
        // Пробный холст: WebGL2 без программного рендера и с глубиной 24 бит. Если
        // контекста нет, настоящий холст остаётся свободным для 2D.
        let probe = OffscreenCanvas::new(1, 1).map_err(js_err)?;
        let ctx = probe
            .get_context_with_context_options("webgl2", &webgl_options())
            .map_err(js_err)?
            .ok_or("WebGL2 недоступен (нет GPU или только программный рендер)")?;
        let gl: WebGl2RenderingContext = ctx
            .dyn_into()
            .map_err(|_| "не контекст WebGL2".to_string())?;
        if !software_ok {
            if let Some(name) = software_renderer(&gl) {
                return Err(format!("программный рендер ({name})"));
            }
        }
        let max_side = gl
            .get_parameter(WebGl2RenderingContext::MAX_TEXTURE_SIZE)
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(2048.0) as u32;
        let atlas = atlas_for(max_side, atlas_side);
        let settings = RenderSettings {
            memory_settings: MemorySettings {
                image_atlas_config: atlas,
                ..MemorySettings::default()
            },
            ..RenderSettings::default()
        };
        // `WebGlRenderer` принимает HtmlCanvasElement, но берёт у него только
        // `getContext`, который у OffscreenCanvas тот же (вызов в wasm-bindgen —
        // по имени метода).
        let fake: &HtmlCanvasElement = canvas.unchecked_ref();
        let (renderer, resources) = vello_gpu::WebGlRenderer::new_with(fake, settings, true)
            .map_err(|e| format!("{e:?}"))?;
        Ok(WebGlSurface {
            canvas: canvas.clone(),
            backend: GpuBackend::new(
                w,
                h,
                WebGlDevice {
                    renderer,
                    resources,
                },
                atlas,
            ),
        })
    }

    fn present(&mut self, read: bool) -> Result<Option<Readback>, String> {
        let (w, h) = self.backend.size();
        resize_canvas(&self.canvas, w, h);
        let (scene, dev) = self.backend.parts();
        dev.renderer
            .render(
                scene,
                &mut dev.resources,
                &RenderSize {
                    width: w,
                    height: h,
                },
                &vello_gpu::WebGlTextureBindings::new(),
                ferrum_render::Rgba::new([0.0, 0.0, 0.0, 0.0]),
            )
            .map_err(|e| format!("{e:?}"))?;
        if !read {
            return Ok(None);
        }
        // Читать сразу после рисования: без `preserveDrawingBuffer` буфер кадра
        // очищается, как только кадр уйдёт на экран.
        let gl = dev.renderer.gl_context();
        let (wu, hu) = (w as usize, h as usize);
        let mut flipped = vec![0u8; wu * hu * 4];
        gl.read_pixels_with_opt_u8_array(
            0,
            0,
            w as i32,
            h as i32,
            WebGl2RenderingContext::RGBA,
            WebGl2RenderingContext::UNSIGNED_BYTE,
            Some(&mut flipped),
        )
        .map_err(js_err)?;
        // В WebGL строки идут снизу вверх.
        let mut out = Vec::with_capacity(flipped.len());
        for r in flipped.chunks_exact(wu * 4).rev() {
            out.extend_from_slice(r);
        }
        Ok(Some(Readback::Ready(Pixels {
            width: w,
            height: h,
            rgba: out,
        })))
    }
}

impl CpuSurface {
    fn create(canvas: &OffscreenCanvas, w: u16, h: u16) -> Result<CpuSurface, String> {
        let opts = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&opts, &"alpha".into(), &false.into());
        let ctx = canvas
            .get_context_with_context_options("2d", &opts)
            .map_err(js_err)?
            .ok_or("холст уже занят другим контекстом")?
            .dyn_into::<OffscreenCanvasRenderingContext2d>()
            .map_err(|_| "не контекст 2D".to_string())?;
        Ok(CpuSurface {
            canvas: canvas.clone(),
            ctx,
            backend: CpuBackend::new(w, h),
        })
    }

    fn present(&mut self, read: bool) -> Result<Option<Readback>, String> {
        let (w, h) = self.backend.size();
        resize_canvas(&self.canvas, w, h);
        let pixmap = self.backend.render();
        // Фон кадра непрозрачный, поэтому RGBA с премультипликацией совпадает с
        // обычным, которого ждёт ImageData.
        let img = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(pixmap.data_as_u8_slice()),
            w as u32,
            h as u32,
        )
        .map_err(js_err)?;
        self.ctx.put_image_data(&img, 0.0, 0.0).map_err(js_err)?;
        Ok(read.then(|| {
            Readback::Ready(Pixels {
                width: w,
                height: h,
                rgba: pixmap.data_as_u8_slice().to_vec(),
            })
        }))
    }
}
````

- [ ] **Step 6: Запустить тесты**

Run: `cargo test --target wasm32-unknown-unknown -p ferrum-wasm --test web`
Expected: PASS, `4 passed` (Node).

Run: `cargo clippy --target wasm32-unknown-unknown -p ferrum-wasm --lib --tests -- -D warnings`
Expected: без предупреждений.

Run: `cargo test -p ferrum-wasm`
Expected: PASS — нативные наборы без изменений.

- [ ] **Step 7: Закоммитить**

```bash
git add .cargo/config.toml Cargo.lock crates/ferrum-wasm
git commit -m "feat(wasm): WasmCore — поверхности WebGPU, WebGL2 и CPU на OffscreenCanvas"
```

---

### Task 8: Стенд `apps/web`: транспорт, worker, ввод

Минимальная страница на Vite и TypeScript: холст на всё окно под управлением worker'а и отладочная плашка (бэкенд, транспорт, fps, время кадра worker'а, бюджет основного потока). Этап 5 оборачивает класс `Core`, а не страницу.

- **`Core` (основной поток):** создаёт worker, выбирает транспорт, передаёт холст `transferControlToOffscreen` и ждёт `ready`. Ввод: `pointerdown` (захват указателя), `pointermove` со всеми `getCoalescedEvents()`, `pointerup`/`pointercancel` (кнопки «назад»/«вперёд» отсекаются), `wheel` (`passive: false`, строки и страницы — в пиксели), жесты Safari, `keydown`/`keyup` на окне (сочетания браузера — перезагрузка, инструменты разработчика — пропускаются), `blur`, `ResizeObserver` и смена dpr (`matchMedia`). Каждый обработчик только кодирует запись — и сам меряет своё время: `mainThreadStats()` отдаёт худший и p95 кадр основного потока. В `requestAnimationFrame` — тик транспорта и чтение живого состояния. Курсор события `cursor` — CSS-курсор холста.
- **`Transport`:** `SabTransport` — кольцо и живое состояние в SharedArrayBuffer (`crossOriginIsolated`); если кольцо полно (worker открывает документ), записи ждут в очереди основного потока до 4096 и уходят следующими кадрами в том же порядке; worker без `Atomics.waitAsync` (Firefox) будится сообщением, не чаще, чем он успевает записать живое состояние. `MessageTransport` — записи копятся в буфере и уходят сообщением кадра; новый кадр не шлётся, пока не пришёл ответ с живым состоянием, — медленный worker не копит очередь.
- **Worker:** `Atomics.waitAsync` на счётчике кадров; если кадр основного потока уже ушёл вперёд (прошлый был долгим), ожидание возвращается сразу, и worker явно уступает циклу событий — иначе команды (Save, Open) ждали бы, пока кадры не станут быстрыми. Счётчик записи из общей памяти не доверенный: больше ёмкости кольца не читается. Байты событий передаются без копии.
- **Заголовки:** `Cross-Origin-Opener-Policy: same-origin` и `Cross-Origin-Embedder-Policy: credentialless` в `vite` и `vite preview`; `FERRUM_NO_ISOLATION=1` — сервер без них (запасной транспорт).

TypeScript проверяется тремя проектами: страница (DOM), worker (WebWorker — вместе с DOM типы конфликтуют) и тесты (Node). Ядро собирает `npm run wasm` (`scripts/build-wasm.mjs`: cargo release для wasm32 → `wasm-bindgen --target web` в `src/wasm`, каталог сборки — `CARGO_TARGET_DIR`, если задан). E2E идут против `vite preview` — того же бандла, что получит пользователь. Проект Playwright `chromium` — headless shell как есть (WebGPU нет, WebGL2 — SwiftShader), `gpu` — с флагами настоящего GPU (ANGLE Metal) и WebGPU: тесты `@gpu` и `@perf`.

**Files:**
- Modify: `.gitignore`
- Create: `apps/web/package.json`
- Create: `apps/web/.npmrc`
- Create: `apps/web/package-lock.json (npm install)`
- Create: `apps/web/tsconfig.json`
- Create: `apps/web/tsconfig.worker.json`
- Create: `apps/web/tsconfig.e2e.json`
- Create: `apps/web/vite.config.ts`
- Create: `apps/web/scripts/build-wasm.mjs`
- Create: `apps/web/index.html`
- Create: `apps/web/src/core/input.ts`
- Create: `apps/web/src/core/messages.ts`
- Create: `apps/web/src/core/transport.ts`
- Create: `apps/web/src/core/core.ts`
- Create: `apps/web/src/worker/core.worker.ts`
- Create: `apps/web/src/hud.ts`
- Create: `apps/web/src/main.ts`
- Create: `apps/web/src/window.d.ts`
- Create: `apps/web/playwright.config.ts`
- Test: `apps/web/e2e/helpers.ts`
- Test: `apps/web/e2e/core.spec.ts`

**Interfaces:**
- Consumes: `WasmCore` и типы `Command`, `CoreEvent`, `Backend`, `CursorKind` из `src/wasm/ferrum_wasm.d.ts` (сборка Task 7); `layout.gen.ts` (Task 4).
- Produces (TypeScript): `Core.start(canvas, { backend?, transport?, platform?, atlasSide? }) → Promise<Core>`; `core.mode`, `core.backend`, `core.fallbacks`, `core.live: LiveState | null`, `core.on(listener) → отписка`, `core.send(cmd)`, `core.mainThreadStats()`, `core.resetMainThreadStats()`, `core.droppedInput()`, `core.dispose()`; `createTransport(worker, force?)`, `Transport`; `cssCursor`, `detectPlatform`. Стенд: параметры `nodes`, `backend`, `transport=message`, `atlas`; `window.__ferrum { core, events, ready }`.

- [ ] **Step 1: Пакет и зависимости**

`.npmrc` фиксирует дату пакетов, поэтому `npm install` даёт тот же `package-lock.json`: 46 пакетов, среди них `vite 8.3.4`, `rolldown 1.2.13`, `typescript 6.0.3`, `@playwright/test 1.64.0`, `playwright-core 1.64.0`, `@types/node 24.19.1`.

`.gitignore` (целиком):

````gitignore
target/
# Стенд apps/web: зависимости, сборка, ядро из wasm-bindgen, отчёты Playwright.
/apps/web/node_modules/
/apps/web/dist/
/apps/web/src/wasm/
/apps/web/test-results/
/apps/web/playwright-report/
````

`apps/web/package.json`:

````json
{
  "name": "ferrum-web",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "wasm": "node scripts/build-wasm.mjs",
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "typecheck": "tsc -p tsconfig.json && tsc -p tsconfig.worker.json && tsc -p tsconfig.e2e.json",
    "e2e": "playwright test"
  },
  "devDependencies": {
    "@playwright/test": "1.64.0",
    "@types/node": "24.19.1",
    "typescript": "6.0.3",
    "vite": "8.3.4"
  }
}
````

`apps/web/.npmrc`:

````ini
save-exact=true
# Пакеты не новее этой даты (≥ 14 дней до начала этапа 4) — и для зависимостей зависимостей.
before=2026-10-10
````

`apps/web/tsconfig.json`:

````json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2024", "DOM", "DOM.Iterable"],
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "types": []
  },
  "include": ["src/**/*.ts"],
  "exclude": ["src/worker/**"]
}
````

`apps/web/tsconfig.worker.json`:

````json
{
  "extends": "./tsconfig.json",
  "compilerOptions": {
    "lib": ["ES2024", "WebWorker"]
  },
  "include": ["src/worker/**/*.ts", "src/core/layout.gen.ts", "src/core/messages.ts"],
  "exclude": []
}
````

`apps/web/tsconfig.e2e.json`:

````json
{
  "extends": "./tsconfig.json",
  "compilerOptions": {
    "lib": ["ES2024", "DOM"],
    "types": ["node"]
  },
  "include": ["e2e/**/*.ts", "src/window.d.ts", "playwright.config.ts", "vite.config.ts", "scripts/**/*.mjs"],
  "exclude": []
}
````

`apps/web/vite.config.ts`:

````ts
import { defineConfig } from 'vite';

// Изоляция (crossOriginIsolated) нужна для SharedArrayBuffer. FERRUM_NO_ISOLATION=1 —
// сервер без заголовков: проверка запасного транспорта на postMessage.
const isolated = process.env.FERRUM_NO_ISOLATION !== '1';
const headers: Record<string, string> = isolated
  ? {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'credentialless',
    }
  : {};

export default defineConfig({
  server: { headers },
  preview: { headers },
  worker: { format: 'es' },
  build: { target: 'es2022' },
});
````

`apps/web/scripts/build-wasm.mjs`:

````js
// Сборка ядра для браузера: cargo (wasm32, release) → wasm-bindgen → src/wasm.
// Каталог сборки — CARGO_TARGET_DIR, если задан (у каждой копии репозитория свой).
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const WASM_BINDGEN = '0.2.129';
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const target = process.env.CARGO_TARGET_DIR ? path.resolve(process.env.CARGO_TARGET_DIR) : path.join(root, 'target');
const out = path.resolve(here, '../src/wasm');

const version = execFileSync('wasm-bindgen', ['--version'], { encoding: 'utf8' }).trim();
if (version !== `wasm-bindgen ${WASM_BINDGEN}`) {
  console.error(`Нужен wasm-bindgen ${WASM_BINDGEN}, найден «${version}».`);
  console.error(`Установите: cargo install wasm-bindgen-cli --version ${WASM_BINDGEN} --locked`);
  process.exit(1);
}
execFileSync(
  'cargo',
  ['build', '--release', '--target', 'wasm32-unknown-unknown', '-p', 'ferrum-wasm'],
  { cwd: root, stdio: 'inherit' },
);
execFileSync(
  'wasm-bindgen',
  ['--target', 'web', '--out-dir', out, path.join(target, 'wasm32-unknown-unknown/release/ferrum_wasm.wasm')],
  { stdio: 'inherit' },
);
console.log(`ядро собрано: ${path.relative(process.cwd(), out)}`);
````

Run: `cd apps/web && npm install`
Expected: `added 46 packages`, создан `package-lock.json`.

Run: `cd apps/web && npx playwright install chromium`
Expected: скачан Chrome Headless Shell 156 (кэш Playwright в домашнем каталоге).

Run: `cd apps/web && npm run wasm`
Expected: `ядро собрано: src/wasm` (`ferrum_wasm.js`, `ferrum_wasm.d.ts`, `ferrum_wasm_bg.wasm` около 7,5 МБ).

- [ ] **Step 2: Написать падающие E2E-тесты**

`apps/web/playwright.config.ts`:

````ts
import { defineConfig } from '@playwright/test';

// Стенд собирается заранее (`npm run wasm && npm run build`); тесты идут против
// `vite preview` — того же бандла, что уйдёт пользователю. Два сервера: с
// заголовками изоляции (SharedArrayBuffer) и без них (запасной транспорт).
// Проекты: `chromium` — headless как есть: WebGPU нет, WebGL2 — программный
// SwiftShader (автовыбор падает на WebGL2); `gpu` — тот же Chromium с настоящим GPU
// (ANGLE Metal / Vulkan) и WebGPU: тесты с меткой @gpu и замеры @perf.
const GPU_ARGS = ['--enable-unsafe-webgpu', '--enable-gpu', '--use-angle=metal', '--ignore-gpu-blocklist'];

export default defineConfig({
  testDir: 'e2e',
  timeout: 120_000,
  // Замеры производительности не должны делить машину с другими тестами.
  workers: 1,
  reporter: [['list']],
  use: { baseURL: 'http://localhost:4173', viewport: { width: 1280, height: 720 } },
  webServer: [
    { command: 'npx vite preview --port 4173 --strictPort', port: 4173, reuseExistingServer: false },
    {
      command: 'npx vite preview --port 4174 --strictPort',
      port: 4174,
      env: { FERRUM_NO_ISOLATION: '1' },
      reuseExistingServer: false,
    },
  ],
  projects: [
    { name: 'chromium', use: { browserName: 'chromium' }, grepInvert: /@gpu/ },
    {
      name: 'gpu',
      use: { browserName: 'chromium', launchOptions: { args: GPU_ARGS } },
      grep: /@gpu|@perf/,
    },
  ],
});
````

`apps/web/e2e/helpers.ts`:

````ts
// Общие шаги E2E: открыть стенд, прочитать живое состояние, перевести мировые
// координаты в экранные, прочитать пиксель снимка.

import { expect, type Page } from '@playwright/test';
import type { LiveState } from '../src/core/core';

export async function open(page: Page, query = '', origin = ''): Promise<void> {
  await page.goto(`${origin}/?nodes=1000${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 90_000 });
  await page.waitForFunction(() => (window.__ferrum!.core.live?.frames ?? 0) > 0);
}

export function live(page: Page): Promise<LiveState> {
  return page.evaluate(() => window.__ferrum!.core.live!);
}

/** Ждёт, пока живое состояние не удовлетворит условию (возвращает его). */
export async function liveWhen(page: Page, check: string): Promise<LiveState> {
  await page.waitForFunction(`(() => { const l = window.__ferrum.core.live; return !!l && (${check}); })()`);
  return live(page);
}

/** Точка мира на экране (CSS-пиксели страницы; холст — на всё окно). */
export function screen(l: LiveState, x: number, y: number): { x: number; y: number } {
  return { x: (x - l.cameraX) * l.zoom + l.viewportW / 2, y: (y - l.cameraY) * l.zoom + l.viewportH / 2 };
}

export async function events(page: Page, type: string): Promise<number> {
  return page.evaluate((t) => window.__ferrum!.events.filter((e) => e.type === t).length, type);
}

/** Пиксель снимка страницы в CSS-точке (снимок декодирует сама страница). */
export async function pixel(page: Page, x: number, y: number): Promise<number[]> {
  const png = (await page.screenshot()).toString('base64');
  return page.evaluate(
    async ({ png, x, y }) => {
      const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
      const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
      const k = bmp.width / innerWidth;
      const c = new OffscreenCanvas(bmp.width, bmp.height);
      const ctx = c.getContext('2d')!;
      ctx.drawImage(bmp, 0, 0);
      return Array.from(ctx.getImageData(Math.round(x * k), Math.round(y * k), 1, 1).data);
    },
    { png, x, y },
  );
}

function near(actual: number[], rgb: number[], tol: number): boolean {
  return [0, 1, 2].every((i) => Math.abs(actual[i] - rgb[i]) <= tol);
}

/** Пиксель на экране становится цвета `rgb`: кадр из worker'а доходит до экрана
 * не в тот же момент, когда обновилось живое состояние, поэтому — с повтором. */
export async function expectPixel(page: Page, x: number, y: number, rgb: number[], tol = 6): Promise<void> {
  let last: number[] = [];
  await expect
    .poll(
      async () => {
        last = await pixel(page, x, y);
        return near(last, rgb, tol);
      },
      { message: `пиксель (${x}, ${y}) не стал ${rgb}`, timeout: 10_000 },
    )
    .toBe(true);
  expect(last.length).toBe(4);
}

/** Перетаскивание мышью с промежуточными шагами (pointermove на каждом). */
export async function drag(page: Page, from: { x: number; y: number }, dx: number, dy: number, steps = 10): Promise<void> {
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + dx, from.y + dy, { steps });
  await page.mouse.up();
}
````

`apps/web/e2e/core.spec.ts`:

````ts
// Стенд этапа 4 в headless Chromium: worker стартует, транспорт выбирается по
// изоляции страницы, холст показывает документ, ввод доходит до редактора,
// отмена работает, бэкенд выбирается автоматически, ресайз и dpr применяются.

import { expect, test, type Page } from '@playwright/test';
import { drag, events, expectPixel, live, liveWhen, open, screen } from './helpers';

const CANVAS_DARK = [36, 35, 34];

async function dragMovesAndUndoReturns(page: Page): Promise<void> {
  const l = await live(page);
  // «Слой 1» первого фрейма: (140, 0) – (260, 80). Сдвиг на (50, 30) CSS-пикселей
  // при зуме 0.5 — (100, 60) в мире.
  await drag(page, screen(l, 200, 40), 50, 30);
  const moved = await liveWhen(page, 'l.steps === 1 && l.busy === 0');
  expect([moved.selectionCount, moved.selectionX, moved.selectionY]).toEqual([1, 240, 60]);
  expect([moved.selectionW, moved.selectionH, moved.canUndo]).toEqual([120, 80, 1]);
  expect(await events(page, 'document')).toBeGreaterThanOrEqual(2);
  await page.keyboard.press('ControlOrMeta+z');
  const back = await liveWhen(page, 'l.canRedo === 1');
  expect([back.selectionX, back.selectionY, back.canUndo]).toEqual([140, 0, 0]);
}

test('стенд запускает worker и транспорт на общей памяти', async ({ page }) => {
  await open(page);
  const info = await page.evaluate(() => ({
    isolated: crossOriginIsolated,
    mode: window.__ferrum!.core.mode,
    backend: window.__ferrum!.core.backend,
    errors: window.__ferrum!.events.filter((e) => e.type === 'error'),
  }));
  expect(info).toMatchObject({ isolated: true, mode: 'sab', errors: [] });
  expect(['webgpu', 'webgl', 'cpu']).toContain(info.backend);
  const l = await live(page);
  expect([l.nodes, l.viewportW, l.viewportH, l.zoom]).toEqual([1000, 1280, 720, 0.5]);
});

test('холст показывает документ', async ({ page }) => {
  await open(page);
  const l = await live(page);
  const rect = screen(l, 200, 40);
  await expectPixel(page, rect.x, rect.y, [1, 120, 200]);
  await expectPixel(page, 600, l.viewportH - 10, CANVAS_DARK);
});

test('перетаскивание двигает узел, отмена возвращает', async ({ page }) => {
  await open(page);
  await dragMovesAndUndoReturns(page);
  // На новом месте после повтора — тот же цвет узла.
  await page.keyboard.press('ControlOrMeta+Shift+z');
  const l = await liveWhen(page, 'l.canUndo === 1 && l.selectionX === 240');
  const p = screen(l, 300, 100);
  await expectPixel(page, p.x, p.y, [1, 120, 200]);
});

test('без заголовков изоляции работает запасной транспорт postMessage', async ({ page }) => {
  await open(page, '', 'http://localhost:4174');
  const info = await page.evaluate(() => ({ isolated: crossOriginIsolated, mode: window.__ferrum!.core.mode }));
  expect(info).toEqual({ isolated: false, mode: 'message' });
  await dragMovesAndUndoReturns(page);
});

test('postMessage можно выбрать и на изолированной странице', async ({ page }) => {
  await open(page, '&transport=message');
  expect(await page.evaluate(() => window.__ferrum!.core.mode)).toBe('message');
  await dragMovesAndUndoReturns(page);
});

test('без WebGPU выбирается запасной бэкенд', async ({ page }) => {
  await open(page);
  const { backend, fallbacks } = await page.evaluate(() => ({
    backend: window.__ferrum!.core.backend,
    fallbacks: window.__ferrum!.core.fallbacks,
  }));
  // В headless Chromium без флагов адаптера WebGPU нет, а WebGL2 — программный
  // (SwiftShader): автовыбор доходит до vello_cpu.
  expect(backend).toBe('cpu');
  expect(fallbacks).toHaveLength(2);
  expect(fallbacks[0]).toMatch(/^webgpu: /);
  expect(fallbacks[1]).toMatch(/^webgl: программный рендер \(.*SwiftShader/);
  expect((await live(page)).backend).toBe(3);
});

test('явно запрошенный WebGL2 берётся и программным', async ({ page }) => {
  await open(page, '&backend=webgl');
  expect(await page.evaluate(() => window.__ferrum!.core.backend)).toBe('webgl');
  await dragMovesAndUndoReturns(page);
});

test('WebGPU выбирается, если доступен @gpu', async ({ page }) => {
  await open(page);
  expect(await page.evaluate(() => window.__ferrum!.core.backend)).toBe('webgpu');
  await dragMovesAndUndoReturns(page);
});

test('ресайз окна меняет размер холста', async ({ page }) => {
  await open(page);
  await page.setViewportSize({ width: 900, height: 500 });
  const l = await liveWhen(page, 'l.viewportW === 900 && l.viewportH === 500');
  expect([l.deviceW, l.deviceH, l.dpr]).toEqual([900, 500, 1]);
  // Камера сохранила центр: «Слой 25» первого фрейма ((700, 200) – (820, 280),
  // заливка (25, 120, 200)) — там, где его ждёт пересчёт по новому размеру.
  const p = screen(l, 760, 240);
  await expectPixel(page, p.x, p.y, [25, 120, 200]);
});

test.describe('плотность экрана 2', () => {
  test.use({ deviceScaleFactor: 2 });

  test('холст в пикселях устройства', async ({ page }) => {
    await open(page);
    const l = await live(page);
    expect([l.dpr, l.deviceW, l.deviceH]).toEqual([2, 2560, 1440]);
    await page.evaluate(() => window.__ferrum!.core.send({ type: 'readPixels', reference: false }));
    await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'pixels'));
    const size = await page.evaluate(() => {
      const e = window.__ferrum!.events.find((e) => e.type === 'pixels')!;
      return e.type === 'pixels' ? [e.width, e.height, e.rgba.length] : [];
    });
    expect(size).toEqual([2560, 1440, 2560 * 1440 * 4]);
  });
});

test('копирование и вставка идут через события буфера обмена', async ({ page }) => {
  await open(page);
  const l = await live(page);
  const p = screen(l, 200, 40);
  await page.mouse.click(p.x, p.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+c');
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'clipboard' && e.bytes.length > 8));
  const magic = await page.evaluate(() => {
    const e = window.__ferrum!.events.find((e) => e.type === 'clipboard');
    return e && e.type === 'clipboard' ? new TextDecoder().decode(e.bytes.subarray(0, 8)) : '';
  });
  expect(magic).toBe('FERRUMCB');
  await page.keyboard.press('ControlOrMeta+v');
  await liveWhen(page, 'l.nodes === 1001 && l.steps === 1');
  // «Скопировать как PNG» — PNG выделения из worker'а.
  await page.keyboard.press('ControlOrMeta+Shift+c');
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'clipboardPng'));
  const png = await page.evaluate(() => {
    const e = window.__ferrum!.events.find((e) => e.type === 'clipboardPng');
    return e && e.type === 'clipboardPng' ? Array.from(e.bytes.subarray(1, 4)) : [];
  });
  expect(String.fromCharCode(...png)).toBe('PNG');
});

test('неверная команда не роняет worker', async ({ page }) => {
  await open(page);
  // Команды из основного потока не доверенные: форма проверяется в worker'е.
  await page.evaluate(() => window.__ferrum!.core.send(JSON.parse('{"type":"formatDisk"}')));
  await page.waitForFunction(() =>
    window.__ferrum!.events.some((e) => e.type === 'error' && e.message.startsWith('Неверная команда')),
  );
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'view.zoom_in' }));
  const l = await liveWhen(page, 'l.zoom > 0.5');
  expect(l.nodes).toBe(1000);
});
````

Run: `cd apps/web && npm run typecheck`
Expected: FAIL — стенда ещё нет: `TS2307: Cannot find module '../src/core/core'` в `e2e/helpers.ts` и `TS2339: Property '__ferrum' does not exist` в тестах.

- [ ] **Step 3: Транспорт, ядро и worker**

`apps/web/src/core/input.ts`:

````ts
// Записи кольцевого буфера ввода — те же байты, что разбирает
// crates/ferrum-wasm/src/input.rs. Кодирование — O(1) на событие: основной поток
// только перекладывает поля события в буфер, вся обработка — в worker'е.

import { Field, Flag, Kind, RECORD_SIZE } from './layout.gen';

/** Куда положить запись: представление буфера и смещение начала записи. */
export type Sink = (fill: (view: DataView, at: number) => void) => void;

const encoder = new TextEncoder();
const scratch = new Uint8Array(Field.TEXT_MAX * 4);

function flags(e: { shiftKey: boolean; altKey: boolean; ctrlKey: boolean; metaKey: boolean }): number {
  return (
    (e.shiftKey ? Flag.SHIFT : 0) |
    (e.altKey ? Flag.ALT : 0) |
    (e.ctrlKey ? Flag.CTRL : 0) |
    (e.metaKey ? Flag.META : 0)
  );
}

function clear(view: DataView, at: number): void {
  for (let i = 0; i < RECORD_SIZE; i += 8) view.setFloat64(at + i, 0, true);
}

/** UTF-8 строки не длиннее TEXT_MAX байт, обрезанной по границе символа. */
function putText(view: DataView, at: number, lenAt: number, textAt: number, text: string): void {
  const { written } = encoder.encodeInto(text, scratch);
  let len = Math.min(written, Field.TEXT_MAX);
  // Продолжение символа UTF-8 — 0b10xxxxxx: отступаем к его началу.
  while (len < written && len > 0 && (scratch[len] & 0xc0) === 0x80) len--;
  view.setUint8(at + lenAt, len);
  for (let i = 0; i < len; i++) view.setUint8(at + textAt + i, scratch[i]);
}

export type PointerKind = typeof Kind.POINTER_DOWN | typeof Kind.POINTER_MOVE | typeof Kind.POINTER_UP;

export function pointer(sink: Sink, kind: PointerKind, e: PointerEvent, x: number, y: number): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, kind);
    // У движения `button` = −1: кнопку редактор берёт из нажатия. Нажатия и
    // отпускания других кнопок (3, 4) сюда не доходят — их отсекает `Core`.
    v.setUint8(at + Field.BUTTON, e.button >= 0 && e.button <= 2 ? e.button : 0);
    v.setUint8(at + Field.FLAGS, flags(e));
    v.setFloat64(at + Field.A, x, true);
    v.setFloat64(at + Field.B, y, true);
    v.setFloat64(at + Field.C, e.timeStamp, true);
  });
}

export function simple(sink: Sink, kind: typeof Kind.POINTER_CANCEL | typeof Kind.BLUR): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, kind);
  });
}

/** Прокрутка в CSS-пикселях: строки — по 16 px, страницы — высота холста. */
export function wheel(sink: Sink, e: WheelEvent, x: number, y: number, pageHeight: number): void {
  const k = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? pageHeight : 1;
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, Kind.WHEEL);
    v.setUint8(at + Field.FLAGS, flags(e));
    v.setFloat64(at + Field.A, x, true);
    v.setFloat64(at + Field.B, y, true);
    v.setFloat64(at + Field.C, e.deltaX * k, true);
    v.setFloat64(at + Field.D, e.deltaY * k, true);
  });
}

export function pinch(sink: Sink, x: number, y: number, scale: number): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, Kind.PINCH);
    v.setFloat64(at + Field.A, x, true);
    v.setFloat64(at + Field.B, y, true);
    v.setFloat64(at + Field.C, scale, true);
  });
}

export function key(sink: Sink, kind: typeof Kind.KEY_DOWN | typeof Kind.KEY_UP, e: KeyboardEvent): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, kind);
    // AltGr — только настоящее состояние клавиши, не вывод из Ctrl+Alt
    // (договор с редактором, см. `KeyEvent::alt_graph`).
    const altGraph = e.getModifierState('AltGraph') ? Flag.ALT_GRAPH : 0;
    v.setUint8(at + Field.FLAGS, flags(e) | altGraph | (e.repeat ? Flag.REPEAT : 0));
    v.setFloat64(at + Field.A, e.timeStamp, true);
    putText(v, at, Field.CODE_LEN, Field.CODE, e.code);
    putText(v, at, Field.KEY_LEN, Field.KEY, e.key);
  });
}

export function resize(sink: Sink, width: number, height: number, dpr: number): void {
  sink((v, at) => {
    clear(v, at);
    v.setUint8(at + Field.KIND, Kind.RESIZE);
    v.setFloat64(at + Field.A, width, true);
    v.setFloat64(at + Field.B, height, true);
    v.setFloat64(at + Field.C, dpr, true);
  });
}
````

`apps/web/src/core/messages.ts`:

````ts
// Сообщения между основным потоком и worker'ом поверх протокола ядра: команды и
// события ядра (`Command`, `CoreEvent`) описаны в Rust и сгенерированы tsify,
// здесь — только конверт.

import type { Command, CoreEvent } from '../wasm/ferrum_wasm';

export type ToWorker =
  | {
      t: 'init';
      canvas: OffscreenCanvas;
      /** Общая память; `null` — страница не изолирована (режим postMessage). */
      sab: SharedArrayBuffer | null;
      platform: string;
      width: number;
      height: number;
      dpr: number;
      /** Принудительный бэкенд (`webgpu`, `webgl`, `cpu`) или автоматический выбор. */
      backend: string | null;
      /** Тесты: атлас изображений GPU из одной страницы этой стороны. */
      atlas: number | null;
    }
  | { t: 'cmd'; cmd: Command }
  /** Кадр в режиме postMessage: записи ввода с прошлого кадра. В режиме SAB — без
   * ввода, только если в worker'е нет `Atomics.waitAsync`. */
  | { t: 'tick'; input: ArrayBuffer | null };

export type FromWorker =
  | { t: 'started'; waitAsync: boolean }
  | { t: 'events'; events: CoreEvent[] }
  /** Живое состояние в режиме postMessage (в режиме SAB оно в общей памяти). */
  | { t: 'live'; live: Float64Array }
  | { t: 'fatal'; message: string };
````

`apps/web/src/core/transport.ts`:

````ts
// Транспорт между основным потоком и worker'ом. Две реализации, выбор
// автоматический: SharedArrayBuffer + postMessage, если страница изолирована
// (`crossOriginIsolated`), иначе только postMessage. Команды и события всегда идут
// сообщениями; ввод и живое состояние — через общую память или в сообщениях кадра.

import { Ctrl, CTRL_BYTES, LIVE_OFFSET, LIVE_SLOTS, RECORD_SIZE, RING_CAPACITY, RING_OFFSET, SAB_BYTES } from './layout.gen';
import type { Sink } from './input';
import type { FromWorker, ToWorker } from './messages';

export type TransportMode = 'sab' | 'message';

export interface Transport {
  readonly mode: TransportMode;
  /** Кладёт одну запись ввода. */
  readonly sink: Sink;
  /** Кадр основного потока (`requestAnimationFrame`): разбудить worker. */
  tick(): void;
  /** Последнее живое состояние или `null`, если его ещё не было. */
  live(): Float64Array | null;
  /** Сколько событий ввода пришлось выбросить (очередь переполнилась). */
  dropped(): number;
  post(msg: ToWorker, transfer?: Transferable[]): void;
  /** Сообщения worker'а, кроме служебных сообщений самого транспорта. */
  onMessage(cb: (msg: FromWorker) => void): void;
  /** Общая память для worker'а (только режим SAB). */
  readonly sab: SharedArrayBuffer | null;
}

/** Сколько записей ждать в очереди основного потока, пока кольцо полно. */
const PENDING_MAX = 4 * RING_CAPACITY;

class SabTransport implements Transport {
  readonly mode = 'sab' as const;
  readonly sab = new SharedArrayBuffer(SAB_BYTES);
  private readonly ctrl = new Int32Array(this.sab, 0, CTRL_BYTES / 4);
  private readonly liveView = new Float64Array(this.sab, LIVE_OFFSET, LIVE_SLOTS);
  private readonly ring = new DataView(this.sab, RING_OFFSET, RING_CAPACITY * RECORD_SIZE);
  private readonly pending = new DataView(new ArrayBuffer(PENDING_MAX * RECORD_SIZE));
  private pendingCount = 0;
  private lastLive: Float64Array | null = null;
  private lastSeq = 0;
  /** Worker без `Atomics.waitAsync` (Firefox) будится сообщением. */
  needsTickMessages = false;
  /** Версия живого состояния при последнем тике-сообщении: пока worker не
   * записал новое состояние, прошлый тик не обработан и новый не шлём. */
  private tickSeq = -1;
  private listener: ((msg: FromWorker) => void) | null = null;

  constructor(private readonly worker: Worker) {
    worker.onmessage = (e: MessageEvent<FromWorker>) => {
      if (e.data.t === 'started') this.needsTickMessages = !e.data.waitAsync;
      this.listener?.(e.data);
    };
  }

  readonly sink: Sink = (fill) => {
    if (this.pendingCount > 0 || !this.tryWrite(fill)) {
      // Кольцо полно (worker занят, например открывает документ): запись ждёт в
      // очереди и уходит следующим кадром; порядок событий сохраняется.
      if (this.pendingCount === PENDING_MAX) {
        Atomics.add(this.ctrl, Ctrl.INPUT_DROPPED, 1);
        return;
      }
      fill(this.pending, this.pendingCount * RECORD_SIZE);
      this.pendingCount++;
    }
  };

  private tryWrite(fill: (view: DataView, at: number) => void): boolean {
    const w = Atomics.load(this.ctrl, Ctrl.INPUT_WRITE);
    const r = Atomics.load(this.ctrl, Ctrl.INPUT_READ);
    if (((w - r) | 0) >= RING_CAPACITY) return false;
    fill(this.ring, ((w >>> 0) % RING_CAPACITY) * RECORD_SIZE);
    // Запись видна worker'у не раньше нового значения счётчика.
    Atomics.store(this.ctrl, Ctrl.INPUT_WRITE, (w + 1) | 0);
    return true;
  }

  private flushPending(): void {
    let sent = 0;
    while (sent < this.pendingCount) {
      const from = sent * RECORD_SIZE;
      const ok = this.tryWrite((view, at) => {
        for (let i = 0; i < RECORD_SIZE; i++) view.setUint8(at + i, this.pending.getUint8(from + i));
      });
      if (!ok) break;
      sent++;
    }
    if (sent > 0) {
      const left = this.pendingCount - sent;
      const bytes = new Uint8Array(this.pending.buffer);
      bytes.copyWithin(0, sent * RECORD_SIZE, this.pendingCount * RECORD_SIZE);
      this.pendingCount = left;
    }
  }

  tick(): void {
    if (this.pendingCount > 0) this.flushPending();
    Atomics.add(this.ctrl, Ctrl.FRAME, 1);
    Atomics.notify(this.ctrl, Ctrl.FRAME);
    if (this.needsTickMessages) {
      const seq = Atomics.load(this.ctrl, Ctrl.LIVE_SEQ);
      if (seq !== this.tickSeq) {
        this.tickSeq = seq;
        this.worker.postMessage({ t: 'tick', input: null } satisfies ToWorker);
      }
    }
  }

  live(): Float64Array | null {
    // Seqlock: нечётная версия — worker пишет; разные версии до и после — запись
    // прошла посередине чтения. Повторяем несколько раз, иначе — прошлое значение.
    for (let i = 0; i < 4; i++) {
      const s1 = Atomics.load(this.ctrl, Ctrl.LIVE_SEQ);
      if (s1 === 0) return null;
      if (s1 & 1) continue;
      if (s1 === this.lastSeq && this.lastLive) return this.lastLive;
      const copy = this.liveView.slice();
      if (Atomics.load(this.ctrl, Ctrl.LIVE_SEQ) === s1) {
        this.lastSeq = s1;
        this.lastLive = copy;
        return copy;
      }
    }
    return this.lastLive;
  }

  dropped(): number {
    return Atomics.load(this.ctrl, Ctrl.INPUT_DROPPED);
  }

  post(msg: ToWorker, transfer: Transferable[] = []): void {
    this.worker.postMessage(msg, transfer);
  }

  onMessage(cb: (msg: FromWorker) => void): void {
    this.listener = cb;
  }
}

class MessageTransport implements Transport {
  readonly mode = 'message' as const;
  readonly sab = null;
  private buf = new ArrayBuffer(256 * RECORD_SIZE);
  private view = new DataView(this.buf);
  private count = 0;
  private lastLive: Float64Array | null = null;
  /** Кадр отправлен, а живое состояние в ответ ещё не пришло: следующий тик ждёт —
   * иначе медленный worker копил бы очередь кадров. */
  private inFlight = false;
  private dropCount = 0;
  private listener: ((msg: FromWorker) => void) | null = null;

  constructor(private readonly worker: Worker) {
    worker.onmessage = (e: MessageEvent<FromWorker>) => {
      if (e.data.t === 'live') {
        this.lastLive = e.data.live;
        this.inFlight = false;
        return;
      }
      this.listener?.(e.data);
    };
  }

  readonly sink: Sink = (fill) => {
    if (this.count * RECORD_SIZE === this.buf.byteLength) {
      if (this.count >= PENDING_MAX) {
        this.dropCount++;
        return;
      }
      const bigger = new ArrayBuffer(this.buf.byteLength * 2);
      new Uint8Array(bigger).set(new Uint8Array(this.buf));
      this.buf = bigger;
      this.view = new DataView(bigger);
    }
    fill(this.view, this.count * RECORD_SIZE);
    this.count++;
  };

  tick(): void {
    if (this.inFlight) return;
    const input = this.buf.slice(0, this.count * RECORD_SIZE);
    this.count = 0;
    this.inFlight = true;
    this.worker.postMessage({ t: 'tick', input } satisfies ToWorker, [input]);
  }

  live(): Float64Array | null {
    return this.lastLive;
  }

  dropped(): number {
    return this.dropCount;
  }

  post(msg: ToWorker, transfer: Transferable[] = []): void {
    this.worker.postMessage(msg, transfer);
  }

  onMessage(cb: (msg: FromWorker) => void): void {
    this.listener = cb;
  }
}

/** Транспорт для worker'а: SAB, если страница изолирована и это не запрещено. */
export function createTransport(worker: Worker, force?: TransportMode): Transport {
  const sab = force !== 'message' && globalThis.crossOriginIsolated === true && typeof SharedArrayBuffer === 'function';
  return sab ? new SabTransport(worker) : new MessageTransport(worker);
}
````

`apps/web/src/core/core.ts`:

````ts
// Ядро для основного потока: worker с WasmCore, транспорт, пересылка ввода с
// холста и живое состояние. Основной поток ничего не считает о документе: он
// кладёт события ввода в буфер, раз в кадр будит worker и читает готовые значения.
// Интерфейс (этап 5) работает с ядром только через этот класс.

import type { Backend, Command, CoreEvent, CursorKind } from '../wasm/ferrum_wasm';
import * as enc from './input';
import { BACKENDS, Kind, LIVE_NAMES } from './layout.gen';
import type { FromWorker } from './messages';
import { createTransport, type Transport, type TransportMode } from './transport';

export type LiveState = Record<(typeof LIVE_NAMES)[number], number>;

export type Platform = 'mac' | 'windows' | 'linux';

export interface CoreOptions {
  /** Принудительный бэкенд рендера; по умолчанию WebGPU → WebGL2 → CPU. */
  backend?: Backend;
  /** `message` — postMessage даже на изолированной странице (тесты). */
  transport?: TransportMode;
  platform?: Platform;
  /** Тесты: атлас изображений GPU из одной страницы этой стороны (вытеснение). */
  atlasSide?: number;
}

/** Время обработчиков основного потока по кадрам — для бюджета ≤ 4 мс. */
export interface MainThreadStats {
  frames: number;
  /** Самый дорогой кадр и 95-й процентиль, мс. */
  maxMs: number;
  p95Ms: number;
}

export function detectPlatform(): Platform {
  const p = navigator.platform.toLowerCase();
  if (p.startsWith('mac')) return 'mac';
  if (p.startsWith('win')) return 'windows';
  return 'linux';
}

/** Сочетания, которые оставляем браузеру (перезагрузка, инструменты разработчика). */
function browserKey(e: KeyboardEvent): boolean {
  if (/^F\d+$/.test(e.key)) return true;
  const mod = e.metaKey || e.ctrlKey;
  return mod && (e.code === 'KeyR' || e.code === 'KeyW' || e.code === 'KeyT' || (e.shiftKey && e.code === 'KeyI'));
}

/** CSS-курсор по курсору редактора (угол ресайза — к ближайшей стрелке). */
export function cssCursor(kind: CursorKind, angle: number): string {
  switch (kind) {
    case 'default':
      return 'default';
    case 'crosshair':
      return 'crosshair';
    case 'grab':
      return 'grab';
    case 'grabbing':
      return 'grabbing';
    case 'duplicate':
      return 'copy';
    case 'rotate':
      return 'alias';
    case 'resize': {
      const a = ((angle % 180) + 180) % 180;
      if (a < 22.5 || a >= 157.5) return 'ew-resize';
      if (a < 67.5) return 'nesw-resize';
      if (a < 112.5) return 'ns-resize';
      return 'nwse-resize';
    }
  }
}

export class Core {
  readonly mode: TransportMode;
  backend: Backend | null = null;
  fallbacks: string[] = [];
  live: LiveState | null = null;
  private readonly listeners = new Set<(e: CoreEvent) => void>();
  private rect = { left: 0, top: 0, width: 0, height: 0 };
  private raf = 0;
  private frameCost = 0;
  private readonly costs: number[] = [];
  private maxCost = 0;
  private disposers: (() => void)[] = [];
  private failed: ((e: Error) => void) | null = null;

  private constructor(
    private readonly worker: Worker,
    private readonly transport: Transport,
    private readonly canvas: HTMLCanvasElement,
  ) {
    this.mode = transport.mode;
  }

  /** Запускает worker на холсте и ждёт события `ready`. */
  static async start(canvas: HTMLCanvasElement, opts: CoreOptions = {}): Promise<Core> {
    const worker = new Worker(new URL('../worker/core.worker.ts', import.meta.url), { type: 'module' });
    const transport = createTransport(worker, opts.transport);
    const core = new Core(worker, transport, canvas);
    const ready = new Promise<void>((resolve, reject) => {
      core.failed = reject;
      core.on((e) => {
        if (e.type === 'ready') {
          core.backend = e.backend;
          core.fallbacks = e.fallbacks;
          resolve();
        }
      });
    });
    transport.onMessage((m) => core.onWorker(m));
    const r = canvas.getBoundingClientRect();
    core.rect = { left: r.left, top: r.top, width: r.width, height: r.height };
    const offscreen = canvas.transferControlToOffscreen();
    transport.post(
      {
        t: 'init',
        canvas: offscreen,
        sab: transport.sab,
        platform: opts.platform ?? detectPlatform(),
        width: Math.max(1, r.width),
        height: Math.max(1, r.height),
        dpr: devicePixelRatio,
        backend: opts.backend ?? null,
        atlas: opts.atlasSide ?? null,
      },
      [offscreen],
    );
    await ready;
    core.listen();
    core.loop();
    return core;
  }

  private onWorker(m: FromWorker): void {
    if (m.t === 'events') {
      for (const e of m.events) {
        if (e.type === 'cursor') this.canvas.style.cursor = cssCursor(e.cursor, e.angle);
        for (const l of this.listeners) l(e);
      }
    } else if (m.t === 'fatal') {
      const err = new Error(m.message);
      this.failed?.(err);
      for (const l of this.listeners) l({ type: 'error', message: `Ядро остановлено: ${m.message}` });
    }
  }

  /** Подписка на события ядра; возвращает отписку. */
  on(listener: (e: CoreEvent) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Команда ядру. Байты (`Uint8Array`) передаются без копии. */
  send(cmd: Command): void {
    const transfer: Transferable[] = [];
    if ('bytes' in cmd) transfer.push(cmd.bytes.buffer as ArrayBuffer);
    this.transport.post({ t: 'cmd', cmd }, transfer);
  }

  /** Бюджет основного потока: время обработчиков ввода и кадра. */
  mainThreadStats(): MainThreadStats {
    const sorted = [...this.costs].sort((a, b) => a - b);
    const p95 = sorted.length ? sorted[Math.min(sorted.length - 1, Math.round((sorted.length - 1) * 0.95))] : 0;
    return { frames: this.costs.length, maxMs: this.maxCost, p95Ms: p95 };
  }

  resetMainThreadStats(): void {
    this.costs.length = 0;
    this.maxCost = 0;
  }

  /** Отброшено событий ввода (очередь основного потока переполнилась). */
  droppedInput(): number {
    return this.transport.dropped();
  }

  /** Обработчик с учётом времени в бюджет кадра. */
  private timed<A extends unknown[]>(fn: (...args: A) => void): (...args: A) => void {
    return (...args: A) => {
      const t = performance.now();
      fn(...args);
      this.frameCost += performance.now() - t;
    };
  }

  private add<K extends keyof HTMLElementEventMap>(
    target: HTMLElement | Window,
    type: K,
    fn: (e: HTMLElementEventMap[K]) => void,
    options?: AddEventListenerOptions,
  ): void {
    const h = this.timed(fn) as EventListener;
    target.addEventListener(type, h, options);
    this.disposers.push(() => target.removeEventListener(type, h, options));
  }

  private listen(): void {
    const sink = this.transport.sink;
    const c = this.canvas;
    const xy = (e: { clientX: number; clientY: number }) => [e.clientX - this.rect.left, e.clientY - this.rect.top] as const;
    this.add(c, 'pointerdown', (e) => {
      c.setPointerCapture(e.pointerId);
      c.focus();
      // Кнопки «назад» и «вперёд» (3, 4) холсту не нужны.
      if (e.button > 2) return;
      const [x, y] = xy(e);
      enc.pointer(sink, Kind.POINTER_DOWN, e, x, y);
    });
    this.add(c, 'pointermove', (e) => {
      // Все промежуточные точки движения, а не одна на кадр.
      const all = e.getCoalescedEvents?.() ?? [];
      for (const p of all.length ? all : [e]) {
        const [x, y] = xy(p);
        enc.pointer(sink, Kind.POINTER_MOVE, p, x, y);
      }
    });
    this.add(c, 'pointerup', (e) => {
      if (e.button > 2) return;
      const [x, y] = xy(e);
      enc.pointer(sink, Kind.POINTER_UP, e, x, y);
    });
    this.add(c, 'pointercancel', () => enc.simple(sink, Kind.POINTER_CANCEL));
    this.add(
      c,
      'wheel',
      (e) => {
        e.preventDefault();
        const [x, y] = xy(e);
        enc.wheel(sink, e, x, y, this.rect.height);
      },
      { passive: false },
    );
    this.add(c, 'contextmenu', (e) => e.preventDefault());
    // Щипок тачпада в Safari приходит жестом, а не колесом с Ctrl.
    let gestureScale = 1;
    const gesture = (type: string, scale: number, e: Event) => {
      e.preventDefault();
      const ge = e as Event & { clientX: number; clientY: number };
      const [x, y] = xy(ge);
      if (type === 'gesturestart') gestureScale = 1;
      enc.pinch(sink, x, y, scale / gestureScale);
      gestureScale = scale;
    };
    for (const type of ['gesturestart', 'gesturechange'] as const) {
      const h = this.timed((e: Event) => gesture(type, (e as Event & { scale: number }).scale, e));
      c.addEventListener(type, h);
      this.disposers.push(() => c.removeEventListener(type, h));
    }
    this.add(window, 'keydown', (e) => {
      if (browserKey(e)) return;
      e.preventDefault();
      enc.key(sink, Kind.KEY_DOWN, e);
    });
    this.add(window, 'keyup', (e) => {
      if (!browserKey(e)) enc.key(sink, Kind.KEY_UP, e);
    });
    this.add(window, 'blur', () => enc.simple(sink, Kind.BLUR));
    // Размер холста и dpr: ResizeObserver и смена плотности экрана.
    const resize = () => {
      const r = c.getBoundingClientRect();
      this.rect = { left: r.left, top: r.top, width: r.width, height: r.height };
      enc.resize(sink, Math.max(1, r.width), Math.max(1, r.height), devicePixelRatio);
    };
    const ro = new ResizeObserver(this.timed(resize));
    ro.observe(c);
    this.disposers.push(() => ro.disconnect());
    let mq: MediaQueryList | null = null;
    const watchDpr = () => {
      mq?.removeEventListener('change', onDpr);
      mq = matchMedia(`(resolution: ${devicePixelRatio}dppx)`);
      mq.addEventListener('change', onDpr);
    };
    const onDpr = this.timed(() => {
      resize();
      watchDpr();
    });
    watchDpr();
    this.disposers.push(() => mq?.removeEventListener('change', onDpr));
  }

  private loop(): void {
    const frame = () => {
      const t = performance.now();
      this.transport.tick();
      const live = this.transport.live();
      if (live) {
        const obj = {} as LiveState;
        LIVE_NAMES.forEach((n, i) => (obj[n] = live[i]));
        this.live = obj;
      }
      this.frameCost += performance.now() - t;
      // Окно последних 10 000 кадров (около трёх минут при 60 Гц).
      if (this.costs.length === 10_000) this.costs.splice(0, 1000);
      this.costs.push(this.frameCost);
      this.maxCost = Math.max(this.maxCost, this.frameCost);
      this.frameCost = 0;
      this.raf = requestAnimationFrame(frame);
    };
    this.raf = requestAnimationFrame(frame);
  }

  /** Имя бэкенда по коду живого состояния. */
  static backendName(code: number): string {
    return BACKENDS[code] ?? 'none';
  }

  dispose(): void {
    cancelAnimationFrame(this.raf);
    for (const d of this.disposers.splice(0)) d();
    this.worker.terminate();
  }
}
````

`apps/web/src/worker/core.worker.ts`:

````ts
// Worker ядра: владеет редактором, документом, сценой и рендером (WasmCore).
// Кадр: забрать ввод (из кольца SAB или из сообщения кадра) → `input` → `frame`
// (рисует, только если нужно) → живое состояние → события.

import init, { WasmCore } from '../wasm/ferrum_wasm.js';
import type { CoreEvent } from '../wasm/ferrum_wasm';
import { Ctrl, CTRL_BYTES, LIVE_OFFSET, LIVE_SLOTS, RECORD_SIZE, RING_CAPACITY, RING_OFFSET } from '../core/layout.gen';
import type { FromWorker, ToWorker } from '../core/messages';

const scope = self as unknown as DedicatedWorkerGlobalScope;

let core: WasmCore | null = null;
let sab: { ctrl: Int32Array; live: Float64Array; ring: Uint8Array } | null = null;
/** Записи, вычитанные из кольца за кадр (не больше ёмкости кольца). */
const drained = new Uint8Array(RING_CAPACITY * RECORD_SIZE);
const early: ToWorker[] = [];

function post(msg: FromWorker, transfer: Transferable[] = []): void {
  scope.postMessage(msg, transfer);
}

function fatal(e: unknown): void {
  post({ t: 'fatal', message: e instanceof Error ? e.message : String(e) });
  core = null;
}

/** Байты событий уходят без копии: их буферы передаются основному потоку. */
function sendEvents(): void {
  if (!core) return;
  const events: CoreEvent[] = core.takeEvents();
  if (events.length === 0) return;
  const transfer: Transferable[] = [];
  for (const e of events) {
    if ('bytes' in e) transfer.push(e.bytes.buffer);
    if (e.type === 'pixels') transfer.push(e.rgba.buffer);
  }
  post({ t: 'events', events }, transfer);
}

/** Вычитывает записи кольца SAB: всё до счётчика записи основного потока. Счётчик
 * из общей памяти не доверенный: больше ёмкости кольца не читается — тогда чтение
 * догоняет запись, а лишнее пропадает (записи всё равно перезаписаны). */
function drainRing(): Uint8Array {
  const s = sab!;
  const w = Atomics.load(s.ctrl, Ctrl.INPUT_WRITE);
  let r = Atomics.load(s.ctrl, Ctrl.INPUT_READ);
  if ((w - r) >>> 0 > RING_CAPACITY) r = (w - RING_CAPACITY) | 0;
  let n = 0;
  while (r !== w) {
    const at = ((r >>> 0) % RING_CAPACITY) * RECORD_SIZE;
    drained.set(s.ring.subarray(at, at + RECORD_SIZE), n * RECORD_SIZE);
    n++;
    r = (r + 1) | 0;
  }
  Atomics.store(s.ctrl, Ctrl.INPUT_READ, r);
  return drained.subarray(0, n * RECORD_SIZE);
}

function writeLive(): void {
  if (!core) return;
  const live = core.live();
  if (sab) {
    Atomics.add(sab.ctrl, Ctrl.LIVE_SEQ, 1);
    sab.live.set(live);
    Atomics.add(sab.ctrl, Ctrl.LIVE_SEQ, 1);
  } else {
    post({ t: 'live', live }, [live.buffer]);
  }
}

function step(input: Uint8Array | null): void {
  if (!core) return;
  try {
    if (input && input.byteLength > 0) core.input(input);
    core.frame();
    writeLive();
    sendEvents();
  } catch (e) {
    fatal(e);
  }
}

/** Отдаёт управление циклу событий worker'а (команды — сообщения). */
const yieldTask = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

async function sabLoop(): Promise<void> {
  const s = sab!;
  let seen = Atomics.load(s.ctrl, Ctrl.FRAME);
  while (core) {
    const wait = Atomics.waitAsync(s.ctrl, Ctrl.FRAME, seen);
    // Кадр основного потока уже ушёл вперёд (прошлый кадр был долгим): ожидание
    // вернулось сразу. Без явной уступки цикл не дал бы дойти сообщениям — Save,
    // Open, Paste ждали бы, пока кадры не станут быстрыми.
    if (wait.async) await wait.value;
    else await yieldTask();
    seen = Atomics.load(s.ctrl, Ctrl.FRAME);
    try {
      step(drainRing());
    } catch (e) {
      fatal(e);
    }
  }
}

function handle(msg: ToWorker): void {
  if (!core) return;
  switch (msg.t) {
    case 'cmd':
      try {
        core.command(msg.cmd);
      } catch (e) {
        fatal(e);
        return;
      }
      // Ответы на команды (сохранение, ошибки) — сразу, кадр нарисуется по тику.
      sendEvents();
      break;
    case 'tick':
      try {
        step(msg.input ? new Uint8Array(msg.input) : sab ? drainRing() : null);
      } catch (e) {
        fatal(e);
      }
      break;
    case 'init':
      break;
  }
}

scope.onmessage = async (e: MessageEvent<ToWorker>) => {
  const msg = e.data;
  if (msg.t !== 'init') {
    if (core) handle(msg);
    else early.push(msg);
    return;
  }
  try {
    await init();
    core = await WasmCore.create(msg.canvas, msg.platform, msg.width, msg.height, msg.dpr, msg.backend, msg.atlas);
  } catch (err) {
    fatal(err);
    return;
  }
  const waitAsync = typeof Atomics.waitAsync === 'function';
  if (msg.sab) {
    sab = {
      ctrl: new Int32Array(msg.sab, 0, CTRL_BYTES / 4),
      live: new Float64Array(msg.sab, LIVE_OFFSET, LIVE_SLOTS),
      ring: new Uint8Array(msg.sab, RING_OFFSET, RING_CAPACITY * RECORD_SIZE),
    };
  }
  post({ t: 'started', waitAsync });
  writeLive();
  sendEvents();
  for (const m of early.splice(0)) handle(m);
  if (sab && waitAsync) void sabLoop();
};
````

- [ ] **Step 4: Страница стенда**

`apps/web/index.html`:

````html
<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Ferrum — холст</title>
    <style>
      html,
      body {
        margin: 0;
        height: 100%;
        overflow: hidden;
        background: #242322;
      }
      #canvas {
        display: block;
        width: 100vw;
        height: 100vh;
        touch-action: none;
        outline: none;
      }
      #hud {
        position: fixed;
        top: 8px;
        left: 8px;
        padding: 6px 8px;
        border-radius: 6px;
        background: rgb(26 25 24 / 0.85);
        color: #e8e4df;
        font: 12px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace;
        white-space: pre;
        pointer-events: none;
      }
    </style>
  </head>
  <body>
    <canvas id="canvas" tabindex="0"></canvas>
    <div id="hud">запуск…</div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
````

`apps/web/src/hud.ts`:

````ts
// Отладочная плашка: бэкенд, транспорт, кадры worker'а и бюджет основного потока.
// Обновляется раз в 250 мс — сама не должна быть заметна в бюджете кадра.

import type { Core } from './core/core';

export function startHud(el: HTMLElement, core: Core): () => void {
  const id = setInterval(() => {
    const l = core.live;
    const main = core.mainThreadStats();
    el.textContent = [
      `бэкенд   ${core.backend ?? '—'}`,
      `транспорт ${core.mode}`,
      `кадры    ${l ? l.fps.toFixed(0) : 0} fps`,
      `кадр     ${l ? l.frameMs.toFixed(1) : 0} мс (p50 ${l ? l.frameP50.toFixed(1) : 0}, p95 ${l ? l.frameP95.toFixed(1) : 0})`,
      `основной ${main.p95Ms.toFixed(2)} мс p95, ${main.maxMs.toFixed(2)} мс макс.`,
      `узлов    ${l ? l.nodes : 0}, зум ${l ? (l.zoom * 100).toFixed(0) : 100}%`,
    ].join('\n');
  }, 250);
  return () => clearInterval(id);
}
````

`apps/web/src/main.ts`:

````ts
// Стенд этапа 4: холст на всё окно под управлением worker'а и отладочная плашка.
// Параметры адреса: `nodes` — размер страницы-примера (по умолчанию 1000),
// `backend` — webgpu | webgl | cpu, `transport=message` — без общей памяти,
// `atlas` — сторона единственной страницы атласа GPU (тесты вытеснения).
// `window.__ferrum` — для E2E-тестов.
//
// Буфер обмена здесь — внутренний (последние байты события `clipboard`):
// системный буфер, диалоги файлов и панели — этап 5.

import type { Backend, CoreEvent } from './wasm/ferrum_wasm';
import { Core } from './core/core';
import type { TransportMode } from './core/transport';
import { startHud } from './hud';

const params = new URLSearchParams(location.search);
const canvas = document.getElementById('canvas') as HTMLCanvasElement;
const hud = document.getElementById('hud') as HTMLElement;

async function main(): Promise<void> {
  const core = await Core.start(canvas, {
    backend: (params.get('backend') as Backend | null) ?? undefined,
    transport: (params.get('transport') as TransportMode | null) ?? undefined,
    atlasSide: params.has('atlas') ? Number(params.get('atlas')) : undefined,
  });
  // Журнал событий для тестов (после `ready` — его забирает `Core.start`).
  const events: CoreEvent[] = [];
  window.__ferrum = { core, events, ready: false };
  let clipboard: Uint8Array | null = null;
  core.on((e) => {
    events.push(e);
    if (e.type === 'error') console.warn('ferrum:', e.message);
    if (e.type === 'opened') window.__ferrum!.ready = true;
    if (e.type === 'clipboard') clipboard = e.bytes;
    // Копия: байты уходят в worker с передачей буфера, свои остаются.
    if (e.type === 'pasteRequested' && clipboard) core.send({ type: 'paste', bytes: clipboard.slice() });
  });
  startHud(hud, core);
  core.send({ type: 'demo', nodes: Number(params.get('nodes') ?? 1000) });
  canvas.focus();
}

main().catch((e: unknown) => {
  hud.textContent = `Ядро не запустилось: ${e instanceof Error ? e.message : String(e)}`;
});
````

`apps/web/src/window.d.ts`:

````ts
// Стенд кладёт ядро и журнал событий в `window.__ferrum` — для E2E-тестов.

import type { Core } from './core/core';
import type { CoreEvent } from './wasm/ferrum_wasm';

declare global {
  interface Window {
    __ferrum?: {
      core: Core;
      events: CoreEvent[];
      /** Документ открыт (событие `opened`). */
      ready: boolean;
    };
  }
}
````

- [ ] **Step 5: Проверить типы, собрать и запустить тесты**

Run: `cd apps/web && npm run typecheck`
Expected: без ошибок.

Run: `cd apps/web && npm run build`
Expected: `✓ built`: `index-*.js` около 11 КБ, `core.worker-*.js` около 47 КБ, `ferrum_wasm_bg-*.wasm` около 7,5 МБ (2,6 МБ gzip).

Run: `cd apps/web && npx playwright test e2e/core.spec.ts`
Expected: PASS, `12 passed`: 11 в проекте `chromium`, `WebGPU выбирается, если доступен @gpu` — в `gpu`.

- [ ] **Step 6: Закоммитить**

```bash
git add .gitignore apps/web
git commit -m "feat(web): стенд на Vite — Core, транспорт SAB и postMessage, worker ядра"
```

---

### Task 9: E2E: пиксели GPU против `vello_cpu`

Каждый бэкенд должен рисовать то же, что эталонный `vello_cpu`. Тест ставит на холст картинку из четырёх цветных четвертей (видно и отражение строк, и сдвиг), выделяет узел, читает пиксели кадра (`readPixels { reference: true }`) и сравнивает с тем же кадром, нарисованным на CPU в том же worker'е: средняя разница по худшему каналу < 0,1, пикселей с разницей больше 16 — меньше 0,05 %. У GPU-бэкендов дополнительно проверяется, что картинка пришла из атласа (`atlasImages ≥ 1`). В прототипе разница — 0,0055 у WebGL2 (SwiftShader и Metal) и WebGPU, ни одного пикселя больше 16.

Вытеснение проверяется атласом из одной страницы 64 × 64 (`?atlas=64`): две картинки 64 × 48 вместе не помещаются; в кадре с обеими вторая — заглушка (`atlasMisses ≥ 1`), после «показать выделение» видна только вторая — первая вытесняется, вторая встаёт на её место. Без немедленной отправки очистки у WebGPU (Task 7) этот тест падает: средняя разница 96, картинка прозрачная.

**Files:**
- Test: `apps/web/e2e/pixels.spec.ts`

**Interfaces:**
- Consumes: стенд и `readPixels` (Tasks 7–8).
- Produces: тесты.

- [ ] **Step 1: Написать тесты**

`apps/web/e2e/pixels.spec.ts`:

````ts
// Каждый бэкенд рисует то же, что эталонный vello_cpu: пиксели читаются из GPU
// (`readPixels`) и сравниваются с тем же кадром, нарисованным на CPU в том же
// worker'е. В кадре — узлы, подписи, выделение и картинка из атласа GPU.
// Отдельно — вытеснение из атласа: на место старой картинки встаёт новая.

import { expect, test, type Page } from '@playwright/test';
import { drag, live, liveWhen, open, screen } from './helpers';

const QUADRANTS = ['#ff0000', '#00ff00', '#0000ff', '#ffff00'];

/** PNG 64 × 48 из четырёх цветных четвертей — видно и отражение, и сдвиг —
 * ставится кликом в точку мира (`x`, `y`); `steps` — шагов истории после этого. */
async function placeQuadrants(page: Page, colors = QUADRANTS, x = 720, y = 1100, steps = 1): Promise<void> {
  await page.evaluate(async (colors) => {
    window.__ferrum!.events.length = 0;
    const c = new OffscreenCanvas(64, 48);
    const ctx = c.getContext('2d')!;
    colors.forEach((col, i) => {
      ctx.fillStyle = col;
      ctx.fillRect((i % 2) * 32, Math.floor(i / 2) * 24, 32, 24);
    });
    const blob = await c.convertToBlob({ type: 'image/png' });
    const bytes = new Uint8Array(await blob.arrayBuffer());
    window.__ferrum!.core.send({ type: 'placeImage', bytes });
  }, colors);
  await page.waitForFunction(() => window.__ferrum!.events.some((e) => e.type === 'tool' && e.tool === 'image'));
  const l = await live(page);
  // Пустое место под первым фреймом: картинка встаёт в исходном размере.
  const at = screen(l, x, y);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, `l.steps === ${steps} && l.selectionW === 64 && l.selectionH === 48`);
}

async function checkAgainstCpu(page: Page, backend: 'auto' | 'cpu' | 'webgl' | 'webgpu'): Promise<void> {
  await placeQuadrants(page);
  // Перетаскивание «Слоя 1»: в кадре — его рамка выделения с ручками.
  const l = await live(page);
  await drag(page, screen(l, 200, 40), 4, 0, 2);
  const diff = await compareWithCpu(page);
  console.log(JSON.stringify(diff));
  const active = await page.evaluate(() => window.__ferrum!.core.backend);
  expect(diff.backend).toBe(backend === 'auto' ? active : backend);
  expect([diff.width, diff.height]).toEqual([1280, 720]);
  expect(diff.mean).toBeLessThan(0.1);
  expect(diff.over16).toBeLessThan(0.0005);
  // У GPU картинка пришла из атласа.
  if (diff.backend !== 'cpu') expect((await live(page)).atlasImages).toBeGreaterThanOrEqual(1);
}

interface Diff {
  backend: string;
  width: number;
  height: number;
  mean: number;
  over16: number;
}

async function compareWithCpu(page: Page): Promise<Diff> {
  await page.evaluate(() => {
    window.__ferrum!.events.length = 0;
    window.__ferrum!.core.send({ type: 'readPixels', reference: true });
  });
  await page.waitForFunction(() => window.__ferrum!.events.filter((e) => e.type === 'pixels').length === 2, null, {
    timeout: 30_000,
  });
  return page.evaluate(() => {
    const px = window.__ferrum!.events.filter((e) => e.type === 'pixels');
    const shown = px.find((e) => e.type === 'pixels' && e.backend !== 'cpu') ?? px[0];
    const cpu = px.find((e) => e !== shown)!;
    if (shown.type !== 'pixels' || cpu.type !== 'pixels') throw new Error('нет пикселей');
    let sum = 0;
    let over = 0;
    for (let i = 0; i < shown.rgba.length; i += 4) {
      let d = 0;
      for (let k = 0; k < 3; k++) d = Math.max(d, Math.abs(shown.rgba[i + k] - cpu.rgba[i + k]));
      sum += d;
      if (d > 16) over++;
    }
    const n = shown.rgba.length / 4;
    return { backend: shown.backend, width: shown.width, height: shown.height, mean: sum / n, over16: over / n };
  });
}

// Проект `chromium`: автовыбор (vello_cpu), CPU и программный WebGL2.
for (const backend of ['auto', 'cpu', 'webgl'] as const) {
  test(`бэкенд ${backend} рисует то же, что vello_cpu`, async ({ page }) => {
    await open(page, backend === 'auto' ? '' : `&backend=${backend}`);
    await checkAgainstCpu(page, backend);
  });
}

// Проект `gpu`: WebGPU и WebGL2 на настоящем GPU.
for (const backend of ['webgpu', 'webgl'] as const) {
  test(`${backend} на GPU рисует то же, что vello_cpu @gpu`, async ({ page }) => {
    await open(page, `&backend=${backend}`);
    await checkAgainstCpu(page, backend);
  });
}

// Атлас из одной страницы 64 × 64: две картинки 64 × 48 вместе не помещаются.
// Кадр с обеими — вторая заглушкой; после «показать выделение» видна только
// вторая: первая вытесняется, вторая загружается на её место. Очистка места не
// должна стереть только что загруженную картинку (у WebGPU запись картинки идёт
// раньше команд кадра).
for (const backend of ['webgpu', 'webgl'] as const) {
  test(`вытеснение из атласа ${backend}: новая картинка на месте старой @gpu`, async ({ page }) => {
    await open(page, `&backend=${backend}&atlas=64`);
    await placeQuadrants(page);
    await placeQuadrants(page, ['#ff00ff', '#00ffff', '#ffffff', '#000000'], 1900, 1100, 2);
    await liveWhen(page, 'l.atlasMisses >= 1');
    await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'view.fit_selection' }));
    await liveWhen(page, 'l.zoom > 2');
    const diff = await compareWithCpu(page);
    console.log(JSON.stringify(diff));
    expect(diff.backend).toBe(backend);
    expect(diff.mean).toBeLessThan(0.1);
    expect(diff.over16).toBeLessThan(0.0005);
    expect((await live(page)).atlasImages).toBe(1);
  });
}
````

- [ ] **Step 2: Запустить**

Run: `cd apps/web && npm run build && npx playwright test e2e/pixels.spec.ts`
Expected: PASS, `7 passed`: `auto`, `cpu`, `webgl` в `chromium`; `webgpu`, `webgl` на GPU и два вытеснения — в `gpu`. В выводе — строки `{"backend":…,"mean":…,"over16":0}`.

- [ ] **Step 3: Закоммитить**

```bash
git add apps/web/e2e/pixels.spec.ts
git commit -m "test(web): пиксели каждого бэкенда сверяются с vello_cpu, вытеснение из атласа"
```

---

### Task 10: Бюджет основного потока и финальная проверка

Страница из 100 000 узлов, 5 секунд взаимодействия: 1,5 с панорамирования колесом, 1,5 с зума Ctrl+колесом (до всей страницы, `zoom = 0.02`: растры фреймов строятся и грузятся в атлас), 2 с перетаскивания узла. Затем документ сохраняется и открывается заново — всё в worker'е, основной поток только передаёт байты. Длинные задачи собирает `PerformanceObserver('longtask')`, время основного потока — `Core.mainThreadStats()`, время кадров worker'а — из живого состояния на каждом новом кадре.

Ворота: длинных задач (> 50 мс) — 0, худший кадр основного потока ≤ 4 мс, выброшенных событий — 0, нехватки места в атласе — 0, кадров worker'а > 30; p95 кадра worker'а ≤ 16,7 мс для `vello_cpu` и для WebGL2 на настоящем GPU. Время кадра worker'а — подготовка на CPU и отправка в GPU; исполнение на GPU асинхронное и в него не входит, поэтому WebGPU — справочно. Программный WebGL2 (SwiftShader) в `chromium` автовыбором не берётся. Результаты пишутся в `test-results/<проект>/perf-<проект>-<бэкенд>.json`.

Замер прототипа (Apple M1 Pro, Chromium 156 headless, 1280 × 720, dpr 1; три прогона):

| Бэкенд | p50 кадра worker'а | p95 | худший | основной поток, худший кадр | длинных задач |
|---|---|---|---|---|---|
| WebGPU (Metal) | 3,6–3,8 мс | 6,9–7,3 мс | 8,8–9,6 мс | ≤ 1,4 мс | 0 |
| WebGL2 (ANGLE Metal) | 1,9–5,3 мс | 6,9–7,4 мс | 9,0–10,1 мс | ≤ 0,4 мс | 0 |
| vello_cpu | 3,8–4,9 мс | 7,9 мс | 9,6–11,4 мс | ≤ 1,3 мс | 0 |

Открытие стенда с генерацией 100 000 узлов — 2,2–2,6 с; файл 28,3 МБ; сохранение в worker'е — 1,15–1,36 с; открытие до первого кадра — 0,95–0,98 с.

**Files:**
- Test: `apps/web/e2e/perf.spec.ts`

**Interfaces:**
- Consumes: стенд (Task 8).
- Produces: тест `@perf` и JSON с замерами.

- [ ] **Step 1: Написать тест**

`apps/web/e2e/perf.spec.ts`:

````ts
// Основной поток тонкий: на странице из 100 000 узлов 5 секунд панорамирования,
// зума (до всей страницы — растры фреймов в атласе GPU) и перетаскивания не дают
// длинных задач (> 50 мс) в основном потоке, а обработчики ввода и кадра
// укладываются в 4 мс на кадр. Время кадров worker'а (медиана и 95-й процентиль)
// печатается по бэкендам и пишется в test-results/<проект>/perf-<проект>-<бэкенд>.json.
//
// Время кадра worker'а — подготовка кадра на CPU и отправка в GPU; исполнение на
// GPU в него не входит (оно асинхронное). Поэтому цель 60 fps (p95 ≤ 16,7 мс)
// проверяется для vello_cpu и для WebGL2 на настоящем GPU, а WebGPU — справочно.

import { expect, test, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { live, screen } from './helpers';

const NODES = 100_000;
/** Бюджет основного потока на кадр (спека 4.2). */
const MAIN_BUDGET_MS = 4;
/** 60 fps: 95-й процентиль времени кадра worker'а (спека 3). */
const WORKER_P95_MS = 16.7;

interface Sample {
  backend: string;
  longTasks: number[];
  mainMaxMs: number;
  mainP95Ms: number;
  mainFrames: number;
  workerFrames: number;
  workerP50: number;
  workerP95: number;
  workerMax: number;
  openMs: number;
  dropped: number;
  atlasImages: number;
  atlasMisses: number;
  minZoom: number;
  /** Сохранение и открытие файла страницы в worker'е: размер, время до события
   * `opened` и до первого кадра. */
  fileMb: number;
  saveMs: number;
  fileOpenMs: number;
  fileFrameMs: number;
}

/** Сохранить документ и открыть его снова — всё в worker'е; основной поток только
 * передаёт байты (transferable). */
async function saveAndOpen(page: Page): Promise<{ fileMb: number; saveMs: number; fileOpenMs: number; fileFrameMs: number }> {
  return page.evaluate(async () => {
    const f = window.__ferrum!;
    const until = (ok: () => boolean) =>
      new Promise<void>((resolve) => {
        const check = () => (ok() ? resolve() : requestAnimationFrame(check));
        check();
      });
    f.events.length = 0;
    const t0 = performance.now();
    f.core.send({ type: 'save' });
    await until(() => f.events.some((e) => e.type === 'saved'));
    const saveMs = performance.now() - t0;
    const saved = f.events.find((e) => e.type === 'saved');
    if (!saved || saved.type !== 'saved') throw new Error('нет сохранения');
    const fileMb = saved.bytes.length / 1e6;
    f.events.length = 0;
    const frames = f.core.live!.frames;
    const t1 = performance.now();
    f.core.send({ type: 'open', bytes: saved.bytes });
    await until(() => f.events.some((e) => e.type === 'opened'));
    const fileOpenMs = performance.now() - t1;
    await until(() => f.core.live!.frames > frames);
    return { fileMb, saveMs, fileOpenMs, fileFrameMs: performance.now() - t1 };
  });
}

/** Повторяет шаг ~60 раз в секунду в течение `ms`. */
async function during(page: Page, ms: number, step: (i: number) => Promise<void>): Promise<void> {
  const end = Date.now() + ms;
  for (let i = 0; Date.now() < end; i++) {
    await step(i);
    await page.waitForTimeout(16);
  }
}

/** 5 секунд: панорамирование, зум к курсору, перетаскивание узла. */
async function interact(page: Page): Promise<void> {
  const l = await live(page);
  await page.mouse.move(l.viewportW / 2, l.viewportH / 2);
  await during(page, 1500, (i) => page.mouse.wheel(i % 60 < 30 ? 12 : -12, i % 30 < 15 ? 8 : -8));
  // Ctrl + колесо — зум, как щипок тачпада: отдаление и обратно.
  await page.keyboard.down('Control');
  await during(page, 1500, (i) => page.mouse.wheel(0, i % 40 < 20 ? 20 : -20));
  await page.keyboard.up('Control');
  const from = screen(await live(page), 760, 240);
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await during(page, 2000, (i) => page.mouse.move(from.x + (i % 100) * 2, from.y + Math.sin(i / 10) * 40));
  await page.mouse.up();
}

async function measure(page: Page, query: string): Promise<Sample> {
  await page.addInitScript(() => {
    const w = window as unknown as { __longTasks: { start: number; duration: number }[] };
    w.__longTasks = [];
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__longTasks.push({ start: e.startTime, duration: e.duration });
    }).observe({ type: 'longtask', buffered: true });
  });
  const t0 = Date.now();
  await page.goto(`/?nodes=${NODES}${query}`);
  await page.waitForFunction(() => window.__ferrum?.ready === true, null, { timeout: 120_000 });
  await page.waitForFunction(() => (window.__ferrum!.core.live?.frames ?? 0) > 0, null, { timeout: 60_000 });
  const openMs = Date.now() - t0;
  // Время кадров worker'а — из живого состояния, на каждом новом кадре.
  const start = await page.evaluate(() => {
    const w = window as unknown as { __frameMs: number[] };
    w.__frameMs = [];
    let seen = window.__ferrum!.core.live!.frames;
    const z = window as unknown as { __minZoom: number };
    z.__minZoom = Infinity;
    const tick = () => {
      const l = window.__ferrum!.core.live!;
      z.__minZoom = Math.min(z.__minZoom, l.zoom);
      if (l.frames !== seen) {
        seen = l.frames;
        w.__frameMs.push(l.frameMs);
      }
      requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
    window.__ferrum!.core.resetMainThreadStats();
    return performance.now();
  });
  await interact(page);
  const file = await saveAndOpen(page);
  await page.waitForTimeout(300);
  const sample = await page.evaluate((start) => {
    const w = window as unknown as { __longTasks: { start: number; duration: number }[]; __frameMs: number[] };
    const core = window.__ferrum!.core;
    const main = core.mainThreadStats();
    const frames = [...w.__frameMs].sort((a, b) => a - b);
    const pick = (q: number) => (frames.length ? frames[Math.min(frames.length - 1, Math.round((frames.length - 1) * q))] : 0);
    return {
      backend: core.backend ?? 'none',
      longTasks: w.__longTasks.filter((t) => t.start >= start && t.duration > 50).map((t) => Math.round(t.duration)),
      mainMaxMs: main.maxMs,
      mainP95Ms: main.p95Ms,
      mainFrames: main.frames,
      workerFrames: frames.length,
      workerP50: pick(0.5),
      workerP95: pick(0.95),
      workerMax: frames.length ? frames[frames.length - 1] : 0,
      openMs: 0,
      dropped: core.droppedInput(),
      atlasImages: core.live!.atlasImages,
      atlasMisses: core.live!.atlasMisses,
      minZoom: (window as unknown as { __minZoom: number }).__minZoom,
    };
  }, start);
  return { ...sample, ...file, openMs };
}

// Проект `chromium`: автовыбор (без GPU это vello_cpu). Проект `gpu`: каждый
// бэкенд на настоящем GPU.
for (const backend of ['auto', 'webgpu', 'webgl', 'cpu'] as const) {
  test(`основной поток тонкий на 100 000 узлах (${backend}) @perf`, async ({ page }, info) => {
    test.skip((info.project.name === 'gpu') === (backend === 'auto'), 'другой проект');
    const s = await measure(page, backend === 'auto' ? '' : `&backend=${backend}`);
    if (backend !== 'auto') expect(s.backend).toBe(backend);
    const out = path.join(info.project.outputDir, `perf-${info.project.name}-${s.backend}.json`);
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.writeFileSync(out, JSON.stringify(s, null, 2));
    console.log(`perf ${s.backend}: ${JSON.stringify(s)}`);
    expect(s.workerFrames, 'worker не рисовал кадры').toBeGreaterThan(30);
    expect(s.mainFrames).toBeGreaterThan(200);
    expect(s.longTasks, 'длинные задачи в основном потоке').toEqual([]);
    expect(s.mainMaxMs, 'обработчики основного потока за кадр').toBeLessThanOrEqual(MAIN_BUDGET_MS);
    expect(s.dropped).toBe(0);
    // Зум дошёл до всей страницы: растровый кэш фреймов был в работе.
    expect(s.minZoom).toBeLessThan(0.05);
    expect(s.atlasMisses, 'картинкам не хватило места в атласе').toBe(0);
    if (s.backend === 'cpu' || s.backend === 'webgl') {
      expect(s.workerP95, 'p95 кадра worker\'а').toBeLessThanOrEqual(WORKER_P95_MS);
    }
  });
}
````

- [ ] **Step 2: Запустить замер**

Run: `cd apps/web && npm run build && npx playwright test e2e/perf.spec.ts`
Expected: PASS, `4 passed` (`auto` в `chromium` — это vello_cpu; `webgpu`, `webgl`, `cpu` в `gpu`), 4 пропущено. Машина под нагрузкой (Chrome, OrbStack) даёт единичные выбросы — p95 кадра worker'а или один кадр основного потока около 8 мс (при воспроизведении плана такое случилось один раз из семи прогонов WebGPU): при провале ворот повторить прогон, прежде чем считать это регрессией; ворота не ослаблять.

- [ ] **Step 3: Полная проверка**

Все ворота этапов 1–4.

Run: `cargo fmt --check`
Expected: без вывода, код 0.

Run: `cargo clippy --all-targets -- -D warnings`
Expected: без предупреждений.

Run: `cargo clippy -p ferrum-render --all-targets --features gpu -- -D warnings`
Expected: без предупреждений.

Run: `cargo clippy --target wasm32-unknown-unknown -p ferrum-wasm --lib --tests -- -D warnings`
Expected: без предупреждений.

Run: `cargo test --workspace`
Expected: PASS — все наборы этапов 1–4 (49 наборов, 378 тестов).

Run: `cargo test --target wasm32-unknown-unknown -p ferrum-wasm --test web`
Expected: PASS, `4 passed`.

Run: `cargo bench --workspace --no-run`
Expected: сборка бенчмарков без ошибок.

Run: `cargo run --release -p ferrum-render --example render_report`
Expected: все строки PASS, код 0.

Run: `cargo run --release -p ferrum-editor --example editor_report`
Expected: все строки PASS, код 0.

Run: `cargo run --release -p ferrum-doc --example bench_report`
Expected: все строки PASS, код 0.

Run: `cd apps/web && npm run typecheck && npm run wasm && npm run build && npx playwright test`
Expected: PASS, `23 passed`, 4 пропущено.

- [ ] **Step 4: Закоммитить**

```bash
git add apps/web/e2e/perf.spec.ts
git commit -m "test(web): бюджет основного потока на 100 000 узлах и время кадров по бэкендам"
```

---

## Что этап 4 сознательно не делает

- **Интерфейс.** Панели, дерево слоёв, тулбар, палитра цвета, планировщик интерфейса (спека 4.4, 9) — этап 5 на SolidJS поверх класса `Core`. Стенд `apps/web` — только холст, плашка и `window.__ferrum` для тестов.
- **Системный буфер обмена, диалоги файлов, `EyeDropper`.** Ядро отдаёт события `clipboard`, `clipboardPng`, `pasteRequested`, `requestImage` и принимает `paste` и `placeImage`; стенд держит буфер в памяти. Чтение и запись системного буфера и выбор файла — этап 5.
- **Воркер ввода-вывода и хранение.** `save`/`open` идут через ядро целиком (на 100 000 узлах — около 1,2 и 1 с в worker'е, основной поток свободен); отдельный воркер ввода-вывода, журнал, IndexedDB и File System Access — этап 6.
- **Восстановление после потерь.** Потеря устройства WebGPU или контекста WebGL2 и паника в worker'е сообщаются событием `error` и останавливают рисование; пересоздание устройства, перезапуск worker'а и восстановление из журнала (спека 11) — этап 6.
- **Открытие из кэша сцены.** `Editor::new` строит сцену по загруженной странице, поэтому первый кадр файла на 100 000 узлов в wasm — около 1 с, а не 0,3 с из кэша сцены (спека 5.4). Ускорение открытия — вместе с хранением (этап 6).
- **Декодирование картинок кроме PNG и уменьшенные копии под зум.** Картинки документа декодирует `decode_png` в worker'е по 2 за кадр; JPEG/WebP через `createImageBitmap` и mip-копии под зум — позже. Картинка больше страницы атласа (4096 px) уменьшается до неё целиком.
- **Многопоточный wasm** (`wasm-bindgen-rayon`) — по результатам бенчмарков, позже.
- **Firefox и Safari в E2E.** Тесты идут в Chromium; режим SAB без `Atomics.waitAsync` (Firefox) реализован будильником-сообщением, но в браузере не проверен.
- **Seqlock живого состояния на слабоупорядоченной памяти.** Значения читаются обычными чтениями между двумя `Atomics.load`; в худшем случае плашка покажет поля из соседних кадров. Для панелей с точными значениями — этап 5.
