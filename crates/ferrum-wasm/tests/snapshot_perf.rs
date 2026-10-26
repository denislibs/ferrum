//! Стоимость превью файла на странице в 100 000 узлов: превью рисуется вместе со
//! снимком и держит worker ядра, поэтому его время ограничено (LOD: мелочь —
//! прямоугольниками или пропуском). Время меряется только в release.
#![cfg(all(not(target_arch = "wasm32"), not(debug_assertions)))]

use std::time::{Duration, Instant};

use ferrum_editor::Platform;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

fn snapshot(core: &mut Core, thumbnail: bool) -> (Duration, bool) {
    let t = Instant::now();
    core.command(Command::Snapshot { thumbnail });
    let d = t.elapsed();
    let thumb = core.take_events().into_iter().any(|e| {
        matches!(
            e,
            CoreEvent::Snapshot {
                thumbnail: Some(_),
                ..
            }
        )
    });
    (d, thumb)
}

#[test]
fn thumbnail_of_100k_nodes_stays_within_budget() {
    let mut core = Core::new(
        Platform::Windows,
        Size::new(1280.0, 720.0),
        1.0,
        Backend::Cpu,
    );
    core.command(Command::Demo { nodes: 100_000 });
    core.take_events();
    // Лучшее из трёх: общая машина бывает занята.
    let best = |core: &mut Core, thumbnail| {
        (0..3)
            .map(|_| snapshot(core, thumbnail))
            .min_by_key(|(d, _)| *d)
            .unwrap()
    };
    let (plain, no_thumb) = best(&mut core, false);
    let (with, thumb) = best(&mut core, true);
    assert!(!no_thumb && thumb);
    let extra = with.saturating_sub(plain);
    eprintln!("snapshot: {plain:?}, with thumbnail: {with:?}, thumbnail ≈ {extra:?}");
    assert!(extra < Duration::from_millis(200), "превью: {extra:?}");
}
