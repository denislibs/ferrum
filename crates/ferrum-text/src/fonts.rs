//! Реестр шрифтов ядра (спека подсистемы 3, раздел 4): семейство → начертания →
//! источник → загруженные байты. Шрифты регистрируются в собственной коллекции
//! `fontique` без системного поиска (в wasm его нет), байты живут в памяти
//! (`Blob` — `Arc`) с пределом `MAX_FONT_MEMORY` и вытеснением давно не
//! использованных семейств, которых нет на текущей странице.
//!
//! Шрифт документа (`FontName`) разрешается в одно из трёх: готов (файл
//! загружен), грузится (файл запрошен — `take_requests`), отсутствует (семейства
//! или начертания нет ни во вшитых, ни в системных) — тогда раскладка берёт
//! Inter того же начертания, иначе Inter Regular, а имя в документе не меняется.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use ferrum_doc::FontName;
use ferrum_fontmeta::{read_faces, FaceInfo, FamilyInfo, MAX_FONT_FILE};
use kurbo::BezPath;
use parley::fontique::{
    Blob, Collection, CollectionOptions, FamilyId, FontStyle, FontWeight, FontWidth, SourceCache,
};
use parley::{FontContext, LayoutContext};
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::style_name::{guess_attrs, style_key};
use crate::ONEST;

/// Имя файла вшитого Onest в реестре.
const ONEST_FILE: &str = "Onest-wght.ttf";

/// Предел памяти загруженных шрифтов (спека 4).
pub const MAX_FONT_MEMORY: usize = 256 * 1024 * 1024;
/// Предел семейств в реестре (спека 4).
pub const MAX_FAMILIES: usize = 4096;
/// Предел имени семейства в байтах (спека 8).
pub const MAX_FAMILY_NAME: usize = 256;
/// Запасное семейство для отсутствующих шрифтов.
pub const FALLBACK_FAMILY: &str = "Inter";
/// Сколько контуров глифов держать (в единицах шрифта); сверх — кэш сбрасывается.
const OUTLINE_CACHE: usize = 65_536;

/// Откуда семейство. Порядок — приоритет: вшитое не подменяется системным с
/// тем же именем (текст выглядит одинаково у всех).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceKind {
    System,
    Bundled,
    Embedded,
}

/// Разрешение шрифта документа.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStatus {
    /// Файл загружен — можно раскладывать.
    Ready,
    /// Файл запрошен, байтов ещё нет.
    Loading,
    /// Ни семейства, ни начертания нет — раскладывается запасным.
    Missing,
    /// Список вшитых ещё не пришёл: отсутствие не проверить.
    Pending,
}

/// Запрос байтов семейства: worker ядра качает вшитые файлы сам, системные
/// просит у основного потока (`FontRequest` протокола).
#[derive(Debug, Clone, PartialEq)]
pub struct FontRequest {
    pub family: String,
    pub source: SourceKind,
    pub files: Vec<String>,
}

/// Чем раскладывать шрифт документа.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub status: FontStatus,
    /// Имя семейства в коллекции `fontique` (из файла шрифта).
    pub family: String,
    pub weight: f32,
    pub italic: bool,
    pub stretch: f32,
    /// Шрифт подменён запасным (отсутствует).
    pub substituted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileState {
    Idle,
    Requested,
    Loaded,
    Failed,
}

struct FileSlot {
    state: FileState,
    bytes: usize,
    /// Что зарегистрировано в коллекции — чтобы снять при вытеснении.
    registered: Vec<(FamilyId, FontWidth, FontStyle, FontWeight)>,
    /// Имя семейства в коллекции (по таблице `name` файла).
    collection_name: Option<String>,
}

struct Family {
    name: String,
    source: SourceKind,
    faces: Vec<FaceInfo>,
    files: BTreeMap<String, FileSlot>,
    last_used: u64,
}

type OutlineKey = (u64, u32, u32, Vec<i16>);

/// Один реестр на ядро: им раскладывает редактор (размер текста в транзакции
/// правки) и рисует рендер. Ядро однопоточное — `Rc<RefCell>`.
pub type SharedFonts = Rc<RefCell<Fonts>>;

/// Реестр, контексты `parley` и кэш контуров глифов.
pub struct Fonts {
    pub(crate) fcx: FontContext,
    pub(crate) lcx: LayoutContext<u32>,
    families: HashMap<String, Family>,
    bundled_ready: bool,
    generation: u64,
    requests: Vec<FontRequest>,
    memory: usize,
    limit: usize,
    tick: u64,
    in_use: HashSet<String>,
    outlines: HashMap<OutlineKey, Arc<BezPath>>,
    upem: HashMap<(u64, u32), f64>,
    errors: Vec<String>,
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::new()
    }
}

fn family_key(name: &str) -> String {
    name.to_lowercase()
}

impl Fonts {
    /// Реестр с вшитым Onest.
    pub fn new() -> Fonts {
        let mut fonts = Fonts {
            fcx: FontContext {
                collection: Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: false,
                }),
                source_cache: SourceCache::default(),
            },
            lcx: LayoutContext::new(),
            families: HashMap::new(),
            bundled_ready: false,
            generation: 0,
            requests: Vec::new(),
            memory: 0,
            limit: MAX_FONT_MEMORY,
            tick: 0,
            in_use: HashSet::new(),
            outlines: HashMap::new(),
            upem: HashMap::new(),
            errors: Vec::new(),
        };
        let embedded = ferrum_fontmeta::group(read_faces(ONEST, ONEST_FILE));
        fonts.add_catalog(SourceKind::Embedded, embedded);
        // Байты уже в памяти wasm: регистрация без запроса.
        let _ = fonts.register("Onest", ONEST_FILE, Blob::new(Arc::new(ONEST)));
        fonts
    }

    /// Номер состояния реестра: растёт при каждом изменении, от которого зависит
    /// раскладка (пришёл список шрифтов, загружен или вытеснен файл). Кэш
    /// раскладок сверяет его.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Пришёл ли список вшитых шрифтов (до него отсутствие не определить).
    pub fn bundled_ready(&self) -> bool {
        self.bundled_ready
    }

    /// Список семейств источника: манифест вшитых или системные шрифты. Семейство,
    /// уже известное из источника не ниже, не заменяется; лишние сверх
    /// `MAX_FAMILIES` и с длинным именем — пропуск с сообщением в журнале ошибок.
    pub fn add_catalog(&mut self, source: SourceKind, list: Vec<FamilyInfo>) {
        if source == SourceKind::Bundled {
            self.bundled_ready = true;
        }
        for info in list {
            if info.family.is_empty()
                || info.family.len() > MAX_FAMILY_NAME
                || info.faces.is_empty()
            {
                self.errors.push(format!(
                    "Шрифт пропущен: неверное имя семейства «{}»",
                    clip(&info.family)
                ));
                continue;
            }
            let key = family_key(&info.family);
            if let Some(old) = self.families.get(&key) {
                if old.source >= source {
                    continue;
                }
            } else if self.families.len() >= MAX_FAMILIES {
                self.errors.push(format!(
                    "Шрифт «{}» пропущен: семейств больше {MAX_FAMILIES}",
                    clip(&info.family)
                ));
                continue;
            }
            let files = info
                .faces
                .iter()
                .map(|f| {
                    (
                        f.file.clone(),
                        FileSlot {
                            state: FileState::Idle,
                            bytes: 0,
                            registered: Vec::new(),
                            collection_name: None,
                        },
                    )
                })
                .collect();
            self.families.insert(
                key,
                Family {
                    name: info.family,
                    source,
                    faces: info.faces,
                    files,
                    last_used: 0,
                },
            );
        }
        self.generation += 1;
    }

    /// Семейства и их начертания по имени, с источником.
    pub fn families(&self) -> Vec<(String, SourceKind, Vec<String>)> {
        let mut out: Vec<_> = self
            .families
            .values()
            .map(|f| {
                (
                    f.name.clone(),
                    f.source,
                    f.faces.iter().map(|x| x.style.clone()).collect(),
                )
            })
            .collect();
        out.sort();
        out
    }

    /// Семейства, нужные текущей странице: их не вытесняют.
    pub fn set_in_use(&mut self, families: impl IntoIterator<Item = String>) {
        self.in_use = families.into_iter().map(|f| family_key(&f)).collect();
    }

    /// Запросы байтов с прошлого вызова.
    pub fn take_requests(&mut self) -> Vec<FontRequest> {
        std::mem::take(&mut self.requests)
    }

    /// Сообщения для журнала ошибок (неразборчивые файлы, пропуски) с прошлого вызова.
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }

    /// Состояние шрифта документа без запроса байтов.
    pub fn status(&self, font: &FontName) -> FontStatus {
        match self.face(font) {
            Some((fam, face)) => match fam.files.get(&face.file).map(|f| f.state) {
                Some(FileState::Loaded) => FontStatus::Ready,
                Some(FileState::Failed) => FontStatus::Missing,
                _ => FontStatus::Loading,
            },
            None if !self.bundled_ready => FontStatus::Pending,
            None => FontStatus::Missing,
        }
    }

    fn face(&self, font: &FontName) -> Option<(&Family, &FaceInfo)> {
        let fam = self.families.get(&family_key(&font.family))?;
        let key = style_key(&font.style);
        let face = fam.faces.iter().find(|f| style_key(&f.style) == key)?;
        Some((fam, face))
    }

    /// Чем раскладывать шрифт: сам шрифт, если он есть (незагруженный — запрос
    /// байтов, `Loading`), иначе запасной Inter. Отметка использования — для
    /// вытеснения.
    pub fn resolve(&mut self, font: &FontName) -> Resolved {
        match self.status(font) {
            FontStatus::Missing => {
                let mut r = self.resolve_fallback(&font.style);
                r.substituted = true;
                r
            }
            FontStatus::Pending => Resolved {
                status: FontStatus::Pending,
                family: font.family.clone(),
                weight: 400.0,
                italic: false,
                stretch: 1.0,
                substituted: false,
            },
            _ => self.resolve_known(font),
        }
    }

    fn resolve_fallback(&mut self, style: &str) -> Resolved {
        let same = FontName::new(FALLBACK_FAMILY, style);
        let font = if self.face(&same).is_some() {
            same
        } else {
            FontName::new(FALLBACK_FAMILY, "Regular")
        };
        if self.face(&font).is_some() {
            return self.resolve_known(&font);
        }
        // Inter нет в списке (сборка без вшитых): начертание угадывается по имени.
        let (weight, italic) = guess_attrs(style);
        Resolved {
            status: FontStatus::Missing,
            family: FALLBACK_FAMILY.to_string(),
            weight,
            italic,
            stretch: 1.0,
            substituted: true,
        }
    }

    fn resolve_known(&mut self, font: &FontName) -> Resolved {
        self.tick += 1;
        let tick = self.tick;
        let key = family_key(&font.family);
        let style = style_key(&font.style);
        let Some(fam) = self.families.get_mut(&key) else {
            unreachable!("семейство проверено в status")
        };
        fam.last_used = tick;
        let face = fam
            .faces
            .iter()
            .find(|f| style_key(&f.style) == style)
            .cloned()
            .expect("начертание проверено в status");
        let slot = fam
            .files
            .get_mut(&face.file)
            .expect("файл из списка начертаний");
        let status = match slot.state {
            FileState::Loaded => FontStatus::Ready,
            FileState::Requested => FontStatus::Loading,
            FileState::Failed => FontStatus::Missing,
            FileState::Idle => {
                // Запрос — всем незагруженным файлам семейства сразу: курсив и
                // прямое начертание обычно нужны вместе.
                let files: Vec<String> = fam
                    .files
                    .iter_mut()
                    .filter(|(_, s)| s.state == FileState::Idle)
                    .map(|(name, s)| {
                        s.state = FileState::Requested;
                        name.clone()
                    })
                    .collect();
                self.requests.push(FontRequest {
                    family: fam.name.clone(),
                    source: fam.source,
                    files,
                });
                FontStatus::Loading
            }
        };
        let slot = &fam.files[&face.file];
        Resolved {
            status,
            family: slot
                .collection_name
                .clone()
                .unwrap_or_else(|| fam.name.clone()),
            weight: face.weight,
            italic: face.italic,
            stretch: face.stretch,
            substituted: false,
        }
    }

    /// Байты файла семейства (ответ на запрос). Ошибка — текст для журнала:
    /// файл не из списка, больше `MAX_FONT_FILE`, неразборчивый.
    pub fn load(&mut self, family: &str, file: &str, bytes: Vec<u8>) -> Result<(), String> {
        if bytes.len() > MAX_FONT_FILE {
            self.fail(family, file);
            return Err(format!(
                "Шрифт «{}» не загружен: файл больше 64 МБ",
                clip(family)
            ));
        }
        let result = self.register(family, file, Blob::new(Arc::new(bytes)));
        if result.is_err() {
            self.fail(family, file);
        }
        self.evict();
        result
    }

    /// Файл не удалось получить: шрифт считается отсутствующим.
    pub fn load_failed(&mut self, family: &str, file: &str) {
        self.fail(family, file);
    }

    fn fail(&mut self, family: &str, file: &str) {
        if let Some(slot) = self
            .families
            .get_mut(&family_key(family))
            .and_then(|f| f.files.get_mut(file))
        {
            slot.state = FileState::Failed;
            self.generation += 1;
        }
    }

    fn register(&mut self, family: &str, file: &str, blob: Blob<u8>) -> Result<(), String> {
        let key = family_key(family);
        let Some(fam) = self.families.get_mut(&key) else {
            return Err(format!(
                "Шрифт «{}» не загружен: нет в списке шрифтов",
                clip(family)
            ));
        };
        let Some(slot) = fam.files.get_mut(file) else {
            return Err(format!(
                "Шрифт «{}» не загружен: файла «{}» нет в списке",
                clip(family),
                clip(file)
            ));
        };
        if slot.state == FileState::Loaded {
            return Ok(());
        }
        let len = blob.as_ref().len();
        let added = self.fcx.collection.register_fonts(blob, None);
        if added.is_empty() {
            return Err(format!(
                "Шрифт «{}» не загружен: файл «{}» не читается",
                clip(family),
                clip(file)
            ));
        }
        slot.collection_name = added
            .first()
            .and_then(|(id, _)| self.fcx.collection.family_name(*id))
            .map(str::to_string);
        slot.registered = added
            .iter()
            .flat_map(|(id, fonts)| {
                fonts
                    .iter()
                    .map(move |f| (*id, f.width(), f.style(), f.weight()))
            })
            .collect();
        slot.state = FileState::Loaded;
        slot.bytes = len;
        self.memory += len;
        self.generation += 1;
        Ok(())
    }

    /// Вытеснение (спека 4): пока память шрифтов больше предела — семейства, не
    /// используемые на странице, начиная с давно не нужных. Вшитый Onest не
    /// вытесняется (его байты всё равно в wasm).
    fn evict(&mut self) {
        while self.memory > self.limit {
            let victim = self
                .families
                .iter()
                .filter(|(k, f)| {
                    f.source != SourceKind::Embedded
                        && !self.in_use.contains(*k)
                        && f.files.values().any(|s| s.state == FileState::Loaded)
                })
                .min_by_key(|(_, f)| f.last_used)
                .map(|(k, _)| k.clone());
            let Some(key) = victim else { break };
            let fam = self.families.get_mut(&key).expect("найдено выше");
            for slot in fam.files.values_mut() {
                if slot.state != FileState::Loaded {
                    continue;
                }
                for (id, w, s, wt) in slot.registered.drain(..) {
                    self.fcx.collection.unregister_font(id, w, s, wt);
                }
                self.memory -= slot.bytes;
                slot.bytes = 0;
                slot.state = FileState::Idle;
            }
            self.outlines.clear();
            self.generation += 1;
        }
    }

    /// Другой предел памяти — чтобы тест вытеснения не грузил 256 МБ шрифтов.
    #[doc(hidden)]
    pub fn set_memory_limit_for_tests(&mut self, bytes: usize) {
        self.limit = bytes;
        self.evict();
    }

    /// Занято байтами загруженных шрифтов.
    pub fn memory(&self) -> usize {
        self.memory
    }

    /// Новый общий реестр.
    pub fn shared() -> SharedFonts {
        Rc::new(RefCell::new(Fonts::new()))
    }

    /// Единиц на em шрифта, из кэша.
    pub fn units_per_em(&mut self, font: &parley::FontData) -> f64 {
        *self
            .upem
            .entry((font.data.id(), font.index))
            .or_insert_with(|| font_units_per_em(font))
    }

    /// Контур глифа в единицах шрифта (ось Y вверх), из кэша.
    pub fn outline(&mut self, font: &parley::FontData, glyph: u32, coords: &[i16]) -> Arc<BezPath> {
        let key = (font.data.id(), font.index, glyph, coords.to_vec());
        if let Some(p) = self.outlines.get(&key) {
            return p.clone();
        }
        if self.outlines.len() >= OUTLINE_CACHE {
            self.outlines.clear();
        }
        let path = Arc::new(glyph_outline(font, glyph, coords));
        self.outlines.insert(key, path.clone());
        path
    }
}

/// Контур глифа; повреждённый — пустой (кадр не роняется).
pub fn glyph_outline(font: &parley::FontData, glyph: u32, coords: &[i16]) -> BezPath {
    let mut pen = Pen(BezPath::new());
    let Ok(font_ref) = FontRef::from_index(font.data.as_ref(), font.index) else {
        return pen.0;
    };
    let location: Vec<NormalizedCoord> = coords
        .iter()
        .map(|c| NormalizedCoord::from_bits(*c))
        .collect();
    if let Some(outline) = font_ref.outline_glyphs().get(GlyphId::new(glyph)) {
        let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::new(&location));
        if outline.draw(settings, &mut pen).is_err() {
            pen.0 = BezPath::new();
        }
    }
    pen.0
}

/// Единиц на em шрифта.
pub fn font_units_per_em(font: &parley::FontData) -> f64 {
    FontRef::from_index(font.data.as_ref(), font.index)
        .map(|f| {
            f.metrics(Size::unscaled(), LocationRef::default())
                .units_per_em
                .max(1) as f64
        })
        .unwrap_or(1000.0)
}

fn clip(s: &str) -> String {
    let mut it = s.chars();
    let head: String = it.by_ref().take(64).collect();
    if it.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

struct Pen(BezPath);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.quad_to((cx as f64, cy as f64), (x as f64, y as f64));
    }
    fn curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        self.0.curve_to(
            (c1x as f64, c1y as f64),
            (c2x as f64, c2y as f64),
            (x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
