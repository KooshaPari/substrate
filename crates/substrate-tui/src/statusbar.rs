//! Status bar — bottom bar showing gateway connectivity, lane count,
//! and keyboard binding hints.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::App;

/// Draw the status bar at the bottom of the screen.
pub fn draw_statusbar(frame: &mut Frame, area: Rect, app: &App) {
    let status = if app.connected {
        Span::styled(
            " ● CONNECTED ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " ○ DISCONNECTED ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    };

    let lanes = Span::styled(
        format!(" {} lanes ", app.active_lanes()),
        Style::default().fg(Color::Cyan),
    );

    let keys = Span::styled(
        " [q] quit  [r] refresh  [h] help  [Esc] close overlay ",
        Style::default().fg(Color::DarkGray),
    );

    let line = Line::from(vec![
        status,
        Span::raw(" │ "),
        lanes,
        Span::raw(" │ "),
        keys,
    ]);
    let paragraph = Paragraph::new(line).block(Block::default());
    frame.render_widget(paragraph, area);
}
