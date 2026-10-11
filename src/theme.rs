//! Note colour palette (light + dark variants), stylesheet and icons.

use std::sync::Once;

use gtk::{gdk, gio};

use crate::drawing::INK_COLORS;

pub const RESOURCE_BASE: &str = "/ch/giosuel/RealerXpad";

#[derive(Clone, Copy, Debug)]
pub struct Swatch {
    /// Note background.
    pub bg: &'static str,
    /// Text colour.
    pub fg: &'static str,
    /// Bullets, links, checkboxes.
    pub accent: &'static str,
    pub label: &'static str,
}

const fn sw(bg: &'static str, fg: &'static str, accent: &'static str, label: &'static str) -> Swatch {
    Swatch { bg, fg, accent, label }
}

/// `(name, light, dark)`
pub const PALETTE: [(&str, Swatch, Swatch); 7] = [
    ("yellow", sw("#fff4b0", "#3d3510", "#b07d00", "Lemon"), sw("#3b3418", "#f5ecc2", "#f0c43c", "Lemon")),
    ("peach", sw("#ffdcc2", "#45250f", "#c4561a", "Peach"), sw("#422a1c", "#f8dccb", "#ff9a5c", "Peach")),
    ("pink", sw("#ffd6e4", "#4a1730", "#c2316b", "Rose"), sw("#40222f", "#f9d6e3", "#ff7aab", "Rose")),
    ("lavender", sw("#e6dcff", "#2c1d57", "#6d48d4", "Lavender"), sw("#2f2846", "#e3dbff", "#ab92ff", "Lavender")),
    ("sky", sw("#d3eaff", "#0f2f4f", "#1d6fc4", "Sky"), sw("#1e3247", "#d5e9ff", "#6cb4ff", "Sky")),
    ("mint", sw("#cdf3de", "#0f3a26", "#1c8a55", "Mint"), sw("#1c3a2d", "#d0f3e0", "#5bd69a", "Mint")),
    ("graphite", sw("#e8e9ed", "#1f2228", "#4a5260", "Graphite"), sw("#26282d", "#e6e7eb", "#a6afbf", "Graphite")),
];

pub fn swatch(color: &str, dark: bool) -> Swatch {
    let (_, light, dark_sw) = PALETTE.iter().find(|p| p.0 == color).unwrap_or(&PALETTE[0]);
    if dark { *dark_sw } else { *light }
}

pub fn rgba(hex: &str, alpha: f64) -> String {
    let h = hex.trim_start_matches('#');
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    format!("rgba({},{},{},{alpha})", c(0), c(2), c(4))
}

pub fn palette_css() -> String {
    let mut rules = Vec::new();
    for (name, light, dark) in PALETTE {
        for (s, sel) in [(light, format!("window.note.{name}")), (dark, format!("window.note.dark.{name}"))] {
            rules.push(format!(
                "{sel} {{ --note-bg: {}; --note-fg: {}; --note-accent: {}; --note-tint: {}; \
                 --note-tint-strong: {}; --note-dim: {}; }}",
                s.bg,
                s.fg,
                s.accent,
                rgba(s.fg, 0.07),
                rgba(s.fg, 0.14),
                rgba(s.fg, 0.45)
            ));
        }
        rules.push(format!(".swatch.{name} {{ background: {}; }}", light.bg));
        rules.push(format!(".swatch.dark.{name} {{ background: {}; }}", dark.bg));
    }
    rules.push(".ink-ink { background: var(--note-fg); }".into());
    for c in INK_COLORS.iter().skip(1) {
        rules.push(format!(".ink-{} {{ background: {c}; }}", c.trim_start_matches('#')));
    }
    rules.join("\n")
}

/// Register the bundled resources (stylesheet, icons). Safe to call repeatedly.
pub fn register_resources() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        gio::resources_register_include!("realer-xpad.gresource").expect("bundled resources");
    });
}

pub fn load_css() {
    register_resources();
    let display = gdk::Display::default().expect("a display");
    let palette = gtk::CssProvider::new();
    palette.load_from_string(&palette_css());
    let style = gtk::CssProvider::new();
    style.load_from_resource(&format!("{RESOURCE_BASE}/note.css"));
    for provider in [palette, style] {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
    gtk::IconTheme::for_display(&display).add_resource_path(&format!("{RESOURCE_BASE}/icons"));
}
