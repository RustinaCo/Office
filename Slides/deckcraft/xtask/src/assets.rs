//! `cargo xtask assets`: every non-code asset must be attributed in `ATTRIBUTION.md`.
//!
//! Scans all git-tracked (and untracked, not ignored) files with an asset extension, plus
//! everything under `assets/`, `docs/images/` and `examples/`, and fails if a path is not listed
//! as `` `path` `` in ATTRIBUTION.md. See the asset policy in AGENTS.md.

use std::path::Path;
use std::process::Command;

const ASSET_EXT: &[&str] = &[
    "png",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "bmp",
    "tif",
    "tiff",
    "ico",
    "icns",
    "svg",
    "pdf",
    "ai",
    "eps",
    "psd",
    "icc",
    "icm",
    "ttf",
    "otf",
    "woff",
    "woff2",
    "ase",
    "indd",
    "idml",
    "aco",
    "abr",
    "deckcraft",
    "pptx",
    "potx",
    "ppsx",
    "thmx",
    "mp4",
    "mov",
    "webm",
    "wav",
    "mp3",
    "m4a",
    "ogg",
    "flac",
];
const ASSET_DIRS: &[&str] = &["assets/", "docs/images/", "docs/brand/", "examples/"];

pub fn is_asset(path: &str) -> bool {
    let ext = Path::new(path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    ASSET_DIRS.iter().any(|d| path.starts_with(d)) || ASSET_EXT.contains(&ext.as_str())
}

/// Paths that need an ATTRIBUTION.md entry but lack one.
pub fn missing(files: &[String], assets_md: &str) -> Vec<String> {
    let patterns: Vec<&str> = assets_md.split('`').skip(1).step_by(2).collect();
    files.iter().filter(|f| is_asset(f) && !patterns.iter().any(|p| glob(p, f))).cloned().collect()
}

/// `*` matches any run of characters except `/`; everything else literally.
fn glob(pat: &str, s: &str) -> bool {
    match pat.split_once('*') {
        None => pat == s,
        Some((head, tail)) => {
            let Some(rest) = s.strip_prefix(head) else { return false };
            (0..=rest.len()).take_while(|&i| i == 0 || !rest[..i].contains('/')).any(|i| rest.is_char_boundary(i) && glob(tail, &rest[i..]))
        }
    }
}

pub fn run(root: &Path) -> Result<(), String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    let files: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().filter(|l| root.join(l).exists()).map(str::to_owned).collect();
    let md = std::fs::read_to_string(root.join("ATTRIBUTION.md")).map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let miss = missing(&files, &md);
    if miss.is_empty() {
        println!("assets: all {} asset files attributed in ATTRIBUTION.md", files.iter().filter(|f| is_asset(f)).count());
        Ok(())
    } else {
        Err(format!("{} asset file(s) lack an ATTRIBUTION.md entry (author, source, licence):\n  {}", miss.len(), miss.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unattributed_assets() {
        let files = vec!["assets/icons/a.svg".to_string(), "crates/x/src/lib.rs".into(), "docs/images/b.png".into(), "tests/fixture.png".into()];
        let md = "| `assets/icons/a.svg` | me | here | MIT |";
        assert_eq!(missing(&files, md), vec!["docs/images/b.png".to_string(), "tests/fixture.png".into()]);
        assert!(!is_asset("crates/x/src/lib.rs"));
        assert!(is_asset("assets/fonts/OFL.txt"));
        let md = "`assets/app-icon/hicolor/*/apps/x.png`";
        assert!(missing(&["assets/app-icon/hicolor/16x16/apps/x.png".into()], md).is_empty());
        assert_eq!(missing(&["assets/app-icon/hicolor/a/b/apps/x.png".into()], md).len(), 1);
    }
}
