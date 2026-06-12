use serde::{de::DeserializeOwned, Deserialize};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use tracing::warn;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppConfig {
    pub window: WindowConfig,
    pub search: SearchConfig,
    pub list: ListConfig,
    pub image: ImageConfig,
    pub behavior: BehaviorConfig,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WindowConfig {
    pub width: i32,
    pub height: i32,
    pub position: WindowPosition,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WindowPosition {
    #[default]
    Center,
    Offset,
    Top,
    TopLeft,
    TopRight,
    Bottom,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SearchConfig {
    pub placeholder: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ListConfig {
    pub max_text_chars: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ImageConfig {
    pub width: u32,
    pub height: u32,
    pub show_details: bool,
    pub preserve_aspect_ratio: bool,
    pub rounded_corners: bool,
    pub concurrent_jobs: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BehaviorConfig {
    pub close_on_copy: bool,
    pub reload_on_open: bool,
    pub start_in_insert: bool,
    pub show_keybinds: bool,
    pub show_vim_mode: bool,
    pub click_to_copy: ClickToCopy,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClickToCopy {
    #[default]
    Single,
    Double,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 760,
            height: 620,
            position: WindowPosition::Center,
            offset_x: 0,
            offset_y: 0,
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            placeholder: "Search clipboard...".into(),
        }
    }
}

impl Default for ListConfig {
    fn default() -> Self {
        Self {
            max_text_chars: 180,
        }
    }
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            width: 260,
            height: 140,
            show_details: true,
            preserve_aspect_ratio: true,
            rounded_corners: true,
            concurrent_jobs: 3,
        }
    }
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            close_on_copy: true,
            reload_on_open: true,
            start_in_insert: false,
            show_keybinds: true,
            show_vim_mode: true,
            click_to_copy: ClickToCopy::Single,
        }
    }
}

pub fn load() -> AppConfig {
    config_path()
        .map(|path| load_path(&path))
        .unwrap_or_default()
}

fn load_path(path: &Path) -> AppConfig {
    match fs::read_to_string(path) {
        Ok(contents) => parse_config(&contents, path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => AppConfig::default(),
        Err(err) => {
            warn!(path = %path.display(), error = %err, "failed to read config; using defaults");
            AppConfig::default()
        }
    }
}

fn parse_config(contents: &str, path: &Path) -> AppConfig {
    match json5::from_str::<Value>(contents) {
        Ok(root) => AppConfig {
            window: load_section(&root, "window", path),
            search: load_section(&root, "search", path),
            list: load_section(&root, "list", path),
            image: load_section(&root, "image", path),
            behavior: load_section(&root, "behavior", path),
        },
        Err(err) => {
            warn!(path = %path.display(), error = %err, "invalid JSON5 config file; using defaults");
            AppConfig::default()
        }
    }
}

fn load_section<T>(root: &Value, section: &str, path: &Path) -> T
where
    T: DeserializeOwned + Default,
{
    let Some(value) = root.get(section) else {
        return T::default();
    };

    match serde_json::from_value::<T>(value.clone()) {
        Ok(section_config) => section_config,
        Err(err) => {
            warn!(path = %path.display(), section, error = %err, "invalid config section; using defaults for section");
            T::default()
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    if let Ok(config_home) = env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("cliphist-ui/config.json5"));
    }

    env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".config/cliphist-ui/config.json5"))
}

pub fn style_path() -> Option<PathBuf> {
    if let Ok(config_home) = env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("cliphist-ui/style.css"));
    }

    env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".config/cliphist-ui/style.css"))
}

pub fn cache_dir() -> PathBuf {
    if let Ok(cache_home) = env::var("XDG_CACHE_HOME") {
        return PathBuf::from(cache_home).join("cliphist-ui/thumbnails");
    }

    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".cache/cliphist-ui/thumbnails")
}

pub fn css() -> &'static str {
    r#"
window.cliphist-ui-window {
  background: transparent;
  color: #dee4e0;
  font-family: Inter, Cantarell, sans-serif;
  font-size: 14px;
}

.cliphist-root {
  background: #0f1512;
  border: 2px solid #27302b;
  border-radius: 10px;
  padding: 10px;
}

.cliphist-header {
  border-radius: 10px;
  margin-bottom: 2px;
}

.cliphist-search {
  background: #141c18;
  color: #dee4e0;
  border: 0;
  border-radius: 10px;
  padding: 9px 12px;
  box-shadow: none;
  caret-color: #dee4e0;
}

.cliphist-search:focus {
  outline: none;
  box-shadow: inset 0 0 0 1px #27302b;
}

.cliphist-list {
  background: #0f1512;
  color: #dee4e0;
  outline: none;
  box-shadow: none;
}

.cliphist-list:focus,
.cliphist-list:focus-visible,
.cliphist-list:focus-within {
  outline: none;
  box-shadow: none;
}

.cliphist-list row {
  background: transparent;
  border-radius: 10px;
  color: #dee4e0;
  margin-bottom: 6px;
  outline: none;
  outline-color: transparent;
  box-shadow: none;
  padding: 8px 10px;
  transition: none;
}

.cliphist-list row:hover,
.cliphist-list row:focus,
.cliphist-list row:focus-visible,
.cliphist-list row:focus-within,
.cliphist-list row:selected:focus,
.cliphist-list row:selected:focus-visible,
.cliphist-list row:selected:focus-within {
  outline: none;
  outline-color: transparent;
  box-shadow: none;
  transition: none;
}

.cliphist-list row:selected {
  background: #a9cbe2;
  color: #0e3446;
  outline: none;
  outline-color: transparent;
  box-shadow: none;
  transition: none;
}

.cliphist-list row:selected label {
  color: #0e3446;
}

.cliphist-kind {
  color: #8f9a95;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.08em;
}

.cliphist-preview {
  color: #dee4e0;
}

.cliphist-muted {
  color: #8f9a95;
}

.cliphist-footer {
  min-height: 24px;
  padding: 4px 2px 0 2px;
}

.cliphist-mode {
  border-radius: 5px;
  min-height: 28px;
  min-width: 6px;
  padding: 0;
}

.cliphist-mode-normal {
  background: #8f9a95;
}

.cliphist-mode-insert {
  background: #a9cbe2;
}

.cliphist-hints {
  color: #8f9a95;
  margin: 0;
}

.cliphist-hints flowboxchild {
  background: transparent;
  padding: 0;
}

.cliphist-hints flowboxchild:selected {
  background: transparent;
}

.cliphist-keyhint {
  background: transparent;
  border-radius: 10px;
  padding: 1px 6px 1px 0;
}

.cliphist-keyhint-key {
  background: #141c18;
  border: 1px solid #27302b;
  border-radius: 10px;
  color: #dee4e0;
  font-family: monospace;
  font-size: 12px;
  font-weight: 800;
  min-width: 18px;
  padding: 1px 5px;
}

.cliphist-keyhint-action {
  color: #8f9a95;
  font-size: 12px;
}

.cliphist-status {
  color: #8f9a95;
  font-size: 12px;
  padding: 2px 0 0 0;
}

.cliphist-thumb-placeholder {
  background: #18201c;
  border-radius: 8px;
}
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_json5_config_uses_defaults() {
        let path = env::temp_dir().join(format!(
            "cliphist-ui-missing-config-{}.json5",
            std::process::id()
        ));

        assert_eq!(load_path(&path), AppConfig::default());
    }

    #[test]
    fn parses_json5_comments_and_trailing_commas() {
        let config = parse_config(
            r##"
            {
              // JSON5 comments and trailing commas are allowed.
              window: {
                width: 900,
                position: "top-right",
              },
              search: {
                placeholder: "Find a clip",
              },
              image: {
                width: 320,
                show_details: false,
                rounded_corners: false,
              },
              behavior: {
                close_on_copy: false,
                show_keybinds: false,
                click_to_copy: "double",
              },
            }
            "##,
            Path::new("test-config.json5"),
        );

        assert_eq!(config.window.width, 900);
        assert_eq!(config.window.position, WindowPosition::TopRight);
        assert_eq!(config.search.placeholder, "Find a clip");
        assert_eq!(config.image.width, 320);
        assert!(!config.image.show_details);
        assert!(!config.image.rounded_corners);
        assert!(!config.behavior.close_on_copy);
        assert!(!config.behavior.show_keybinds);
        assert_eq!(config.behavior.click_to_copy, ClickToCopy::Double);
    }

    #[test]
    fn missing_sections_fall_back_to_defaults() {
        let config = parse_config(
            r#"
            {
              list: {
                max_text_chars: 90,
              },
            }
            "#,
            Path::new("test-config.json5"),
        );

        assert_eq!(config.list.max_text_chars, 90);
        assert_eq!(config.window, WindowConfig::default());
        assert_eq!(config.image, ImageConfig::default());
        assert_eq!(config.behavior, BehaviorConfig::default());
    }

    #[test]
    fn invalid_section_falls_back_to_default_section() {
        let config = parse_config(
            r#"
            {
              window: "not a window table",
              list: {
                max_text_chars: 72,
              },
            }
            "#,
            Path::new("test-config.json5"),
        );

        assert_eq!(config.window, WindowConfig::default());
        assert_eq!(config.list.max_text_chars, 72);
    }

    #[test]
    fn invalid_json5_root_uses_defaults() {
        assert_eq!(
            parse_config("{ window: ", Path::new("test-config.json5")),
            AppConfig::default()
        );
    }

    #[test]
    fn parses_offset_window_position() {
        let config = parse_config(
            r#"
            {
              window: {
                position: "offset",
                offset_x: 24,
                offset_y: 48,
              },
            }
            "#,
            Path::new("test-config.json5"),
        );

        assert_eq!(config.window.position, WindowPosition::Offset);
        assert_eq!(config.window.offset_x, 24);
        assert_eq!(config.window.offset_y, 48);
    }
}
