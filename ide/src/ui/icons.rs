//! Icons of components and widgets: the built-in set (`ide/assets/palette/`,
//! 48 px, sources and generator `make.py` next to them) and icons a pack
//! brings as `icons/<name>.png` in its folder (they take precedence).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ruf::Image;

/// The built-in icons by name.
const BUILTIN: &[(&str, &[u8])] = &[
    (
        "accordion",
        include_bytes!("../../assets/palette/accordion.png"),
    ),
    ("button", include_bytes!("../../assets/palette/button.png")),
    (
        "calendar",
        include_bytes!("../../assets/palette/calendar.png"),
    ),
    ("camera", include_bytes!("../../assets/palette/camera.png")),
    ("chart", include_bytes!("../../assets/palette/chart.png")),
    ("check", include_bytes!("../../assets/palette/check.png")),
    (
        "checkbox",
        include_bytes!("../../assets/palette/checkbox.png"),
    ),
    ("chip", include_bytes!("../../assets/palette/chip.png")),
    ("clippy", include_bytes!("../../assets/palette/clippy.png")),
    (
        "colorpicker",
        include_bytes!("../../assets/palette/colorpicker.png"),
    ),
    (
        "component",
        include_bytes!("../../assets/palette/component.png"),
    ),
    (
        "container",
        include_bytes!("../../assets/palette/container.png"),
    ),
    ("cpu", include_bytes!("../../assets/palette/cpu.png")),
    (
        "database",
        include_bytes!("../../assets/palette/database.png"),
    ),
    ("doc", include_bytes!("../../assets/palette/doc.png")),
    (
        "dropdown",
        include_bytes!("../../assets/palette/dropdown.png"),
    ),
    ("edit", include_bytes!("../../assets/palette/edit.png")),
    ("form", include_bytes!("../../assets/palette/form.png")),
    ("format", include_bytes!("../../assets/palette/format.png")),
    ("frame", include_bytes!("../../assets/palette/frame.png")),
    ("globe", include_bytes!("../../assets/palette/globe.png")),
    ("image", include_bytes!("../../assets/palette/image.png")),
    (
        "imagebox",
        include_bytes!("../../assets/palette/imagebox.png"),
    ),
    (
        "imagescrollbox",
        include_bytes!("../../assets/palette/imagescrollbox.png"),
    ),
    ("label", include_bytes!("../../assets/palette/label.png")),
    ("link", include_bytes!("../../assets/palette/link.png")),
    ("list", include_bytes!("../../assets/palette/list.png")),
    ("loader", include_bytes!("../../assets/palette/loader.png")),
    ("memo", include_bytes!("../../assets/palette/memo.png")),
    ("menu", include_bytes!("../../assets/palette/menu.png")),
    (
        "pointer",
        include_bytes!("../../assets/palette/pointer.png"),
    ),
    ("popup", include_bytes!("../../assets/palette/popup.png")),
    (
        "progress",
        include_bytes!("../../assets/palette/progress.png"),
    ),
    ("radio", include_bytes!("../../assets/palette/radio.png")),
    (
        "segmented",
        include_bytes!("../../assets/palette/segmented.png"),
    ),
    (
        "separator",
        include_bytes!("../../assets/palette/separator.png"),
    ),
    (
        "skeleton",
        include_bytes!("../../assets/palette/skeleton.png"),
    ),
    ("slider", include_bytes!("../../assets/palette/slider.png")),
    ("spin", include_bytes!("../../assets/palette/spin.png")),
    (
        "splitter",
        include_bytes!("../../assets/palette/splitter.png"),
    ),
    ("sqlx", include_bytes!("../../assets/palette/sqlx.png")),
    ("switch", include_bytes!("../../assets/palette/switch.png")),
    ("table", include_bytes!("../../assets/palette/table.png")),
    ("tabs", include_bytes!("../../assets/palette/tabs.png")),
    (
        "terminal",
        include_bytes!("../../assets/palette/terminal.png"),
    ),
    ("test", include_bytes!("../../assets/palette/test.png")),
    ("timer", include_bytes!("../../assets/palette/timer.png")),
    ("update", include_bytes!("../../assets/palette/update.png")),
    ("video", include_bytes!("../../assets/palette/video.png")),
];

/// Icon of a type without an icon of its own.
pub const FALLBACK: &str = "component";

thread_local! {
    static PACK_FILES: RefCell<HashMap<String, PathBuf>> = RefCell::new(HashMap::new());
    static CACHE: RefCell<HashMap<String, Option<Image>>> = RefCell::new(HashMap::new());
}

/// Names of the built-in icons.
pub fn builtin_names() -> Vec<&'static str> {
    BUILTIN.iter().map(|(name, _)| *name).collect()
}

/// Registers the `icons/*.png` of a pack folder.
pub fn add_pack_dir(pack_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(pack_dir.join("icons")) else {
        return;
    };
    PACK_FILES.with(|files| {
        let mut files = files.borrow_mut();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "png")
                && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            {
                files.insert(stem.to_owned(), path);
            }
        }
    });
    CACHE.with(|cache| cache.borrow_mut().clear());
}

/// The icon `name` (pack icon first, then built-in), shown at `size` × `size`
/// on buttons.
pub fn icon(name: &str) -> Option<Image> {
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry(name.to_owned())
            .or_insert_with(|| {
                let from_pack = PACK_FILES.with(|f| f.borrow().get(name).cloned());
                let bytes: Vec<u8> = match from_pack.and_then(|p| std::fs::read(p).ok()) {
                    Some(bytes) => bytes,
                    None => BUILTIN.iter().find(|(n, _)| *n == name)?.1.to_vec(),
                };
                Some(Image::from_bytes(bytes).with_display_size(24, 24))
            })
            .clone()
    })
}

/// The icon `name`, or the fallback icon.
pub fn icon_or_fallback(name: Option<&str>) -> Image {
    name.and_then(icon)
        .or_else(|| icon(FALLBACK))
        .expect("built-in fallback icon")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_icons_are_png_48() {
        for (name, data) in BUILTIN {
            assert!(data.starts_with(b"\x89PNG"), "{name}");
            let width = u32::from_be_bytes(data[16..20].try_into().unwrap());
            assert_eq!(width, 48, "{name}");
        }
        // Kern-Widgets und vorhandene Packs haben ein Symbol.
        for name in [
            "form",
            "container",
            "label",
            "button",
            "edit",
            "checkbox",
            "imagebox",
            "imagescrollbox",
            "cpu",
            "database",
            "table",
            "globe",
            FALLBACK,
        ] {
            assert!(builtin_names().contains(&name), "{name}");
        }
    }

    /// Jedes Symbol in `assets/palette/` steht in der Tabelle (sonst zeigen
    /// Palette und Designer still das Ersatzsymbol).
    #[test]
    fn every_palette_png_is_builtin() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/palette");
        let names = builtin_names();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "png") {
                let name = path.file_stem().unwrap().to_str().unwrap();
                assert!(names.contains(&name), "{name}.png fehlt in BUILTIN");
            }
        }
    }
}
