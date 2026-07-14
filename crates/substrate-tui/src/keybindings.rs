#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyAction {
    Quit,
    Up,
    Down,
    Left,
    Right,
    Select,
    Back,
    Help,
    Refresh,
    Tab,
    Unknown(String),
}
impl KeyAction {
    pub fn label(&self) -> &str {
        match self {
            Self::Quit => "q",
            Self::Up => "k",
            Self::Down => "j",
            Self::Left => "h",
            Self::Right => "l",
            Self::Select => "Enter",
            Self::Back => "Esc",
            Self::Help => "?",
            Self::Refresh => "r",
            Self::Tab => "Tab",
            Self::Unknown(s) => s.as_str(),
        }
    }
    pub fn is_nav(&self) -> bool {
        matches!(self, Self::Up | Self::Down | Self::Left | Self::Right)
    }
}
pub fn parse_key(s: &str) -> KeyAction {
    match s {
        "q" | "Q" => KeyAction::Quit,
        "Up" | "k" => KeyAction::Up,
        "Down" | "j" => KeyAction::Down,
        "Left" | "h" => KeyAction::Left,
        "Right" | "l" => KeyAction::Right,
        "Enter" => KeyAction::Select,
        "Esc" => KeyAction::Back,
        "?" => KeyAction::Help,
        "r" | "R" => KeyAction::Refresh,
        "Tab" => KeyAction::Tab,
        o => KeyAction::Unknown(o.to_string()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quit() {
        assert_eq!(parse_key("q"), KeyAction::Quit);
    }
    #[test]
    fn nav_up() {
        assert_eq!(parse_key("Up"), KeyAction::Up);
    }
    #[test]
    fn unknown() {
        assert_eq!(parse_key("x"), KeyAction::Unknown("x".into()));
    }
    #[test]
    fn label() {
        assert_eq!(KeyAction::Quit.label(), "q");
    }
    #[test]
    fn is_nav() {
        assert!(KeyAction::Up.is_nav());
        assert!(!KeyAction::Quit.is_nav());
    }
    #[test]
    fn enter() {
        assert_eq!(parse_key("Enter"), KeyAction::Select);
    }
}
