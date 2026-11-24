use std::{env, fs, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Palette {
    pub background: u32,
    pub text: u32,
    pub snake: u32,
    pub food: u32,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            background: 0x171922,
            text: 0xeff0f5,
            snake: 0x8bdcc8,
            food: 0xff7a9c,
        }
    }
}

impl Palette {
    fn parse(contents: &str) -> Self {
        let mut palette = Self::default();
        let (mut accent, mut color2) = (None, None);
        for (key, value) in contents.lines().filter_map(|line| line.split_once('=')) {
            let Some(color) = hex_color(value.trim_start()) else {
                continue;
            };
            match key.trim() {
                "background" => palette.background = color,
                "foreground" => palette.text = color,
                "accent" => accent = Some(color),
                "color2" => color2 = Some(color),
                "color1" => palette.food = color,
                _ => {}
            }
        }
        palette.snake = accent.or(color2).unwrap_or(palette.snake);
        palette
    }

    pub fn head_mark(self) -> u32 {
        let linear = |shift| {
            let channel = ((self.snake >> shift) & 0xff_u32) as f32 / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        let luminance = 0.2126 * linear(16) + 0.7152 * linear(8) + 0.0722 * linear(0);
        if luminance > 0.179 { 0 } else { 0xffffff }
    }

    pub fn shade(self, percent: u32) -> u32 {
        let channel = |shift| -> u32 {
            let background = (self.background >> shift) & 0xff_u32;
            let text = (self.text >> shift) & 0xff_u32;
            ((background * (100 - percent) + text * percent) / 100_u32) << shift
        };
        channel(16) | channel(8) | channel(0)
    }
}

fn hex_color(value: &str) -> Option<u32> {
    let quote = value.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    let (literal, trailing) = value[1..].split_once(quote)?;
    let hex = literal.strip_prefix('#')?;
    let trailing = trailing.trim_start();
    if hex.len() == 6
        && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        && (trailing.is_empty() || trailing.starts_with('#'))
    {
        u32::from_str_radix(hex, 16).ok()
    } else {
        None
    }
}

pub fn active_path() -> Option<PathBuf> {
    let absolute_env = |name| {
        env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let state = absolute_env("XDG_STATE_HOME")
        .or_else(|| Some(absolute_env("HOME")?.join(".local/state")))?;
    Some(state.join("omarchy/current/theme/colors.toml"))
}

pub struct Theme {
    path: Option<PathBuf>,
    pub palette: Palette,
}

impl Theme {
    pub fn new(path: Option<PathBuf>) -> Self {
        let mut theme = Self {
            path,
            palette: Palette::default(),
        };
        theme.reload();
        theme
    }

    pub fn reload(&mut self) -> bool {
        let Some(contents) = self
            .path
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
        else {
            return false;
        };
        let next = Palette::parse(&contents);
        if next == self.palette {
            return false;
        }
        self.palette = next;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_palette_variants_and_keeps_defaults_for_invalid_values() {
        let palette = Palette::parse(
            "background = '#102030'\nforeground = \"#effeed\"\naccent = \"#4a91ca\" # active\ncolor1 = 'invalid'\n",
        );
        assert_eq!(palette.background, 0x102030);
        assert_eq!(palette.text, 0xeffeed);
        assert_eq!(palette.snake, 0x4a91ca);
        assert_eq!(palette.food, Palette::default().food);
    }

    #[test]
    fn reload_detects_atomic_palette_replacement_without_recreating_watcher() {
        use std::time::{SystemTime, UNIX_EPOCH};

        struct Fixture(PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!("snake-theme-test-{}-{nonce}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let fixture = Fixture(dir);
        let path = fixture.0.join("colors.toml");
        let replacement = fixture.0.join("new.toml");
        fs::write(&path, "accent = '#c0ff33'\n").unwrap();
        let mut theme = Theme::new(Some(path.clone()));
        assert_eq!(theme.palette.snake, 0xc0ff33);
        assert!(!theme.reload());
        fs::write(&replacement, "accent = '#33ffca'\n").unwrap();
        fs::rename(&replacement, &path).unwrap();
        assert!(theme.reload());
        assert_eq!(theme.palette.snake, 0x33ffca);
    }
}
