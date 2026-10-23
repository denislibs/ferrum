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
    c.set_center(Point::new(w / 2.0, h / 2.0));
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
