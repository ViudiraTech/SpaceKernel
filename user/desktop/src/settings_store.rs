/*
 *
 *       user/desktop/src/settings_store.rs
 *       Persistent SPUI appearance settings and wallpaper catalog
 *
 *       2026/10/30 By JiTianYu391
 *       Copyright (C) 2026 ViudiraTech.
 *
 */

//! Reads and writes `/etc/spui/settings.conf` (falling back to `/tmp/spui-settings.conf`
//! when running unprivileged) so theme mode, accent color and wallpaper choices
//! persist across launches and broadcast live via `_SPUI_SETTINGS`.

use std::fs;
use std::path::Path;

pub const SETTINGS_PATH: &str = "/etc/spui/settings.conf";
pub const SETTINGS_FALLBACK_PATH: &str = "/tmp/spui-settings.conf";
pub const WALLPAPER_DIR: &str = "/usr/share/spui/wallpapers";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn parse(raw: &str) -> Self {
        if raw.trim().eq_ignore_ascii_case("light") {
            Self::Light
        } else {
            Self::Dark
        }
    }
}

pub struct WallpaperInfo {
    pub file: &'static str,
    pub label: &'static str,
}

pub const WALLPAPERS: [WallpaperInfo; 4] = [
    WallpaperInfo {
        file: "alpine-peaks.jpg",
        label: "雪峰晴空",
    },
    WallpaperInfo {
        file: "misty-pines.jpg",
        label: "松林远湾",
    },
    WallpaperInfo {
        file: "coastal-horizon.jpg",
        label: "海岸礁石",
    },
    WallpaperInfo {
        file: "emerald-forest.jpg",
        label: "翠谷溪林",
    },
];

pub const ACCENT_PRESETS: [(u32, &str); 5] = [
    (0x3b82f6, "Space 蓝"),
    (0x0ea5e9, "天际青"),
    (0x10b981, "翡翠绿"),
    (0xf59e0b, "琥珀橙"),
    (0xe11d48, "绯红"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpuiSettings {
    pub mode: ThemeMode,
    pub accent: u32,
    pub wallpaper: String,
}

impl Default for SpuiSettings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            accent: ACCENT_PRESETS[0].0,
            wallpaper: WALLPAPERS[0].file.to_string(),
        }
    }
}

impl SpuiSettings {
    pub fn load() -> Self {
        if let Ok(raw) = fs::read_to_string(SETTINGS_PATH) {
            return Self::decode(&raw);
        }
        if let Ok(raw) = fs::read_to_string(SETTINGS_FALLBACK_PATH) {
            return Self::decode(&raw);
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let encoded = self.encode();
        if let Some(parent) = Path::new(SETTINGS_PATH).parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::write(SETTINGS_PATH, &encoded).is_ok() {
            return Ok(());
        }
        fs::write(SETTINGS_FALLBACK_PATH, &encoded).map_err(|e| format!("无法保存配置文件: {e}"))
    }

    pub fn encode(&self) -> String {
        format!(
            "mode={}\naccent={:#08x}\nwallpaper={}\n",
            self.mode.as_str(),
            self.accent & 0x00ff_ffff,
            self.wallpaper
        )
    }

    pub fn decode(raw: &str) -> Self {
        let mut out = Self::default();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "mode" => out.mode = ThemeMode::parse(value),
                "accent" => {
                    let hex = value
                        .strip_prefix("0x")
                        .or_else(|| value.strip_prefix("0X"))
                        .or_else(|| value.strip_prefix('#'))
                        .unwrap_or(value);
                    if let Ok(parsed) = u32::from_str_radix(hex, 16) {
                        out.accent = parsed & 0x00ff_ffff;
                    }
                }
                "wallpaper" => {
                    if !value.is_empty() && !value.contains('/') && !value.contains("..") {
                        out.wallpaper = value.to_string();
                    }
                }
                _ => {}
            }
        }
        out
    }

    pub fn wallpaper_path(&self) -> String {
        format!("{WALLPAPER_DIR}/{}", self.wallpaper)
    }
}
