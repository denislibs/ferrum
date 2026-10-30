//! Системные шрифты десктопа (спека подсистемы 3, раздел 4): каталоги шрифтов
//! ОС сканируются в Rust, метаданные — `skrifa` (`ferrum-fontmeta`, как у
//! манифеста вшитых), байты — сырым ответом IPC. Интерфейс путей не видит: файл
//! начертания — номер файла в списке этого процесса.

use std::path::{Path, PathBuf};

use ferrum_fontmeta::{group, read_faces, FamilyInfo, MAX_FONT_FILE};

/// Наибольшее число файлов шрифтов при сканировании (страховка от огромных каталогов).
pub const MAX_FONT_FILES: usize = 20_000;

/// Наибольшая вложенность каталогов при сканировании.
const MAX_DEPTH: usize = 16;

/// Пределы списка для ядра (спека 8) — те же, что `MAX_FONT_FAMILIES`,
/// `MAX_FONT_FACES` протокола и `MAX_FAMILY_NAME` реестра (сверяет
/// `apps/web/unit/limits.test.ts`): список отдаётся уже в пределах.
pub const MAX_FONT_FAMILIES: usize = 4096;
pub const MAX_FONT_FACES: usize = 1000;
pub const MAX_FAMILY_NAME: usize = 256;

fn name_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_FAMILY_NAME
}

/// Семейства в пределах ядра: неверные имена пропускаются, лишние начертания и
/// семейства отбрасываются; что пропущено — в `skipped` (журнал ошибок).
fn bound(families: Vec<FamilyInfo>, skipped: &mut Vec<String>) -> Vec<FamilyInfo> {
    let total = families.len();
    let mut out = Vec::with_capacity(total.min(MAX_FONT_FAMILIES));
    for mut f in families {
        if !name_ok(&f.family) {
            skipped.push(format!(
                "Шрифт пропущен: имя семейства длиннее {MAX_FAMILY_NAME} байт"
            ));
            continue;
        }
        let before = f.faces.len();
        f.faces.retain(|x| name_ok(&x.style));
        if f.faces.len() < before {
            skipped.push(format!(
                "Шрифт «{}»: пропущено начертаний с неверным именем: {}",
                f.family,
                before - f.faces.len()
            ));
        }
        if f.faces.len() > MAX_FONT_FACES {
            skipped.push(format!(
                "Шрифт «{}»: начертаний {}, взяты первые {MAX_FONT_FACES}",
                f.family,
                f.faces.len()
            ));
            f.faces.truncate(MAX_FONT_FACES);
        }
        if f.faces.is_empty() {
            continue;
        }
        if out.len() == MAX_FONT_FAMILIES {
            skipped.push(format!(
                "Шрифтов {total} семейств, взяты первые {MAX_FONT_FAMILIES}"
            ));
            break;
        }
        out.push(f);
    }
    out
}

/// Список системных шрифтов: файлы и семейства (номер файла — `FaceInfo::file`).
#[derive(Default)]
pub struct SystemFonts {
    files: Vec<PathBuf>,
    pub families: Vec<FamilyInfo>,
    /// Пропущенные файлы (неразборчивые, больше 64 МБ) — для журнала ошибок.
    pub skipped: Vec<String>,
}

/// Каталоги шрифтов ОС.
pub fn font_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs = Vec::new();
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(h) = &home {
            dirs.push(h.join("Library/Fonts"));
        }
    } else if cfg!(target_os = "windows") {
        if let Some(w) = std::env::var_os("WINDIR") {
            dirs.push(PathBuf::from(w).join("Fonts"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(l).join("Microsoft/Windows/Fonts"));
        }
    } else {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(h) = &home {
            dirs.push(h.join(".local/share/fonts"));
            dirs.push(h.join(".fonts"));
        }
    }
    dirs
}

/// Ссылки на каталоги не обходятся: петля ссылок иначе — бесконечная рекурсия.
fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
    entries.sort();
    for p in entries {
        if out.len() >= MAX_FONT_FILES {
            return;
        }
        let link = p
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink());
        if p.is_dir() {
            if !link {
                collect(&p, depth + 1, out);
            }
        } else if p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "otc"
            )
        }) {
            out.push(p);
        }
    }
}

impl SystemFonts {
    /// Сканирует каталоги: каждый файл читается один раз ради метаданных.
    pub fn scan(dirs: &[PathBuf]) -> SystemFonts {
        let mut paths = Vec::new();
        for d in dirs {
            collect(d, 0, &mut paths);
        }
        let mut out = SystemFonts::default();
        let mut faces = Vec::new();
        for path in paths {
            let shown = path.display().to_string();
            match std::fs::metadata(&path) {
                Ok(m) if m.len() as usize > MAX_FONT_FILE => {
                    out.skipped
                        .push(format!("Шрифт пропущен: {shown} больше 64 МБ"));
                    continue;
                }
                Err(_) => continue,
                _ => {}
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let id = out.files.len().to_string();
            let found = read_faces(&bytes, &id);
            if found.is_empty() {
                out.skipped
                    .push(format!("Шрифт пропущен: {shown} не читается"));
                continue;
            }
            out.files.push(path);
            faces.extend(found);
        }
        out.families = bound(group(faces), &mut out.skipped);
        out
    }

    /// Байты файла по номеру из списка.
    pub fn read(&self, id: usize) -> Result<Vec<u8>, String> {
        let path = self.files.get(id).ok_or("Неверный номер файла шрифта")?;
        std::fs::read(path).map_err(|e| format!("Не удалось прочитать шрифт: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_fonts_skips_garbage_and_reads_by_id() {
        let dir = std::env::temp_dir().join(format!("ferrum-fonts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/public/fonts");
        std::fs::copy(web.join("ptsans/PTSans-Regular.ttf"), dir.join("a.ttf")).unwrap();
        std::fs::copy(web.join("ptsans/PTSans-Bold.ttf"), dir.join("nested/b.TTF")).unwrap();
        std::fs::write(dir.join("broken.otf"), b"garbage").unwrap();
        std::fs::write(dir.join("readme.txt"), b"not a font").unwrap();
        let fonts = SystemFonts::scan(std::slice::from_ref(&dir));
        assert_eq!(fonts.families.len(), 1);
        assert_eq!(fonts.families[0].family, "PT Sans");
        let styles: Vec<&str> = fonts.families[0]
            .faces
            .iter()
            .map(|f| f.style.as_str())
            .collect();
        assert_eq!(styles, vec!["Regular", "Bold"]);
        assert_eq!(fonts.skipped.len(), 1, "{:?}", fonts.skipped);
        let id: usize = fonts.families[0].faces[1].file.parse().unwrap();
        assert_eq!(
            fonts.read(id).unwrap().len(),
            std::fs::metadata(dir.join("nested/b.TTF")).unwrap().len() as usize
        );
        assert!(fonts.read(99).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn family(name: String, faces: usize) -> FamilyInfo {
        FamilyInfo {
            family: name,
            faces: (0..faces)
                .map(|i| ferrum_fontmeta::FaceInfo {
                    style: format!("S{i}"),
                    weight: 400.0,
                    italic: false,
                    stretch: 1.0,
                    file: i.to_string(),
                    index: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn list_is_bounded_and_bad_names_skipped() {
        let mut list = vec![
            family("x".repeat(MAX_FAMILY_NAME + 1), 1),
            family("Many".into(), MAX_FONT_FACES + 5),
        ];
        list[1].faces[0].style = "y".repeat(MAX_FAMILY_NAME + 1);
        list.extend((0..MAX_FONT_FAMILIES + 3).map(|i| family(format!("F{i}"), 1)));
        let mut skipped = Vec::new();
        let out = bound(list, &mut skipped);
        assert_eq!(out.len(), MAX_FONT_FAMILIES);
        assert_eq!(out[0].family, "Many");
        assert_eq!(out[0].faces.len(), MAX_FONT_FACES);
        assert_eq!(out[0].faces[0].style, "S1", "неверное начертание пропущено");
        // Длинное имя, неверное начертание, лишние начертания, лишние семейства.
        assert_eq!(skipped.len(), 4, "{skipped:?}");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_loop_does_not_recurse() {
        let dir = std::env::temp_dir().join(format!("ferrum-fonts-loop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::os::unix::fs::symlink(&dir, dir.join("sub/loop")).unwrap();
        let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/public/fonts");
        std::fs::copy(web.join("ptsans/PTSans-Regular.ttf"), dir.join("sub/a.ttf")).unwrap();
        let fonts = SystemFonts::scan(std::slice::from_ref(&dir));
        assert_eq!(fonts.families.len(), 1);
        assert_eq!(fonts.families[0].faces.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
