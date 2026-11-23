//! The fonts installed on the machine, so the font list can offer them and cells can render with
//! them (#4).
//!
//! egui has no font discovery of its own and panics when asked for a family that was never bound
//! to font data. So we do both halves: scan the platform's font folders for family names (a read
//! of each font file's `name` table — damaged or unreadable files yield nothing, never a panic),
//! and bind the files a workbook actually uses into the families
//! [`theme::font_definitions`](crate::theme::font_definitions) assembles.
//!
//! Native only: on wasm there are no system fonts, every function here reports "nothing", and the
//! font list is the standard [`crate::ribbon::FONTS`] set.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

/// A font file is skipped by the scan past this size: a catalog only needs each face's name, and a
/// damaged or enormous file must not turn startup into a long read.
#[cfg(not(target_arch = "wasm32"))]
const MAX_FONT_FILE: u64 = 64 << 20;

/// One face of one installed font file.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// Typographic family name, e.g. `"Segoe UI"`.
    pub family: String,
    /// The file the face lives in.
    pub path: PathBuf,
    /// Face index within the file (collections hold several, e.g. a family's Regular and Bold).
    pub index: u32,
    /// Weight implied by the face's style name (400 = regular).
    pub weight: f32,
    /// Whether the style name reads as italic or oblique.
    pub italic: bool,
}

/// The font folders of the platform (the system's and the user's). Empty on wasm.
pub fn system_font_dirs() -> Vec<PathBuf> {
    #[cfg(target_arch = "wasm32")]
    {
        Vec::new()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
        if cfg!(target_os = "macos") {
            dirs.extend(["/System/Library/Fonts", "/System/Library/Fonts/Supplemental", "/Library/Fonts"].map(Into::into));
            if let Some(h) = &home {
                dirs.push(h.join("Library/Fonts"));
            }
        } else if cfg!(windows) {
            let root = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
            dirs.push(root.join("Fonts"));
            if let Some(l) = std::env::var_os("LOCALAPPDATA") {
                dirs.push(PathBuf::from(l).join("Microsoft\\Windows\\Fonts"));
            }
        } else {
            dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts", "/usr/share/fonts/TTF"].map(Into::into));
            if let Some(h) = &home {
                dirs.push(h.join(".fonts"));
                dirs.push(h.join(".local/share/fonts"));
            }
        }
        dirs
    }
}

/// Weight implied by a style name (`"Bold"` → 700, `"Light"` → 300).
#[cfg(not(target_arch = "wasm32"))]
fn style_weight(style: &str) -> f32 {
    let s: String = style.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect();
    const TABLE: &[(&str, f32)] = &[
        ("hairline", 100.0),
        ("thin", 100.0),
        ("extralight", 200.0),
        ("ultralight", 200.0),
        ("light", 300.0),
        ("medium", 500.0),
        ("demibold", 600.0),
        ("semibold", 600.0),
        ("extrabold", 800.0),
        ("ultrabold", 800.0),
        ("bold", 700.0),
        ("black", 900.0),
        ("heavy", 900.0),
    ];
    TABLE.iter().find(|(k, _)| s.contains(k)).map(|(_, w)| *w).unwrap_or(400.0)
}

/// Whether a style name reads as italic or oblique.
#[cfg(not(target_arch = "wasm32"))]
fn style_italic(style: &str) -> bool {
    let s: String = style.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect();
    s.contains("italic") || s.contains("oblique")
}

/// Every face in `data` as `(index, family, style)`. Damaged data yields nothing, never a panic.
#[cfg(not(target_arch = "wasm32"))]
fn file_faces(data: &[u8]) -> Vec<(u32, String, String)> {
    use skrifa::raw::FileRef;
    use skrifa::{MetadataProvider, string::StringId};
    let count = match FileRef::new(data) {
        Ok(FileRef::Font(_)) => 1,
        Ok(FileRef::Collection(c)) => c.len(),
        Err(_) => return Vec::new(),
    };
    let name = |f: &skrifa::FontRef<'_>, ids: &[StringId]| -> Option<String> {
        ids.iter().find_map(|id| f.localized_strings(*id).english_or_first().map(|s| s.to_string()).filter(|s| !s.is_empty()))
    };
    (0..count)
        .filter_map(|i| {
            let f = skrifa::FontRef::from_index(data, i).ok()?;
            let family = name(&f, &[StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME])?;
            let style = name(&f, &[StringId::TYPOGRAPHIC_SUBFAMILY_NAME, StringId::SUBFAMILY_NAME]).unwrap_or_else(|| "Regular".into());
            Some((i, family, style))
        })
        .collect()
}

/// Catalog every face under `dirs`, recursing into subfolders. A folder reached twice (through a
/// symlink) is read once, so a link back to a parent ends the walk.
#[cfg(not(target_arch = "wasm32"))]
pub fn scan_dirs(dirs: &[PathBuf]) -> Vec<Face> {
    let mut out: Vec<Face> = Vec::new();
    let mut stack: Vec<PathBuf> = dirs.to_vec();
    let mut seen_dirs: HashSet<PathBuf> = HashSet::new();
    while let Some(dir) = stack.pop() {
        let key = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if !seen_dirs.insert(key) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let is_font =
                path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc"));
            if !is_font || std::fs::metadata(&path).map(|m| m.len() > MAX_FONT_FILE).unwrap_or(true) {
                continue;
            }
            let Ok(data) = std::fs::read(&path) else { continue };
            for (index, family, style) in file_faces(&data) {
                out.push(Face { family, path: path.clone(), index, weight: style_weight(&style), italic: style_italic(&style) });
            }
        }
    }
    out
}

#[cfg(target_arch = "wasm32")]
pub fn scan_dirs(_dirs: &[PathBuf]) -> Vec<Face> {
    Vec::new()
}

/// The scanned system faces, the fonts the user added, and the merged list painting consults.
#[derive(Default)]
struct Catalog {
    /// Every face of the platform's font folders; empty until the scan finishes.
    system: Vec<Face>,
    /// The scan has finished (even when it found nothing).
    scanned: bool,
    /// Fonts the user added, in the order added.
    user: Vec<UserFont>,
    /// Where fonts added later are stored ([`load_user_fonts`]).
    dir: Option<PathBuf>,
    /// `system` followed by every user font's faces; rebuilt whenever either changes.
    merged: Arc<Vec<Face>>,
    /// Bumped when `merged` changes, so the UI rebinds and the font list refreshes.
    generation: u64,
}

static CATALOG: OnceLock<RwLock<Catalog>> = OnceLock::new();

fn catalog_lock() -> &'static RwLock<Catalog> {
    CATALOG.get_or_init(|| RwLock::new(Catalog::default()))
}

fn read_catalog() -> std::sync::RwLockReadGuard<'static, Catalog> {
    catalog_lock().read().unwrap_or_else(|e| e.into_inner())
}

fn write_catalog() -> std::sync::RwLockWriteGuard<'static, Catalog> {
    catalog_lock().write().unwrap_or_else(|e| e.into_inner())
}

impl Catalog {
    /// Rebuild `merged` from the system and user faces, and bump the generation.
    fn rebuild(&mut self) {
        let mut faces: Vec<Face> = self.system.clone();
        for u in &self.user {
            faces.extend(u.faces.iter().cloned());
        }
        self.merged = Arc::new(faces);
        self.generation = self.generation.wrapping_add(1);
    }
}

/// The catalog of installed faces (the system's and the user's), scanned on first use. Blocks
/// until the system scan finishes.
pub fn catalog() -> Arc<Vec<Face>> {
    {
        let c = read_catalog();
        if c.scanned {
            return c.merged.clone();
        }
    }
    let faces = scan_dirs(&system_font_dirs());
    let mut c = write_catalog();
    if !c.scanned {
        c.system = faces;
        c.scanned = true;
        c.rebuild();
    }
    c.merged.clone()
}

/// The catalog if the system scan has finished, else `None` — never blocks, for use while painting.
/// Cheap to call: it hands back a shared handle to the merged list.
fn catalog_now() -> Option<Arc<Vec<Face>>> {
    let c = read_catalog();
    c.scanned.then(|| c.merged.clone())
}

/// Start cataloging the installed fonts on a background thread, so the first frame doesn't wait for
/// it. A no-op on wasm and after the scan has started.
pub fn start_scan() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        #[cfg(not(target_arch = "wasm32"))]
        if !read_catalog().scanned {
            // A failed spawn leaves the scan to the first blocking lookup.
            let _ = std::thread::Builder::new().name("font-scan".into()).spawn(|| {
                catalog();
            });
        }
    });
}

/// Whether the background scan has finished.
pub fn scan_done() -> bool {
    read_catalog().scanned
}

/// A counter that changes whenever user fonts are added or removed (by any route), so the UI
/// rebinds the fonts and refreshes the lists when it moves.
pub fn generation() -> u64 {
    read_catalog().generation
}

/// Put the catalog in a known, scanned state without walking the machine's font folders, so the
/// user-font tests are hermetic. Tests hold [`TEST_LOCK`].
#[cfg(test)]
pub(crate) fn reset_for_test(system: Vec<Face>) {
    let mut c = write_catalog();
    c.system = system;
    c.user.clear();
    c.dir = None;
    c.scanned = true;
    c.rebuild();
}

/// The faces of one installed family, regular-weight first (the order roles pick from).
pub fn faces_of(family: &str) -> Vec<Face> {
    let Some(cat) = catalog_now() else { return Vec::new() };
    let mut v: Vec<Face> = cat.iter().filter(|f| f.family.eq_ignore_ascii_case(family)).cloned().collect();
    v.sort_by(|a, b| a.weight.total_cmp(&b.weight).then(a.italic.cmp(&b.italic)).then(a.index.cmp(&b.index)));
    v
}

/// The subset of `names` that is installed, sorted and deduped. These are the families whose files
/// get bound for a workbook.
pub fn installed_among(names: &[String]) -> Vec<String> {
    let Some(cat) = catalog_now() else { return Vec::new() };
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for wanted in names {
        if !seen.insert(wanted.to_lowercase()) {
            continue;
        }
        if let Some(f) = cat.iter().find(|f| f.family.eq_ignore_ascii_case(wanted)) {
            out.push(f.family.clone());
        }
    }
    out.sort_by_key(|a| a.to_lowercase());
    out
}

/// The font list a combo box shows: the standard [`crate::ribbon::FONTS`] names first (what Excel
/// offers, and what we substitute), then every other installed family A–Z. Never blocks: until the
/// background scan finishes it returns just the standard names.
pub fn families() -> Vec<String> {
    let mut out: Vec<String> = crate::ribbon::FONTS.iter().map(|s| s.to_string()).collect();
    let Some(cat) = catalog_now() else { return out };
    let mut seen: HashSet<String> = out.iter().map(|s| s.to_lowercase()).collect();
    let mut installed: Vec<String> = cat.iter().map(|f| f.family.clone()).collect();
    installed.sort_by_key(|a| a.to_lowercase());
    installed.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    for f in installed {
        if seen.insert(f.to_lowercase()) {
            out.push(f);
        }
    }
    out
}

/// Largest font file accepted (TrueType, OpenType or a collection).
pub const MAX_FONT_FILE_BYTES: usize = 50 << 20;
/// Most faces one file may add (collections hold several).
const MAX_FACES_PER_FILE: usize = 256;
/// Most user fonts at once.
const MAX_USER_FONTS: usize = 1000;
/// Extensions a picked or dropped font file may have (fonts are stored unwrapped).
pub const FONT_FILE_EXTS: &[&str] = &["ttf", "otf", "ttc", "otc"];

/// Is `name` (a file name or path) named like a font file?
pub fn is_font_file_name(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    ext.is_some_and(|e| FONT_FILE_EXTS.contains(&e.as_str()))
}

/// The kind of font file `bytes` holds, from its signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFormat {
    TrueType,
    OpenType,
    Collection,
    /// A WOFF file: a web font this reads only to report it can't be used.
    Woff,
    /// A WOFF2 file: as above.
    Woff2,
}

/// The font format `bytes` starts with, or `None` when it isn't a font file.
pub fn sniff(bytes: &[u8]) -> Option<FontFormat> {
    match bytes.get(..4)? {
        [0, 1, 0, 0] | b"true" | b"typ1" => Some(FontFormat::TrueType),
        b"OTTO" => Some(FontFormat::OpenType),
        b"ttcf" => Some(FontFormat::Collection),
        b"wOFF" => Some(FontFormat::Woff),
        b"wOF2" => Some(FontFormat::Woff2),
        _ => None,
    }
}

/// Why a font couldn't be added or removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserFontError {
    Empty,
    TooLarge { bytes: usize },
    /// Not a font file this reads (TrueType, OpenType or a collection).
    NotAFont,
    /// A WOFF/WOFF2 file: web fonts aren't unwrapped here — pick the TTF/OTF instead.
    WebFont,
    /// A font file that doesn't parse, or has no usable face.
    Unusable(String),
    /// Every face in the file is already available.
    AlreadyAvailable(Vec<String>),
    TooMany,
    /// The family isn't one the user added (installed fonts can't be removed).
    NotUserFont(String),
    /// Adding fonts at run time isn't available on this platform.
    Unsupported,
}

impl std::fmt::Display for UserFontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserFontError::Empty => write!(f, "the font file is empty"),
            UserFontError::TooLarge { bytes } => {
                write!(f, "the font file is too large ({} MB; at most {} MB)", bytes.div_ceil(1 << 20), MAX_FONT_FILE_BYTES >> 20)
            }
            UserFontError::NotAFont => write!(f, "not a font file (a TrueType, OpenType, TTC or OTC file)"),
            UserFontError::WebFont => write!(f, "web fonts (WOFF/WOFF2) aren't supported; add the TrueType or OpenType file instead"),
            UserFontError::Unusable(e) => write!(f, "the font couldn't be read: {e}"),
            UserFontError::AlreadyAvailable(families) if families.is_empty() => write!(f, "the font is already available"),
            UserFontError::AlreadyAvailable(families) => write!(f, "already available: {}", families.join(", ")),
            UserFontError::TooMany => write!(f, "too many added fonts (at most {MAX_USER_FONTS}); remove some first"),
            UserFontError::NotUserFont(family) => write!(f, "{family:?} isn't a font you added"),
            UserFontError::Unsupported => write!(f, "adding fonts isn't available here"),
        }
    }
}

impl std::error::Error for UserFontError {}

/// A font file the user added.
#[derive(Clone, Debug, PartialEq)]
pub struct UserFont {
    /// Its file name in the user font folder (unique among the user fonts).
    pub file: String,
    /// The families it added (each once, in file order).
    pub families: Vec<String>,
    /// Where it is stored; `None` when it lasts for the session only.
    pub path: Option<PathBuf>,
    /// The faces it contributed to the catalog.
    faces: Vec<Face>,
}

/// What [`install_font`] did.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub font: UserFont,
    /// The font was added for this session but couldn't be stored for the next one.
    pub warning: Option<String>,
}

/// The font file in `bytes` as plain OpenType (TTF/OTF/TTC) bytes after the size and signature
/// checks. WOFF and WOFF2 are refused: they'd have to be unwrapped first, and this has no decoder.
pub fn decode_font_file(bytes: &[u8]) -> Result<Vec<u8>, UserFontError> {
    if bytes.is_empty() {
        return Err(UserFontError::Empty);
    }
    if bytes.len() > MAX_FONT_FILE_BYTES {
        return Err(UserFontError::TooLarge { bytes: bytes.len() });
    }
    match sniff(bytes) {
        Some(FontFormat::TrueType | FontFormat::OpenType | FontFormat::Collection) => Ok(bytes.to_vec()),
        Some(FontFormat::Woff | FontFormat::Woff2) => Err(UserFontError::WebFont),
        None => Err(UserFontError::NotAFont),
    }
}

/// Read a font file from `path`, refusing one larger than the cap before reading it.
pub fn read_font_file(path: &str) -> Result<Vec<u8>, UserFontError> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        Err(UserFontError::Unsupported)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        match std::fs::metadata(path) {
            Ok(m) if m.len() > MAX_FONT_FILE_BYTES as u64 => {
                Err(UserFontError::TooLarge { bytes: usize::try_from(m.len()).unwrap_or(usize::MAX) })
            }
            Ok(_) => std::fs::read(path).map_err(|e| UserFontError::Unusable(e.to_string())),
            Err(e) => Err(UserFontError::Unusable(e.to_string())),
        }
    }
}

/// The distinct families of `faces`, in order.
fn families_of(faces: &[Face]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for f in faces {
        if !v.iter().any(|g| g.eq_ignore_ascii_case(&f.family)) {
            v.push(f.family.clone());
        }
    }
    v
}

/// A file name for a user font from the name it came with: letters, digits, spaces, `-` and `_` of
/// its stem (others become `_`, at most 64), and the extension of its unwrapped format.
fn stored_name(name: &str, sfnt: &[u8]) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);
    let mut clean: String = stem.chars().take(64).map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' }).collect();
    clean = clean.trim().to_string();
    if clean.is_empty() || clean.starts_with('-') {
        clean = format!("font{clean}");
    }
    let ext = match sniff(sfnt) {
        Some(FontFormat::OpenType) => "otf",
        Some(FontFormat::Collection) => "ttc",
        _ => "ttf",
    };
    format!("{clean}.{ext}")
}

/// `name`, or `name` with `-2`, `-3`… before its extension, the first that `taken` refuses.
fn unique_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (2..10_000).map(|n| format!("{stem}-{n}.{ext}")).find(|n| !taken(n)).unwrap_or_else(|| format!("{stem}-x.{ext}"))
}

/// Write `bytes` to a new file in `dir` named `name` (or a numbered variant when that exists),
/// through a temporary file so a crash never leaves half a font.
#[cfg(not(target_arch = "wasm32"))]
fn write_unique(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = unique_name(name, |n| dir.join(n).exists());
    let path = dir.join(&name);
    let tmp = dir.join(format!(".{name}.part"));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(path)
}

/// Where session-only fonts go when the host named no folder (tests, the web): a private temp dir.
#[cfg(not(target_arch = "wasm32"))]
fn session_dir() -> PathBuf {
    std::env::temp_dir().join(format!("gridcraft-fonts-{}", std::process::id()))
}

/// Check the faces in `sfnt` (already decoded) and add the ones not already available, recording
/// them as a user font stored at `path`.
#[cfg(not(target_arch = "wasm32"))]
fn register_user(file: &str, sfnt: &[u8], path: PathBuf) -> Result<UserFont, UserFontError> {
    let found = file_faces(sfnt);
    if found.is_empty() {
        return Err(UserFontError::Unusable("no face with a family name".into()));
    }
    if found.len() > MAX_FACES_PER_FILE {
        return Err(UserFontError::Unusable(format!("more than {MAX_FACES_PER_FILE} faces")));
    }
    let mut c = write_catalog();
    if c.user.len() >= MAX_USER_FONTS {
        return Err(UserFontError::TooMany);
    }
    let mut faces: Vec<Face> = Vec::new();
    let mut all: Vec<String> = Vec::new();
    for (index, family, style) in found {
        if !all.iter().any(|f| f.eq_ignore_ascii_case(&family)) {
            all.push(family.clone());
        }
        let weight = style_weight(&style);
        let italic = style_italic(&style);
        // A face whose family and style are already available is skipped: the installed baseline
        // is never replaced, so documents not naming the added font keep their metrics.
        let taken = c
            .merged
            .iter()
            .any(|g| g.family.eq_ignore_ascii_case(&family) && (g.weight - weight).abs() < 1.0 && g.italic == italic);
        if !taken {
            faces.push(Face { family, path: path.clone(), index, weight, italic });
        }
    }
    if faces.is_empty() {
        return Err(UserFontError::AlreadyAvailable(all));
    }
    let font = UserFont { file: file.to_string(), families: families_of(&faces), path: Some(path), faces };
    c.user.push(font.clone());
    c.rebuild();
    Ok(font)
}

/// Add the font file `bytes` (TTF, OTF, TTC or OTC) named `name` (its file name, for the stored
/// copy). Every face is checked first; faces whose family and style are already available are
/// skipped. With a user font folder (see [`load_user_fonts`]) the font is stored there too, for the
/// next start; without one it lasts for the session.
pub fn install_font(name: &str, bytes: &[u8]) -> Result<Installed, UserFontError> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (name, bytes);
        Err(UserFontError::Unsupported)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let sfnt = decode_font_file(bytes)?;
        let (dir, persistent) = match read_catalog().dir.clone() {
            Some(d) => (d, true),
            None => (session_dir(), false),
        };
        let path = write_unique(&dir, &stored_name(name, &sfnt), &sfnt).map_err(|e| UserFontError::Unusable(e.to_string()))?;
        let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| stored_name(name, &sfnt));
        // A file that adds nothing new is removed again, so the folder doesn't collect copies.
        let font = match register_user(&file, &sfnt, path.clone()) {
            Ok(f) => f,
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(e);
            }
        };
        let warning = (!persistent).then(|| "added for this session only: no fonts folder to store it in".to_string());
        Ok(Installed { font, warning })
    }
}

/// The fonts the user added, in the order added.
pub fn user_fonts() -> Vec<UserFont> {
    read_catalog().user.clone()
}

/// The folder user fonts are stored in, if any ([`load_user_fonts`]).
pub fn user_font_dir() -> Option<PathBuf> {
    read_catalog().dir.clone()
}

/// Remove the user fonts that added `family`: their faces leave the font list and their stored
/// files are deleted. Installed system fonts can't be removed.
pub fn remove_user_font(family: &str) -> Result<Vec<UserFont>, UserFontError> {
    let mut c = write_catalog();
    let (gone, kept): (Vec<UserFont>, Vec<UserFont>) =
        std::mem::take(&mut c.user).into_iter().partition(|u| u.families.iter().any(|f| f.eq_ignore_ascii_case(family)));
    c.user = kept;
    if gone.is_empty() {
        return Err(UserFontError::NotUserFont(family.to_string()));
    }
    c.rebuild();
    drop(c);
    for u in &gone {
        if let Some(p) = &u.path
            && let Err(e) = std::fs::remove_file(p)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!("couldn't delete the font file {}: {e}", p.display());
        }
    }
    Ok(gone)
}

/// Use `dir` as the user font folder: load the fonts stored there (each checked like an added
/// font; one that fails is logged and skipped) and store fonts added later there. Returns the
/// fonts loaded and the files skipped (with why).
pub fn load_user_fonts(dir: &Path) -> (Vec<UserFont>, Vec<(String, String)>) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = dir;
        (Vec::new(), Vec::new())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        write_catalog().dir = Some(dir.to_path_buf());
        let mut loaded = Vec::new();
        let mut skipped = Vec::new();
        let Ok(rd) = std::fs::read_dir(dir) else { return (loaded, skipped) };
        let mut paths: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| FONT_FILE_EXTS.contains(&e.to_ascii_lowercase().as_str())))
            .collect();
        paths.sort();
        for p in paths {
            let file = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let read = match std::fs::metadata(&p) {
                Ok(m) if m.len() > MAX_FONT_FILE_BYTES as u64 => {
                    Err(UserFontError::TooLarge { bytes: usize::try_from(m.len()).unwrap_or(usize::MAX) })
                }
                Ok(_) => std::fs::read(&p).map_err(|e| UserFontError::Unusable(e.to_string())),
                Err(e) => Err(UserFontError::Unusable(e.to_string())),
            };
            match read.and_then(|b| decode_font_file(&b)).and_then(|sfnt| register_user(&file, &sfnt, p.clone())) {
                Ok(f) => loaded.push(f),
                Err(e) => {
                    log::warn!("skipped the user font {}: {e}", p.display());
                    skipped.push((file, e.to_string()));
                }
            }
        }
        (loaded, skipped)
    }
}

/// The JSON form of a user font, as the `font.*` commands report it.
pub fn font_json(font: &UserFont, warning: Option<String>) -> serde_json::Value {
    serde_json::json!({
        "file": font.file,
        "families": font.families.clone(),
        "faces": font.faces.iter().map(|f| serde_json::json!({"family": f.family, "weight": f.weight, "italic": f.italic})).collect::<Vec<_>>(),
        "path": font.path.as_ref().map(|p| p.to_string_lossy().to_string()),
        "warning": warning,
    })
}

/// The `font.list` result: the user fonts, their folder, and (with `all`) every available family.
pub fn list_json(all: bool) -> serde_json::Value {
    let mut out = serde_json::json!({
        "fonts": user_fonts().iter().map(|f| font_json(f, None)).collect::<Vec<_>>(),
        "folder": user_font_dir().map(|p| p.to_string_lossy().to_string()),
    });
    if all {
        out["families"] = serde_json::json!(families());
    }
    out
}

/// The egui family name for one role of one family: `fnt:{family}:{b|n}{i|n}`.
pub fn role_key(family: &str, bold: bool, italic: bool) -> String {
    format!("fnt:{family}:{}{}", if bold { 'b' } else { 'n' }, if italic { 'i' } else { 'n' })
}

/// The face to use for a role, or `None` when the family has no faces. Missing bold/italic faces
/// fall back to the family's regular face, so every bound role has data behind it.
fn pick_face(faces: &[Face], bold: bool, italic: bool) -> Option<&Face> {
    let regular = || faces.iter().find(|f| !f.italic && (f.weight - 400.0).abs() < 1.0);
    let exact = |wanted_bold: bool, wanted_italic: bool| {
        faces.iter().find(|f| f.italic == wanted_italic && if wanted_bold { f.weight >= 600.0 } else { f.weight < 600.0 })
    };
    (match (bold, italic) {
        (true, true) => exact(true, true).or_else(|| exact(true, false)).or_else(|| exact(false, true)),
        (true, false) => exact(true, false),
        (false, true) => exact(false, true),
        (false, false) => None,
    })
    .or_else(regular)
    .or_else(|| faces.first())
}

/// A stable key for a face's font data, unique per file/index within the process.
fn face_key(family: &str, face: &Face) -> String {
    let file = face.path.file_name().and_then(|s| s.to_str()).unwrap_or("font");
    format!("fnt-src:{family}:{file}:{}", face.index)
}

/// Bind one installed family's faces into `fonts`, one egui family per cell role, and return the
/// role keys that are now bound. A role with no exact face chains to the family's regular face, so
/// no role is ever left pointing at missing data (egui panics on that). A family with no faces
/// binds nothing.
///
/// Each role's chain ends with egui's default proportional fonts, so glyphs the installed font
/// lacks (emoji, a stray script) still render.
pub fn bind_faces(fonts: &mut egui::FontDefinitions, family: &str, faces: &[Face]) -> Vec<String> {
    if faces.is_empty() {
        return Vec::new();
    }
    let base: Vec<String> = fonts.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    let mut bound: Vec<String> = Vec::new();
    for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
        let Some(primary) = pick_face(faces, bold, italic) else { continue };
        let mut chain: Vec<String> = Vec::new();
        for face in [Some(primary), pick_face(faces, false, false)] {
            let Some(face) = face else { continue };
            let key = face_key(family, face);
            if chain.contains(&key) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&face.path) else { continue };
            let mut fd = egui::FontData::from_owned(bytes);
            fd.index = face.index;
            fonts.font_data.entry(key.clone()).or_insert_with(|| std::sync::Arc::new(fd));
            chain.push(key);
        }
        if chain.is_empty() {
            continue;
        }
        chain.extend(base.iter().cloned());
        let role = role_key(family, bold, italic);
        fonts.families.insert(egui::FontFamily::Name(role.clone().into()), chain);
        bound.push(role);
    }
    bound
}

/// Bind every installed face of `family` (see [`bind_faces`]).
pub fn bind_family(fonts: &mut egui::FontDefinitions, family: &str) -> Vec<String> {
    bind_faces(fonts, family, &faces_of(family))
}

/// The role families currently bound to font data. `cell_family` consults this, so it never names a
/// family egui hasn't been given.
static BOUND: OnceLock<std::sync::RwLock<HashSet<String>>> = OnceLock::new();

fn bound() -> &'static std::sync::RwLock<HashSet<String>> {
    BOUND.get_or_init(|| std::sync::RwLock::new(HashSet::new()))
}

/// Record the role families a freshly built [`egui::FontDefinitions`] bound.
pub fn set_bound(keys: impl IntoIterator<Item = String>) {
    *bound().write().unwrap_or_else(|e| e.into_inner()) = keys.into_iter().collect();
}

/// Is this exact role family bound to font data?
pub fn is_bound(role: &str) -> bool {
    bound().read().unwrap_or_else(|e| e.into_inner()).contains(role)
}

/// Serializes tests that mutate the process-wide bound-family registry.
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_names_map_to_weight_and_slant() {
        assert_eq!(style_weight("Bold"), 700.0);
        assert_eq!(style_weight("SemiBold Italic"), 600.0);
        assert_eq!(style_weight("Regular"), 400.0);
        assert_eq!(style_weight("Light Oblique"), 300.0);
        assert!(style_italic("Bold Italic") && style_italic("Oblique"));
        assert!(!style_italic("Bold"));
        // The weight table is checked in order, so "extralight" wins over "light".
        assert_eq!(style_weight("ExtraLight"), 200.0);
    }

    #[test]
    fn role_keys_are_stable_and_distinct() {
        assert_eq!(role_key("Segoe UI", false, false), "fnt:Segoe UI:nn");
        assert_eq!(role_key("Segoe UI", true, false), "fnt:Segoe UI:bn");
        assert_eq!(role_key("Segoe UI", false, true), "fnt:Segoe UI:ni");
        assert_eq!(role_key("Segoe UI", true, true), "fnt:Segoe UI:bi");
    }

    #[test]
    fn nothing_is_bound_until_font_definitions_says_so() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // A role key is only safe to name after font_definitions bound it.
        let k = role_key("Some Installed Font", true, false);
        assert!(!is_bound(&k));
        set_bound([k.clone()]);
        assert!(is_bound(&k));
        set_bound([]);
        assert!(!is_bound(&k));
    }

    #[test]
    fn damaged_or_non_font_files_contribute_nothing() {
        assert!(file_faces(b"ttcf\x00\x01\x00\x00\xff\xff\xff\xff").is_empty());
        assert!(file_faces(b"not a font at all").is_empty());
        assert!(file_faces(&[]).is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_scan_catalogs_fonts_and_reads_each_folder_once() {
        let dir = std::env::temp_dir().join(format!("gridcraft-fonts-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Sub")).expect("temp dir");
        std::fs::write(dir.join("Ubuntu-Light.ttf"), epaint_default_fonts::UBUNTU_LIGHT).expect("write");
        std::fs::write(dir.join("damaged.ttf"), b"ttcf\x00\x01\x00\x00\xff\xff\xff\xff").expect("write");
        std::fs::write(dir.join("readme.txt"), b"not a font").expect("write");
        // The same folder listed twice (once through `Sub/..`) is read once.
        let faces = scan_dirs(&[dir.clone(), dir.join("Sub/..")]);
        let families: Vec<&str> = faces.iter().map(|f| f.family.as_str()).collect();
        assert_eq!(families, ["Ubuntu"], "one face, family read from the name table");
        assert_eq!(faces[0].weight, 300.0, "“Light” reads as weight 300");
        assert!(!faces[0].italic);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_back_to_a_parent_folder_ends_the_scan() {
        let dir = std::env::temp_dir().join(format!("gridcraft-fonts-loop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Sub")).expect("temp dir");
        std::fs::write(dir.join("Sub/Ubuntu-Light.ttf"), epaint_default_fonts::UBUNTU_LIGHT).expect("write");
        std::os::unix::fs::symlink(&dir, dir.join("Sub/loop")).expect("symlink");
        assert_eq!(scan_dirs(std::slice::from_ref(&dir)).len(), 1, "the loop doesn't multiply faces");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn binding_a_family_binds_all_four_roles_and_never_stores_missing_data() {
        let dir = std::env::temp_dir().join(format!("gridcraft-fonts-bind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("Ubuntu-Light.ttf");
        std::fs::write(&path, epaint_default_fonts::UBUNTU_LIGHT).expect("write");
        let face = Face { family: "Ubuntu".into(), path, index: 0, weight: 300.0, italic: false };

        let mut fonts = egui::FontDefinitions::default();
        let bound = bind_faces(&mut fonts, "Ubuntu", std::slice::from_ref(&face));
        assert_eq!(bound.len(), 4, "regular, bold, italic and bold-italic roles");
        for key in &bound {
            let chain = fonts.families.get(&egui::FontFamily::Name(key.as_str().into())).expect("role family");
            assert!(chain.iter().all(|k| fonts.font_data.contains_key(k)), "every chained key has data");
        }
        // A family with no faces binds nothing and never touches the map.
        assert!(bind_faces(&mut fonts, "No Such Font", &[]).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn fresh_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gridcraft-user-fonts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn only_font_file_names_are_recognised() {
        assert!(is_font_file_name("Arial.ttf") && is_font_file_name("/x/y.OTF") && is_font_file_name("a.TTC"));
        assert!(!is_font_file_name("book.xlsx") && !is_font_file_name("noext") && !is_font_file_name("a.woff2"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn an_added_font_is_stored_listed_and_removable() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = fresh_dir("add");
        reset_for_test(Vec::new());
        load_user_fonts(&dir);
        // Ubuntu Light is our test face (a real, bundled TTF): family "Ubuntu".
        let r = install_font("Ubuntu-Light.ttf", epaint_default_fonts::UBUNTU_LIGHT).expect("install");
        assert_eq!(r.font.families, ["Ubuntu"]);
        assert!(r.warning.is_none(), "a folder was named, so it is stored: {:?}", r.warning);
        let path = r.font.path.clone().expect("stored path");
        assert!(path.exists() && path.starts_with(&dir));
        // It shows in the font list and its faces are catalogued.
        assert!(families().iter().any(|f| f == "Ubuntu"), "families: {:?}", families());
        assert_eq!(faces_of("Ubuntu").len(), 1);
        assert_eq!(user_fonts().len(), 1);
        assert_eq!(user_font_dir().as_deref(), Some(dir.as_path()));
        // Removing takes it out and deletes the stored file.
        let gone = remove_user_font("Ubuntu").expect("remove");
        assert_eq!(gone.len(), 1);
        assert!(!path.exists(), "the stored file is deleted");
        assert!(user_fonts().is_empty());
        assert!(!families().iter().any(|f| f == "Ubuntu"));
        // Removing it again is an error, not a panic.
        assert!(matches!(remove_user_font("Ubuntu"), Err(UserFontError::NotUserFont(_))));
        let _ = std::fs::remove_dir_all(&dir);
        reset_for_test(Vec::new());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn a_face_already_available_is_refused_and_leaves_no_copy() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = fresh_dir("dup");
        reset_for_test(vec![Face { family: "Ubuntu".into(), path: PathBuf::from("installed"), index: 0, weight: 300.0, italic: false }]);
        load_user_fonts(&dir);
        let r = install_font("Ubuntu-Light.ttf", epaint_default_fonts::UBUNTU_LIGHT);
        assert_eq!(r, Err(UserFontError::AlreadyAvailable(vec!["Ubuntu".to_string()])), "a face already available is refused");
        assert!(user_fonts().is_empty());
        assert_eq!(std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0, "no file is left behind");
        let _ = std::fs::remove_dir_all(&dir);
        reset_for_test(Vec::new());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn damaged_and_web_font_files_are_refused() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = fresh_dir("bad");
        reset_for_test(Vec::new());
        load_user_fonts(&dir);
        assert!(matches!(install_font("x.ttf", &[]), Err(UserFontError::Empty)));
        assert!(matches!(install_font("x.ttf", b"not a font"), Err(UserFontError::NotAFont)));
        assert!(matches!(install_font("x.woff", b"wOFF\x00\x01\x00\x00"), Err(UserFontError::WebFont)));
        assert!(matches!(install_font("x.ttf", b"ttcf\x00\x01\x00\x00\xff\xff\xff\xff"), Err(UserFontError::Unusable(_))));
        assert!(user_fonts().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        reset_for_test(Vec::new());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn stored_fonts_load_again_on_the_next_start() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = fresh_dir("load");
        reset_for_test(Vec::new());
        load_user_fonts(&dir);
        install_font("Ubuntu-Light.ttf", epaint_default_fonts::UBUNTU_LIGHT).expect("install");
        // A fresh catalog (the next launch) finds the stored file in the folder.
        reset_for_test(Vec::new());
        let (loaded, skipped) = load_user_fonts(&dir);
        assert_eq!(loaded.len(), 1);
        assert!(skipped.is_empty(), "skipped: {skipped:?}");
        assert_eq!(loaded[0].families, ["Ubuntu"]);
        assert!(families().iter().any(|f| f == "Ubuntu"));
        let _ = std::fs::remove_dir_all(&dir);
        reset_for_test(Vec::new());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn without_a_folder_a_font_lasts_for_the_session_only() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test(Vec::new());
        // No `load_user_fonts`: the catalog has no folder, so the add is session-only.
        let r = install_font("Ubuntu-Light.ttf", epaint_default_fonts::UBUNTU_LIGHT).expect("install");
        assert!(r.warning.is_some(), "a session-only add warns");
        assert!(families().iter().any(|f| f == "Ubuntu"));
        if let Some(p) = &r.font.path {
            let _ = std::fs::remove_file(p);
        }
        reset_for_test(Vec::new());
    }
}
