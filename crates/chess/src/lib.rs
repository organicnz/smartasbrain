//! Chess — 8×8 rules engine and TUI for smartasbrain.
//!
//! Implements full piece movement, pins via clone-filter legality, castling,
//! en passant, auto-queen promotion, and checkmate/stalemate detection, plus
//! a setup menu for two-player, vs-AI, or spectated AI-duel matches.

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyEvent, MouseEvent};
use game_core::{Difficulty, Game};
use ratatui::Frame;

pub use engine::{Chess, Side, Status};

/// Starting cursor square: e2, a pawn most moves can begin from.
const START_CURSOR: usize = engine::sq(6, 4);

const SETUP_ITEMS: [&str; 9] = [
    "TWO PLAYERS",
    "VS AI - EASY",
    "VS AI - MEDIUM",
    "VS AI - HARD",
    "VS AI - EXPERT",
    "AI DUEL - EASY",
    "AI DUEL - MEDIUM",
    "AI DUEL - HARD",
    "AI DUEL - EXPERT",
];

/// Match configuration: two humans, a human against the engine, or a
/// spectated engine-vs-engine duel where all board input is inert.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Opponent {
    Human,
    Ai(Difficulty),
    Battle(Difficulty),
}

#[derive(Clone, Copy)]
enum Button {
    Restart,
    /// Setup-menu entry identified by its index.
    MenuItem(usize),
}

pub struct ChessGame {
    engine: Chess,
    cursor: usize,
    selected: Option<usize>,
    /// Last seen mouse position, for button hover styling.
    hover: Option<(u16, u16)>,
    /// Screen geometry captured during draw so clicks map to squares.
    geom: Option<ui::Geom>,
    buttons: Vec<(ratatui::layout::Rect, Button)>,
    quit: bool,
    opponent: Opponent,
    /// Colour the engine plays when [`Opponent::Ai`] is confirmed.
    ai_side: Side,
    setup_open: bool,
    setup_sel: usize,
    last_ai_move: Option<(usize, usize)>,
}

impl ChessGame {
    pub fn new() -> Self {
        Self {
            engine: Chess::new(),
            cursor: START_CURSOR,
            selected: None,
            hover: None,
            geom: None,
            buttons: Vec::new(),
            quit: false,
            opponent: Opponent::Human,
            ai_side: Side::Black,
            setup_open: true,
            setup_sel: 0,
            last_ai_move: None,
        }
    }

    /// Fresh board with the same opponent settings (`r` restart).
    fn reset(&mut self) {
        self.engine = Chess::new();
        self.cursor = START_CURSOR;
        self.selected = None;
        self.last_ai_move = None;
        self.quit = false;
    }

    /// Apply a confirmed setup-menu choice and start the match.
    fn confirm_setup(&mut self, choice: usize) {
        let count = Difficulty::ALL.len();
        self.opponent = match choice {
            0 => Opponent::Human,
            i if (1..=count).contains(&i) => Opponent::Ai(Difficulty::ALL[i - 1]),
            i if (count + 1..=2 * count).contains(&i) => {
                Opponent::Battle(Difficulty::ALL[i - count - 1])
            }
            _ => return,
        };
        if matches!(self.opponent, Opponent::Ai(_)) {
            self.ai_side = Side::Black;
        }
        self.setup_open = false;
        self.reset();
    }

    fn cycle_setup(&mut self, dir: isize) {
        let len = SETUP_ITEMS.len() as isize;
        self.setup_sel = (self.setup_sel as isize + dir).rem_euclid(len) as usize;
    }

    /// True when board select/play input may act right now. During a duel
    /// both seats are automated, so the human never acts.
    fn human_can_act(&self) -> bool {
        if self.setup_open {
            return false;
        }
        match self.opponent {
            Opponent::Human => true,
            Opponent::Ai(_) => self.engine.turn() != self.ai_side,
            Opponent::Battle(_) => false,
        }
    }

    /// True while an automated seat is on turn in a live game. During a
    /// duel both seats qualify, so this stays true until the game ends.
    fn ai_to_move(&self) -> bool {
        if self.setup_open || self.engine.status() != Status::Ongoing {
            return false;
        }
        match self.opponent {
            Opponent::Human => false,
            Opponent::Ai(_) => self.engine.turn() == self.ai_side,
            Opponent::Battle(_) => true,
        }
    }

    /// Difficulty preset for whichever automated seat is due to move.
    fn ai_seat_difficulty(&self) -> Option<Difficulty> {
        if !self.ai_to_move() {
            return None;
        }
        match self.opponent {
            Opponent::Ai(difficulty) | Opponent::Battle(difficulty) => Some(difficulty),
            Opponent::Human => None,
        }
    }

    /// Side the panel should mark as deliberating: whoever is on turn while
    /// an automated seat moves — during a duel that alternates every frame.
    fn thinking_side(&self) -> Option<Side> {
        self.ai_to_move().then_some(self.engine.turn())
    }

    fn mode_label(&self) -> String {
        match self.opponent {
            Opponent::Human => "two players".to_string(),
            Opponent::Ai(difficulty) => {
                format!("vs ai {} ({})", difficulty.label(), self.ai_side.name())
            }
            Opponent::Battle(difficulty) => format!("ai duel {}", difficulty.label()),
        }
    }

    fn move_cursor(&mut self, dr: isize, dc: isize) {
        let r = (engine::row(self.cursor) as isize + dr).clamp(0, 7);
        let c = (engine::col(self.cursor) as isize + dc).clamp(0, 7);
        self.cursor = engine::sq(r as usize, c as usize);
    }

    /// Reopen the setup menu, dropping any pending board state.
    fn open_setup(&mut self) {
        self.setup_open = true;
        self.setup_sel = 0;
        self.selected = None;
        self.last_ai_move = None;
    }

    /// Shared select/play/reselect logic for Enter/Space and mouse clicks.
    fn activate(&mut self, at: usize) {
        if !self.human_can_act() {
            return;
        }
        self.cursor = at;
        match self.selected {
            None => {
                let own = self
                    .engine
                    .board()
                    .get(at)
                    .copied()
                    .flatten()
                    .is_some_and(|p| p.side == self.engine.turn());
                if own {
                    self.selected = Some(at);
                }
            }
            Some(from) => {
                if self.engine.play(from, at) {
                    self.selected = None;
                } else {
                    let own = self
                        .engine
                        .board()
                        .get(at)
                        .copied()
                        .flatten()
                        .is_some_and(|p| p.side == self.engine.turn());
                    if own {
                        self.selected = Some(at); // reselect
                    } else if at == from {
                        self.selected = None; // click same square toggles off
                    }
                }
            }
        }
    }
}

impl Default for ChessGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for ChessGame {
    fn id(&self) -> &'static str {
        "chess"
    }

    fn title(&self) -> &'static str {
        "chess"
    }

    fn handle_key(&mut self, key: KeyEvent) {
        use crossterm::event::KeyCode;
        if self.setup_open {
            match key.code {
                KeyCode::Char('q' | 'Q') | KeyCode::Esc => self.quit = true,
                KeyCode::Up | KeyCode::Char('k') => self.cycle_setup(-1),
                KeyCode::Down | KeyCode::Char('j') => self.cycle_setup(1),
                KeyCode::Enter | KeyCode::Char(' ') => self.confirm_setup(self.setup_sel),
                KeyCode::Char(c)
                    if c.to_digit(10)
                        .is_some_and(|d| (1..=SETUP_ITEMS.len() as u32).contains(&d)) =>
                {
                    self.setup_sel = c.to_digit(10).unwrap() as usize - 1;
                }
                _ => {}
            }
            return;
        }
        // Spectating an AI duel: the machine plays both seats, so board
        // actions are inert and lowercase 'r' stays reserved for human
        // seats. Only quit / deselect / capital-R restart / menu respond.
        if matches!(self.opponent, Opponent::Battle(_)) {
            match key.code {
                KeyCode::Char('q' | 'Q') => self.quit = true,
                KeyCode::Esc => self.selected = None,
                KeyCode::Char('R') => self.reset(),
                KeyCode::Char('m' | 'M') => self.open_setup(),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('q' | 'Q') => self.quit = true,
            KeyCode::Esc => {
                // Esc deselects while a piece is held, quits otherwise.
                if self.selected.is_some() {
                    self.selected = None;
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => self.move_cursor(0, -1),
            KeyCode::Right | KeyCode::Char('l') => self.move_cursor(0, 1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1, 0),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1, 0),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(self.cursor),
            KeyCode::Char('r' | 'R') => self.reset(),
            KeyCode::Char('m' | 'M') => self.open_setup(),
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                self.hover = Some((mouse.column, mouse.row));
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // Duels are spectate-only: buttons and squares never react.
                if matches!(self.opponent, Opponent::Battle(_)) {
                    return;
                }
                for (rect, button) in &self.buttons {
                    if game_core::geom::in_rect(*rect, mouse.column, mouse.row) {
                        match button {
                            Button::Restart => self.reset(),
                            Button::MenuItem(index) => self.confirm_setup(*index),
                        }
                        return;
                    }
                }
                if !self.setup_open
                    && let Some(idx) = self.geom.and_then(|g| g.cell_at(mouse.column, mouse.row))
                {
                    self.activate(idx);
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self) {
        let Some(difficulty) = self.ai_seat_difficulty() else {
            return;
        };
        if let Some((from, to)) = ai::best_move(&self.engine, difficulty)
            && self.engine.play(from, to)
        {
            self.last_ai_move = Some((from, to));
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        self.buttons.clear();
        if self.setup_open {
            self.geom = None;
            ui::draw_setup(frame, area, self.setup_sel, self.hover, &mut self.buttons);
            return;
        }
        let mode = self.mode_label();
        let inter = ui::Interaction {
            cursor: self.cursor,
            selected: self.selected,
            hover: self.hover,
            last_ai_move: self.last_ai_move,
            mode,
            thinking: self.thinking_side(),
        };
        ui::draw(
            frame,
            &self.engine,
            area,
            &inter,
            &mut self.geom,
            &mut self.buttons,
        );
    }

    fn wants_quit(&self) -> bool {
        self.quit
    }
}
