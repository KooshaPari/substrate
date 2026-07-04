// Scaffolding WIP: Screen trait is not yet consumed by the event loop.
#![allow(dead_code, unused_imports)]
//! Tab-index and shared traits for TUI widgets.

pub mod dashboard;

use ratatui::Frame;

/// A renderable screen within a tab.
pub trait Screen {
    /// Draw this screen into the main content area.
    fn draw(&self, frame: &mut Frame, area: ratatui::layout::Rect);

    /// Title shown in the tab bar.
    fn title(&self) -> &'static str;
}
