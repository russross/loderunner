//! XDG paths and durable snapshots of application preferences.

use crate::presentation::RenderMode;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const CLASSIC: &str = "Classic Lode Runner";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DisplaySettings {
    pub renderer: RenderMode,
    pub size: [u32; 2],
    pub commodore: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            renderer: RenderMode::Crt,
            size: [1120, 768],
            commodore: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub display: DisplaySettings,
    pub muted: bool,
    pub frame_period: u8,
    pub demo_speed: u8,
    pub pack: String,
    pub levels: BTreeMap<String, u8>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            display: DisplaySettings::default(),
            muted: false,
            frame_period: 6,
            demo_speed: 2,
            pack: CLASSIC.into(),
            levels: BTreeMap::new(),
        }
    }
}

pub struct Paths {
    pub catalog: PathBuf,
    pub player: PathBuf,
    pub config: PathBuf,
}

fn xdg(variable: &str, fallback: &Path) -> PathBuf {
    env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| fallback.to_owned())
}

impl Paths {
    pub fn discover() -> Result<Self, String> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or("HOME must be an absolute path")?;
        let data = xdg("XDG_DATA_HOME", &home.join(".local/share")).join("loderunner");
        let mut catalog = data.join("levels.sqlite3");
        // System-wide catalogs follow the same search order as other XDG assets.
        if !catalog.is_file() {
            for directory in env::split_paths(
                &env::var_os("XDG_DATA_DIRS")
                    .unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
            ) {
                if directory.is_absolute() && directory.join("loderunner/levels.sqlite3").is_file()
                {
                    catalog = directory.join("loderunner/levels.sqlite3");
                    break;
                }
            }
        }
        Ok(Self {
            catalog,
            player: data.join("player.sqlite3"),
            config: xdg("XDG_CONFIG_HOME", &home.join(".config")).join("loderunner/config.json"),
        })
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        let settings: Self =
            serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        if settings.frame_period > 15
            || !(1..=16).contains(&settings.demo_speed)
            || settings
                .display
                .size
                .iter()
                .any(|&size| size == 0 || size > 16384)
            || settings
                .levels
                .values()
                .any(|&level| !(1..=150).contains(&level))
        {
            return Err(format!("{}: invalid preferences", path.display()));
        }
        Ok(settings)
    }

    // Atomic replacement prevents an interrupted write from corrupting preferences.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or("configuration has no parent directory")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = parent.join(format!("config.{}.tmp", std::process::id()));
        let text = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        fs::write(&temporary, text)
            .and_then(|()| fs::rename(&temporary, path))
            .map_err(|error| format!("{}: {error}", path.display()))
    }
}
