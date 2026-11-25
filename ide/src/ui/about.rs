//! Program icon, splash screen and Help → About Freya TDE.
//!
//! The pictures live in `ide/assets/` (from `images/Freya-F.svg` and
//! `images/Freya-TDE-Logo.svg`) and are compiled into the program.

use crate::tr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rncl::prelude::*;
use ruf::{Color, Image, Shape};

use super::Ide;

/// Product name as shown to the user.
pub const PRODUCT: &str = "Freya TDE";
/// Name of program, icon and configuration folder.
pub const PROGRAM: &str = "freya-tde";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The program icon in all sizes (PNG).
pub const ICONS: [(u32, &[u8]); 7] = [
    (16, include_bytes!("../../assets/icons/freya-tde-16.png")),
    (24, include_bytes!("../../assets/icons/freya-tde-24.png")),
    (32, include_bytes!("../../assets/icons/freya-tde-32.png")),
    (48, include_bytes!("../../assets/icons/freya-tde-48.png")),
    (64, include_bytes!("../../assets/icons/freya-tde-64.png")),
    (128, include_bytes!("../../assets/icons/freya-tde-128.png")),
    (256, include_bytes!("../../assets/icons/freya-tde-256.png")),
];

const LOGO_PNG: &[u8] = include_bytes!("../../assets/freya-tde-logo.png");

thread_local! {
    static LOGO: Image = Image::from_bytes(LOGO_PNG);
}

/// The product logo (F with "TDE").
pub fn logo() -> Image {
    LOGO.with(Image::clone)
}

/// Sets the program icon for all windows.
pub fn set_program_icon() -> bool {
    Application::set_icon(PROGRAM, &ICONS)
}

const PAPER: Color = Color::rgb(0xff, 0xff, 0xff);
const BORDER: Color = Color::rgb(0x99, 0x00, 0x00);
const INK: Color = Color::rgb(0x20, 0x20, 0x20);
const GRAY: Color = Color::rgb(0x6a, 0x6a, 0x6a);

/// Logo, name and `lines` centred on a white card of `width` × `height`.
fn card(
    width: i32,
    height: i32,
    logo_height: i32,
    lines: &[(&str, i32, bool, Color)],
) -> Vec<Shape> {
    let mut shapes = vec![
        Shape::Rect {
            rect: Rect::new(0, 0, width, height),
            fill: Some(PAPER),
            stroke: Some(BORDER),
            radius: 0,
            dashed: false,
        },
        Shape::Image {
            rect: Rect::new(24, 22, width - 48, logo_height),
            image: logo(),
            fit: ruf::ImageFit::Contain,
        },
    ];
    let mut y = 22 + logo_height + 16;
    for (text, size, bold, color) in lines {
        let measured = ruf::measure_text(text, *size, *bold, false);
        shapes.push(Shape::Text {
            x: (width - measured.width) / 2,
            y,
            text: (*text).to_owned(),
            color: *color,
            size: *size,
            bold: *bold,
            italic: false,
            family: None,
        });
        y += measured.height + 6;
    }
    shapes
}

const SPLASH_W: i32 = 480;
const SPLASH_H: i32 = 330;

/// The splash screen, shown while the IDE starts.
pub struct Splash {
    window: RnclWindow,
    shown: Instant,
}

impl Splash {
    /// Shows the splash screen centred on the main screen.
    pub fn show() -> Self {
        let window = RnclWindow::new(PRODUCT);
        window.widget().set_window_decorated(false);
        window.set_size(SPLASH_W, SPLASH_H);
        let canvas = RnclCanvas::new(&window);
        canvas.set_geometry(0, 0, SPLASH_W, SPLASH_H);
        let version = tr!("about.version", version = VERSION);
        let tagline = tr!("about.tagline");
        canvas.set_shapes(card(
            SPLASH_W,
            SPLASH_H,
            190,
            &[
                (PRODUCT, 24, true, INK),
                (&tagline, 14, false, GRAY),
                (&version, 12, false, GRAY),
            ],
        ));
        if let Some(screen) = Application::screen_rect() {
            window.set_screen_position(
                screen.x + (screen.width - SPLASH_W) / 2,
                screen.y + (screen.height - SPLASH_H) / 2,
            );
        }
        window.show();
        // Zeichnen lassen, bevor der Start der IDE den Hauptthread belegt.
        for _ in 0..10 {
            Application::process_events();
        }
        Self {
            window,
            shown: Instant::now(),
        }
    }

    /// Closes the splash screen, but not before it was visible for a moment.
    pub fn finish(self) {
        const MIN: Duration = Duration::from_millis(1200);
        let left = MIN.saturating_sub(self.shown.elapsed());
        let window = self.window;
        // Die IDE-Fenster sind inzwischen erschienen: wieder nach vorn.
        window.show();
        ruf::Timer::single_shot(left.as_millis() as i32, move || window.close());
    }
}

const ABOUT_W: i32 = 460;
const ABOUT_H: i32 = 430;

/// Help → About Freya TDE.
pub struct AboutWindow {
    pub window: RnclWindow,
    canvas: RnclCanvas,
    close: RnclButton,
}

impl AboutWindow {
    pub fn build() -> Self {
        let window = RnclWindow::new(&tr!("about.title", product = PRODUCT));
        window.set_size(ABOUT_W, ABOUT_H);
        let canvas = RnclCanvas::new(&window);
        let close = RnclButton::new(&window, &tr!("about.close"));
        let about = Self {
            window,
            canvas,
            close,
        };
        about.layout(ABOUT_W, ABOUT_H);
        about
    }

    /// The text of the window (also for tests and automation).
    pub fn lines() -> Vec<String> {
        vec![
            PRODUCT.to_owned(),
            tr!("about.tagline"),
            tr!("about.version", version = VERSION),
            tr!("about.toolkit", toolkit = ruf::toolkit()),
            "© 2026 Dragon Software".to_owned(),
            tr!("about.license", license = env!("CARGO_PKG_LICENSE")),
        ]
    }

    fn layout(&self, w: i32, h: i32) {
        let lines = Self::lines();
        let styled: Vec<(&str, i32, bool, Color)> = lines
            .iter()
            .enumerate()
            .map(|(i, text)| match i {
                0 => (text.as_str(), 22, true, INK),
                1 => (text.as_str(), 14, false, GRAY),
                _ => (text.as_str(), 12, false, GRAY),
            })
            .collect();
        let button_h = 32;
        let card_h = h - button_h - 20;
        self.canvas.set_geometry(0, 0, w, card_h);
        self.canvas.set_shapes(card(w, card_h, 160, &styled));
        self.close
            .set_geometry(w - 120 - 12, card_h + 10, 120, button_h);
    }
}

pub fn connect(ide: &Rc<Ide>) {
    let weak = Rc::downgrade(ide);
    ide.about.window.on_resized(move |w, h| {
        if let Some(ide) = weak.upgrade() {
            ide.about.layout(w, h);
        }
    });
    let window = ide.about.window.clone();
    ide.about.close.on_clicked(move || window.hide());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_are_png() {
        for (size, data) in ICONS {
            assert!(data.starts_with(b"\x89PNG"), "{size}");
            // Breite steht ab Byte 16 im IHDR-Block.
            let width = u32::from_be_bytes(data[16..20].try_into().unwrap());
            assert_eq!(width, size);
        }
        assert!(LOGO_PNG.starts_with(b"\x89PNG"));
    }
}
