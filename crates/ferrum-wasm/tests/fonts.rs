//! Шрифты и текст в ядре worker'а (подсистема 3, этап 1): список вшитых,
//! запрос и загрузка байтов, правка, ждущая шрифтов, отсутствующие шрифты и
//! замена одним шагом, пределы команд, пример из 10 000 текстов.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use ferrum_editor::Platform;
use ferrum_render::CpuBackend;
use ferrum_text::read_faces;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const FILES: [&str; 4] = [
    "inter/Inter-Variable.ttf",
    "inter/Inter-Italic-Variable.ttf",
    "roboto/Roboto-Variable.ttf",
    "roboto/Roboto-Italic-Variable.ttf",
];

/// Манифест вшитых — как его отдаёт `font_manifest` (файлы относительно `fonts/`).
fn manifest() -> Vec<FontFamilyDto> {
    let faces = FILES
        .iter()
        .flat_map(|r| read_faces(&font_file(r), r))
        .collect::<Vec<_>>();
    ferrum_text::group(faces)
        .into_iter()
        .map(|f| FontFamilyDto {
            family: f.family,
            faces: f
                .faces
                .into_iter()
                .map(|x| FontFaceDto {
                    style: x.style,
                    weight: Some(x.weight as f64),
                    italic: Some(x.italic),
                    stretch: Some(x.stretch as f64),
                    file: x.file,
                    index: Some(x.index),
                })
                .collect(),
        })
        .collect()
}

fn core() -> Core {
    let mut c = Core::new(Platform::Mac, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    c.command(Command::Demo { nodes: 0 });
    c.command(Command::FontsAvailable {
        source: FontSource::Bundled,
        families: manifest(),
    });
    c.take_events();
    c
}

/// Отвечает на все запросы шрифтов из событий (как worker ядра по манифесту).
fn serve(c: &mut Core, events: &[CoreEvent]) -> usize {
    let mut n = 0;
    for e in events {
        if let CoreEvent::FontRequest {
            family,
            source,
            files,
        } = e
        {
            assert_eq!(*source, FontSource::Bundled);
            for f in files {
                c.load_font_for_tests(family, f, font_file(f));
                n += 1;
            }
        }
    }
    n
}

fn create(c: &mut Core, text: &str, font: Option<(&str, &str)>) {
    c.command(Command::TestCreateText {
        x: 20.0,
        y: 20.0,
        text: text.into(),
        font: font.map(|(f, s)| FontNameDto {
            family: f.into(),
            style: s.into(),
        }),
        size: Some(32.0),
        sizing: None,
        width: None,
        height: None,
    });
}

fn nodes(c: &Core) -> usize {
    c.editor().scene().len()
}

#[test]
fn text_waits_for_requested_fonts_then_appears_and_draws() {
    let mut c = core();
    create(&mut c, "Ferrum", None);
    assert_eq!(nodes(&c), 0, "шрифт ещё не загружен");
    let events = c.take_events();
    assert_eq!(serve(&mut c, &events), 2);
    assert_eq!(nodes(&c), 1);
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Document { can_undo: true, .. })));
    // Текст на холсте: тёмные пиксели в его рамке (камера примера — зум 0.5).
    let (w, h) = c.device_size();
    let mut b = CpuBackend::new(w, h);
    c.draw(&mut b);
    let px = b.render();
    let dark = px.data().iter().filter(|p| p.r < 128 && p.a > 200).count();
    assert!(dark > 20, "{dark}");
}

#[test]
fn missing_fonts_are_reported_and_replaced_in_one_step() {
    let mut c = core();
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    serve(&mut c, &events);
    create(&mut c, "Comic", Some(("Comic Sans", "Bold")));
    let events = c.take_events();
    let missing = events.iter().rev().find_map(|e| match e {
        CoreEvent::MissingFonts { list } => Some(list.clone()),
        _ => None,
    });
    assert_eq!(
        missing,
        Some(vec![MissingFontDto {
            family: "Comic Sans".into(),
            style: "Bold".into(),
            nodes: 1,
        }])
    );
    c.command(Command::ReplaceMissingFont {
        from: FontNameDto {
            family: "Comic Sans".into(),
            style: "Bold".into(),
        },
        to: FontNameDto {
            family: "Roboto".into(),
            style: "Bold".into(),
        },
    });
    // Roboto ещё не загружен: замена ждёт его, запросив байты.
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::FontRequest { family, .. } if family == "Roboto")));
    serve(&mut c, &events);
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::MissingFonts { list } if list.is_empty())));
    c.command(Command::Execute {
        command: "edit.undo".into(),
    });
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::MissingFonts { list } if list.len() == 1)));
}

#[test]
fn catalog_and_logs_and_limits() {
    let mut c = Core::new(Platform::Mac, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    c.command(Command::FontsAvailable {
        source: FontSource::Bundled,
        families: manifest(),
    });
    let events = c.take_events();
    let catalog = events.iter().find_map(|e| match e {
        CoreEvent::FontCatalog { families } => Some(families.clone()),
        _ => None,
    });
    let catalog = catalog.expect("каталог");
    assert!(catalog
        .iter()
        .any(|f| f.family == "Inter" && f.styles.contains(&"Bold".to_string())));
    assert!(catalog.iter().any(|f| f.family == "Onest"));
    // Неразборчивые байты — сообщение в журнал, не уведомление.
    c.command(Command::LoadFont {
        family: "Inter".into(),
        file: FILES[0].into(),
        bytes: Some(ByteBuf::from(b"garbage".to_vec())),
    });
    let events = c.take_events();
    assert!(events.iter().any(|e| matches!(e, CoreEvent::Log { .. })));
    assert!(!events.iter().any(|e| matches!(e, CoreEvent::Error { .. })));
    // Пределы: длинное имя семейства, длинный текст.
    c.command(Command::FontsAvailable {
        source: FontSource::System,
        families: vec![FontFamilyDto {
            family: "x".repeat(300),
            faces: vec![],
        }],
    });
    c.command(Command::TestCreateText {
        x: 0.0,
        y: 0.0,
        text: "я".repeat(40_000),
        font: None,
        size: None,
        sizing: None,
        width: None,
        height: None,
    });
    let errors = c
        .take_events()
        .into_iter()
        .filter(|e| matches!(e, CoreEvent::Error { .. }))
        .count();
    assert_eq!(errors, 2);
}

#[test]
fn demo_with_ten_thousand_texts_opens_without_layout() {
    let mut c = core();
    let t = std::time::Instant::now();
    c.command(Command::DemoText { texts: 10_000 });
    let elapsed = t.elapsed();
    let events = c.take_events();
    let opened = events.iter().find_map(|e| match e {
        CoreEvent::Opened { nodes, .. } => Some(*nodes),
        _ => None,
    });
    assert_eq!(opened, Some(10_000 + 100));
    // Шрифты страницы запрошены сразу (а не при первом рисовании), и один раз.
    let requests = events
        .iter()
        .filter(|e| matches!(e, CoreEvent::FontRequest { family, .. } if family == "Inter"))
        .count();
    assert_eq!(requests, 1);
    eprintln!("пример из 10 000 текстов: {elapsed:?}");
}

#[test]
fn font_list_limits_are_errors_and_catalog_stays_unchanged() {
    let mut c = Core::new(Platform::Mac, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    let face = |file: String| FontFaceDto {
        style: "Regular".into(),
        weight: None,
        italic: None,
        stretch: None,
        file,
        index: None,
    };
    let family = |faces| FontFamilyDto {
        family: "Long".into(),
        faces,
    };
    // Путь к файлу — не больше 4096 байт, начертаний у семейства — не больше 1000.
    for list in [
        vec![family(vec![face("f".repeat(MAX_TEXT_BYTES + 1))])],
        vec![family(
            (0..=MAX_FONT_FACES)
                .map(|i| face(format!("f{i}")))
                .collect(),
        )],
    ] {
        c.command(Command::FontsAvailable {
            source: FontSource::System,
            families: list,
        });
    }
    let events = c.take_events();
    let errors = events
        .iter()
        .filter(|e| matches!(e, CoreEvent::Error { .. }))
        .count();
    assert_eq!(errors, 2);
    assert!(!events
        .iter()
        .any(|e| matches!(e, CoreEvent::FontCatalog { families } if families.iter().any(|f| f.family == "Long"))));
    // Предельные значения принимаются.
    c.command(Command::FontsAvailable {
        source: FontSource::System,
        families: vec![family(vec![face("f".repeat(MAX_TEXT_BYTES))])],
    });
    let events = c.take_events();
    assert!(!events.iter().any(|e| matches!(e, CoreEvent::Error { .. })));
}

#[test]
fn document_refusal_is_reported_and_not_queued() {
    // Файл «из будущего» открывается только для чтения: правка текста — отказ
    // документа, а не ожидание шрифтов.
    let bytes = ferrum_doc::container::encode(&ferrum_doc::Document::new()).unwrap();
    let future =
        ferrum_doc::container::testing::with_schema_version(&bytes, ferrum_doc::SCHEMA_VERSION + 1);
    let mut c = core();
    c.command(Command::Open {
        bytes: ByteBuf::from(future),
    });
    c.take_events();
    // Inter нужен для раскладки, чтобы правка дошла до документа.
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    serve(&mut c, &events);
    let events = c.take_events();
    let messages: Vec<&String> = events
        .iter()
        .filter_map(|e| match e {
            CoreEvent::Error { message } => Some(message),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 1, "{events:?}");
    assert!(messages[0].contains("только для чтения"), "{messages:?}");
    assert_eq!(nodes(&c), 0);
    // Очередь пуста: следующее изменение реестра не повторяет правку.
    c.load_font_for_tests("Inter", FILES[0], font_file(FILES[0]));
    assert!(!c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
}
