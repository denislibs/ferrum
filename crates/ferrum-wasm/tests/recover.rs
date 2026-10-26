//! Потеря GPU (этап 6): пересоздать на месте, пока потери не повторяются, затем —
//! запасной бэкенд по порядку WebGPU → WebGL2 → CPU.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_wasm::protocol::Backend;
use ferrum_wasm::recover::{next_backend, OnLost, Recovery, MAX_RESTORES};

#[test]
fn fallback_order() {
    assert_eq!(next_backend(Backend::Webgpu), Backend::Webgl);
    assert_eq!(next_backend(Backend::Webgl), Backend::Cpu);
    assert_eq!(next_backend(Backend::Cpu), Backend::Cpu);
}

#[test]
fn recreate_until_losses_repeat() {
    let mut r = Recovery::default();
    for _ in 0..MAX_RESTORES {
        assert_eq!(r.lost(Backend::Webgpu), OnLost::Recreate);
        r.restored();
    }
    assert_eq!(r.lost(Backend::Webgpu), OnLost::Fallback(Backend::Webgl));
    assert_eq!(r.lost(Backend::Webgl), OnLost::Fallback(Backend::Cpu));
}

#[test]
fn cpu_is_never_recreated() {
    assert_eq!(
        Recovery::default().lost(Backend::Cpu),
        OnLost::Fallback(Backend::Cpu)
    );
}
