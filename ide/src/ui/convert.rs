//! Designer display list ([`Primitive`]) → RUF canvas shapes.

use ruf::{Color, Point, Rect, Shape};
use tdr_designer::Primitive;

fn color(c: tdr_core::Color, over: tdr_core::Color) -> Color {
    // RUF malt ohne Alphakanal: halbtransparente Farben über `over` mischen.
    let mix = |fg: u8, bg: u8| -> u8 {
        let a = u32::from(c.a);
        ((u32::from(fg) * a + u32::from(bg) * (255 - a)) / 255) as u8
    };
    Color::rgb(mix(c.r, over.r), mix(c.g, over.g), mix(c.b, over.b))
}

fn rect(r: tdr_designer::Rect) -> Rect {
    Rect::new(r.x, r.y, r.width, r.height)
}

/// Converts the display list; `background` is what transparent colors are
/// mixed with (the form's window color).
pub fn shapes(
    primitives: &[Primitive],
    background: tdr_core::Color,
    project: Option<&std::path::Path>,
) -> Vec<Shape> {
    let mut out = Vec::with_capacity(primitives.len());
    for p in primitives {
        match p {
            Primitive::Rect {
                rect: r,
                fill,
                stroke,
                radius,
                dashed,
            } => out.push(Shape::Rect {
                rect: rect(*r),
                fill: fill.map(|c| color(c, background)),
                stroke: stroke.map(|c| color(c, background)),
                radius: *radius,
                dashed: *dashed,
            }),
            Primitive::Text {
                x,
                y,
                text,
                color: c,
                size,
                bold,
                italic,
                family,
            } => out.push(Shape::Text {
                x: *x,
                y: *y,
                text: text.clone(),
                color: color(*c, background),
                size: *size,
                bold: *bold,
                italic: *italic,
                family: family.clone(),
            }),
            Primitive::Line { from, to, color: c } => out.push(Shape::Line {
                from: Point::new(from.x, from.y),
                to: Point::new(to.x, to.y),
                color: color(*c, background),
            }),
            // Das Symbol des Typs (eingebaut oder aus dem Pack).
            Primitive::Icon { rect: r, name } => out.push(Shape::Image {
                rect: rect(*r),
                image: super::icons::icon_or_fallback(Some(name)),
                fit: ruf::ImageFit::Contain,
            }),
            Primitive::Image {
                rect: r,
                path,
                fit,
                opacity,
                gamma,
            } => {
                if let Some(image) = picture(project, path, *gamma as f32, *opacity as f32) {
                    out.push(Shape::Image {
                        rect: rect(*r),
                        image,
                        fit: match fit.as_str() {
                            "Stretch" => ruf::ImageFit::Stretch,
                            "Contain" => ruf::ImageFit::Contain,
                            "Center" => ruf::ImageFit::Center,
                            _ => ruf::ImageFit::Cover,
                        },
                    });
                }
            }
        }
    }
    out
}

/// A loaded picture with the modification time of its file.
type Picture = (Option<std::time::SystemTime>, Option<ruf::Image>);

/// Key of a picture: file, gamma and opacity (as bits).
type PictureKey = (std::path::PathBuf, u32, u32);

thread_local! {
    /// Pictures of the design surface by file and tone (one decoded image
    /// each in RUF, not one per redraw).
    static PICTURES: std::cell::RefCell<std::collections::HashMap<PictureKey, Picture>> =
        Default::default();
}

/// A picture file (relative to the project) with gamma and opacity, loaded
/// again when it changed.
fn picture(
    project: Option<&std::path::Path>,
    path: &str,
    gamma: f32,
    opacity: f32,
) -> Option<ruf::Image> {
    let path = std::path::Path::new(path);
    let file = match project {
        Some(dir) if path.is_relative() => dir.join(path),
        _ => path.to_path_buf(),
    };
    let modified = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    let key = (file, gamma.to_bits(), opacity.to_bits());
    PICTURES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((time, image)) = cache.get(&key)
            && *time == modified
        {
            return image.clone();
        }
        let image = std::fs::read(&key.0)
            .ok()
            .map(|bytes| ruf::Image::from_bytes(bytes).with_tone(gamma, opacity));
        cache.insert(key, (modified, image.clone()));
        image
    })
}
