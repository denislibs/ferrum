//! Общий стенд сценарных тестов: редактор на пустом документе, камера с зумом 1,
//! при которой экранные координаты совпадают с мировыми.
#![allow(dead_code)]

pub mod text;

use ferrum_doc::{NodeId, NodeKind, Props, PropsPatch};
use ferrum_editor::*;
use ferrum_render::Camera;
use ferrum_scene::{testing::snapshot, Scene};
use kurbo::{Point, Size, Vec2};

pub const VIEW: Size = Size::new(1000.0, 800.0);

pub struct H {
    pub ed: Editor,
    /// Время событий: каждое следующее позже на 600 мс, чтобы клики не
    /// склеивались в двойные (порог 500 мс) без явного намерения.
    pub t: f64,
}

pub fn patch(x: f64, y: f64, w: f64, h: f64) -> PropsPatch {
    PropsPatch {
        x: Some(x),
        y: Some(y),
        w: Some(w),
        h: Some(h),
        ..Default::default()
    }
}

/// Узлы страницы в порядке обхода: id, родитель, свойства — для сравнения
/// состояний документа до и после отмены.
pub type DocState = Vec<(NodeId, Option<NodeId>, Props)>;

impl H {
    pub fn new() -> H {
        H::with_platform(Platform::Windows)
    }

    pub fn with_platform(platform: Platform) -> H {
        let mut ed = Editor::new(ferrum_doc::Document::new(), None, VIEW, 1.0, platform).unwrap();
        let mut cam = Camera::new(VIEW, 1.0);
        cam.set_center(Point::new(VIEW.width / 2.0, VIEW.height / 2.0));
        ed.set_camera(cam);
        ed.take_events();
        H { ed, t: 1000.0 }
    }

    fn tick(&mut self) -> f64 {
        self.t += 600.0;
        self.t
    }

    pub fn add(&mut self, parent: Option<NodeId>, kind: NodeKind, p: PropsPatch) -> NodeId {
        let sel = self.ed.selection().to_vec();
        let id = self
            .ed
            .transact(|tx| {
                let id = tx.create_node(parent, None, kind)?;
                tx.set_props(id, &p)?;
                Ok(vec![id])
            })
            .unwrap()[0];
        self.ed.select(&sel);
        id
    }

    pub fn rect(&mut self, parent: Option<NodeId>, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        self.add(parent, NodeKind::Rect, patch(x, y, w, h))
    }

    pub fn frame(&mut self, parent: Option<NodeId>, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        self.add(parent, NodeKind::Frame, patch(x, y, w, h))
    }

    pub fn set(&mut self, id: NodeId, p: PropsPatch) {
        let sel = self.ed.selection().to_vec();
        self.ed
            .transact(|tx| {
                tx.set_props(id, &p)?;
                Ok(sel)
            })
            .unwrap();
    }

    pub fn props(&mut self, id: NodeId) -> Props {
        self.ed.page_doc().props(id).unwrap()
    }

    pub fn parent(&mut self, id: NodeId) -> Option<NodeId> {
        self.ed.page_doc().parent(id)
    }

    pub fn children(&mut self, id: Option<NodeId>) -> Vec<NodeId> {
        self.ed.page_doc().children(id)
    }

    pub fn state(&mut self) -> DocState {
        let page = self.ed.page_doc();
        page.walk()
            .into_iter()
            .map(|id| (id, page.parent(id), page.props(id).unwrap()))
            .collect()
    }

    pub fn ev(&mut self, input: Input) {
        self.ed.handle(input);
    }

    pub fn pe(&mut self, x: f64, y: f64, mods: Modifiers) -> PointerEvent {
        let t = self.tick();
        PointerEvent::new((x, y), mods, t)
    }

    pub fn down(&mut self, x: f64, y: f64, mods: Modifiers) {
        let e = self.pe(x, y, mods);
        self.ev(Input::PointerDown(e));
    }

    pub fn mv(&mut self, x: f64, y: f64, mods: Modifiers) {
        let e = self.pe(x, y, mods);
        self.ev(Input::PointerMove(e));
    }

    pub fn up(&mut self, x: f64, y: f64, mods: Modifiers) {
        let e = self.pe(x, y, mods);
        self.ev(Input::PointerUp(e));
    }

    pub fn hover(&mut self, x: f64, y: f64) {
        self.mv(x, y, Modifiers::NONE);
    }

    /// Нажатие, движение в несколько шагов и отпускание.
    pub fn drag(&mut self, from: (f64, f64), to: (f64, f64), mods: Modifiers) {
        self.down(from.0, from.1, mods);
        for i in 1..=4 {
            let k = i as f64 / 4.0;
            self.mv(
                from.0 + (to.0 - from.0) * k,
                from.1 + (to.1 - from.1) * k,
                mods,
            );
        }
        self.up(to.0, to.1, mods);
    }

    /// Нажатие и движение без отпускания.
    pub fn drag_hold(&mut self, from: (f64, f64), to: (f64, f64), mods: Modifiers) {
        self.down(from.0, from.1, mods);
        self.mv((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0, mods);
        self.mv(to.0, to.1, mods);
    }

    pub fn click(&mut self, x: f64, y: f64, mods: Modifiers) {
        self.down(x, y, mods);
        self.up(x, y, mods);
    }

    pub fn dbl(&mut self, x: f64, y: f64) {
        self.click(x, y, Modifiers::NONE);
        // Второе нажатие через 50 мс после первого отпускания.
        self.t -= 550.0;
        self.click(x, y, Modifiers::NONE);
    }

    pub fn key(&mut self, code: &str, mods: Modifiers) {
        self.ev(Input::KeyDown(KeyEvent::new(code, mods)));
        self.ev(Input::KeyUp(KeyEvent::new(code, mods)));
    }

    pub fn sel(&self) -> Vec<NodeId> {
        self.ed.selection().to_vec()
    }

    /// Сцена совпадает со сценой, построенной заново по документу, и превью нет.
    pub fn assert_scene_clean(&mut self) {
        assert!(!self.ed.scene().has_preview(), "превью осталось");
        let fresh = Scene::from_page(self.ed.page_doc());
        assert_eq!(snapshot(self.ed.scene()), snapshot(&fresh));
    }

    /// Жест дал ровно один шаг истории: отмена возвращает `before`, повтор — `after`.
    pub fn assert_one_step(&mut self, steps_before: u64, before: &DocState) {
        assert_eq!(self.ed.step_count(), steps_before + 1, "один шаг истории");
        let after = self.state();
        self.ed.execute(CommandId::Undo);
        assert_eq!(&self.state(), before, "отмена вернула документ");
        self.assert_scene_clean();
        self.ed.execute(CommandId::Redo);
        assert_eq!(self.state(), after, "повтор вернул правку");
        self.assert_scene_clean();
    }
}

pub fn ctrl() -> Modifiers {
    Modifiers::CTRL
}

pub fn shift() -> Modifiers {
    Modifiers::SHIFT
}

pub fn alt() -> Modifiers {
    Modifiers::ALT
}

pub fn none() -> Modifiers {
    Modifiers::NONE
}

pub fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

pub fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}
