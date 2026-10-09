//! Color tokens (docs/03-ui-style.md § Color tokens): Catppuccin base and neutrals with
//! saturated, widely spaced role hues; a 16-color fallback that follows the terminal theme.

use crate::layout::Role;
use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum ThemeName {
    Auto,
    Mocha,
    Latte,
    Ansi,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub name: ThemeName,
    pub bg: Option<Color>,
    fg: Color,
    dim: Color,
    accent: Color,
    own: Color,
    live: Color,
    fork: Color,
    subtle: Color,
    inferred: Color,
    warn: Color,
    danger: Color,
    pub sel: Color,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl Theme {
    pub fn resolve(name: ThemeName) -> Theme {
        match name {
            ThemeName::Mocha => Theme::mocha(),
            ThemeName::Latte => Theme::latte(),
            ThemeName::Ansi => Theme::ansi(),
            ThemeName::Auto => {
                let truecolor = std::env::var("COLORTERM")
                    .is_ok_and(|v| v.contains("truecolor") || v.contains("24bit"));
                if !truecolor || std::env::var_os("NO_COLOR").is_some() {
                    return Theme::ansi();
                }
                // COLORFGBG="fg;bg": bg 7 or 15 means a light terminal
                let light = std::env::var("COLORFGBG")
                    .ok()
                    .and_then(|v| v.rsplit(';').next().and_then(|b| b.parse::<u8>().ok()))
                    .is_some_and(|bg| bg == 7 || bg == 15);
                if light {
                    Theme::latte()
                } else {
                    Theme::mocha()
                }
            }
        }
    }

    pub fn mocha() -> Theme {
        Theme {
            name: ThemeName::Mocha,
            bg: Some(rgb(0x1e1e2e)),
            fg: rgb(0xcdd6f4),
            dim: rgb(0x7f849c),
            accent: rgb(0xff9e44),
            own: rgb(0x9be05c),
            live: rgb(0x2ed9cf),
            fork: rgb(0xc38bff),
            subtle: rgb(0xa6adc8),
            inferred: rgb(0xff6ab8),
            warn: rgb(0xffd43b),
            danger: rgb(0xff5555),
            sel: rgb(0x313244),
        }
    }

    pub fn latte() -> Theme {
        Theme {
            name: ThemeName::Latte,
            bg: Some(rgb(0xeff1f5)),
            fg: rgb(0x4c4f69),
            dim: rgb(0x7c7f93),
            accent: rgb(0xd9480f),
            own: rgb(0x2b8a3e),
            live: rgb(0x0b7f8c),
            fork: rgb(0x7c3aed),
            subtle: rgb(0x6c6f85),
            inferred: rgb(0xc2255c),
            warn: rgb(0xa66a00),
            danger: rgb(0xe03131),
            sel: rgb(0xdce0e8),
        }
    }

    pub fn ansi() -> Theme {
        Theme {
            name: ThemeName::Ansi,
            bg: None,
            fg: Color::Reset,
            dim: Color::DarkGray,
            accent: Color::LightRed,
            own: Color::Green,
            live: Color::Cyan,
            fork: Color::Magenta,
            subtle: Color::Gray,
            inferred: Color::LightMagenta,
            warn: Color::Red,
            danger: Color::Red,
            sel: Color::Reset, // selection uses reverse video instead
        }
    }

    pub fn color(&self, role: Role) -> Color {
        match role {
            Role::Fg => self.fg,
            Role::Dim => self.dim,
            Role::Accent => self.accent,
            Role::Own => self.own,
            Role::Live => self.live,
            Role::Fork => self.fork,
            Role::Subtle => self.subtle,
            Role::Inferred => self.inferred,
            Role::Warn => self.warn,
            Role::Danger => self.danger,
        }
    }

    pub fn style(&self, role: Role, is_bold: bool, selected: bool) -> Style {
        let mut st = Style::new().fg(self.color(role));
        if is_bold {
            st = st.add_modifier(Modifier::BOLD);
        }
        if selected {
            st = if self.name == ThemeName::Ansi {
                st.add_modifier(Modifier::REVERSED)
            } else {
                st.bg(self.sel)
            };
        } else if let Some(bg) = self.bg {
            st = st.bg(bg);
        }
        st
    }

    pub fn base(&self) -> Style {
        let st = Style::new().fg(self.fg);
        match self.bg {
            Some(bg) => st.bg(bg),
            None => st,
        }
    }
}
