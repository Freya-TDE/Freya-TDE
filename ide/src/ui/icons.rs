//! Icons of components and widgets: the built-in set (`ide/assets/palette/`,
//! 48 px, sources and generator `make.py` next to them) and icons a pack
//! brings as `icons/<name>.png` in its folder (they take precedence).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ruf::Image;

/// The built-in icons by name.
const BUILTIN: &[(&str, &[u8])] = &[
    ("button", include_bytes!("../../assets/palette/button.png")),
    (
        "checkbox",
        include_bytes!("../../assets/palette/checkbox.png"),
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
    (
        "dropdown",
        include_bytes!("../../assets/palette/dropdown.png"),
    ),
    ("edit", include_bytes!("../../assets/palette/edit.png")),
    ("form", include_bytes!("../../assets/palette/form.png")),
    ("frame", include_bytes!("../../assets/palette/frame.png")),
    ("globe", include_bytes!("../../assets/palette/globe.png")),
    ("image", include_bytes!("../../assets/palette/image.png")),
    ("label", include_bytes!("../../assets/palette/label.png")),
    ("link", include_bytes!("../../assets/palette/link.png")),
    ("memo", include_bytes!("../../assets/palette/memo.png")),
    (
        "pointer",
        include_bytes!("../../assets/palette/pointer.png"),
    ),
    (
        "progress",
        include_bytes!("../../assets/palette/progress.png"),
    ),
    ("radio", include_bytes!("../../assets/palette/radio.png")),
    (
        "separator",
        include_bytes!("../../assets/palette/separator.png"),
    ),
    ("slider", include_bytes!("../../assets/palette/slider.png")),
    ("spin", include_bytes!("../../assets/palette/spin.png")),
    ("switch", include_bytes!("../../assets/palette/switch.png")),
    ("table", include_bytes!("../../assets/palette/table.png")),
    ("tabs", include_bytes!("../../assets/palette/tabs.png")),
    ("timer", include_bytes!("../../assets/palette/timer.png")),
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
            "cpu",
            "database",
            "table",
            "globe",
            FALLBACK,
        ] {
            assert!(builtin_names().contains(&name), "{name}");
        }
    }
}
