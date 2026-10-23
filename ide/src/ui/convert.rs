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
pub fn shapes(primitives: &[Primitive], background: tdr_core::Color) -> Vec<Shape> {
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
            } => out.push(Shape::Text {
                x: *x,
                y: *y,
                text: text.clone(),
                color: color(*c, background),
                size: *size,
                bold: *bold,
            }),
            Primitive::Line { from, to, color: c } => out.push(Shape::Line {
                from: Point::new(from.x, from.y),
                to: Point::new(to.x, to.y),
                color: color(*c, background),
            }),
            // Symbol-Platzhalter: Kästchen mit dem Anfangsbuchstaben des Symbols.
            Primitive::Icon { rect: r, name } => {
                out.push(Shape::Rect {
                    rect: rect(*r),
                    fill: Some(Color::rgb(120, 130, 150)),
                    stroke: None,
                    radius: 5,
                    dashed: false,
                });
                let letter: String = name.chars().take(1).flat_map(char::to_uppercase).collect();
                out.push(Shape::Text {
                    x: r.x + r.width / 2 - 5,
                    y: r.y + r.height / 2 - 9,
                    text: letter,
                    color: Color::rgb(255, 255, 255),
                    size: 15,
                    bold: true,
                });
            }
        }
    }
    out
}
