use crate::StyledSegment;
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
    SolarizedDark,
    SolarizedLight,
}

impl Theme {
    pub fn name(&self) -> &str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::SolarizedDark => "solarized-dark",
            Theme::SolarizedLight => "solarized-light",
        }
    }

    pub fn from_name(s: &str) -> Option<Theme> {
        match s {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            "solarized-dark" => Some(Theme::SolarizedDark),
            "solarized-light" => Some(Theme::SolarizedLight),
            _ => None,
        }
    }

    pub fn next_theme(&self) -> Theme {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::SolarizedDark,
            Theme::SolarizedDark => Theme::SolarizedLight,
            Theme::SolarizedLight => Theme::Dark,
        }
    }

    pub fn background(&self) -> Color {
        match self {
            Theme::Dark => Color::Rgb(0, 0, 0),
            Theme::Light => Color::Rgb(253, 246, 227),
            Theme::SolarizedDark => Color::Rgb(7, 54, 66),
            Theme::SolarizedLight => Color::Rgb(238, 232, 213),
        }
    }

    pub fn foreground(&self) -> Color {
        match self {
            Theme::Dark => Color::Rgb(204, 204, 204),
            Theme::Light => Color::Rgb(0, 0, 0),
            Theme::SolarizedDark => Color::Rgb(131, 148, 150),
            Theme::SolarizedLight => Color::Rgb(101, 123, 131),
        }
    }
}

/// Fixed dialog colors, deliberately independent of the active theme so that
/// dialogs read the same in every terminal and never blend into the page.
///
/// Informational dialogs (help, dictionary, confirmations, status) use a blue
/// background; error dialogs use a red background.  Both use white text.
pub const INFO_BG: Color = Color::Rgb(30, 64, 140);
pub const ERROR_BG: Color = Color::Rgb(150, 30, 30);
pub const DIALOG_FG: Color = Color::White;

pub fn style_for_segment(seg: &StyledSegment, theme: &Theme) -> Style {
    let mut mods = Modifier::empty();
    if seg.is_heading {
        mods |= Modifier::BOLD;
    }
    if seg.style.bold {
        mods |= Modifier::BOLD;
    }
    if seg.style.italic {
        mods |= Modifier::ITALIC;
    }
    if seg.style.underline {
        mods |= Modifier::UNDERLINED;
    }
    if seg.style.strike {
        mods |= Modifier::CROSSED_OUT;
    }

    let fg = if seg.is_heading {
        Color::Yellow
    } else if let Some(rgb) = seg.style.foreground {
        map_rgb_to_terminal(rgb[0], rgb[1], rgb[2], theme)
    } else {
        theme.foreground()
    };

    Style::default()
        .fg(fg)
        .bg(theme.background())
        .add_modifier(mods)
}

/// Maps a book-specified color to something visible on the active theme.
///
/// Book CSS colors are authored against an unknown (usually light) page
/// background.  On a dark theme a near-black color would be unreadable, and
/// on a light theme a near-white color would vanish.  We therefore flip
/// colors whose relative luminance is on the "wrong" side for the theme:
/// dark colors are lightened on dark themes, light colors are darkened on
/// light themes.  Mid-tone colors pass through unchanged.
fn map_rgb_to_terminal(r: u8, g: u8, b: u8, theme: &Theme) -> Color {
    // ITU-R BT.709 relative luminance, scaled to 0..=255.
    let luminance = (0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)) as u16;

    let is_dark_theme = matches!(theme, Theme::Dark | Theme::SolarizedDark);

    if is_dark_theme && luminance < 128 {
        // Dark text on a dark page: flip to a light color.
        Color::Rgb(255 - r, 255 - g, 255 - b)
    } else if !is_dark_theme && luminance > 128 {
        // Light text on a light page: flip to a dark color.
        Color::Rgb(255 - r, 255 - g, 255 - b)
    } else {
        Color::Rgb(r, g, b)
    }
}
