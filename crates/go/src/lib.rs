//! Go — 9×9 rules engine and TUI for smartasbrain.
//!
//! Implements alternating play, captures, suicide prohibition, positional
//! superko, passing, and simple area scoring with 6.5 komi.

mod engine;
mod ui;

use crossterm::event::{KeyEvent, MouseEvent};
use game_core::Game;
use ratatui::Frame;

pub use engine::{GoState, KOMI, MoveResult, Player, Score};

/// Default side length. Small enough to finish, big enough to fight.
pub const SIZE: usize = 9;

pub struct GoGame {
    state: engine::GoState,
    cursor: usize,
    /// Screen geometry captured during draw so clicks map to intersections.
    geom: Option<ui::Geom>,
    buttons: Vec<(ratatui::layout::Rect, Button)>,
    quit: bool,
}

#[derive(Clone, Copy)]
enum Button {
    Pass,
    Restart,
}

impl GoGame {
    pub fn new() -> Self {
        Self {
            state: engine::GoState::new(SIZE),
            cursor: 0,
            geom: None,
            buttons: Vec::new(),
            quit: false,
        }
    }

    fn reset(&mut self) {
        self.state = engine::GoState::new(SIZE);
        self.cursor = (SIZE / 2) * SIZE + SIZE / 2;
        self.quit = false;
    }

    fn move_cursor(&mut self, dr: isize, dc: isize) {
        let r = (self.cursor / SIZE) as isize + dr;
        let c = (self.cursor % SIZE) as isize + dc;
        if (0..SIZE as isize).contains(&r) && (0..SIZE as isize).contains(&c) {
            self.cursor = (r * SIZE as isize + c) as usize;
        }
    }
}

impl Default for GoGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for GoGame {
    fn id(&self) -> &'static str {
        "go"
    }

    fn title(&self) -> &'static str {
        "go"
    }

    fn handle_key(&mut self, key: KeyEvent) {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => self.quit = true,
            KeyCode::Left | KeyCode::Char('h') => self.move_cursor(0, -1),
            KeyCode::Right | KeyCode::Char('l') => self.move_cursor(0, 1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1, 0),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1, 0),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let _ = self.state.play(self.cursor);
            }
            KeyCode::Char('p' | 'P') => {
                let _ = self.state.pass();
            }
            KeyCode::Char('r' | 'R') => self.reset(),
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        for (rect, button) in &self.buttons {
            if crate::ui::in_rect(*rect, mouse.column, mouse.row) {
                match button {
                    Button::Pass => {
                        let _ = self.state.pass();
                    }
                    Button::Restart => self.reset(),
                }
                return;
            }
        }
        if let Some(idx) = self.geom.and_then(|g| g.cell_at(mouse.column, mouse.row)) {
            self.cursor = idx;
            let _ = self.state.play(idx);
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        self.buttons.clear();
        ui::draw(
            frame,
            &mut self.state,
            area,
            self.cursor,
            &mut self.geom,
            &mut self.buttons,
        );
    }

    fn wants_quit(&self) -> bool {
        self.quit
    }
}
