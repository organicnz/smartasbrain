//! Contracts every smartasbrain game implements, plus the shell types used to
//! route input and render tabs.

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{Frame, layout::Rect};

/// A pluggable game or screen. Human-played games consume key events; AI
/// battlefields advance inside [`Game::tick`] and mostly ignore input.
pub trait Game {
    /// Stable identifier, e.g. `"sudoku"` — also used for persistence paths.
    fn id(&self) -> &'static str;

    /// Short label shown in the tab bar.
    fn title(&self) -> &'static str;

    fn handle_key(&mut self, key: KeyEvent);
    fn handle_mouse(&mut self, mouse: MouseEvent);

    /// Called once per frame before draw. Use for AI turns, animations,
    /// timers — anything that advances without input.
    fn tick(&mut self) {}

    fn draw(&mut self, frame: &mut Frame, area: Rect);

    /// True when the game wants the whole application to exit.
    fn wants_quit(&self) -> bool {
        false
    }
}
