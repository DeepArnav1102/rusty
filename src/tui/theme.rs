use ratatui::style::{Color, Modifier, Style};

#[allow(dead_code)]
pub struct Theme;

#[allow(dead_code)]
impl Theme {
    // Backgrounds
    pub const BG_ROOT: Color = Color::Rgb(13, 17, 28);
    pub const BG_HEADER: Color = Color::Rgb(15, 21, 35);
    pub const BG_SIDEBAR: Color = Color::Rgb(12, 16, 26);
    pub const BG_CARD: Color = Color::Rgb(18, 24, 38);
    pub const BG_CARD_LIGHTER: Color = Color::Rgb(22, 30, 48);
    pub const BG_INPUT: Color = Color::Rgb(15, 20, 32);
    pub const BG_NAV_ACTIVE: Color = Color::Rgb(29, 78, 216); // Bright royal blue
    pub const BG_BADGE: Color = Color::Rgb(30, 41, 59);

    // Borders
    pub const BORDER_DEFAULT: Color = Color::Rgb(30, 41, 59);
    pub const BORDER_CARD: Color = Color::Rgb(35, 50, 77);
    pub const BORDER_FOCUSED: Color = Color::Rgb(59, 130, 246);
    pub const BORDER_SUCCESS: Color = Color::Rgb(16, 185, 129);
    pub const BORDER_PURPLE: Color = Color::Rgb(147, 51, 234);
    pub const BORDER_WARN: Color = Color::Rgb(245, 158, 11);
    pub const BORDER_DANGER: Color = Color::Rgb(239, 68, 68);

    // Accents
    pub const ACCENT_ORANGE: Color = Color::Rgb(249, 115, 22); // Rusty brand orange
    pub const ACCENT_BLUE: Color = Color::Rgb(59, 130, 246);
    pub const ACCENT_GREEN: Color = Color::Rgb(16, 185, 129);
    pub const ACCENT_PURPLE: Color = Color::Rgb(168, 85, 247);
    pub const ACCENT_YELLOW: Color = Color::Rgb(245, 158, 11);
    pub const ACCENT_RED: Color = Color::Rgb(239, 68, 68);
    pub const ACCENT_CYAN: Color = Color::Rgb(6, 182, 212);

    // Text
    pub const TEXT_PRIMARY: Color = Color::Rgb(240, 244, 250);
    pub const TEXT_SECONDARY: Color = Color::Rgb(148, 163, 184);
    pub const TEXT_MUTED: Color = Color::Rgb(100, 116, 139);
    pub const TEXT_FAINT: Color = Color::Rgb(71, 85, 105);

    // Styles
    pub fn title() -> Style {
        Style::default()
            .fg(Self::ACCENT_ORANGE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn text() -> Style {
        Style::default().fg(Self::TEXT_PRIMARY)
    }

    pub fn text_muted() -> Style {
        Style::default().fg(Self::TEXT_MUTED)
    }

    pub fn text_secondary() -> Style {
        Style::default().fg(Self::TEXT_SECONDARY)
    }

    pub fn card_title() -> Style {
        Style::default()
            .fg(Self::TEXT_PRIMARY)
            .add_modifier(Modifier::BOLD)
    }

    pub fn nav_active() -> Style {
        Style::default()
            .fg(Color::White)
            .bg(Self::BG_NAV_ACTIVE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn nav_inactive() -> Style {
        Style::default().fg(Self::TEXT_SECONDARY)
    }

    pub fn nav_category() -> Style {
        Style::default()
            .fg(Self::TEXT_MUTED)
            .add_modifier(Modifier::BOLD)
    }

    pub fn quick_action_key() -> Style {
        Style::default()
            .fg(Color::White)
            .bg(Self::ACCENT_BLUE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn badge_green() -> Style {
        Style::default()
            .fg(Self::ACCENT_GREEN)
            .add_modifier(Modifier::BOLD)
    }

    pub fn badge_purple() -> Style {
        Style::default()
            .fg(Self::ACCENT_PURPLE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn badge_blue() -> Style {
        Style::default()
            .fg(Self::ACCENT_BLUE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn badge_warn() -> Style {
        Style::default()
            .fg(Self::ACCENT_YELLOW)
            .add_modifier(Modifier::BOLD)
    }

    pub fn badge_danger() -> Style {
        Style::default()
            .fg(Self::ACCENT_RED)
            .add_modifier(Modifier::BOLD)
    }
}
