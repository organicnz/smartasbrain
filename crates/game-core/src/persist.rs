//! Session persistence: atomic text blobs under a per-user data directory.
//!
//! Layout: `$SMARTASBRAIN_DATA_DIR` override, else `$HOME/.local/share/
//! smartasbrain/`. Writes go through a temp file + rename so a crash can
//! never leave a half-written save behind.

use std::{fs, io, path::PathBuf, sync::OnceLock};

static DIR_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

/// Test hook: redirect every save/load to an isolated directory.
#[doc(hidden)]
pub fn set_data_dir(path: PathBuf) {
    let _ = DIR_OVERRIDE.set(path);
}

fn base_dir() -> Option<PathBuf> {
    if let Some(p) = DIR_OVERRIDE.get() {
        return Some(p.clone());
    }
    if let Ok(d) = std::env::var("SMARTASBRAIN_DATA_DIR") {
        return Some(PathBuf::from(d));
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".local/share/smartasbrain"))
}

/// Atomically writes `blob` as `<dir>/<name>.sav`.
pub fn save(name: &str, blob: &str) -> io::Result<()> {
    let Some(dir) = base_dir() else {
        return Err(io::Error::other("no data directory available"));
    };
    fs::create_dir_all(&dir)?;
    let target = dir.join(format!("{name}.sav"));
    let tmp = dir.join(format!("{name}.sav.tmp"));
    fs::write(&tmp, blob)?;
    fs::rename(&tmp, &target)
}

/// Loads `<dir>/<name>.sav`, tolerating absence and bad bytes.
pub fn load(name: &str) -> Option<String> {
    let dir = base_dir()?;
    let raw = fs::read_to_string(dir.join(format!("{name}.sav"))).ok()?;
    Some(raw.trim_end().to_string())
}

/// Removes a blob (used when a game restarts from scratch).
pub fn clear(name: &str) {
    if let Some(dir) = base_dir() {
        let _ = fs::remove_file(dir.join(format!("{name}.sav")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_clear_round_trip_in_isolated_dir() {
        let tmp = std::env::temp_dir().join(format!("sab-persist-test-{}", std::process::id()));
        set_data_dir(tmp.clone());
        assert!(load("roundtrip").is_none());
        save("roundtrip", "v1|hello|42").expect("save");
        assert_eq!(load("roundtrip").as_deref(), Some("v1|hello|42"));
        // Overwrite stays atomic and last-write-wins.
        save("roundtrip", "v2").expect("resave");
        assert_eq!(load("roundtrip").as_deref(), Some("v2"));
        clear("roundtrip");
        assert!(load("roundtrip").is_none());
        let _ = fs::remove_dir_all(tmp);
    }
}
