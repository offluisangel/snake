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
    pub best: u32,
    pub muted: bool,
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

    pub fn record(&mut self, score: u32) -> io::Result<()> {
        if score <= self.preferences.best {
            return Ok(());
        }
        self.preferences.best = score;
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
    lines
        .next()
        .is_none()
        .then_some(Preferences { best, muted })
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
    let result = writeln!(
        file,
        "best={}\nmuted={}",
        preferences.best, preferences.muted
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
    fn record_never_decreases_and_both_preferences_survive_reopen() {
        let scratch = Scratch::new();
        let path = scratch.state_path();
        let mut player = Player::load(Some(path.clone()));
        assert_eq!(player.preferences(), Preferences::default());
        player.record(10).unwrap();
        player.toggle_mute().unwrap();
        let mut reopened = Player::load(Some(path.clone()));
        assert_eq!(
            reopened.preferences(),
            Preferences {
                best: 10,
                muted: true
            }
        );
        reopened.record(7).unwrap();
        reopened.toggle_mute().unwrap();
        assert_eq!(
            Player::load(Some(path)).preferences(),
            Preferences {
                best: 10,
                muted: false
            }
        );
    }

    #[test]
    fn failed_save_keeps_running_preferences_and_cleans_up_staging_file() {
        let scratch = Scratch::new();
        let path = scratch.state_path();
        let mut player = Player::load(Some(path.clone()));

        fs::create_dir_all(&path).unwrap();
        assert!(player.record(12).is_err());
        assert!(player.toggle_mute().is_err());
        assert_eq!(
            player.preferences(),
            Preferences {
                best: 12,
                muted: true
            }
        );
        assert!(path.is_dir());
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }
}
