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
