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
    ("yellow", sw("#fff4b0", "#3d3510", "#a07000", "Lemon"), sw("#3b3418", "#f5ecc2", "#f0c43c", "Lemon")),
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
        // Colour dots show the note colour, ringed with its accent: the note
        // colours alone are too close to each other (and to the popover) to see.
        for (s, sel) in [(light, format!(".swatch.{name}")), (dark, format!(".swatch.dark.{name}"))] {
            rules.push(format!("{sel} {{ background: {}; box-shadow: inset 0 0 0 3px {}; }}", s.bg, s.accent));
            rules.push(format!(
                "{sel}:checked {{ box-shadow: inset 0 0 0 3px {}, 0 0 0 2px var(--note-bg), 0 0 0 4px var(--note-fg); }}",
                s.accent
            ));
        }
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
    // GTK cascades by provider priority before selector specificity, so the
    // generated per-colour rules must sit above the generic ones in note.css.
    let app = gtk::STYLE_PROVIDER_PRIORITY_APPLICATION;
    for (provider, priority) in [(style, app), (palette, app + 1)] {
        gtk::style_context_add_provider_for_display(&display, &provider, priority);
    }
    gtk::IconTheme::for_display(&display).add_resource_path(&format!("{RESOURCE_BASE}/icons"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(hex: &str) -> f64 {
        let h = hex.trim_start_matches('#');
        let channel = |i: usize| {
            let c = u8::from_str_radix(&h[i..i + 2], 16).unwrap() as f64 / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4)
    }

    fn contrast(a: &str, b: &str) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// Colour dots are ringed with their accent and shown on the popover of a
    /// note of any colour, so every accent must stand out on every note (WCAG
    /// asks 3:1 for graphical elements).
    #[test]
    fn colour_dot_rings_are_visible_on_every_note() {
        for dark in [false, true] {
            for (dot, ..) in PALETTE {
                for (note, ..) in PALETTE {
                    let (ring, bg) = (swatch(dot, dark).accent, swatch(note, dark).bg);
                    let ratio = contrast(ring, bg);
                    assert!(ratio >= 3.0, "{dot} ring on {note} note (dark: {dark}): {ratio:.2}:1");
                }
            }
        }
    }

    #[test]
    fn palette_css_rings_every_colour_dot() {
        let css = palette_css();
        for (name, light, dark) in PALETTE {
            assert!(css.contains(&format!(
                ".swatch.{name} {{ background: {}; box-shadow: inset 0 0 0 3px {}",
                light.bg, light.accent
            )));
            assert!(css.contains(&format!(
                ".swatch.dark.{name} {{ background: {}; box-shadow: inset 0 0 0 3px {}",
                dark.bg, dark.accent
            )));
        }
    }
}
