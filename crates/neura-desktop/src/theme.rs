use ratatui::style::Color;
use neura_app_framework::palette;

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub border: Color,
    pub border_focused: Color,
    pub statusbar_bg: Color,
    pub statusbar_fg: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,
    pub prompt: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub muted: Color,
}

impl Theme {
    pub fn parse_hex(hex: &str) -> Color {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return Color::White;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        Color::Rgb(r, g, b)
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "catppuccin" => Self::catppuccin(),
            "dracula"    => Self::dracula(),
            "gruvbox"    => Self::gruvbox(),
            "nord"       => Self::nord(),
            "solarized"  => Self::solarized(),
            "monokai"    => Self::monokai(),
            "light"      => Self::light_mode(),
            _            => Self::tokyo_night(),
        }
    }

    pub fn light_mode() -> Self {
        Self {
            name:          "light".to_string(),
            bg:            Color::Rgb(250, 250, 255),
            fg:            Color::Rgb(30,  30,  50),
            accent:        Color::Rgb(10,  100, 210),
            border:        Color::Rgb(180, 185, 210),
            border_focused: Color::Rgb(10, 100, 210),
            statusbar_bg:  Color::Rgb(215, 220, 240),
            statusbar_fg:  Color::Rgb(50,  55,  80),
            error:         Color::Rgb(200, 20,  60),
            warning:       Color::Rgb(170, 110, 0),
            success:       Color::Rgb(0,   140, 70),
            info:          Color::Rgb(0,   120, 180),
            prompt:        Color::Rgb(0,   140, 70),
            selected_bg:   Color::Rgb(205, 215, 240),
            selected_fg:   Color::Rgb(10,  100, 210),
            muted:         Color::Rgb(110, 120, 155),
        }
    }

    pub fn with_high_contrast(mut self) -> Self {
        self.fg = Color::White;
        self.border = self.accent;
        self.border_focused = Color::White;
        self.muted = Color::Rgb(180, 180, 180);
        self
    }

    pub fn tokyo_night() -> Self {
        Self {
            name:           "tokyo_night".to_string(),
            bg:             palette::BG,
            fg:             palette::FG,
            accent:         palette::PRIMARY,
            border:         palette::BORDER,
            border_focused: palette::BORDER_FOCUSED,
            statusbar_bg:   palette::STATUSBAR_BG,
            statusbar_fg:   palette::MUTED,
            error:          palette::RED,
            warning:        palette::ORANGE,
            success:        palette::GREEN,
            info:           palette::CYAN,
            prompt:         palette::GREEN,
            selected_bg:    palette::SEL_BG2,
            selected_fg:    palette::SEL_FG,
            muted:          palette::STATUSBAR_MUTED,
        }
    }

    pub fn catppuccin() -> Self {
        Self {
            name: "catppuccin".to_string(),
            bg: Color::Rgb(30, 30, 46),
            fg: Color::Rgb(205, 214, 244),
            accent: Color::Rgb(137, 180, 250),
            border: Color::Rgb(69, 71, 90),
            border_focused: Color::Rgb(137, 180, 250),
            statusbar_bg: Color::Rgb(24, 24, 37),
            statusbar_fg: Color::Rgb(186, 194, 222),
            error: Color::Rgb(243, 139, 168),
            warning: Color::Rgb(249, 226, 175),
            success: Color::Rgb(166, 227, 161),
            info: Color::Rgb(137, 220, 235),
            prompt: Color::Rgb(166, 227, 161),
            selected_bg: Color::Rgb(49, 50, 68),
            selected_fg: Color::Rgb(137, 180, 250),
            muted: Color::Rgb(127, 132, 156),
        }
    }

    pub fn dracula() -> Self {
        Self {
            name: "dracula".to_string(),
            bg: Color::Rgb(40, 42, 54),
            fg: Color::Rgb(248, 248, 242),
            accent: Color::Rgb(189, 147, 249),
            border: Color::Rgb(68, 71, 90),
            border_focused: Color::Rgb(189, 147, 249),
            statusbar_bg: Color::Rgb(33, 34, 44),
            statusbar_fg: Color::Rgb(98, 114, 164),
            error: Color::Rgb(255, 85, 85),
            warning: Color::Rgb(241, 250, 140),
            success: Color::Rgb(80, 250, 123),
            info: Color::Rgb(139, 233, 253),
            prompt: Color::Rgb(80, 250, 123),
            selected_bg: Color::Rgb(68, 71, 90),
            selected_fg: Color::Rgb(189, 147, 249),
            muted: Color::Rgb(98, 114, 164),
        }
    }

    pub fn gruvbox() -> Self {
        Self {
            name: "gruvbox".to_string(),
            bg: Color::Rgb(40, 40, 40),
            fg: Color::Rgb(235, 219, 178),
            accent: Color::Rgb(250, 189, 47),
            border: Color::Rgb(80, 73, 69),
            border_focused: Color::Rgb(250, 189, 47),
            statusbar_bg: Color::Rgb(29, 32, 33),
            statusbar_fg: Color::Rgb(168, 153, 132),
            error: Color::Rgb(251, 73, 52),
            warning: Color::Rgb(254, 128, 25),
            success: Color::Rgb(184, 187, 38),
            info: Color::Rgb(131, 165, 152),
            prompt: Color::Rgb(184, 187, 38),
            selected_bg: Color::Rgb(60, 56, 54),
            selected_fg: Color::Rgb(250, 189, 47),
            muted: Color::Rgb(146, 131, 116),
        }
    }

    pub fn nord() -> Self {
        Self {
            name: "nord".to_string(),
            bg: Color::Rgb(46, 52, 64),
            fg: Color::Rgb(216, 222, 233),
            accent: Color::Rgb(136, 192, 208),
            border: Color::Rgb(59, 66, 82),
            border_focused: Color::Rgb(136, 192, 208),
            statusbar_bg: Color::Rgb(36, 40, 50),
            statusbar_fg: Color::Rgb(76, 86, 106),
            error: Color::Rgb(191, 97, 106),
            warning: Color::Rgb(235, 203, 139),
            success: Color::Rgb(163, 190, 140),
            info: Color::Rgb(129, 161, 193),
            prompt: Color::Rgb(163, 190, 140),
            selected_bg: Color::Rgb(59, 66, 82),
            selected_fg: Color::Rgb(136, 192, 208),
            muted: Color::Rgb(76, 86, 106),
        }
    }

    pub fn solarized() -> Self {
        Self {
            name: "solarized".to_string(),
            bg: Color::Rgb(0, 43, 54),
            fg: Color::Rgb(131, 148, 150),
            accent: Color::Rgb(38, 139, 210),
            border: Color::Rgb(7, 54, 66),
            border_focused: Color::Rgb(38, 139, 210),
            statusbar_bg: Color::Rgb(0, 34, 43),
            statusbar_fg: Color::Rgb(88, 110, 117),
            error: Color::Rgb(220, 50, 47),
            warning: Color::Rgb(181, 137, 0),
            success: Color::Rgb(133, 153, 0),
            info: Color::Rgb(42, 161, 152),
            prompt: Color::Rgb(133, 153, 0),
            selected_bg: Color::Rgb(7, 54, 66),
            selected_fg: Color::Rgb(38, 139, 210),
            muted: Color::Rgb(88, 110, 117),
        }
    }

    pub fn monokai() -> Self {
        Self {
            name: "monokai".to_string(),
            bg: Color::Rgb(39, 40, 34),
            fg: Color::Rgb(248, 248, 242),
            accent: Color::Rgb(102, 217, 239),
            border: Color::Rgb(62, 61, 50),
            border_focused: Color::Rgb(102, 217, 239),
            statusbar_bg: Color::Rgb(30, 31, 28),
            statusbar_fg: Color::Rgb(117, 113, 94),
            error: Color::Rgb(249, 38, 114),
            warning: Color::Rgb(230, 219, 116),
            success: Color::Rgb(166, 226, 46),
            info: Color::Rgb(102, 217, 239),
            prompt: Color::Rgb(166, 226, 46),
            selected_bg: Color::Rgb(62, 61, 50),
            selected_fg: Color::Rgb(102, 217, 239),
            muted: Color::Rgb(117, 113, 94),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::tokyo_night()
    }
}
