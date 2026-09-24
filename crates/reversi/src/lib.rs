//! Reversi — 8×8 flipping engine and TUI for smartasbrain.
//!
//! Classic Othello rules: bracket opposing lines to flip them, pass
//! automatically when stranded, and win by majority once neither side can
//! move or the board fills. See [`engine`] for the rules model and [`ai`]
//! for the negamax search behind the built-in opponents.

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyEvent, MouseEvent};
use game_core::{Difficulty, Game};
use ratatui::Frame;
use ratatui::layout::Rect;

pub use engine::{Reversi, Side, Status};

/// Starting cursor square: d3, a legal opening placement.
const START_CURSOR: usize = engine::sq(2, 3);

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Button {
    Restart,
}

pub struct ReversiGame {
    state: Reversi,
    cursor: usize,
    /// Last seen mouse position, for button hover styling.
    hover: Option<(u16, u16)>,
    /// Screen geometry captured during draw so clicks map to squares.
    geom: Option<ui::Geom>,
    buttons: Vec<(Rect, Button)>,
    quit: bool,
    opponent: Opponent,
    /// Seat the engine plays; black, which also opens every match.
    ai_side: Side,
    setup_open: bool,
    setup_sel: usize,
    /// Screen rects of menu rows, rebuilt each draw for mouse handling.
    menu_rects: Vec<(Rect, usize)>,
    last_ai_move: Option<usize>,
}

impl ReversiGame {
    pub fn new() -> Self {
        Self {
            state: Reversi::new(),
            cursor: START_CURSOR,
            hover: None,
            geom: None,
            buttons: Vec::new(),
            quit: false,
            opponent: Opponent::Human,
            ai_side: Side::Black,
            setup_open: true,
            setup_sel: 0,
            menu_rects: Vec::new(),
            last_ai_move: None,
        }
    }

    /// Fresh board with the same opponent settings (`R` restart).
    fn reset(&mut self) {
        self.state = Reversi::new();
        self.cursor = START_CURSOR;
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
        self.setup_sel = 0;
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
            Opponent::Ai(_) => self.state.turn() != self.ai_side,
            Opponent::Battle(_) => false,
        }
    }

    /// True while an automated seat is on turn in a live game. During a
    /// duel both seats qualify, so this stays true until the game ends.
    fn ai_to_move(&self) -> bool {
        if self.setup_open || self.state.status() != Status::Ongoing {
            return false;
        }
        match self.opponent {
            Opponent::Human => false,
            Opponent::Ai(_) => self.state.turn() == self.ai_side,
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
        self.ai_to_move().then_some(self.state.turn())
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

    /// Reopen the setup menu over a fresh board.
    fn open_setup(&mut self) {
        self.setup_open = true;
        self.setup_sel = 0;
        self.last_ai_move = None;
        self.reset();
    }

    /// Shared play-at-square logic for Enter/Space and mouse clicks.
    fn activate(&mut self, at: usize) {
        if !self.human_can_act() {
            return;
        }
        self.cursor = at;
        if self.state.status() == Status::Ongoing
            && self.state.legal_moves().contains(&at)
            && self.state.play(at)
        {
            self.last_ai_move = None;
        }
    }
}

impl Default for ReversiGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for ReversiGame {
    fn id(&self) -> &'static str {
        "reversi"
    }

    fn title(&self) -> &'static str {
        "reversi"
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
        // Automated play owns the board: the machine plays both seats in a
        // duel or holds its turn in vs-AI, so everything except quit /
        // restart / menu stays inert.
        if self.ai_to_move() {
            match key.code {
                KeyCode::Char('q' | 'Q') => self.quit = true,
                KeyCode::Char('R') => self.reset(),
                KeyCode::Char('m' | 'M') => self.open_setup(),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('q' | 'Q') => self.quit = true,
            KeyCode::Esc => self.quit = true,
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
                if self.setup_open {
                    for (rect, index) in &self.menu_rects {
                        if game_core::geom::in_rect(*rect, mouse.column, mouse.row) {
                            self.confirm_setup(*index);
                            break;
                        }
                    }
                    return;
                }
                for (rect, button) in &self.buttons {
                    if game_core::geom::in_rect(*rect, mouse.column, mouse.row) {
                        // Automated play owns the action; clicks do nothing.
                        if !self.ai_to_move() && *button == Button::Restart {
                            self.reset();
                        }
                        return;
                    }
                }
                if let Some(idx) = self.geom.and_then(|g| g.cell_at(mouse.column, mouse.row)) {
                    self.activate(idx);
                }
            }
            _ => {}
        }
    }

    /// Advances automated play one move per frame. The engine's auto-pass
    /// keeps whoever owns the turn mobile, so a `Some` from the search is
    /// always playable; duels drive both seats, vs-AI only its own side.
    fn tick(&mut self) {
        let Some(difficulty) = self.ai_seat_difficulty() else {
            return;
        };
        if let Some(mv) = ai::best_move(&self.state, difficulty)
            && self.state.play(mv)
        {
            self.last_ai_move = Some(mv);
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        self.buttons.clear();
        if self.setup_open {
            self.geom = None;
            ui::draw_setup(
                frame,
                area,
                self.setup_sel,
                self.hover,
                &mut self.menu_rects,
            );
            return;
        }
        let inter = ui::Interaction {
            cursor: self.cursor,
            hover: self.hover,
            last_move: self.last_ai_move,
            mode: self.mode_label(),
            thinking: self.thinking_side(),
            show_legal_moves: !self.ai_to_move(),
        };
        ui::draw(
            frame,
            &self.state,
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
