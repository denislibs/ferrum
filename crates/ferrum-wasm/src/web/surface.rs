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

use crate::present::{PresentError, MAX_SOFT_ERRORS};
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
    /// Все ошибки с прошлого кадра, по порядку: политика (`SoftErrors`) считает
    /// каждую. Больше предела не копим — после него поверхность и так остановится.
    uncaptured: Vec<String>,
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
    /// Предел стороны текстуры устройства: холст больше не настроить.
    max_side: u32,
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
    /// `MAX_TEXTURE_SIZE`: холст больше не нарисовать.
    max_side: u32,
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
    /// контекста), и ошибки WebGPU, пришедшие вне вызовов с прошлого раза.
    pub fn take_errors(&mut self) -> (Option<String>, Vec<String>) {
        match self {
            Surface::WebGpu(s) => {
                let mut e = s.errors.lock().unwrap_or_else(|p| p.into_inner());
                (e.lost.take(), std::mem::take(&mut e.uncaptured))
            }
            Surface::WebGl(s) => {
                let gl: &WebGl2RenderingContext = s.backend.device().renderer.gl_context();
                let lost = gl
                    .is_context_lost()
                    .then(|| "Контекст WebGL2 потерян".to_string());
                (lost, Vec::new())
            }
            Surface::Cpu(_) => (None, Vec::new()),
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
    pub fn present(&mut self, read: bool) -> Result<Option<Readback>, PresentError> {
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
        if !software_ok {
            let info = adapter.get_info();
            // Тип адаптера браузер сообщает не всегда: SwiftShader узнаём и по имени.
            if info.device_type == wgpu::DeviceType::Cpu
                || is_software_name(&[&info.name, &info.driver, &info.driver_info])
            {
                return Err(format!("только программный адаптер ({})", info.name));
            }
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
        let max_side = device.limits().max_texture_dimension_2d;
        if w as u32 > max_side || h as u32 > max_side {
            return Err(format!(
                "холст {w} × {h} больше предела устройства ({max_side})"
            ));
        }
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
                if e.uncaptured.len() <= MAX_SOFT_ERRORS as usize {
                    e.uncaptured.push(format!("Ошибка WebGPU: {err}"));
                }
            }));
        }
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
        let atlas = atlas_for(max_side, atlas_side);
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
        // Контекст холста — последним, после всего, что может не получиться: занятый
        // `webgpu` холст уже не отдать ни WebGL2, ни 2D. Дальше только `configure`,
        // у которого нет ошибки на выходе (проблемы придут обработчиком устройства).
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .map_err(|e| format!("нет поверхности ({e})"))?;
        surface.configure(&dev.device, &config);
        Ok(WebGpuSurface {
            canvas: canvas.clone(),
            surface,
            config,
            backend: GpuBackend::new(w, h, dev, atlas),
            errors,
            readback: None,
            max_side,
        })
    }

    fn present(&mut self, read: bool) -> Result<Option<Readback>, PresentError> {
        let (w, h) = self.backend.size();
        if w as u32 > self.max_side || h as u32 > self.max_side {
            // Окно выросло: пауза, а не смерть — уменьшат, и кадры пойдут снова.
            return Err(PresentError::TooLarge(format!(
                "{w} × {h} больше предела устройства ({})",
                self.max_side
            )));
        }
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
            other @ (wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Outdated) => {
                // Конфигурация устарела (смена размера, вкладка в фоне): настроить
                // заново, кадр нарисуется следующим.
                self.surface
                    .configure(&self.backend.device().device, &self.config);
                return Err(PresentError::Skipped(format!("{other:?}")));
            }
            // Потеря поверхности или ошибка проверки: повтор не поможет.
            other => return Err(PresentError::Failed(format!("{other:?}"))),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (scene, dev) = self.backend.parts();
        let mut enc = dev.take_encoder();
        let rendered = dev.renderer.render(
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
        );
        if let Err(e) = rendered {
            // Загрузки картинок этого кадра были закодированы в `enc`, который не
            // уйдёт в GPU: зеркало атласа считало бы их загруженными. Забыть всё —
            // следующий кадр загрузит заново (место, занятое в атласе vello, до
            // перезапуска не освободится, но ошибка рендера и так постоянная).
            self.backend.reset_atlas();
            return Err(PresentError::Failed(format!("{e:?}")));
        }
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
    is_software_name(&[&name]).then_some(name)
}

/// Имя (адаптера, драйвера, рендерера) программного GPU.
fn is_software_name(names: &[&str]) -> bool {
    const SOFTWARE: [&str; 5] = [
        "swiftshader",
        "llvmpipe",
        "softpipe",
        "software",
        "basic render",
    ];
    names.iter().any(|n| {
        let lower = n.to_lowercase();
        SOFTWARE.iter().any(|s| lower.contains(s))
    })
}

impl WebGlSurface {
    fn create(
        canvas: &OffscreenCanvas,
        w: u16,
        h: u16,
        software_ok: bool,
        atlas_side: Option<u16>,
    ) -> Result<WebGlSurface, String> {
        // Сначала полный рендерер на пробном холсте 1×1: у `WebGlRenderer` контекст
        // создаётся первым, а компиляция и сборка шейдеров падают позже. Упади они
        // на настоящем холсте, он остался бы занят WebGL2 и CPU не получил бы 2D.
        // Параметры контекста те же, что ставит сам `WebGlRenderer`.
        let probe = OffscreenCanvas::new(1, 1).map_err(js_err)?;
        let probe_el: &HtmlCanvasElement = probe.unchecked_ref();
        let (probe_renderer, _probe_resources) =
            vello_gpu::WebGlRenderer::new_with(probe_el, RenderSettings::default(), true)
                .map_err(|e| format!("WebGL2 недоступен ({e:?})"))?;
        let gl = probe_renderer.gl_context().clone();
        let checked = Self::check(&gl, w, h, software_ok);
        // Контекст пробы отдаём сразу (браузеры держат мало живых контекстов).
        drop(probe_renderer);
        if let Ok(Some(ext)) = gl.get_extension("WEBGL_lose_context") {
            let _ = js_sys::Reflect::get(&ext, &"loseContext".into())
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
                .map(|f| f.call0(&ext));
        }
        let max_side = checked?;
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
            max_side,
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

    /// Годится ли контекст: не программный и не мал для холста. Возвращает
    /// `MAX_TEXTURE_SIZE`.
    fn check(
        gl: &WebGl2RenderingContext,
        w: u16,
        h: u16,
        software_ok: bool,
    ) -> Result<u32, String> {
        if !software_ok {
            if let Some(name) = software_renderer(gl) {
                return Err(format!("программный рендер ({name})"));
            }
        }
        let max_side = gl
            .get_parameter(WebGl2RenderingContext::MAX_TEXTURE_SIZE)
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(2048.0) as u32;
        if w as u32 > max_side || h as u32 > max_side {
            return Err(format!(
                "холст {w} × {h} больше предела устройства ({max_side})"
            ));
        }
        Ok(max_side)
    }

    fn present(&mut self, read: bool) -> Result<Option<Readback>, PresentError> {
        let (w, h) = self.backend.size();
        if w as u32 > self.max_side || h as u32 > self.max_side {
            // Окно выросло: пауза, а не смерть — уменьшат, и кадры пойдут снова.
            return Err(PresentError::TooLarge(format!(
                "{w} × {h} больше предела устройства ({})",
                self.max_side
            )));
        }
        resize_canvas(&self.canvas, w, h);
        let (scene, dev) = self.backend.parts();
        let rendered = dev.renderer.render(
            scene,
            &mut dev.resources,
            &RenderSize {
                width: w,
                height: h,
            },
            &vello_gpu::WebGlTextureBindings::new(),
            ferrum_render::Rgba::new([0.0, 0.0, 0.0, 0.0]),
        );
        if let Err(e) = rendered {
            // Зеркало атласа могло посчитать загруженными картинки этого кадра:
            // забыть всё, следующий кадр загрузит заново.
            self.backend.reset_atlas();
            return Err(PresentError::Failed(format!("{e:?}")));
        }
        let dev = self.backend.device_mut();
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
        .map_err(|e| PresentError::Failed(js_err(e)))?;
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

    fn present(&mut self, read: bool) -> Result<Option<Readback>, PresentError> {
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
        .map_err(|e| PresentError::Failed(js_err(e)))?;
        self.ctx
            .put_image_data(&img, 0.0, 0.0)
            .map_err(|e| PresentError::Failed(js_err(e)))?;
        Ok(read.then(|| {
            Readback::Ready(Pixels {
                width: w,
                height: h,
                rgba: pixmap.data_as_u8_slice().to_vec(),
            })
        }))
    }
}
