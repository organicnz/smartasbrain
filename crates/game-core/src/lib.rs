//! Contracts every smartasbrain game implements, plus the shell types used to
//! route input and render tabs.

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{Frame, layout::Rect};

pub mod geom;
pub mod persist;

/// Strength presets for built-in AI opponents, in menu order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Difficulty {
    /// Shallow search, prone to mistakes.
    Easy,
    /// Solid tactical heuristics.
    Medium,
    /// Deeper search, few free gifts.
    Hard,
}

impl Difficulty {
    /// All presets in menu order.
    pub const ALL: [Difficulty; 3] = [Self::Easy, Self::Medium, Self::Hard];

    /// Short uppercase label for menus, e.g. `"EASY"`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Easy => "EASY",
            Self::Medium => "MEDIUM",
            Self::Hard => "HARD",
        }
    }
}

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

    /// Out-of-band navigation request, sampled by the shell right after
    /// input handling. Returning `Some(tab_index)` switches to that game;
    /// implementations must consume the request here (e.g. `Option::take`)
    /// so it fires exactly once.
    fn poll_navigation(&mut self) -> Option<usize> {
        None
    }

    /// Mid-game state blob for session persistence, or None when the game
    /// is not worth saving (fresh start, setup screen).
    fn snapshot(&self) -> Option<String> {
        None
    }

    /// Applies a previous [`Game::snapshot`] blob; false rejects it.
    fn restore(&mut self, _blob: &str) -> bool {
        false
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect);

    /// True when the game wants the whole application to exit.
    fn wants_quit(&self) -> bool {
        false
    }
}
