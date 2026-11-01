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
use crate::recover::{OnLost, Recovery};
pub use clock::PerformanceClock;
use surface::{Pixels, Readback, Surface};

#[wasm_bindgen(start)]
pub fn start() {
    // Паника в Rust — в консоль worker'а с текстом и стеком (а не «unreachable»), и
    // её текст — worker'у до остановки wasm: «Ядро остановлено: …» несёт причину.
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        report_panic(&panic_text(info));
    }));
}

/// Текст паники без места в коде: его увидит пользователь.
fn panic_text(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "без описания".into())
}

/// Отдаёт текст `globalThis.__ferrumPanic`, если worker его ждёт (в тестах wasm в
/// Node его нет — тогда только консоль).
fn report_panic(message: &str) {
    let global = js_sys::global();
    if let Ok(f) = js_sys::Reflect::get(&global, &JsValue::from_str("__ferrumPanic")) {
        if let Some(f) = f.dyn_ref::<js_sys::Function>() {
            let _ = f.call1(&global, &JsValue::from_str(message));
        }
    }
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
    /// Потери устройства GPU: пересоздать поверхность или перейти на запасную.
    recovery: Recovery,
}

/// Поверхность, созданная заново после потери устройства или контекста GPU (этап 6):
/// создаётся асинхронно на том же холсте и отдаётся ядру `replaceSurface`.
#[wasm_bindgen]
pub struct WasmSurface {
    surface: Option<Surface>,
}

#[wasm_bindgen]
impl WasmSurface {
    /// Поверхность бэкенда `backend` заново на холсте `width × height` пикселей
    /// устройства (`Surface::recreate`).
    pub async fn create(
        canvas: OffscreenCanvas,
        backend: String,
        width: u16,
        height: u16,
        atlas_side: Option<u16>,
    ) -> Result<WasmSurface, JsError> {
        let b = Backend::parse(&backend).ok_or_else(|| JsError::new("неизвестный бэкенд"))?;
        let s = Surface::recreate(b, &canvas, width.max(1), height.max(1), atlas_side)
            .await
            .map_err(|e| JsError::new(&e))?;
        Ok(WasmSurface { surface: Some(s) })
    }
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
        core.set_export_clock(PerformanceClock::new().map(|c| Box::new(c) as _));
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
            recovery: Recovery::default(),
        })
    }

    /// Размер кадра в пикселях устройства, `[ширина, высота]` — для новой поверхности.
    #[wasm_bindgen(js_name = deviceSize)]
    pub fn device_size(&self) -> Vec<u16> {
        let (w, h) = self.core.device_size();
        vec![w, h]
    }

    /// Новая поверхность вместо потерянной: кадры идут снова, атлас и кэши GPU — с
    /// нуля (новый `GpuBackend`), счётчики ошибок показа сброшены.
    #[wasm_bindgen(js_name = replaceSurface)]
    pub fn replace_surface(&mut self, surface: &mut WasmSurface) -> Result<(), JsError> {
        let s = surface
            .surface
            .take()
            .ok_or_else(|| JsError::new("поверхность уже отдана ядру"))?;
        self.surface = s;
        self.dead = false;
        self.awaiting = false;
        self.atlas_reported = false;
        self.watch = PresentWatch::default();
        self.soft = SoftErrors::default();
        self.recovery.restored();
        let backend = self.surface.kind();
        self.core.set_backend(backend);
        self.core.push_event(CoreEvent::SurfaceRestored { backend });
        Ok(())
    }

    /// Пересоздать поверхность не удалось: интерфейс перезапустит ядро с запасным
    /// бэкендом (`SurfaceFailed { next }`).
    #[wasm_bindgen(js_name = surfaceFailed)]
    pub fn surface_failed(&mut self, message: String) {
        let next = crate::recover::next_backend(self.surface.kind());
        self.core
            .push_event(CoreEvent::SurfaceFailed { next, message });
    }

    /// Только для тестов (worker зовёт её лишь в тестовом режиме): настоящая паника
    /// ядра — проверка перехвата и перезапуска.
    #[wasm_bindgen(js_name = panicForTests)]
    pub fn panic_for_tests(&mut self) {
        panic!("тестовая паника ядра");
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
        // Время — для склейки ввода в один шаг и мигания курсора.
        let now = self.now();
        self.core.set_time(now);
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
        // Мигание курсора правки текста: кадр нужен, когда курсор гаснет и горит.
        let now = self.now();
        self.core.set_time(now);
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
        // Потеря устройства или контекста: поверхность больше не рисует. Worker создаёт
        // её заново (`replaceSurface`), а не выйдет или потери повторяются — интерфейс
        // перезапускает ядро с запасным бэкендом. Событие одно: потерянный контекст
        // WebGL2 `take_errors` отдаёт каждый кадр.
        if let Some(message) = fatal.filter(|_| !self.dead) {
            self.dead = true;
            let backend = self.surface.kind();
            self.core.push_event(match self.recovery.lost(backend) {
                OnLost::Recreate => CoreEvent::SurfaceLost { backend, message },
                OnLost::Fallback(next) => CoreEvent::SurfaceFailed { next, message },
            });
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
