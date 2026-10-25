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
use crate::present::{Action, PresentWatch, Soft, SoftErrors};
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
    /// Ошибки показа кадра: повторы и постоянные отказы.
    watch: PresentWatch,
    /// Ошибки WebGPU вне вызовов: каждый текст — один раз, слишком много — стоп.
    soft: SoftErrors,
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
            watch: PresentWatch::default(),
            soft: SoftErrors::default(),
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
        for message in soft {
            if self.dead {
                break;
            }
            // Одна и та же ошибка проверки обычно повторяется каждый кадр.
            match self.soft.error(message) {
                Soft::Report(message) => self.core.push_event(CoreEvent::Error { message }),
                Soft::Quiet => {}
                Soft::Dead(message) => {
                    self.dead = true;
                    self.core.push_event(CoreEvent::Error { message });
                }
            }
        }
        // Потерянный контекст WebGL2 остаётся потерянным: `take_errors` отдаёт его
        // каждый кадр, а сообщение нужно одно.
        if let Some(message) = fatal.filter(|_| !self.dead) {
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
        // Кадр не показан и сам повторяться не будет (пауза или смерть поверхности).
        let mut stopped = false;
        let served = match self.surface.present(read_now) {
            Ok(readback) => {
                self.watch.ok();
                match readback {
                    Some(Readback::Ready(p)) => {
                        self.pixels(self.surface.kind(), p);
                        true
                    }
                    Some(Readback::Pending) => {
                        self.awaiting = true;
                        true
                    }
                    None => false,
                }
            }
            Err(e) => {
                // Временный пропуск — рисуем снова, но не бесконечно; постоянная
                // ошибка доходит до пользователя и останавливает перерисовку.
                let message = match self.watch.error(e) {
                    Action::Redraw => {
                        self.core.request_frame();
                        None
                    }
                    Action::Pause(m) => {
                        stopped = true;
                        m
                    }
                    Action::Dead(m) => {
                        self.dead = true;
                        stopped = true;
                        m
                    }
                };
                if let Some(message) = message {
                    self.core.push_event(CoreEvent::Error { message });
                }
                false
            }
        };
        match read {
            // Кадр не показан, и следующего не будет: вернуть запрос в очередь —
            // значит перерисовывать вечно. Ответ — ошибка.
            Some(_) if !served && stopped => self.core.push_event(CoreEvent::Error {
                message: if self.dead {
                    "Пиксели не прочитать: поверхность не рисует".into()
                } else {
                    "Пиксели не прочитать: кадр не показан".into()
                },
            }),
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
