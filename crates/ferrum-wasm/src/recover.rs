//! Потеря устройства GPU (этап 6, спека 11): что делать, когда поверхность WebGPU или
//! WebGL2 потеряна. Отдельно от браузерного кода, чтобы проверяться обычными тестами.
//!
//! Потерю сначала пробуют пережить на месте: worker создаёт поверхность того же
//! бэкенда заново на том же холсте (новое устройство WebGPU, восстановленный
//! контекст WebGL2). Не вышло — или поверхность теряется снова и снова (драйвер
//! падает на каждом кадре) — запасной бэкенд: WebGPU → WebGL2 → CPU. Контекст холста
//! выбирается один раз, поэтому запасной бэкенд — это новое ядро с новым холстом, и
//! перезапуск ведёт интерфейс.

use crate::protocol::Backend;

/// Сколько раз поверхность пересоздаётся на месте за жизнь ядра.
pub const MAX_RESTORES: u32 = 2;

/// Запасной бэкенд после `b`.
pub fn next_backend(b: Backend) -> Backend {
    match b {
        Backend::Webgpu => Backend::Webgl,
        Backend::Webgl | Backend::Cpu => Backend::Cpu,
    }
}

/// Что делать с потерянной поверхностью.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnLost {
    /// Создать поверхность того же бэкенда заново.
    Recreate,
    /// Перезапустить ядро с этим бэкендом.
    Fallback(Backend),
}

#[derive(Debug, Default)]
pub struct Recovery {
    restored: u32,
}

impl Recovery {
    pub fn lost(&self, backend: Backend) -> OnLost {
        if backend == Backend::Cpu || self.restored >= MAX_RESTORES {
            OnLost::Fallback(next_backend(backend))
        } else {
            OnLost::Recreate
        }
    }

    /// Поверхность создана заново.
    pub fn restored(&mut self) {
        self.restored += 1;
    }
}
