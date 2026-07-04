use ratatui::{
    widgets::{Block, Borders, List, ListItem},
    style::{Style, Color},
    layout::Rect,
    Frame,
    text::{Line, Span},
};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq)]
pub enum LogLevel { Info, Warn, Error, Debug }

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    pub timestamp: String,
}

pub struct LogPanel {
    pub entries: VecDeque<LogEntry>,
    pub max_entries: usize,
}

impl LogPanel {
    pub fn new(max_entries: usize) -> Self {
        Self { entries: VecDeque::new(), max_entries }
    }

    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.entries.iter().map(|e| {
            let (prefix, col) = match e.level {
                LogLevel::Error => ("[ERR]", Color::Red),
                LogLevel::Warn  => ("[WRN]", Color::Yellow),
                LogLevel::Info  => ("[INF]", Color::Cyan),
                LogLevel::Debug => ("[DBG]", Color::DarkGray),
            };
            let line = Line::from(vec![
                Span::styled(format!("{} ", e.timestamp), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", prefix), Style::default().fg(col)),
                Span::raw(e.message.clone()),
            ]);
            ListItem::new(line)
        }).collect();
        let list = List::new(items)
            .block(Block::default().title("Logs").borders(Borders::ALL));
        f.render_widget(list, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_respects_max() {
        let mut p = LogPanel::new(3);
        for i in 0..5 {
            p.push(LogEntry { level: LogLevel::Info, message: format!("msg {}", i), timestamp: "00:00".into() });
        }
        assert_eq!(p.entries.len(), 3);
        assert_eq!(p.entries.back().unwrap().message, "msg 4");
    }
    #[test]
    fn push_oldest_evicted() {
        let mut p = LogPanel::new(2);
        p.push(LogEntry { level: LogLevel::Info, message: "first".into(), timestamp: "t".into() });
        p.push(LogEntry { level: LogLevel::Info, message: "second".into(), timestamp: "t".into() });
        p.push(LogEntry { level: LogLevel::Info, message: "third".into(), timestamp: "t".into() });
        assert_eq!(p.entries.front().unwrap().message, "second");
    }
    #[test]
    fn empty_panel_no_panic() {
        let p = LogPanel::new(100);
        assert_eq!(p.entries.len(), 0);
    }
    #[test]
    fn all_levels_construct() {
        let levels = [LogLevel::Info, LogLevel::Warn, LogLevel::Error, LogLevel::Debug];
        for l in levels {
            let e = LogEntry { level: l, message: "x".into(), timestamp: "t".into() };
            assert_eq!(e.message, "x");
        }
    }
}
