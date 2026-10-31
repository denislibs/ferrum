//! Правка текста на холсте (спека подсистемы 3, раздел 7): вход и выход,
//! курсор и выделение (их держит ядро; позиции — символы Unicode, как у
//! `LoroText`), выделение мышью (щелчок, протяжка, двойной — слово, тройной —
//! абзац, Shift — расширить) и ход клавишами по кластерам, словам, строкам и
//! документу. Геометрию даёт раскладка узла (`ferrum_text::TextLayout`).
//!
//! Пока идёт правка, раскладка клавиш не действует: клавиши приходят сюда
//! (`text_key`), а набор — командами ввода (`text_input`, модуль `text_input`).

use std::sync::Arc;

use ferrum_doc::{NodeId, NodeKind, Props, TextAttrs, TextContent, TextMerge};
use ferrum_render::TextEditOverlay;
use ferrum_text::{
    next_word_end, paragraph_range, prev_word_start, word_range, Caret, LayoutRequest, TextLayout,
};
use kurbo::{Point, Rect};

use crate::editor::Editor;
use crate::input::{Modifiers, Platform, PointerEvent};
use crate::tools::select::{DOUBLE_CLICK_MS, DOUBLE_CLICK_PX};

/// Полупериод мигания курсора: 1 Гц — полсекунды виден, полсекунды нет.
pub const CARET_BLINK_MS: f64 = 500.0;

/// Как войти в правку текста.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnterAt {
    /// Выделить весь текст (Enter по выделенному тексту, команда).
    All,
    /// Курсор в точку мира (двойной щелчок).
    Point(Point),
}

/// Чем расширяется выделение протяжкой: символами, словами или абзацами.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unit {
    Char,
    Word,
    Paragraph,
}

/// Узел черновика инструмента T: в документе его нет, пока не набран первый
/// символ (пустой новый текст при выходе исчезает без шага отмены).
#[derive(Debug, Clone)]
pub(crate) struct DraftText {
    pub parent: Option<NodeId>,
    pub props: Props,
}

/// Состояние правки текста.
#[derive(Debug, Clone)]
pub(crate) struct TextEdit {
    pub id: NodeId,
    /// Новый текст инструмента T, ещё не в документе.
    pub draft: Option<DraftText>,
    /// Начало выделения (неподвижный конец) и курсор.
    pub anchor: usize,
    pub focus: Caret,
    /// x для хода вверх и вниз: держится, пока жмут ↑/↓.
    pub goal_x: Option<f64>,
    /// Стиль следующего ввода, заданный при курсоре без выделения (панель,
    /// Ctrl/Cmd+B). Сбрасывается сменой позиции.
    pub caret_attrs: Option<TextAttrs>,
    /// Составляемый текст IME (превью — в сцене, в документе его нет).
    pub composition: Option<String>,
    /// Протяжка мышью: якорный диапазон и единица выделения.
    pub drag: Option<(std::ops::Range<usize>, Unit)>,
    /// Последний щелчок: время отпускания, экранная точка и кратность (1–3).
    pub clicks: Option<(f64, Point, u8)>,
    /// С какого момента курсор горит (мигание начинается заново после действия).
    pub blink_from: f64,
    /// Метка последней правки ввода: продолжение склеивается в один шаг.
    pub merge: Option<TextMerge>,
    /// Шагов редактора на момент `merge`: другой шаг между — склейки нет.
    pub merge_steps: u64,
}

impl TextEdit {
    pub fn new(id: NodeId, anchor: usize, focus: usize, now: f64) -> TextEdit {
        TextEdit {
            id,
            draft: None,
            anchor,
            focus: Caret::at(focus),
            goal_x: None,
            caret_attrs: None,
            composition: None,
            drag: None,
            clicks: None,
            blink_from: now,
            merge: None,
            merge_steps: 0,
        }
    }

    /// Выделенный диапазон (пустой — курсор).
    pub fn range(&self) -> std::ops::Range<usize> {
        let (a, b) = (self.anchor, self.focus.index);
        a.min(b)..a.max(b)
    }
}

/// Что видит ядро о правке: узел, выделение и курсор на экране (поле ввода
/// встаёт под курсор, окно IME — у текста).
#[derive(Debug, Clone, PartialEq)]
pub struct TextEditInfo {
    pub id: NodeId,
    /// Узел ещё черновик (новый текст до первого символа).
    pub draft: bool,
    pub anchor: usize,
    pub focus: usize,
    /// Прямоугольник курсора в CSS-пикселях холста.
    pub caret: Rect,
    pub composing: bool,
}

/// Последний допустимый символ курсора: перед последним переводом строки.
fn last_index(content: &TextContent) -> usize {
    content.char_len().saturating_sub(1)
}

impl Editor {
    /// Идёт ли правка текста.
    pub fn is_editing_text(&self) -> bool {
        self.text_edit.is_some()
    }

    /// Состояние правки для ядра (`None` — правки нет).
    pub fn text_edit_info(&self) -> Option<TextEditInfo> {
        let t = self.text_edit.as_ref()?;
        let caret = self.text_overlay().and_then(|o| {
            let (x, top, bottom) = o.caret_geometry?;
            let to_screen = self.camera.world_to_screen_transform() * o.overlay.transform;
            let a = to_screen * Point::new(x, top);
            let b = to_screen * Point::new(x, bottom);
            Some(Rect::from_points(a, b))
        });
        Some(TextEditInfo {
            id: t.id,
            draft: t.draft.is_some(),
            anchor: t.anchor,
            focus: t.focus.index,
            caret: caret.unwrap_or(Rect::ZERO),
            composing: t.composition.is_some(),
        })
    }

    /// Текущее время ядра (мс): мигание курсора и склейка ввода в один шаг.
    pub fn set_time(&mut self, now_ms: f64) {
        if now_ms.is_finite() {
            self.now_ms = now_ms;
            if self.text_edit.is_some() {
                self.emit();
            }
        }
    }

    /// Горит ли курсор сейчас (мигание 1 Гц от последнего действия).
    pub(crate) fn caret_on(&self) -> bool {
        let Some(t) = &self.text_edit else {
            return false;
        };
        let k = ((self.now_ms - t.blink_from).max(0.0) / CARET_BLINK_MS).floor() as u64;
        k.is_multiple_of(2)
    }

    /// Содержимое правимого узла в документе (без превью IME); у черновика —
    /// его собственное.
    pub(crate) fn edit_content(&self) -> Option<TextContent> {
        let t = self.text_edit.as_ref()?;
        if let Some(d) = &t.draft {
            return d.props.text.as_ref().map(|x| x.content.clone());
        }
        let n = self.scene.node(t.id)?;
        let doc = self.doc.loaded_page(self.page)?.props(n.id).ok()?;
        doc.text.as_ref().map(|x| x.content.clone())
    }

    /// Раскладка правимого узла по свойствам сцены (кэш — по версии узла и
    /// поколению реестра шрифтов).
    pub(crate) fn edit_layout(&self) -> Option<Arc<TextLayout>> {
        let t = self.text_edit.as_ref()?;
        let n = self.scene.node(t.id)?;
        let text = n.props.text.as_ref()?;
        let generation = self.fonts.borrow().generation();
        let key = (t.id, n.version, generation);
        if let Some((k, l)) = self.edit_layout_cache.borrow().as_ref() {
            if *k == key {
                return Some(l.clone());
            }
        }
        let req = LayoutRequest::for_node(text, &n.props.fills, n.props.w, n.props.h);
        let layout = Arc::new(self.fonts.borrow_mut().layout(&req));
        *self.edit_layout_cache.borrow_mut() = Some((key, layout.clone()));
        Some(layout)
    }

    /// Оверлей правки: рамка, выделение, курсор (если горит) и подчёркивание IME.
    pub(crate) fn text_overlay(&self) -> Option<EditOverlay> {
        let t = self.text_edit.as_ref()?;
        let n = self.scene.node(t.id)?;
        let layout = self.edit_layout()?;
        let comp = t.composition.as_ref().map_or(0, |c| c.chars().count());
        let range = t.range();
        let mut overlay = TextEditOverlay {
            transform: n.world,
            frame: Rect::new(0.0, 0.0, n.props.w, n.props.h),
            selection: Vec::new(),
            caret: None,
            composition: Vec::new(),
        };
        // Пока идёт составление, выделение уже заменено превью: курсор — за ним.
        let caret = if comp > 0 {
            let r = range.start..range.start + comp;
            overlay.composition = layout
                .selection_rects(r.clone())
                .into_iter()
                .map(|q| (q.x0, q.x1, q.y1 - 1.0))
                .collect();
            Caret::at(r.end)
        } else {
            overlay.selection = layout.selection_rects(range.clone());
            t.focus
        };
        let geometry = layout.caret_rect(caret);
        if (range.is_empty() || comp > 0) && self.caret_on() {
            overlay.caret = geometry;
        }
        Some(EditOverlay {
            overlay,
            caret_geometry: geometry,
        })
    }

    /// Входит в правку текста `id` (выделенного или нет): узел выделяется,
    /// жест отменяется. `false` — не текст, заблокирован или его нет.
    pub fn start_text_edit(&mut self, id: NodeId, at: EnterAt) -> bool {
        self.cancel_gesture();
        self.finish_text_edit();
        if self.doc.is_read_only() {
            return false;
        }
        let Some(slot) = self.scene.slot(id) else {
            return false;
        };
        if self.scene.is_draft(id)
            || self.scene.at(slot).props.kind != NodeKind::Text
            || crate::selection::locked(&self.scene, slot)
        {
            return false;
        }
        let Some(content) = self
            .scene
            .at(slot)
            .props
            .text
            .as_ref()
            .map(|t| t.content.clone())
        else {
            return false;
        };
        let last = last_index(&content);
        self.selection.set(vec![id]);
        self.hover = None;
        self.text_hover = false;
        self.text_edit = Some(TextEdit::new(id, 0, last, self.now_ms));
        match at {
            EnterAt::All => {}
            EnterAt::Point(world) => {
                let c = self.hit_text(world).unwrap_or(Caret::at(last));
                let t = self.text_edit.as_mut().expect("задана выше");
                (t.anchor, t.focus) = (c.index, c);
            }
        }
        self.emit();
        true
    }

    /// Выход из правки (Esc, команда): узел остаётся выделенным; пустой текст
    /// удаляется (`finish_text_edit`).
    pub fn end_text_edit(&mut self) {
        let Some(id) = self.text_edit.as_ref().map(|t| t.id) else {
            return;
        };
        self.finish_text_edit();
        if self.scene.slot(id).is_some() && !self.scene.is_draft(id) {
            self.selection.set(vec![id]);
        }
        self.emit();
    }

    /// Позиция по точке мира в правимом узле (или ближайшая к ней).
    pub(crate) fn hit_text(&self, world: Point) -> Option<Caret> {
        let t = self.text_edit.as_ref()?;
        let n = self.scene.node(t.id)?;
        let local = n.world.inverse() * world;
        let layout = self.edit_layout()?;
        let content = self.edit_content()?;
        let mut c = layout.hit(local);
        c.index = c.index.min(last_index(&content));
        Some(c)
    }

    /// Точка мира внутри рамки правимого узла (с допуском в 4 px экрана).
    fn inside_edited(&self, world: Point) -> bool {
        let Some(n) = self.text_edit.as_ref().and_then(|t| self.scene.node(t.id)) else {
            return false;
        };
        let tol = crate::tools::HIT_TOLERANCE_PX / self.camera.zoom();
        let local = n.world.inverse() * world;
        Rect::new(0.0, 0.0, n.props.w, n.props.h)
            .inflate(tol, tol)
            .contains(local)
    }

    /// Нажатие во время правки. `false` — мимо текста: правка закончена, нажатие
    /// обработает инструмент.
    pub(crate) fn text_pointer_down(&mut self, e: &PointerEvent) -> bool {
        let world = self.camera.screen_to_world(e.point);
        if !self.inside_edited(world) {
            // Нажатие достаётся инструменту как обычное: без выделения нет ручек
            // правимого текста, и оно не начнёт поворот или размер.
            self.finish_text_edit();
            self.selection.set(Vec::new());
            self.emit();
            return false;
        }
        let Some(caret) = self.hit_text(world) else {
            return true;
        };
        let content = self.edit_content().unwrap_or_default();
        let chars: Vec<char> = content.text().chars().collect();
        let now = self.now_ms;
        let t = self.text_edit.as_mut().expect("правка идёт");
        let count = match t.clicks {
            Some((at, p, n))
                if e.time_ms - at <= DOUBLE_CLICK_MS
                    && (e.point - p).hypot() <= DOUBLE_CLICK_PX
                    && !e.modifiers.shift =>
            {
                n % 3 + 1
            }
            _ => 1,
        };
        t.clicks = Some((e.time_ms, e.point, count));
        t.goal_x = None;
        t.caret_attrs = None;
        t.blink_from = now;
        if e.modifiers.shift && count == 1 {
            // Shift+щелчок расширяет выделение от прежнего якоря.
            t.focus = caret;
            t.drag = Some((t.anchor..t.anchor, Unit::Char));
        } else {
            let (range, unit) = match count {
                2 => (word_range(&chars, caret.index), Unit::Word),
                3 => (paragraph_range(&chars, caret.index), Unit::Paragraph),
                _ => (caret.index..caret.index, Unit::Char),
            };
            t.anchor = range.start;
            t.focus = if range.is_empty() {
                caret
            } else {
                Caret::at(range.end)
            };
            t.drag = Some((range, unit));
        }
        true
    }

    /// Движение с нажатой кнопкой во время правки: выделение до точки (словами
    /// или абзацами — после двойного и тройного щелчка).
    pub(crate) fn text_pointer_move(&mut self, e: &PointerEvent) {
        let Some((base, unit)) = self.text_edit.as_ref().and_then(|t| t.drag.clone()) else {
            return;
        };
        let world = self.camera.screen_to_world(e.point);
        let Some(caret) = self.hit_text(world) else {
            return;
        };
        let content = self.edit_content().unwrap_or_default();
        let chars: Vec<char> = content.text().chars().collect();
        let extent = match unit {
            Unit::Char => caret.index..caret.index,
            Unit::Word => word_range(&chars, caret.index),
            Unit::Paragraph => paragraph_range(&chars, caret.index),
        };
        let now = self.now_ms;
        let t = self.text_edit.as_mut().expect("правка идёт");
        if extent.start < base.start {
            t.anchor = base.end;
            t.focus = Caret::at(extent.start);
        } else {
            t.anchor = base.start;
            t.focus = if unit == Unit::Char {
                caret
            } else {
                Caret::at(extent.end.max(base.end))
            };
        }
        t.blink_from = now;
    }

    /// Экранная точка над правимым текстом (курсор — текстовый).
    pub(crate) fn over_edited_text(&self, screen: Point) -> bool {
        self.inside_edited(self.camera.screen_to_world(screen))
    }

    /// Правимый узел пропал — правка кончается (черновик — в сцене, его не
    /// трогаем); курсор и якорь — не дальше конца текста.
    pub(crate) fn clamp_text_edit(&mut self) {
        let Some(t) = &self.text_edit else {
            return;
        };
        let id = t.id;
        if self.scene.slot(id).is_none() {
            if t.draft.is_some() {
                // Черновик убран вместе с превью чужого жеста — вернуть; не
                // встал (родитель пропал) — правка кончается.
                self.restore_draft();
                if self.scene.slot(id).is_none() {
                    self.text_edit = None;
                    self.text_hover = false;
                    self.scene.clear_preview();
                }
            } else {
                // Узла нет: правка кончается, превью IME уходит вместе с ней.
                self.text_edit = None;
                self.text_hover = false;
                self.scene.clear_preview();
            }
            return;
        }
        let Some(content) = self.edit_content() else {
            return;
        };
        let last = last_index(&content);
        let t = self.text_edit.as_mut().expect("проверено выше");
        if t.anchor > last || t.focus.index > last {
            t.anchor = t.anchor.min(last);
            t.focus = Caret::at(t.focus.index.min(last));
        }
    }

    /// Отпускание: протяжка кончается; следующий щелчок считается двойным от
    /// этого момента (как у «Перемещения»).
    pub(crate) fn text_pointer_up(&mut self, e: &PointerEvent) {
        if let Some(t) = self.text_edit.as_mut() {
            t.drag = None;
            if let Some(c) = t.clicks.as_mut() {
                c.0 = e.time_ms;
            }
        }
    }

    /// Клавиша во время правки текста. Возвращает, обработана ли она.
    pub fn text_key(&mut self, code: &str, mods: Modifiers) -> bool {
        if self.text_edit.is_none() {
            return false;
        }
        let handled = self.text_navigation(code, mods) || self.text_command_key(code, mods);
        if handled {
            if let Some(t) = self.text_edit.as_mut() {
                t.blink_from = self.now_ms;
            }
        }
        self.emit();
        handled
    }

    /// Клавиши правки, кроме хода курсора: Esc — выход, удаление и отмена —
    /// `text_edit_key` (модуль `text_input`).
    fn text_command_key(&mut self, code: &str, mods: Modifiers) -> bool {
        match code {
            "Escape" => {
                self.end_text_edit();
                true
            }
            _ => self.text_edit_key(code, mods),
        }
    }

    /// Ход курсора и выделение клавишами (см. описание модуля).
    fn text_navigation(&mut self, code: &str, mods: Modifiers) -> bool {
        let platform = self.settings.platform;
        let mac = platform == Platform::Mac;
        let primary = mods.primary(platform);
        let word = if mac { mods.alt } else { mods.ctrl };
        let extend = mods.shift;
        let Some(content) = self.edit_content() else {
            return false;
        };
        let Some(layout) = self.edit_layout() else {
            return false;
        };
        let last = last_index(&content);
        let chars: Vec<char> = content.text().chars().collect();
        let t = self.text_edit.as_ref().expect("правка идёт");
        let (range, focus) = (t.range(), t.focus);
        let x_now = || {
            t.goal_x
                .or_else(|| layout.caret_rect(focus).map(|r| r.0))
                .unwrap_or(0.0)
        };
        let mut goal = None;
        let target: Caret = match code {
            "ArrowLeft" | "ArrowRight" => {
                let right = code == "ArrowRight";
                if mac && primary {
                    let (a, b) = layout.line_bounds(focus).unwrap_or_default();
                    if right {
                        b
                    } else {
                        a
                    }
                } else if word {
                    Caret::at(if right {
                        next_word_end(&chars, focus.index)
                    } else {
                        prev_word_start(&chars, focus.index)
                    })
                } else if !extend && !range.is_empty() {
                    // Выделение схлопывается к своему краю.
                    Caret::at(if right { range.end } else { range.start })
                } else {
                    Caret::at(if right {
                        layout.next_stop(focus.index)
                    } else {
                        layout.prev_stop(focus.index)
                    })
                }
            }
            "ArrowUp" | "ArrowDown" => {
                let down = code == "ArrowDown";
                if mac && primary {
                    Caret::at(if down { last } else { 0 })
                } else if (mac && mods.alt) || (!mac && mods.ctrl) {
                    // Абзац: к началу текущего (или предыдущего) и к концу.
                    let p = paragraph_range(&chars, focus.index);
                    if down {
                        let end = if focus.index >= p.end {
                            paragraph_range(&chars, (p.end + 1).min(last)).end
                        } else {
                            p.end
                        };
                        Caret::at(end)
                    } else if focus.index > p.start {
                        Caret::at(p.start)
                    } else {
                        Caret::at(paragraph_range(&chars, p.start.saturating_sub(1)).start)
                    }
                } else {
                    let x = x_now();
                    goal = Some(x);
                    let from = if !extend && !range.is_empty() {
                        Caret::at(if down { range.end } else { range.start })
                    } else {
                        focus
                    };
                    layout
                        .vertical(from, x, if down { 1 } else { -1 })
                        .unwrap_or(Caret::at(if down { last } else { 0 }))
                }
            }
            "Home" | "End" => {
                let end = code == "End";
                if !mac && mods.ctrl {
                    Caret::at(if end { last } else { 0 })
                } else {
                    let (a, b) = layout.line_bounds(focus).unwrap_or_default();
                    if end {
                        b
                    } else {
                        a
                    }
                }
            }
            "PageUp" => Caret::at(0),
            "PageDown" => Caret::at(last),
            "KeyA" if primary && !mods.alt && !mods.shift => {
                let t = self.text_edit.as_mut().expect("правка идёт");
                t.anchor = 0;
                t.focus = Caret::at(last);
                t.goal_x = None;
                t.caret_attrs = None;
                return true;
            }
            _ => return false,
        };
        let target = Caret {
            index: target.index.min(last),
            upstream: target.upstream && target.index <= last,
        };
        let t = self.text_edit.as_mut().expect("правка идёт");
        if !extend {
            t.anchor = target.index;
        }
        t.focus = target;
        t.goal_x = goal;
        t.caret_attrs = None;
        true
    }
}

/// Оверлей правки и геометрия курсора (для поля ввода — и когда курсор погашен).
pub(crate) struct EditOverlay {
    pub overlay: TextEditOverlay,
    pub caret_geometry: Option<(f64, f64, f64)>,
}
