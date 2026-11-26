use omarchy_snake::Difficulty;
use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Preferences {
    // The original `best` entry remains the Normal record.
    pub best: u32,
    pub muted: bool,
    easy_best: u32,
    hard_best: u32,
    pub last_difficulty: Difficulty,
}

impl Preferences {
    pub(crate) fn best_for(self, difficulty: Difficulty) -> u32 {
        match difficulty {
            Difficulty::Easy => self.easy_best,
            Difficulty::Normal => self.best,
            Difficulty::Hard => self.hard_best,
        }
    }
}

fn state_path(state_home: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    let root = state_home
        .filter(|path| path.is_absolute())
        .or_else(|| Some(home.filter(|path| path.is_absolute())?.join(".local/state")))?;
    Some(root.join("omarchy-snake/player-state"))
}

pub fn user_state_path() -> Option<PathBuf> {
    state_path(
        env::var_os("XDG_STATE_HOME").map(PathBuf::from),
        env::var_os("HOME").map(PathBuf::from),
    )
}

pub struct Player {
    path: Option<PathBuf>,
    preferences: Preferences,
}

impl Player {
    pub fn load(path: Option<PathBuf>) -> Self {
        let preferences = path.as_deref().map_or_else(Preferences::default, |path| {
            match fs::read_to_string(path).and_then(|contents| {
                parse(&contents).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid player state")
                })
            }) {
                Ok(preferences) => preferences,
                Err(error) => {
                    if error.kind() != io::ErrorKind::NotFound {
                        eprintln!("Omarchy Snake: could not load player state: {error}");
                    }
                    Preferences::default()
                }
            }
        });
        Self { path, preferences }
    }

    pub fn preferences(&self) -> Preferences {
        self.preferences
    }

    pub fn record(&mut self, difficulty: Difficulty, score: u32) -> io::Result<()> {
        let best = match difficulty {
            Difficulty::Easy => &mut self.preferences.easy_best,
            Difficulty::Normal => &mut self.preferences.best,
            Difficulty::Hard => &mut self.preferences.hard_best,
        };
        if score <= *best {
            return Ok(());
        }
        *best = score;
        self.save()
    }

    pub fn remember_difficulty(&mut self, difficulty: Difficulty) -> io::Result<()> {
        if difficulty == self.preferences.last_difficulty {
            return Ok(());
        }
        self.preferences.last_difficulty = difficulty;
        self.save()
    }

    pub fn toggle_mute(&mut self) -> io::Result<()> {
        self.preferences.muted = !self.preferences.muted;
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        if let Some(path) = &self.path {
            return save_atomically(path, self.preferences);
        }
        Ok(())
    }
}

fn parse(contents: &str) -> Option<Preferences> {
    let mut lines = contents.lines();
    let best = lines.next()?.strip_prefix("best=")?.parse().ok()?;
    let muted = lines.next()?.strip_prefix("muted=")?.parse().ok()?;
    let mut preferences = Preferences {
        best,
        muted,
        ..Preferences::default()
    };
    if let Some(easy) = lines.next() {
        preferences.easy_best = easy.strip_prefix("easy_best=")?.parse().ok()?;
        preferences.hard_best = lines.next()?.strip_prefix("hard_best=")?.parse().ok()?;
        preferences.last_difficulty = match lines.next()?.strip_prefix("last_difficulty=")? {
            "easy" => Difficulty::Easy,
            "normal" => Difficulty::Normal,
            "hard" => Difficulty::Hard,
            _ => return None,
        };
    }
    lines.next().is_none().then_some(preferences)
}

fn save_atomically(path: &Path, preferences: Preferences) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "player state needs a parent")
    })?;
    fs::create_dir_all(dir)?;
    // Stage beside the destination so a rename never crosses filesystems.
    let (temp, mut file) = loop {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let temp = dir.join(format!(".player-state-{}-{serial}", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
        {
            Ok(file) => break (temp, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let last = match preferences.last_difficulty {
        Difficulty::Easy => "easy",
        Difficulty::Normal => "normal",
        Difficulty::Hard => "hard",
    };
    let result = writeln!(
        file,
        "best={}\nmuted={}\neasy_best={}\nhard_best={}\nlast_difficulty={last}",
        preferences.best, preferences.muted, preferences.easy_best, preferences.hard_best,
    );
    drop(file);
    let result = result.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    pub(crate) struct Scratch(PathBuf);

    impl Scratch {
        pub(crate) fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "omarchy-snake-test-{}-{nonce}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&dir).unwrap();
            Self(dir)
        }

        pub(crate) fn state_path(&self) -> PathBuf {
            self.0.join("app/player-state")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn state_path_uses_only_absolute_user_local_roots() {
        let home = || Some(PathBuf::from("/home/test"));
        assert_eq!(
            state_path(Some("/my/state".into()), home()),
            Some("/my/state/omarchy-snake/player-state".into())
        );
        assert_eq!(
            state_path(Some("relative".into()), home()),
            Some("/home/test/.local/state/omarchy-snake/player-state".into())
        );
        assert_eq!(state_path(None, Some("relative".into())), None);
    }

    #[test]
    fn legacy_state_becomes_normal_and_records_stay_separate_across_reopens() {
        let scratch = Scratch::new();
        let path = scratch.state_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "best=42\nmuted=true\n").unwrap();
        let mut player = Player::load(Some(path.clone()));
        let records = |snapshot: Preferences| {
            [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard]
                .map(|difficulty| snapshot.best_for(difficulty))
        };
        assert_eq!(records(player.preferences()), [0, 42, 0]);
        assert_eq!(player.preferences().last_difficulty, Difficulty::Normal);
        assert!(player.preferences().muted);
        player.record(Difficulty::Easy, 7).unwrap();
        player.record(Difficulty::Hard, 4).unwrap();
        player.record(Difficulty::Easy, 2).unwrap();
        let reopened = Player::load(Some(path)).preferences();
        assert_eq!(records(reopened), [7, 42, 4]);
        assert!(reopened.muted);
    }

    #[test]
    fn record_never_decreases_and_both_preferences_survive_reopen() {
        let scratch = Scratch::new();
        let path = scratch.state_path();
        let mut player = Player::load(Some(path.clone()));
        assert_eq!(player.preferences(), Preferences::default());
        player.record(Difficulty::Normal, 10).unwrap();
        player.toggle_mute().unwrap();
        let mut reopened = Player::load(Some(path.clone()));
        assert_eq!(
            reopened.preferences(),
            Preferences {
                best: 10,
                muted: true,
                ..Preferences::default()
            }
        );
        reopened.record(Difficulty::Normal, 7).unwrap();
        reopened.toggle_mute().unwrap();
        assert_eq!(
            Player::load(Some(path)).preferences(),
            Preferences {
                best: 10,
                ..Preferences::default()
            }
        );
    }

    #[test]
    fn failed_save_keeps_running_preferences_and_cleans_up_staging_file() {
        let scratch = Scratch::new();
        let path = scratch.state_path();
        let mut player = Player::load(Some(path.clone()));

        fs::create_dir_all(&path).unwrap();
        assert!(player.record(Difficulty::Normal, 12).is_err());
        assert!(player.toggle_mute().is_err());
        assert_eq!(
            player.preferences(),
            Preferences {
                best: 12,
                muted: true,
                ..Preferences::default()
            }
        );
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }
}
