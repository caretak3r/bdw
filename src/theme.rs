//! Runtime color themes, modeled on well-known terminal/editor palettes so
//! the choice is instantly familiar rather than a bespoke guess at what
//! "dark" or "light" should look like. `t` cycles Nord → Gruvbox → Dracula →
//! Solarized Light → Nord; the choice persists across launches via a
//! one-line file in `~/.config/bdw/` so `bdw` remembers the last pick
//! without needing a config file format.

use std::io::Write;
use std::path::PathBuf;

use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemeKind {
    Nord,
    Gruvbox,
    Dracula,
    CatppuccinMocha,
    TokyoNight,
    RosePine,
    Everforest,
    SolarizedLight,
    RosePineDawn,
}

impl ThemeKind {
    pub(crate) fn next(self) -> Self {
        match self {
            ThemeKind::Nord => ThemeKind::Gruvbox,
            ThemeKind::Gruvbox => ThemeKind::Dracula,
            ThemeKind::Dracula => ThemeKind::CatppuccinMocha,
            ThemeKind::CatppuccinMocha => ThemeKind::TokyoNight,
            ThemeKind::TokyoNight => ThemeKind::RosePine,
            ThemeKind::RosePine => ThemeKind::Everforest,
            ThemeKind::Everforest => ThemeKind::SolarizedLight,
            ThemeKind::SolarizedLight => ThemeKind::RosePineDawn,
            ThemeKind::RosePineDawn => ThemeKind::Nord,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            ThemeKind::Nord => "nord",
            ThemeKind::Gruvbox => "gruvbox",
            ThemeKind::Dracula => "dracula",
            ThemeKind::CatppuccinMocha => "catppuccin-mocha",
            ThemeKind::TokyoNight => "tokyo-night",
            ThemeKind::RosePine => "rose-pine",
            ThemeKind::Everforest => "everforest",
            ThemeKind::SolarizedLight => "solarized-light",
            ThemeKind::RosePineDawn => "rose-pine-dawn",
        }
    }

    fn from_label(s: &str) -> Option<Self> {
        match s.trim() {
            "nord" => Some(ThemeKind::Nord),
            "gruvbox" => Some(ThemeKind::Gruvbox),
            "dracula" => Some(ThemeKind::Dracula),
            "catppuccin-mocha" => Some(ThemeKind::CatppuccinMocha),
            "tokyo-night" => Some(ThemeKind::TokyoNight),
            "rose-pine" => Some(ThemeKind::RosePine),
            "everforest" => Some(ThemeKind::Everforest),
            "solarized-light" => Some(ThemeKind::SolarizedLight),
            "rose-pine-dawn" => Some(ThemeKind::RosePineDawn),
            _ => None,
        }
    }

    pub(crate) fn theme(self) -> Theme {
        match self {
            ThemeKind::Nord => Theme::nord(),
            ThemeKind::Gruvbox => Theme::gruvbox(),
            ThemeKind::Dracula => Theme::dracula(),
            ThemeKind::CatppuccinMocha => Theme::catppuccin_mocha(),
            ThemeKind::TokyoNight => Theme::tokyo_night(),
            ThemeKind::RosePine => Theme::rose_pine(),
            ThemeKind::Everforest => Theme::everforest(),
            ThemeKind::SolarizedLight => Theme::solarized_light(),
            ThemeKind::RosePineDawn => Theme::rose_pine_dawn(),
        }
    }
}

/// Every color the UI draws with. Swapping this swaps the whole app's look;
/// nothing in `ui/*` should reach for a hardcoded `Color` directly.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Theme {
    pub(crate) fg: Color,
    pub(crate) dim: Color,
    pub(crate) dimmer: Color,
    pub(crate) border: Color,
    pub(crate) green: Color,
    pub(crate) yellow: Color,
    pub(crate) red: Color,
    pub(crate) orange: Color,
    pub(crate) cyan: Color,
    pub(crate) blue: Color,
    pub(crate) magenta: Color,
    /// Background of the selected row. Selection readability doesn't rely on
    /// this alone — `board::draw` also forces the selected row's text to
    /// `fg`/`highlight_fg`, since a `List`'s `highlight_style` only patches
    /// the line's own style, not the per-span colors already baked into
    /// dimmed/stale/closed rows.
    pub(crate) sel_bg: Color,
    /// Text color forced onto the selected row so it reads clearly against
    /// `sel_bg` regardless of the row's own dim/stale/closed styling.
    pub(crate) highlight_fg: Color,
    /// Background the whole screen/popups clear to. `None` lets the
    /// terminal's own background show through (kept as an option for any
    /// future theme that wants to inherit the terminal, though all four
    /// current palettes set it — each is a known, fixed-background theme).
    pub(crate) bg: Option<Color>,
}

impl Theme {
    /// <https://www.nordtheme.com> — cool, low-saturation dark palette.
    pub(crate) fn nord() -> Self {
        Self {
            fg: Color::Rgb(0xe5, 0xe9, 0xf0),
            dim: Color::Rgb(0x81, 0xa1, 0xc1),
            dimmer: Color::Rgb(0x4c, 0x56, 0x6a),
            border: Color::Rgb(0x4c, 0x56, 0x6a),
            green: Color::Rgb(0xa3, 0xbe, 0x8c),
            yellow: Color::Rgb(0xeb, 0xcb, 0x8b),
            red: Color::Rgb(0xbf, 0x61, 0x6a),
            orange: Color::Rgb(0xd0, 0x87, 0x70),
            cyan: Color::Rgb(0x88, 0xc0, 0xd0),
            blue: Color::Rgb(0x5e, 0x81, 0xac),
            magenta: Color::Rgb(0xb4, 0x8e, 0xad),
            sel_bg: Color::Rgb(0x43, 0x4c, 0x5e),
            highlight_fg: Color::Rgb(0xec, 0xef, 0xf4),
            bg: Some(Color::Rgb(0x2e, 0x34, 0x40)),
        }
    }

    /// <https://github.com/morhetz/gruvbox> — warm, retro-groove dark palette.
    pub(crate) fn gruvbox() -> Self {
        Self {
            fg: Color::Rgb(0xeb, 0xdb, 0xb2),
            dim: Color::Rgb(0xa8, 0x99, 0x84),
            dimmer: Color::Rgb(0x7c, 0x6f, 0x64),
            border: Color::Rgb(0x50, 0x49, 0x45),
            green: Color::Rgb(0xb8, 0xbb, 0x26),
            yellow: Color::Rgb(0xfa, 0xbd, 0x2f),
            red: Color::Rgb(0xfb, 0x49, 0x34),
            orange: Color::Rgb(0xfe, 0x80, 0x19),
            cyan: Color::Rgb(0x8e, 0xc0, 0x7c),
            blue: Color::Rgb(0x83, 0xa5, 0x98),
            magenta: Color::Rgb(0xd3, 0x86, 0x9b),
            sel_bg: Color::Rgb(0x50, 0x49, 0x45),
            highlight_fg: Color::Rgb(0xfb, 0xf1, 0xc7),
            bg: Some(Color::Rgb(0x28, 0x28, 0x28)),
        }
    }

    /// <https://draculatheme.com> — vivid, high-saturation accents on a
    /// desaturated blue-gray dark background.
    pub(crate) fn dracula() -> Self {
        Self {
            fg: Color::Rgb(0xf8, 0xf8, 0xf2),
            dim: Color::Rgb(0x62, 0x72, 0xa4),
            dimmer: Color::Rgb(0x44, 0x47, 0x5a),
            border: Color::Rgb(0x44, 0x47, 0x5a),
            green: Color::Rgb(0x50, 0xfa, 0x7b),
            yellow: Color::Rgb(0xf1, 0xfa, 0x8c),
            red: Color::Rgb(0xff, 0x55, 0x55),
            orange: Color::Rgb(0xff, 0xb8, 0x6c),
            cyan: Color::Rgb(0x8b, 0xe9, 0xfd),
            blue: Color::Rgb(0xbd, 0x93, 0xf9),
            magenta: Color::Rgb(0xff, 0x79, 0xc6),
            sel_bg: Color::Rgb(0x44, 0x47, 0x5a),
            highlight_fg: Color::Rgb(0xff, 0xff, 0xff),
            bg: Some(Color::Rgb(0x28, 0x2a, 0x36)),
        }
    }

    /// <https://ethanschoonover.com/solarized> — light variant, tuned for a
    /// low-contrast paper feel without losing accent legibility.
    pub(crate) fn solarized_light() -> Self {
        Self {
            fg: Color::Rgb(0x58, 0x6e, 0x75),
            dim: Color::Rgb(0x65, 0x7b, 0x83),
            dimmer: Color::Rgb(0x93, 0xa1, 0xa1),
            border: Color::Rgb(0x93, 0xa1, 0xa1),
            green: Color::Rgb(0x85, 0x99, 0x00),
            yellow: Color::Rgb(0xb5, 0x89, 0x00),
            red: Color::Rgb(0xdc, 0x32, 0x2f),
            orange: Color::Rgb(0xcb, 0x4b, 0x16),
            cyan: Color::Rgb(0x2a, 0xa1, 0x98),
            blue: Color::Rgb(0x26, 0x8b, 0xd2),
            magenta: Color::Rgb(0xd3, 0x36, 0x82),
            sel_bg: Color::Rgb(0xee, 0xe8, 0xd5),
            highlight_fg: Color::Rgb(0x00, 0x2b, 0x36),
            bg: Some(Color::Rgb(0xfd, 0xf6, 0xe3)),
        }
    }

    /// <https://catppuccin.com/palette> — Mocha variant, "the original":
    /// darkest, cozy, pastel accents.
    pub(crate) fn catppuccin_mocha() -> Self {
        Self {
            fg: Color::Rgb(0xcd, 0xd6, 0xf4),
            dim: Color::Rgb(0xa6, 0xad, 0xc8),
            dimmer: Color::Rgb(0x6c, 0x70, 0x86),
            border: Color::Rgb(0x45, 0x47, 0x5a),
            green: Color::Rgb(0xa6, 0xe3, 0xa1),
            yellow: Color::Rgb(0xf9, 0xe2, 0xaf),
            red: Color::Rgb(0xf3, 0x8b, 0xa8),
            orange: Color::Rgb(0xfa, 0xb3, 0x87),
            cyan: Color::Rgb(0x89, 0xdc, 0xeb),
            blue: Color::Rgb(0x89, 0xb4, 0xfa),
            magenta: Color::Rgb(0xcb, 0xa6, 0xf7),
            sel_bg: Color::Rgb(0x31, 0x32, 0x44),
            highlight_fg: Color::Rgb(0xf5, 0xe0, 0xdc),
            bg: Some(Color::Rgb(0x1e, 0x1e, 0x2e)),
        }
    }

    /// <https://github.com/tokyo-night/tokyo-night-vscode-theme> — clean,
    /// cool-blue dark palette styled on Tokyo's night skyline.
    pub(crate) fn tokyo_night() -> Self {
        Self {
            fg: Color::Rgb(0xc0, 0xca, 0xf5),
            dim: Color::Rgb(0xa9, 0xb1, 0xd6),
            dimmer: Color::Rgb(0x56, 0x5f, 0x89),
            border: Color::Rgb(0x3b, 0x42, 0x61),
            green: Color::Rgb(0x9e, 0xce, 0x6a),
            yellow: Color::Rgb(0xe0, 0xaf, 0x68),
            red: Color::Rgb(0xf7, 0x76, 0x8e),
            orange: Color::Rgb(0xff, 0x9e, 0x64),
            cyan: Color::Rgb(0x7d, 0xcf, 0xff),
            blue: Color::Rgb(0x7a, 0xa2, 0xf7),
            magenta: Color::Rgb(0xbb, 0x9a, 0xf7),
            sel_bg: Color::Rgb(0x28, 0x34, 0x57),
            highlight_fg: Color::Rgb(0xff, 0xff, 0xff),
            bg: Some(Color::Rgb(0x1a, 0x1b, 0x26)),
        }
    }

    /// <https://rosepinetheme.com/palette> — main (dark) variant: soho vibes,
    /// low-saturation rose/pine/foam/iris accents.
    pub(crate) fn rose_pine() -> Self {
        Self {
            fg: Color::Rgb(0xe0, 0xde, 0xf4),
            dim: Color::Rgb(0x90, 0x8c, 0xaa),
            dimmer: Color::Rgb(0x6e, 0x6a, 0x86),
            border: Color::Rgb(0x40, 0x3d, 0x52),
            green: Color::Rgb(0x31, 0x74, 0x8f),
            yellow: Color::Rgb(0xf6, 0xc1, 0x77),
            red: Color::Rgb(0xeb, 0x6f, 0x92),
            orange: Color::Rgb(0xeb, 0xbc, 0xba),
            cyan: Color::Rgb(0x9c, 0xcf, 0xd8),
            blue: Color::Rgb(0xc4, 0xa7, 0xe7),
            magenta: Color::Rgb(0xeb, 0xbc, 0xba),
            sel_bg: Color::Rgb(0x26, 0x23, 0x3a),
            highlight_fg: Color::Rgb(0xe0, 0xde, 0xf4),
            bg: Some(Color::Rgb(0x19, 0x17, 0x24)),
        }
    }

    /// <https://rosepinetheme.com/palette> — Dawn variant: the light
    /// companion, same accent hues, paper-warm background.
    pub(crate) fn rose_pine_dawn() -> Self {
        Self {
            fg: Color::Rgb(0x57, 0x52, 0x79),
            dim: Color::Rgb(0x79, 0x75, 0x93),
            dimmer: Color::Rgb(0x98, 0x93, 0xa5),
            border: Color::Rgb(0xdf, 0xda, 0xd9),
            green: Color::Rgb(0x28, 0x69, 0x83),
            yellow: Color::Rgb(0xea, 0x9d, 0x34),
            red: Color::Rgb(0xb4, 0x63, 0x7a),
            orange: Color::Rgb(0xd7, 0x82, 0x7e),
            cyan: Color::Rgb(0x56, 0x94, 0x9f),
            blue: Color::Rgb(0x90, 0x7a, 0xa9),
            magenta: Color::Rgb(0xd7, 0x82, 0x7e),
            sel_bg: Color::Rgb(0xf2, 0xe9, 0xe1),
            highlight_fg: Color::Rgb(0x57, 0x52, 0x79),
            bg: Some(Color::Rgb(0xfa, 0xf4, 0xed)),
        }
    }

    /// <https://github.com/sainnhe/everforest> — dark, medium contrast:
    /// warm, green-forward, designed to be easy on the eyes.
    pub(crate) fn everforest() -> Self {
        Self {
            fg: Color::Rgb(0xd3, 0xc6, 0xaa),
            dim: Color::Rgb(0x9d, 0xa9, 0xa0),
            dimmer: Color::Rgb(0x7a, 0x84, 0x78),
            border: Color::Rgb(0x47, 0x52, 0x58),
            green: Color::Rgb(0xa7, 0xc0, 0x80),
            yellow: Color::Rgb(0xdb, 0xbc, 0x7f),
            red: Color::Rgb(0xe6, 0x7e, 0x80),
            orange: Color::Rgb(0xe6, 0x98, 0x75),
            cyan: Color::Rgb(0x83, 0xc0, 0x92),
            blue: Color::Rgb(0x7f, 0xbb, 0xb3),
            magenta: Color::Rgb(0xd6, 0x99, 0xb6),
            sel_bg: Color::Rgb(0x3d, 0x48, 0x4d),
            highlight_fg: Color::Rgb(0xd3, 0xc6, 0xaa),
            bg: Some(Color::Rgb(0x2d, 0x35, 0x3b)),
        }
    }
}

/// Per-channel linear interpolation between two theme colors, `t` clamped to
/// `0.0..=1.0`. Every theme color here is a concrete `Color::Rgb`, so the
/// row-pulse/actor-pulse fade animations can lerp toward/away from an accent
/// without ever hitting a named/indexed `Color` this can't blend.
pub(crate) fn lerp_rgb(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (fr, fg, fb) = as_rgb(from);
    let (tr, tg, tb) = as_rgb(to);
    Color::Rgb(
        lerp_channel(fr, tr, t),
        lerp_channel(fg, tg, t),
        lerp_channel(fb, tb, t),
    )
}

fn as_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    }
}

fn lerp_channel(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn config_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/bdw/theme"))
}

/// Best-effort: a missing/unreadable/corrupt file just falls back to Nord,
/// never blocks startup.
pub(crate) fn load_theme_kind() -> ThemeKind {
    config_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|s| ThemeKind::from_label(&s))
        .unwrap_or(ThemeKind::Nord)
}

/// Best-effort: a write failure (read-only home, missing dir) is silently
/// ignored — the picked theme still applies for this session, it just won't
/// persist to the next one.
pub(crate) fn save_theme_kind(kind: ThemeKind) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(mut file) = std::fs::File::create(&path) {
        let _ = file.write_all(kind.label().as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [ThemeKind; 9] = [
        ThemeKind::Nord,
        ThemeKind::Gruvbox,
        ThemeKind::Dracula,
        ThemeKind::CatppuccinMocha,
        ThemeKind::TokyoNight,
        ThemeKind::RosePine,
        ThemeKind::Everforest,
        ThemeKind::SolarizedLight,
        ThemeKind::RosePineDawn,
    ];

    #[test]
    fn theme_kind_cycles_and_wraps() {
        for pair in ALL_KINDS.windows(2) {
            assert_eq!(pair[0].next(), pair[1]);
        }
        assert_eq!(ALL_KINDS[ALL_KINDS.len() - 1].next(), ALL_KINDS[0]);
    }

    #[test]
    fn theme_kind_label_roundtrips() {
        for kind in ALL_KINDS {
            assert_eq!(ThemeKind::from_label(kind.label()), Some(kind));
        }
    }

    #[test]
    fn from_label_rejects_garbage() {
        assert_eq!(ThemeKind::from_label("nonsense"), None);
    }

    #[test]
    fn lerp_rgb_hits_endpoints_and_midpoint() {
        let black = Color::Rgb(0, 0, 0);
        let white = Color::Rgb(0xff, 0xff, 0xff);
        assert_eq!(lerp_rgb(black, white, 0.0), black);
        assert_eq!(lerp_rgb(black, white, 1.0), white);
        assert_eq!(lerp_rgb(black, white, 0.5), Color::Rgb(0x80, 0x80, 0x80));
    }

    #[test]
    fn lerp_rgb_clamps_out_of_range_t() {
        let black = Color::Rgb(0, 0, 0);
        let white = Color::Rgb(0xff, 0xff, 0xff);
        assert_eq!(lerp_rgb(black, white, -1.0), black);
        assert_eq!(lerp_rgb(black, white, 2.0), white);
    }
}
