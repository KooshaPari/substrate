pub fn colorize(text: &str, fg: Color) -> String {
    format!("\x1b[{}m{}\x1b[0m", fg.code(), text)
}
pub fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' { in_escape = true; continue; }
        if in_escape { if c == 'm' { in_escape = false; } continue; }
        out.push(c);
    }
    out
}
#[derive(Debug,Clone,Copy)]
pub enum Color { Red, Green, Yellow, Blue, Magenta, Cyan, White }
impl Color {
    pub fn code(&self) -> u8 {
        match self {
            Self::Red => 31, Self::Green => 32, Self::Yellow => 33,
            Self::Blue => 34, Self::Magenta => 35, Self::Cyan => 36, Self::White => 37,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn red_text() { assert_eq!(colorize("hi", Color::Red), "\x1b[31mhi\x1b[0m"); }
    #[test] fn strip_color() { assert_eq!(strip("\x1b[31mhello\x1b[0m"), "hello"); }
    #[test] fn strip_multiple() { assert_eq!(strip("\x1b[32m\x1b[1mfoo\x1b[0m"), "foo"); }
    #[test] fn strip_no_ansi() { assert_eq!(strip("plain"), "plain"); }
    #[test] fn all_colors() { let _ = colorize("x", Color::Green); let _ = colorize("x", Color::Yellow); }
}
