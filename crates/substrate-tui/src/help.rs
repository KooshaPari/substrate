//! Help overlay — lists keyboard shortcuts.

use ratatui::{
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Draw the help overlay with keyboard shortcuts.
pub fn draw_help(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Help ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .style(Style::default().bg(ratatui::style::Color::Rgb(20, 20, 30)));

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Keyboard Shortcuts ",
            Style::default().bold().underlined(),
        )),
        Line::from(""),
        Line::from(Span::raw(" q       — Quit the dashboard")),
        Line::from(Span::raw(" Tab     — Switch between tabs")),
        Line::from(Span::raw(" r       — Refresh data")),
        Line::from(Span::raw(" h       — Toggle this help screen")),
        Line::from(Span::raw(" ↑ / ↓   — Scroll lists")),
        Line::from(Span::raw(" Enter   — Select / drill down")),
        Line::from(""),
        Line::from(Span::styled(
            " Press any key to close ",
            Style::default().dim(),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left);

    // Clear area first for overlay effect.
    frame.render_widget(Clear, area);
    frame.render_widget(paragraph, area);
}
