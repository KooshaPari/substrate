#[derive(Debug,Clone,PartialEq,Eq,Hash)]
pub enum KeyAction { Quit, Up, Down, Left, Right, Select, Back, Help, Refresh, Tab, Unknown(String) }

impl KeyAction {
    pub fn label(&self) -> &str {
        match self {
            Self::Quit => "q", Self::Up => "↑", Self::Down => "↓",
            Self::Left => "←", Self::Right => "→", Self::Select => "Enter",
            Self::Back => "Esc", Self::Help => "?", Self::Refresh => "r",
            Self::Tab => "Tab", Self::Unknown(s) => s.as_str(),
        }
    }
    pub fn is_navigation(&self) -> bool {
        matches!(self, Self::Up|Self::Down|Self::Left|Self::Right)
    }
}

pub fn parse_key(s: &str) -> KeyAction {
    match s {
        "q"|"Q" => KeyAction::Quit,
        "Up"|"k" => KeyAction::Up,
        "Down"|"j" => KeyAction::Down,
        "Left"|"h" => KeyAction::Left,
        "Right"|"l" => KeyAction::Right,
        "Enter"|"Return" => KeyAction::Select,
        "Esc" => KeyAction::Back,
        "?" => KeyAction::Help,
        "r"|"R" => KeyAction::Refresh,
        "Tab" => KeyAction::Tab,
        other => KeyAction::Unknown(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parse_quit() { assert_eq!(parse_key("q"), KeyAction::Quit); }
    #[test] fn parse_nav() { assert_eq!(parse_key("Up"), KeyAction::Up); assert_eq!(parse_key("Down"), KeyAction::Down); }
    #[test] fn parse_unknown() { assert_eq!(parse_key("x"), KeyAction::Unknown("x".into())); }
    #[test] fn label_quit() { assert_eq!(KeyAction::Quit.label(), "q"); }
    #[test] fn is_navigation() { assert!(KeyAction::Up.is_navigation()); assert!(!KeyAction::Quit.is_navigation()); }
    #[test] fn parse_enter() { assert_eq!(parse_key("Enter"), KeyAction::Select); }
}
