//! Fonts the end user adds at run time, without rebuilding.
//!
//! The bundled faces (and the system fonts) stay the baseline: a user font only adds faces, never
//! replaces one (a face whose family and style are already loaded is skipped), so documents that
//! don't name it keep their metrics. A file is checked before anything is added: its size is
//! capped and every face must parse.
//!
//! Desktop: [`FontDb::load_user_fonts`] names a folder (the app's `fonts` folder); the fonts there
//! load at start-up and each font added later is copied there. Without a folder (the web, tests)
//! added fonts last for the session; the web shell keeps them in the browser itself.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use skrifa::MetadataProvider;
use skrifa::raw::FileRef;

use super::{FontBytes, FontDb, enumerate_faces, make_face};

/// Largest font file accepted (TTF, OTF, TTC or OTC).
pub const MAX_FONT_FILE_BYTES: usize = 50 << 20;
/// Most faces one file may add (collections, variable fonts' named instances).
const MAX_FACES_PER_FILE: usize = 256;
/// Most user fonts at once.
const MAX_USER_FONTS: usize = 1000;
/// Font file extensions a user may add and the folder scan reads (fonts are stored unwrapped).
pub const FONT_FILE_EXTS: &[&str] = &["ttf", "otf", "ttc", "otc"];

/// Is `name` (a file name or path) named like a font file?
pub fn is_font_file_name(name: &str) -> bool {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    ext.is_some_and(|e| FONT_FILE_EXTS.contains(&e.as_str()))
}

/// Why a font couldn't be added or removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserFontError {
    Empty,
    TooLarge {
        bytes: usize,
    },
    /// Not a font file this reads (TrueType, OpenType or a collection).
    NotAFont,
    /// A font file that doesn't parse, or has no usable face.
    Unusable(String),
    /// Every face in the file is already available.
    AlreadyAvailable(Vec<String>),
    TooMany,
    /// The family isn't one the user added (bundled and system fonts can't be removed).
    NotUserFont(String),
}

impl std::fmt::Display for UserFontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserFontError::Empty => write!(f, "the font file is empty"),
            UserFontError::TooLarge { bytes } => {
                write!(f, "the font file is too large ({} MB; at most {} MB)", bytes.div_ceil(1 << 20), MAX_FONT_FILE_BYTES >> 20)
            }
            UserFontError::NotAFont => write!(f, "not a font file (TrueType, OpenType or a collection)"),
            UserFontError::Unusable(e) => write!(f, "the font couldn't be read: {e}"),
            UserFontError::AlreadyAvailable(families) => write!(f, "already available: {}", families.join(", ")),
            UserFontError::TooMany => write!(f, "too many added fonts (at most {MAX_USER_FONTS}); remove some first"),
            UserFontError::NotUserFont(family) => write!(f, "{family:?} isn't a font you added"),
        }
    }
}

impl std::error::Error for UserFontError {}

/// A font file the user added.
#[derive(Clone, Debug, PartialEq)]
pub struct UserFont {
    /// Its file name in the user font folder (unique among the user fonts), also the key the web
    /// shell stores it under.
    pub file: String,
    /// The faces it added: (typographic family, style).
    pub faces: Vec<(String, String)>,
    /// Where it is stored (desktop); `None` when it lasts for the session only.
    pub path: Option<PathBuf>,
    face_ids: Vec<u32>,
}

impl UserFont {
    /// The families it added (typographic names, in file order, each once).
    pub fn families(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for (family, _) in &self.faces {
            if !v.iter().any(|f| f.eq_ignore_ascii_case(family)) {
                v.push(family.clone());
            }
        }
        v
    }
}

/// What [`FontDb::install_font`] did.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub font: UserFont,
    /// The font was added for this session but couldn't be stored for the next one.
    pub warning: Option<String>,
}

#[derive(Default)]
pub(super) struct UserFonts {
    fonts: Vec<UserFont>,
    dir: Option<PathBuf>,
}

/// The kind of font file `bytes` holds, from its signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FontFormat {
    TrueType,
    OpenType,
    Collection,
}

fn sniff(bytes: &[u8]) -> Option<FontFormat> {
    match bytes.get(..4)? {
        [0, 1, 0, 0] | b"true" | b"typ1" => Some(FontFormat::TrueType),
        b"OTTO" => Some(FontFormat::OpenType),
        b"ttcf" => Some(FontFormat::Collection),
        _ => None,
    }
}

/// The font file in `bytes` as plain OpenType (TTF/OTF/TTC) bytes, checking the size and the
/// signature only; [`FontDb::install_font`] checks the faces.
fn decode_font_file(bytes: &[u8]) -> Result<Vec<u8>, UserFontError> {
    if bytes.is_empty() {
        return Err(UserFontError::Empty);
    }
    if bytes.len() > MAX_FONT_FILE_BYTES {
        return Err(UserFontError::TooLarge { bytes: bytes.len() });
    }
    match sniff(bytes) {
        Some(FontFormat::TrueType | FontFormat::OpenType | FontFormat::Collection) => Ok(bytes.to_vec()),
        None => Err(UserFontError::NotAFont),
    }
}

/// A file name for a user font from the name it came with: letters, digits, spaces, `-` and `_`
/// of its stem (others become `_`, at most 64), and the extension of its format.
fn stored_name(name: &str, sfnt: &[u8]) -> String {
    // The last component, whichever separator the name came with.
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

impl FontDb {
    /// A counter that changes whenever user fonts are added or removed: a UI caching the font
    /// list or a layout redraws when it moves.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn read_user(&self) -> std::sync::RwLockReadGuard<'_, UserFonts> {
        self.user.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write_user(&self) -> std::sync::RwLockWriteGuard<'_, UserFonts> {
        self.user.write().unwrap_or_else(|e| e.into_inner())
    }

    /// The fonts the user added, in the order added.
    pub fn user_fonts(&self) -> Vec<UserFont> {
        self.read_user().fonts.clone()
    }

    /// The folder user fonts are stored in, if any ([`FontDb::load_user_fonts`]).
    pub fn user_font_dir(&self) -> Option<PathBuf> {
        self.read_user().dir.clone()
    }

    /// Add the font file `bytes` (TTF, OTF, TTC or OTC) named `name` (its file name, for the
    /// stored copy). Every face is checked first; faces whose family and style are already
    /// available are skipped (the bundled baseline is never replaced). With a user font folder the
    /// font is stored there too, for the next start.
    pub fn install_font(&self, name: &str, bytes: &[u8]) -> Result<Installed, UserFontError> {
        let sfnt = decode_font_file(bytes)?;
        let mut font = self.add_user_faces(name, sfnt.clone(), None)?;
        let warning = match self.user_font_dir() {
            Some(dir) => match write_unique(&dir, &font.file, &sfnt) {
                Ok(path) => {
                    font.file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(font.file);
                    font.path = Some(path);
                    None
                }
                Err(e) => {
                    log::warn!("couldn't store the font {name} in {}: {e}", dir.display());
                    Some(format!("added for this session only: couldn't store it in {}: {e}", dir.display()))
                }
            },
            None => None,
        };
        if let Some(f) = self.write_user().fonts.iter_mut().find(|f| f.face_ids == font.face_ids) {
            f.file.clone_from(&font.file);
            f.path.clone_from(&font.path);
        }
        Ok(Installed { font, warning })
    }

    /// Check the faces in `sfnt` and add the new ones, recording them as a user font.
    fn add_user_faces(&self, name: &str, sfnt: Vec<u8>, path: Option<PathBuf>) -> Result<UserFont, UserFontError> {
        if self.read_user().fonts.len() >= MAX_USER_FONTS {
            return Err(UserFontError::TooMany);
        }
        if FileRef::new(&sfnt).is_err() {
            return Err(UserFontError::Unusable("not a valid OpenType file".into()));
        }
        let found = enumerate_faces(&sfnt);
        if found.is_empty() {
            return Err(UserFontError::Unusable("no face with a family name".into()));
        }
        if found.len() > MAX_FACES_PER_FILE {
            return Err(UserFontError::Unusable(format!("more than {MAX_FACES_PER_FILE} faces")));
        }
        let file = stored_name(name, &sfnt);
        let data = Arc::new(sfnt);
        let mut new = Vec::new();
        for (index, family, style, coords) in found {
            let label = format!("{family} {style}");
            let face = make_face(FontBytes::Owned(data.clone()), index, family, style, coords)
                .ok_or_else(|| UserFontError::Unusable(format!("{label} doesn't parse")))?;
            if face.skrifa().is_none_or(|s| s.charmap().mappings().next().is_none()) {
                return Err(UserFontError::Unusable(format!("{label} maps no characters")));
            }
            new.push(Arc::new(face));
        }
        let families = |faces: &[Arc<super::FontFace>]| {
            let mut v: Vec<String> = faces.iter().map(|f| f.family.clone()).collect();
            v.dedup();
            v
        };
        let mut user = self.write_user();
        // Unique among the user fonts (the folder is checked when writing).
        let file = unique_name(&file, |n| user.fonts.iter().any(|f| f.file.eq_ignore_ascii_case(n)));
        let added: Vec<Arc<super::FontFace>> = {
            let mut faces = self.faces.write().unwrap_or_else(|e| e.into_inner());
            let fresh: Vec<_> = new
                .iter()
                .filter(|f| !faces.iter().any(|g| g.family.eq_ignore_ascii_case(&f.family) && g.style.eq_ignore_ascii_case(&f.style)))
                .cloned()
                .collect();
            faces.extend(fresh.iter().cloned());
            fresh
        };
        if added.is_empty() {
            return Err(UserFontError::AlreadyAvailable(families(&new)));
        }
        let font = UserFont {
            file,
            faces: added.iter().map(|f| (f.family.clone(), f.style.clone())).collect(),
            path,
            face_ids: added.iter().map(|f| f.id).collect(),
        };
        user.fonts.push(font.clone());
        drop(user);
        self.fonts_changed();
        Ok(font)
    }

    /// Forget cached fallback choices and tell watchers ([`FontDb::generation`]).
    fn fonts_changed(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.sys.lock().unwrap_or_else(|e| e.into_inner()).misses.clear();
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Remove the user fonts that added `family` (a typographic or legacy family name): their
    /// faces leave the font list and their stored files are deleted. Text set in the family falls
    /// back as for any font that isn't installed. Bundled and system fonts can't be removed.
    pub fn remove_user_font(&self, family: &str) -> Result<Vec<UserFont>, UserFontError> {
        let mut user = self.write_user();
        let ids: Vec<u32> = {
            let faces = self.read_faces();
            faces.iter().filter(|f| names_family(f, family)).map(|f| f.id).collect()
        };
        let (gone, kept): (Vec<UserFont>, Vec<UserFont>) = std::mem::take(&mut user.fonts)
            .into_iter()
            .partition(|u| u.faces.iter().any(|(f, _)| f.eq_ignore_ascii_case(family)) || u.face_ids.iter().any(|i| ids.contains(i)));
        user.fonts = kept;
        drop(user);
        if gone.is_empty() {
            return Err(UserFontError::NotUserFont(family.to_string()));
        }
        let drop_ids: Vec<u32> = gone.iter().flat_map(|u| u.face_ids.iter().copied()).collect();
        self.faces.write().unwrap_or_else(|e| e.into_inner()).retain(|f| !drop_ids.contains(&f.id));
        for u in &gone {
            if let Some(p) = &u.path
                && let Err(e) = std::fs::remove_file(p)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                log::warn!("couldn't delete the font file {}: {e}", p.display());
            }
        }
        self.fonts_changed();
        Ok(gone)
    }

    /// Use `dir` as the user font folder: load the fonts stored there (each checked like an added
    /// font; one that fails is logged and skipped) and store fonts added later there. The folder is
    /// made when the first font is stored. Returns the fonts loaded and the files skipped (with
    /// why).
    pub fn load_user_fonts(&self, dir: &Path) -> (Vec<UserFont>, Vec<(String, String)>) {
        self.write_user().dir = Some(dir.to_path_buf());
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
            let file = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let read = match std::fs::metadata(&p) {
                Ok(m) if m.len() > MAX_FONT_FILE_BYTES as u64 => {
                    Err(UserFontError::TooLarge { bytes: usize::try_from(m.len()).unwrap_or(usize::MAX) })
                }
                Ok(_) => std::fs::read(&p).map_err(|e| UserFontError::Unusable(e.to_string())),
                Err(e) => Err(UserFontError::Unusable(e.to_string())),
            };
            let r = read.and_then(|b| decode_font_file(&b)).and_then(|sfnt| self.add_user_faces(&file, sfnt, Some(p.clone())));
            match r {
                Ok(mut f) => {
                    // Its own file name, so removing it deletes this file.
                    if let Some(u) = self.write_user().fonts.iter_mut().find(|u| u.face_ids == f.face_ids) {
                        u.file.clone_from(&file);
                    }
                    f.file = file;
                    loaded.push(f);
                }
                Err(e) => {
                    log::warn!("skipped the user font {}: {e}", p.display());
                    skipped.push((file, e.to_string()));
                }
            }
        }
        (loaded, skipped)
    }
}

/// Does `face` answer to the family name `family` (its typographic or Windows name)?
fn names_family(face: &super::FontFace, family: &str) -> bool {
    face.family.eq_ignore_ascii_case(family) || face.legacy.as_ref().is_some_and(|(lf, _)| lf.eq_ignore_ascii_case(family))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The test fixture face: family "DeckCraft Legacy", style "Semibold" (with the Windows name
    /// "DeckCraft Legacy Semibold"/"Regular").
    const FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/legacy-semibold.ttf");
    const FAMILY: &str = "DeckCraft Legacy";

    fn db() -> FontDb {
        FontDb::with_font_dirs(vec![])
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deck-userfonts-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn an_added_font_is_listed_resolved_and_measured() {
        let db = db();
        let gen0 = db.generation();
        assert!(!db.has_family(FAMILY));
        let r = db.install_font("Legacy-Semibold.ttf", FIXTURE).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.font.families(), vec![FAMILY.to_string()]);
        assert_eq!(r.font.file, "Legacy-Semibold.ttf");
        assert!(r.font.path.is_none() && r.warning.is_none(), "no folder: kept for the session only");
        assert!(db.generation() > gen0);
        assert!(db.families().iter().any(|f| f == FAMILY));
        let face = db.face(FAMILY, "Semibold");
        assert_eq!((face.family.as_str(), face.style.as_str()), (FAMILY, "Semibold"));
        // Measured with its own bytes (its own glyphs, whatever the fixture covers).
        let c = face.chars().first().map(|(c, _)| *c).expect("the fixture maps characters");
        assert!(face.glyph_for(c) != 0);
        assert_eq!(db.user_fonts().len(), 1);
    }

    #[test]
    fn the_baseline_is_never_replaced() {
        let db = db();
        let bytes = FIXTURE.to_vec();
        db.install_font("Legacy-Semibold.ttf", &bytes).unwrap_or_else(|e| panic!("{e}"));
        // Adding the same faces again changes nothing.
        assert!(matches!(db.install_font("Again.ttf", &bytes), Err(UserFontError::AlreadyAvailable(_))));
        // The compiled-in last-resort family isn't a user font and can't be removed.
        assert!(matches!(db.remove_user_font(super::super::LAST_RESORT_FAMILY), Err(UserFontError::NotUserFont(_))));
        assert!(db.has_family(super::super::LAST_RESORT_FAMILY));
    }

    #[test]
    fn junk_is_rejected_without_adding_anything() {
        let db = db();
        let n = db.families().len();
        let cases: Vec<Vec<u8>> = vec![
            vec![],
            b"not a font at all".to_vec(),
            b"\0\x01\0\0".to_vec(),
            b"\0\x01\0\0\xff\xff\0\0\0\0\0\0".to_vec(),
            b"ttcf\0\x01\0\0\xff\xff\xff\xff".to_vec(),
            b"OTTO\0\x01".to_vec(),
            b"wOFF\0\x01\0\0\0\0\0\x08".to_vec(),
            b"wOF2\0\x01\0\0\xff\xff\xff\xff\0\x05".to_vec(),
            FIXTURE[..FIXTURE.len() / 3].to_vec(),
        ];
        for (i, c) in cases.iter().enumerate() {
            assert!(db.install_font("junk.ttf", c).is_err(), "case {i}");
        }
        assert_eq!(db.families().len(), n);
        let mut big = FIXTURE.to_vec();
        big.resize(MAX_FONT_FILE_BYTES + 1, 0);
        assert!(matches!(db.install_font("big.ttf", &big), Err(UserFontError::TooLarge { .. })));
    }

    #[test]
    fn user_fonts_are_stored_loaded_and_removed() {
        let dir = temp_dir("store");
        let db = db();
        let (loaded, skipped) = db.load_user_fonts(&dir);
        assert!(loaded.is_empty() && skipped.is_empty(), "a missing folder is empty");
        let r = db.install_font("../../evil/Legacy? Semibold.ttf", FIXTURE).unwrap_or_else(|e| panic!("{e}"));
        assert!(r.warning.is_none());
        let path = r.font.path.clone().unwrap();
        assert_eq!(path.parent(), Some(dir.as_path()), "a file name can't leave the folder");
        assert_eq!(r.font.file, "Legacy_ Semibold.ttf");
        assert!(path.exists());
        // A damaged file in the folder is skipped, never fatal.
        std::fs::write(dir.join("damaged.ttf"), b"ttcf\0\x01\0\0\xff\xff\xff\xff").unwrap();
        std::fs::write(dir.join("notes.txt"), b"hello").unwrap();

        // The next start loads it from the folder.
        let next = FontDb::with_font_dirs(vec![]);
        let (loaded, skipped) = next.load_user_fonts(&dir);
        assert_eq!(loaded.len(), 1, "{skipped:?}");
        assert_eq!(skipped.len(), 1);
        assert!(next.has_family(FAMILY));

        // Removing the family deletes the file; text in it falls back.
        let gone = next.remove_user_font(FAMILY).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(gone.len(), 1);
        assert!(!next.has_family(FAMILY));
        assert_ne!(next.face(FAMILY, "Regular").family, FAMILY, "text in the family falls back");
        assert!(!path.exists());
        assert!(next.user_fonts().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn removing_a_family_by_its_windows_name_works() {
        let db = db();
        db.install_font("Legacy-Semibold.ttf", FIXTURE).unwrap_or_else(|e| panic!("{e}"));
        // "DeckCraft Legacy Semibold" is the face's Windows family name.
        let gone = db.remove_user_font("DeckCraft Legacy Semibold").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(gone.len(), 1);
        assert!(db.user_fonts().is_empty());
    }

    #[test]
    fn stored_names_are_safe_and_unique() {
        assert_eq!(stored_name("C:\\x\\..\\a.b.TTF", FIXTURE), "a_b.ttf");
        assert_eq!(stored_name("", FIXTURE), "font.ttf");
        assert_eq!(stored_name("-rf.otf", b"OTTO"), "font-rf.otf");
        assert_eq!(unique_name("a.ttf", |n| n == "a.ttf" || n == "a-2.ttf"), "a-3.ttf");
    }
}
