//! Метаданные файлов шрифтов на вшитых файлах (`apps/web/public/fonts`):
//! семейство и начертания вариативного шрифта — именованные экземпляры, у
//! статического — подсемейство; неразборчивый файл — пустой список.

use std::path::PathBuf;

use ferrum_fontmeta::{group, read_faces};

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn variable_and_static_faces_are_named_like_figma() {
    let faces = group(read_faces(&font_file("inter/Inter-Variable.ttf"), "x.ttf"));
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].family, "Inter");
    let styles: Vec<&str> = faces[0].faces.iter().map(|f| f.style.as_str()).collect();
    assert!(
        styles.contains(&"Regular") && styles.contains(&"Bold"),
        "{styles:?}"
    );
    let bold = faces[0].faces.iter().find(|f| f.style == "Bold").unwrap();
    assert_eq!(
        (bold.weight, bold.italic, bold.file.as_str()),
        (700.0, false, "x.ttf")
    );
    let static_faces = group(read_faces(
        &font_file("ptsans/PTSans-BoldItalic.ttf"),
        "y.ttf",
    ));
    assert_eq!(static_faces[0].family, "PT Sans");
    assert_eq!(static_faces[0].faces[0].style, "Bold Italic");
    assert!(static_faces[0].faces[0].italic);
    // Мусор — пустой список, не паника.
    assert!(read_faces(b"not a font", "z.ttf").is_empty());
}
