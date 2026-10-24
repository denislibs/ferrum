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
