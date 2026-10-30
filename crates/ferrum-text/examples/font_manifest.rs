//! Манифест вшитых шрифтов (спека подсистемы 3, раздел 4): семейства и
//! начертания из файлов `apps/web/public/fonts/**.ttf` — теми же функциями, что
//! читают системные шрифты. Запуск после `node apps/web/scripts/fetch-fonts.mjs`:
//! `cargo run -p ferrum-text --example font_manifest` (с `--check` — сверить
//! записанный манифест с файлами, код выхода 1 при расхождении).

use std::path::{Path, PathBuf};

use ferrum_text::{group, read_faces, FamilyInfo};

fn ttf_files(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            ttf_files(&p, root, out);
        } else if p.extension().is_some_and(|x| x == "ttf") {
            out.push(p.strip_prefix(root).unwrap().to_path_buf());
        }
    }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/public/fonts");
    let mut files = Vec::new();
    ttf_files(&root, &root, &mut files);
    let mut faces = Vec::new();
    for rel in &files {
        let name = rel.to_string_lossy().replace('\\', "/");
        let bytes = std::fs::read(root.join(rel)).unwrap();
        let found = read_faces(&bytes, &name);
        assert!(!found.is_empty(), "{name}: не шрифт");
        faces.extend(found);
    }
    let mut families: Vec<FamilyInfo> = group(faces);
    families.sort_by(|a, b| a.family.cmp(&b.family));
    let manifest = serde_json::json!({
        "source": "https://github.com/google/fonts (коммит в apps/web/scripts/fetch-fonts.mjs)",
        "license": "SIL Open Font License 1.1 (OFL.txt в каталоге семейства)",
        "families": families,
    });
    let text = serde_json::to_string_pretty(&manifest).unwrap() + "\n";
    let path = root.join("manifest.json");
    if std::env::args().any(|a| a == "--check") {
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        if old != text {
            eprintln!("{} устарел: запустите без --check", path.display());
            std::process::exit(1);
        }
        return;
    }
    std::fs::write(&path, text).unwrap();
    println!("{}: {} семейств", path.display(), families.len());
}
