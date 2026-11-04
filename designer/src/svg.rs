//! The display list as SVG – to look at a form without a window (docs,
//! tests, review by a human or an AI).

use std::fmt::Write as _;

use tdr_core::Color;

use crate::render::Primitive;

fn color(c: Color) -> String {
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!(
            "rgba({},{},{},{:.3})",
            c.r,
            c.g,
            c.b,
            f32::from(c.a) / 255.0
        )
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Renders `primitives` on a `width` × `height` canvas.
pub fn to_svg(primitives: &[Primitive], width: i32, height: i32) -> String {
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         font-family=\"sans-serif\">\n"
    );
    for p in primitives {
        let _ = match p {
            Primitive::Rect {
                rect,
                fill,
                stroke,
                radius,
                dashed,
            } => writeln!(
                out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{radius}\" fill=\"{}\" stroke=\"{}\"{}/>",
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                fill.map_or("none".to_owned(), color),
                stroke.map_or("none".to_owned(), color),
                if *dashed {
                    " stroke-dasharray=\"4 3\""
                } else {
                    ""
                }
            ),
            Primitive::Text {
                x,
                y,
                text,
                color: c,
                size,
                bold,
                italic,
                family,
            } => writeln!(
                out,
                "<text x=\"{x}\" y=\"{}\" font-size=\"{size}\" fill=\"{}\"{}{}{}>{}</text>",
                y + size * 4 / 5,
                color(*c),
                if *bold { " font-weight=\"bold\"" } else { "" },
                if *italic {
                    " font-style=\"italic\""
                } else {
                    ""
                },
                family
                    .as_ref()
                    .map(|f| format!(" font-family=\"{}\"", escape(f)))
                    .unwrap_or_default(),
                escape(text)
            ),
            Primitive::Line { from, to, color: c } => writeln!(
                out,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\"/>",
                from.x,
                from.y,
                to.x,
                to.y,
                color(*c)
            ),
            Primitive::Icon { rect, name } => writeln!(
                out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"4\" fill=\"#8a8a8a\"><title>{}</title></rect>",
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                escape(name)
            ),
            Primitive::Image { rect, path, fit } => writeln!(
                out,
                "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" href=\"{}\" preserveAspectRatio=\"{}\"/>",
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                escape(path),
                match fit.as_str() {
                    "Stretch" => "none",
                    "Contain" => "xMidYMid meet",
                    _ => "xMidYMid slice",
                }
            ),
        };
    }
    out.push_str("</svg>\n");
    out
}
