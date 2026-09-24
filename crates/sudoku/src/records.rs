use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const DIFFICULTY_COUNT: usize = 4;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Records {
    pub best_ms: [Option<u64>; DIFFICULTY_COUNT],
    pub wins: [u32; DIFFICULTY_COUNT],
    pub played: [u32; DIFFICULTY_COUNT],
}

impl Records {
    /// Registers a win. Returns `true` when it sets a new personal best.
    pub fn record_win(&mut self, difficulty: usize, ms: u64) -> bool {
        self.wins[difficulty] = self.wins[difficulty].saturating_add(1);
        match self.best_ms[difficulty] {
            Some(best) if best <= ms => false,
            _ => {
                self.best_ms[difficulty] = Some(ms);
                true
            }
        }
    }

    pub fn record_start(&mut self, difficulty: usize) {
        self.played[difficulty] = self.played[difficulty].saturating_add(1);
    }

    /// Platform-appropriate data directory, if one can be determined.
    pub fn data_dir() -> Option<PathBuf> {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
            && !xdg.is_empty()
        {
            return Some(PathBuf::from(xdg));
        }
        std::env::var("HOME")
            .ok()
            .filter(|h| !h.is_empty())
            .map(|home| {
                #[cfg(target_os = "macos")]
                {
                    PathBuf::from(home)
                        .join("Library")
                        .join("Application Support")
                }
                #[cfg(not(target_os = "macos"))]
                {
                    PathBuf::from(home).join(".local").join("share")
                }
            })
    }

    fn file_path(base: &Path) -> PathBuf {
        base.join("sudoku").join("records.txt")
    }

    pub fn load_default() -> Self {
        Self::data_dir()
            .map(|dir| Self::load(&Self::file_path(&dir)))
            .unwrap_or_default()
    }

    pub fn save_default(&self) {
        if let Some(dir) = Self::data_dir() {
            let _ = self.save(&Self::file_path(&dir));
        }
    }

    pub fn load(path: &Path) -> Self {
        let mut records = Self::default();
        let Ok(text) = fs::read_to_string(path) else {
            return records;
        };
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let (Some(kind), Some(diff), Some(value)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let Ok(difficulty) = diff.parse::<usize>() else {
                continue;
            };
            if difficulty >= DIFFICULTY_COUNT {
                continue;
            }
            match kind {
                "best" => {
                    if let Ok(ms) = value.parse::<u64>() {
                        records.best_ms[difficulty] = Some(ms);
                    }
                }
                "wins" => {
                    if let Ok(n) = value.parse::<u32>() {
                        records.wins[difficulty] = n;
                    }
                }
                "played" => {
                    if let Ok(n) = value.parse::<u32>() {
                        records.played[difficulty] = n;
                    }
                }
                _ => {}
            }
        }
        records
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = String::new();
        for d in 0..DIFFICULTY_COUNT {
            if let Some(ms) = self.best_ms[d] {
                out.push_str(&format!("best {d} {ms}\n"));
            }
            if self.wins[d] > 0 {
                out.push_str(&format!("wins {d} {}\n", self.wins[d]));
            }
            if self.played[d] > 0 {
                out.push_str(&format!("played {d} {}\n", self.played[d]));
            }
        }
        fs::write(path, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sudoku-records-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip_preserves_records() {
        let dir = temp_dir("roundtrip");
        let path = Records::file_path(&dir);

        let mut records = Records::default();
        assert!(records.record_win(0, 61_500));
        records.record_start(0);
        records.record_win(2, 30_000);
        records.record_start(3);
        assert!(records.record_win(3, 20_000));
        assert!(
            !records.record_win(2, 35_000),
            "slower time is not a record"
        );
        records.save(&path).unwrap();

        let loaded = Records::load(&path);
        assert_eq!(records, loaded);
        assert_eq!(loaded.best_ms[0], Some(61_500));
        assert_eq!(loaded.wins[2], 2);
        assert_eq!(loaded.played[0], 1);
        assert_eq!(loaded.best_ms[3], Some(20_000));
        assert_eq!(loaded.wins[3], 1);
        assert_eq!(loaded.played[3], 1);
        assert_eq!(loaded.best_ms[1], None);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_tolerates_missing_and_corrupt_files() {
        let dir = temp_dir("corrupt");
        let path = Records::file_path(&dir);

        assert_eq!(Records::load(&path), Records::default());

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "best 1 notanumber\nbogus x y\nbest 9 5\n\nwins 1 4").unwrap();
        let records = Records::load(&path);
        assert_eq!(records.best_ms[1], None, "malformed values are skipped");
        assert_eq!(records.wins[1], 4);
        assert!(
            records.best_ms.iter().all(|b| b.is_none()),
            "out-of-range difficulty skipped"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn record_win_tracks_best_per_difficulty() {
        let mut records = Records::default();
        assert!(records.record_win(1, 100_000));
        assert!(!records.record_win(1, 200_000));
        assert!(records.record_win(1, 50_000));
        assert_eq!(records.best_ms[1], Some(50_000));
        assert_eq!(records.wins[1], 3);
    }
}
