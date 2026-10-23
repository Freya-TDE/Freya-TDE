//! Theme and metric tables per platform: spacing, font size, control sizes
//! and colors the designer draws with.
//!
//! Optical adjustments are data changes, not code changes: the built-in
//! tables live in `designer/themes/<platform>.toml`, and a project or user
//! can load its own with [`Theme::parse`].
//!
//! ```toml
//! name = "linux"
//! font-size = 14          # px
//! char-width = 0.55       # average glyph width in em (text is estimated)
//! title-bar-height = 30
//! padding = 8             # default padding of containers without a layout
//! spacing = 6             # default gap between children
//! radius = 4
//! check-size = 16
//! tray-height = 64        # strip for non-visual components
//! icon-size = 28
//! margin = 24             # free space around the form
//!
//! [colors]                # "#rrggbb" or "#rrggbbaa"
//! desktop = "#e4e4e4"
//! …
//!
//! [widgets."core/Button"] # natural size of a type (by type reference)
//! height = 34
//! min-width = 80
//! padding-x = 14
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use tdr_core::{Color, Platform, TypeRef};

/// Natural size rules of one widget type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct WidgetMetrics {
    pub height: i32,
    pub min_width: i32,
    /// Space left and right of the text.
    #[serde(default)]
    pub padding_x: i32,
}

impl Default for WidgetMetrics {
    fn default() -> Self {
        Self {
            height: 30,
            min_width: 80,
            padding_x: 8,
        }
    }
}

/// Colors of the design surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Colors {
    #[serde(deserialize_with = "color")]
    pub desktop: Color,
    #[serde(deserialize_with = "color")]
    pub window: Color,
    #[serde(deserialize_with = "color")]
    pub window_border: Color,
    #[serde(deserialize_with = "color")]
    pub title_bar: Color,
    #[serde(deserialize_with = "color")]
    pub title_text: Color,
    #[serde(deserialize_with = "color")]
    pub control: Color,
    #[serde(deserialize_with = "color")]
    pub control_border: Color,
    #[serde(deserialize_with = "color")]
    pub input: Color,
    #[serde(deserialize_with = "color")]
    pub text: Color,
    #[serde(deserialize_with = "color")]
    pub disabled_text: Color,
    #[serde(deserialize_with = "color")]
    pub placeholder: Color,
    #[serde(deserialize_with = "color")]
    pub accent: Color,
    #[serde(deserialize_with = "color")]
    pub container_border: Color,
    #[serde(deserialize_with = "color")]
    pub selection: Color,
    #[serde(deserialize_with = "color")]
    pub tray: Color,
}

/// A theme: metrics and colors of one platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Theme {
    pub name: String,
    pub font_size: i32,
    pub char_width: f32,
    pub title_bar_height: i32,
    pub padding: i32,
    pub spacing: i32,
    pub radius: i32,
    pub check_size: i32,
    pub tray_height: i32,
    pub icon_size: i32,
    pub margin: i32,
    pub colors: Colors,
    /// Natural sizes by type reference (`"core/Button"`).
    #[serde(default)]
    pub widgets: BTreeMap<String, WidgetMetrics>,
}

const LINUX: &str = include_str!("../themes/linux.toml");
const WINDOWS: &str = include_str!("../themes/windows.toml");
const MACOS: &str = include_str!("../themes/macos.toml");

impl Theme {
    /// Reads a theme table.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// The built-in theme of `platform` (Linux for platforms without one).
    pub fn for_platform(platform: &Platform) -> Self {
        let text = match platform {
            Platform::Windows => WINDOWS,
            Platform::MacOs | Platform::Ios => MACOS,
            _ => LINUX,
        };
        Self::parse(text).expect("built-in themes are valid")
    }

    /// Natural size rules of a type.
    pub fn metrics(&self, type_ref: &TypeRef) -> WidgetMetrics {
        self.widgets
            .get(&type_ref.to_string())
            .copied()
            .unwrap_or_default()
    }

    /// Estimated width of `text` in this theme's font. The designer only
    /// needs a stable approximation; the real look comes from the debug run.
    pub fn text_width(&self, text: &str) -> i32 {
        let em = self.font_size as f32;
        (text.chars().count() as f32 * em * self.char_width).ceil() as i32
    }
}

fn color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse_color(&text).ok_or_else(|| {
        serde::de::Error::custom(format!(
            "expected \"#rrggbb\" or \"#rrggbbaa\", found {text:?}"
        ))
    })
}

fn parse_color(text: &str) -> Option<Color> {
    let hex = text.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Color::rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_themes_load() {
        for platform in [Platform::Linux, Platform::Windows, Platform::MacOs] {
            let theme = Theme::for_platform(&platform);
            assert!(theme.metrics(&tdr_core::builtin::button()).height > 0);
        }
        let theme = Theme::for_platform(&Platform::Linux);
        assert_eq!(theme.text_width(""), 0);
        assert!(theme.text_width("OK") > 0);
        assert!(Theme::parse("name = 1").is_err());
    }
}
