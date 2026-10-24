//! Случайные последовательности событий (детерминированный ГСЧ): редактор не
//! падает, после отпускания и Esc превью не остаётся, сцена совпадает с
//! построенной заново, а каждый записанный жест или команда — ровно один шаг,
//! отмена которого возвращает документ к прежнему состоянию.

mod common;

use std::cell::RefCell;

use common::*;
use ferrum_doc::{NodeKind, PropsPatch};
use ferrum_editor::geometry::SelectionBox;
use ferrum_editor::*;
use ferrum_scene::Scene;
use kurbo::Point;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestRng, TestRunner};

/// Точка относительно узла или рамки выделения: доли рамки и сдвиг в пикселях.
/// Доля 3 — чуть снаружи угла (зона поворота).
#[derive(Debug, Clone)]
struct Anchor {
    node: u8,
    fx: u8,
    fy: u8,
    jx: i8,
    jy: i8,
}

#[derive(Debug, Clone, Copy)]
enum End {
    Up,
    Escape,
    Cancel,
    Blur,
    /// Отпускание с нечисловой координатой: событие отбрасывается, жест должен отмениться.
    NanUp,
}

#[derive(Debug, Clone)]
enum Act {
    Gesture {
        tool: u8,
        at: Anchor,
        to: (i16, i16),
        mods: u8,
        /// Шаг, на котором меняется модификатор (Shift или Alt).
        toggle: u8,
        steps: u8,
        end: End,
    },
    Key(u8),
    Wheel {
        dy: i8,
        zoom: bool,
    },
}

const COMMANDS_USED: [CommandId; 24] = [
    CommandId::Undo,
    CommandId::Redo,
    CommandId::Delete,
    CommandId::Duplicate,
    CommandId::Group,
    CommandId::Ungroup,
    CommandId::FrameSelection,
    CommandId::BringForward,
    CommandId::SendBackward,
    CommandId::BringToFront,
    CommandId::SendToBack,
    CommandId::NudgeRight,
    CommandId::NudgeDownBig,
    CommandId::ToggleVisible,
    CommandId::ToggleLock,
    CommandId::SelectAll,
    CommandId::Escape,
    CommandId::ZoomToFit,
    CommandId::ZoomOut,
    CommandId::Copy,
    CommandId::Cut,
    CommandId::Paste,
    CommandId::FlipHorizontal,
    CommandId::FlipVertical,
];

const TOOL_KEYS: [&str; 6] = ["KeyV", "KeyF", "KeyR", "KeyO", "KeyL", "KeyH"];

fn act() -> impl Strategy<Value = Act> {
    let anchor =
        (any::<u8>(), 0u8..4, 0u8..4, -4i8..5, -4i8..5).prop_map(|(node, fx, fy, jx, jy)| Anchor {
            node,
            fx,
            fy,
            jx,
            jy,
        });
    let end = prop_oneof![
        8 => Just(End::Up),
        1 => Just(End::Escape),
        1 => Just(End::Cancel),
        1 => Just(End::Blur),
        1 => Just(End::NanUp),
    ];
    prop_oneof![
        8 => (
            prop_oneof![6 => Just(0u8), 1 => 1u8..6],
            anchor,
            (-160i16..160, -160i16..160),
            0u8..8,
            0u8..6,
            1u8..5,
            end,
        )
            .prop_map(|(tool, at, to, mods, toggle, steps, end)| Act::Gesture {
                tool,
                at,
                to,
                mods,
                toggle,
                steps,
                end,
            }),
        3 => (0u8..COMMANDS_USED.len() as u8).prop_map(Act::Key),
        1 => (any::<i8>(), any::<bool>()).prop_map(|(dy, zoom)| Act::Wheel { dy, zoom }),
    ]
}

fn mods(bits: u8) -> Modifiers {
    Modifiers {
        shift: bits & 1 != 0,
        alt: bits & 2 != 0,
        ctrl: bits & 4 != 0,
        meta: false,
    }
}

/// Стартовая страница: фрейм с прямоугольником, эллипсом и группой; в корне —
/// прямоугольник, повёрнутый прямоугольник и линия.
fn setup() -> H {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 400.0, 300.0);
    h.rect(Some(f), 20.0, 20.0, 80.0, 60.0);
    h.add(Some(f), NodeKind::Ellipse, patch(150.0, 40.0, 60.0, 60.0));
    let g = h.add(Some(f), NodeKind::Group, PropsPatch::default());
    h.rect(Some(g), 40.0, 150.0, 50.0, 50.0);
    h.rect(Some(g), 110.0, 170.0, 50.0, 50.0);
    h.rect(None, 600.0, 150.0, 100.0, 80.0);
    let t = h.rect(None, 650.0, 400.0, 80.0, 40.0);
    h.set(
        t,
        PropsPatch {
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    h.add(None, NodeKind::Line, patch(100.0, 600.0, 200.0, 0.0));
    h
}

/// Экранная точка якоря при текущем состоянии.
fn resolve(h: &H, a: &Anchor) -> Point {
    let scene = h.ed.scene();
    let frac = |f: u8| [0.0, 0.5, 1.0, 1.1][f as usize];
    let world = if a.node.is_multiple_of(3) && !h.ed.selection().is_empty() {
        SelectionBox::of(scene, h.ed.selection()).map(|b| b.point(frac(a.fx), frac(a.fy)))
    } else {
        let mut all = Vec::new();
        let mut stack: Vec<_> = scene.roots().to_vec();
        while let Some(s) = stack.pop() {
            all.push(s);
            stack.extend(scene.at(s).children.iter().copied());
        }
        if all.is_empty() || a.node.is_multiple_of(7) {
            None
        } else {
            let id = scene.at(all[a.node as usize % all.len()]).id;
            SelectionBox::of(scene, &[id]).map(|b| b.point(frac(a.fx), frac(a.fy)))
        }
    };
    let world = world.unwrap_or(Point::new(900.0, 750.0));
    let s = h.ed.camera().world_to_screen(world);
    Point::new(s.x + a.jx as f64, s.y + a.jy as f64)
}

#[derive(Default, Debug)]
struct Stats {
    gesture_steps: u64,
    draw_steps: u64,
    resize_steps: u64,
    rotate_steps: u64,
    duplicate_steps: u64,
    reparent_steps: u64,
    command_steps: u64,
    cancelled_previews: u64,
    marquees: u64,
    undo_checks: u64,
    /// Записанные шаги (жесты и команды), после которых у какого-то узла сменилось
    /// отражение: ресайз за противоположную сторону или команда отражения.
    flips: u64,
    /// Из них — ресайзы за противоположную сторону.
    resize_flips: u64,
}

fn check_clean(h: &mut H, what: &str) -> Result<(), TestCaseError> {
    prop_assert!(!h.ed.is_busy(), "{what}: жест не закончился");
    prop_assert!(!h.ed.scene().has_preview(), "{what}: превью осталось");
    let fresh = Scene::from_page(h.ed.page_doc());
    prop_assert_eq!(
        ferrum_scene::testing::snapshot(h.ed.scene()),
        ferrum_scene::testing::snapshot(&fresh),
        "{}: сцена разошлась с документом",
        what
    );
    for id in h.ed.selection() {
        prop_assert!(
            h.ed.scene().node(*id).is_some(),
            "{what}: выделен удалённый"
        );
        prop_assert!(!h.ed.scene().is_draft(*id), "{what}: выделен черновик");
    }
    Ok(())
}

/// Что сделал один шаг сценария — для подсчётов и проверок `run`.
#[derive(Default)]
struct Played {
    kind: &'static str,
    marquees: u64,
    /// Жест закончен не отпусканием (Esc, потеря указателя или фокуса).
    cancelled: bool,
    had_preview: bool,
}

/// Подаёт редактору события одного шага сценария. Тот же код у проверок
/// согласованности (`run`) и у проверки детерминизма (`replay`).
fn play(
    h: &mut H,
    a: &Act,
    clipboard: &mut Option<Vec<u8>>,
) -> Result<Played, ferrum_editor::clipboard::ClipboardError> {
    let mut out = Played::default();
    match a {
        Act::Gesture {
            tool,
            at,
            to,
            mods: bits,
            toggle,
            steps: n,
            end,
        } => {
            if *tool > 0 {
                h.key(TOOL_KEYS[*tool as usize], none());
            }
            let m = mods(*bits);
            let p = resolve(h, at);
            h.hover(p.x, p.y);
            out.kind = match (h.ed.tool_kind(), h.ed.cursor()) {
                (ToolKind::Move, Cursor::Resize { .. }) => "resize",
                (ToolKind::Move, Cursor::Rotate { .. }) => "rotate",
                (ToolKind::Move, _) => "move",
                (ToolKind::Hand, _) => "hand",
                _ => "draw",
            };
            h.down(p.x, p.y, m);
            let mut cur = m;
            for k in 1..=*n {
                if k == *toggle {
                    // Модификатор меняется посреди жеста.
                    cur.shift = !cur.shift;
                    cur.alt = !cur.alt && bits & 4 == 0;
                    h.ev(Input::KeyDown(KeyEvent::new("ShiftLeft", cur)));
                }
                let t = k as f64 / *n as f64;
                h.mv(p.x + to.0 as f64 * t, p.y + to.1 as f64 * t, cur);
                if h.ed.overlay().marquee.is_some() {
                    out.marquees += 1;
                }
            }
            if h.ed.cursor() == Cursor::Duplicate {
                out.kind = "duplicate";
            }
            let had_preview = h.ed.scene().has_preview();
            let (ex, ey) = (p.x + to.0 as f64, p.y + to.1 as f64);
            match end {
                End::Up => h.up(ex, ey, cur),
                End::Escape => {
                    h.ev(Input::KeyDown(KeyEvent::new("Escape", none())));
                    h.up(ex, ey, cur);
                }
                End::Cancel => h.ev(Input::PointerCancel),
                End::Blur => h.ev(Input::Blur),
                End::NanUp => h.up(f64::NAN, ey, cur),
            }
            out.cancelled = !matches!(end, End::Up);
            out.had_preview = had_preview;
        }
        Act::Key(k) => {
            let cmd = COMMANDS_USED[*k as usize];
            out.kind = "command";
            if cmd == CommandId::Paste {
                if let Some(bytes) = clipboard.as_ref() {
                    h.ed.paste(bytes)?;
                }
            } else {
                h.ed.execute(cmd);
            }
            for e in h.ed.take_events() {
                if let EditorEvent::Clipboard(b) = e {
                    *clipboard = Some(b);
                }
            }
            if matches!(cmd, CommandId::Undo | CommandId::Redo) {
                out.kind = "history";
            }
        }
        Act::Wheel { dy, zoom } => {
            let m = if *zoom { ctrl() } else { none() };
            h.ev(Input::Wheel(WheelEvent {
                point: Point::new(500.0, 400.0),
                delta: v(0.0, *dy as f64),
                modifiers: m,
            }));
        }
    }
    Ok(out)
}

fn run(acts: &[Act], stats: &mut Stats) -> Result<(), TestCaseError> {
    let mut h = setup();
    let mut clipboard: Option<Vec<u8>> = None;
    for (i, a) in acts.iter().enumerate() {
        let what = format!("шаг {i}: {a:?}");
        let before = h.state();
        let steps = h.ed.step_count();
        let parents: Vec<_> = before.iter().map(|(id, p, _)| (*id, *p)).collect();
        let played = play(&mut h, a, &mut clipboard)
            .map_err(|e| TestCaseError::fail(format!("{what}: {e}")))?;
        stats.marquees += played.marquees;
        let kind = played.kind;
        if played.cancelled {
            prop_assert_eq!(
                h.ed.step_count(),
                steps,
                "{}: отменённый жест записан",
                what
            );
            if played.had_preview {
                stats.cancelled_previews += 1;
            }
        }
        check_clean(&mut h, &what)?;
        let made = h.ed.step_count() - steps;
        prop_assert!(made <= 1, "{}: больше одного шага ({})", what, made);
        if kind == "history" || made == 0 {
            continue;
        }
        let after = h.state();
        let flipped = after.iter().any(|(id, _, p)| {
            before
                .iter()
                .any(|(q, _, b)| q == id && (b.flip_x, b.flip_y) != (p.flip_x, p.flip_y))
        });
        if flipped {
            stats.flips += 1;
            if kind == "resize" {
                stats.resize_flips += 1;
            }
        }
        match kind {
            "command" => stats.command_steps += 1,
            "draw" => stats.draw_steps += 1,
            "resize" => stats.resize_steps += 1,
            "rotate" => stats.rotate_steps += 1,
            "duplicate" => stats.duplicate_steps += 1,
            _ => {}
        }
        if kind != "command" {
            stats.gesture_steps += 1;
            let moved = after
                .iter()
                .any(|(id, p, _)| parents.iter().any(|(q, pp)| q == id && pp != p));
            if moved {
                stats.reparent_steps += 1;
            }
        }
        // Отмена возвращает документ к состоянию до шага, повтор — к состоянию после.
        h.ed.execute(CommandId::Undo);
        prop_assert_eq!(&h.state(), &before, "{}: отмена не вернула документ", what);
        check_clean(&mut h, &what)?;
        h.ed.execute(CommandId::Redo);
        prop_assert_eq!(&h.state(), &after, "{}: повтор не вернул правку", what);
        check_clean(&mut h, &what)?;
        stats.undo_checks += 1;
    }
    Ok(())
}

#[test]
fn random_event_sequences_keep_editor_consistent() {
    let stats = RefCell::new(Stats::default());
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 96,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    runner
        .run(&prop::collection::vec(act(), 1..40), |acts| {
            run(&acts, &mut stats.borrow_mut())
        })
        .unwrap();
    let st = stats.into_inner();
    eprintln!("{st:?}");
    // Против пустых прогонов: каждый вид жеста действительно записывался и
    // проверялся отменой.
    assert!(st.gesture_steps > 300, "{st:?}");
    assert!(st.draw_steps > 40, "{st:?}");
    assert!(st.resize_steps > 30, "{st:?}");
    assert!(st.rotate_steps > 10, "{st:?}");
    assert!(st.duplicate_steps > 20, "{st:?}");
    assert!(st.reparent_steps > 10, "{st:?}");
    assert!(st.command_steps > 150, "{st:?}");
    assert!(st.cancelled_previews > 30, "{st:?}");
    assert!(st.marquees > 30, "{st:?}");
    assert!(st.undo_checks > 300, "{st:?}");
    assert!(st.flips > 30, "{st:?}");
    assert!(st.resize_flips > 10, "{st:?}");
}

/// Узлы страницы в порядке обхода: идентификатор, родитель, свойства.
type Nodes = Vec<(String, Option<String>, ferrum_doc::Props)>;

/// Узлы страницы с идентификаторами, где пир Loro заменён номером по порядку
/// появления. Пир каждого документа Loro выбирает сам (случайно, вне редактора),
/// а счётчик в `NodeId` и всё остальное задаёт последовательность правок.
fn canonical(h: &mut H) -> (Nodes, Vec<String>) {
    let mut peers: Vec<String> = Vec::new();
    let mut canon = |id: ferrum_doc::NodeId| {
        let s = id.to_string();
        let (counter, peer) = s.split_once('@').expect("NodeId — счётчик@пир");
        let n = match peers.iter().position(|p| p == peer) {
            Some(n) => n,
            None => {
                peers.push(peer.to_string());
                peers.len() - 1
            }
        };
        format!("{counter}@p{n}")
    };
    let nodes = h
        .state()
        .into_iter()
        .map(|(id, parent, props)| (canon(id), parent.map(&mut canon), props))
        .collect();
    let selection = h.sel().into_iter().map(canon).collect();
    (nodes, selection)
}

/// Проигрывает сценарий на свежем редакторе без проверок между шагами.
fn replay(acts: &[Act]) -> (Nodes, Vec<String>, u64) {
    let mut h = setup();
    let mut clipboard = None;
    for a in acts {
        // Ошибка вставки — тоже результат: у второго прогона она должна повториться.
        let _ = play(&mut h, a, &mut clipboard);
    }
    let (nodes, selection) = canonical(&mut h);
    (nodes, selection, h.ed.step_count())
}

/// Одна последовательность событий — один документ (раздел 7 спеки): два свежих
/// редактора после одного сценария совпадают узел в узел, с идентификаторами,
/// родителями, свойствами, выделением и числом шагов истории.
#[test]
fn same_events_give_same_document() {
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 48,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    let total = RefCell::new((0usize, 0u64));
    runner
        .run(&prop::collection::vec(act(), 1..40), |acts| {
            let a = replay(&acts);
            let b = replay(&acts);
            prop_assert_eq!(&a, &b);
            let mut t = total.borrow_mut();
            t.0 += a.0.len();
            t.1 += a.2;
            Ok(())
        })
        .unwrap();
    // Против пустых прогонов: сценарии создавали узлы и записывали шаги.
    let (nodes, steps) = total.into_inner();
    assert!(
        nodes > 48 * 4 && steps > 200,
        "{nodes} узлов, {steps} шагов"
    );
}
