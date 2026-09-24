//! Go — 9×9 rules engine and TUI for smartasbrain.
//!
//! Implements alternating play, captures, suicide prohibition, positional
//! superko, passing, and simple area scoring with 6.5 komi. A heuristic AI
//! can take White, or two AIs can duel while the human spectates; modes are
//! chosen from a pre-game setup menu.

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyEvent, MouseEvent};
use game_core::{Difficulty, Game};
use ratatui::Frame;

pub use engine::{GoState, KOMI, MoveResult, Player, Score};

/// Default side length. Small enough to finish, big enough to fight.
pub const SIZE: usize = 9;

/// Seat control: two humans, a human against the AI (which takes White), or
/// a spectated AI-vs-AI duel at a given strength.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Opponent {
    Human,
    Ai(Difficulty),
    Battle(Difficulty),
}

pub struct GoGame {
    state: engine::GoState,
    cursor: usize,
    /// Screen geometry captured during draw so clicks map to intersections.
    geom: Option<ui::Geom>,
    buttons: Vec<(ratatui::layout::Rect, Button)>,
    quit: bool,
    opponent: Opponent,
    /// True until a mode is picked; the setup overlay replaces the board.
    setup_open: bool,
    setup_sel: usize,
    setup_hover: Option<usize>,
    /// Screen rects of the menu entries, rebuilt on every draw.
    menu_rects: Vec<(ratatui::layout::Rect, usize)>,
    /// Colour the AI plays; humans always take Black and move first.
    ai_side: Player,
    last_ai_move: Option<usize>,
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
            opponent: Opponent::Human,
            setup_open: true,
            setup_sel: 0,
            setup_hover: None,
            menu_rects: Vec::new(),
            ai_side: Player::White,
            last_ai_move: None,
        }
    }

    /// Fresh board, same opponent settings (`r` key).
    fn reset(&mut self) {
        self.state = engine::GoState::new(SIZE);
        self.cursor = (SIZE / 2) * SIZE + SIZE / 2;
        self.quit = false;
        self.last_ai_move = None;
    }

    /// Reopen the setup menu (`m` key); discards the current game.
    fn open_setup(&mut self) {
        self.setup_sel = match self.opponent {
            Opponent::Human => 0,
            Opponent::Ai(d) => 1 + Difficulty::ALL.iter().position(|&x| x == d).unwrap_or(0),
            Opponent::Battle(d) => {
                Difficulty::ALL.len()
                    + 1
                    + Difficulty::ALL.iter().position(|&x| x == d).unwrap_or(0)
            }
        };
        self.setup_hover = None;
        self.setup_open = true;
        self.reset();
    }

    fn confirm_setup(&mut self, item: usize) {
        let count = Difficulty::ALL.len();
        self.opponent = match item {
            0 => Opponent::Human,
            i if (1..=count).contains(&i) => Opponent::Ai(Difficulty::ALL[i - 1]),
            i if (count + 1..=2 * count).contains(&i) => {
                Opponent::Battle(Difficulty::ALL[i - count - 1])
            }
            _ => return,
        };
        self.ai_side = Player::White;
        self.setup_open = false;
        self.reset();
    }

    /// True when whoever holds the turn is automated right now.
    fn ai_to_move(&self) -> bool {
        if self.setup_open || self.state.over {
            return false;
        }
        match self.opponent {
            Opponent::Human => false,
            Opponent::Ai(_) => self.state.turn == self.ai_side,
            Opponent::Battle(_) => true,
        }
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

        if self.setup_open {
            let count = ui::SETUP_ITEMS.len();
            match key.code {
                KeyCode::Char('q' | 'Q') | KeyCode::Esc => self.quit = true,
                KeyCode::Up | KeyCode::Char('k') => {
                    self.setup_sel = (self.setup_sel + count - 1) % count
                }
                KeyCode::Down | KeyCode::Char('j') => self.setup_sel = (self.setup_sel + 1) % count,
                KeyCode::Enter | KeyCode::Char(' ') => self.confirm_setup(self.setup_sel),
                KeyCode::Char(c)
                    if c.to_digit(10)
                        .is_some_and(|d| (1..=count as u32).contains(&d)) =>
                {
                    let item = c.to_digit(10).unwrap() as usize - 1;
                    self.setup_sel = item;
                    self.confirm_setup(item);
                }
                _ => {}
            }
            return;
        }

        let duel = matches!(self.opponent, Opponent::Battle(_));
        let ai_turn = self.ai_to_move();
        match key.code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => self.quit = true,
            KeyCode::Left | KeyCode::Char('h') => self.move_cursor(0, -1),
            KeyCode::Right | KeyCode::Char('l') => self.move_cursor(0, 1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1, 0),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1, 0),
            // Never play a stone on the AI's behalf.
            KeyCode::Enter | KeyCode::Char(' ') if !ai_turn => {
                let _ = self.state.play(self.cursor);
            }
            KeyCode::Char('p' | 'P') if !ai_turn => {
                let _ = self.state.pass();
            }
            // Mid-duel only capital R restarts; lowercase `r` stays inert.
            KeyCode::Char('r') if !duel => self.reset(),
            KeyCode::Char('R') => self.reset(),
            KeyCode::Char('m' | 'M') => self.open_setup(),
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};

        if self.setup_open {
            match mouse.kind {
                MouseEventKind::Moved => {
                    self.setup_hover = self
                        .menu_rects
                        .iter()
                        .find(|(r, _)| ui::in_rect(*r, mouse.column, mouse.row))
                        .map(|&(_, i)| i);
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(&(_, i)) = self
                        .menu_rects
                        .iter()
                        .find(|(r, _)| ui::in_rect(*r, mouse.column, mouse.row))
                    {
                        self.setup_sel = i;
                        self.confirm_setup(i);
                    }
                }
                _ => {}
            }
            return;
        }

        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        // Spectate mode: board and panel clicks drive nothing.
        if matches!(self.opponent, Opponent::Battle(_)) {
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
        if self.ai_to_move() {
            return;
        }
        if let Some(idx) = self.geom.and_then(|g| g.cell_at(mouse.column, mouse.row)) {
            self.cursor = idx;
            let _ = self.state.play(idx);
        }
    }

    fn tick(&mut self) {
        if !self.ai_to_move() {
            return;
        }
        let difficulty = match self.opponent {
            Opponent::Ai(d) | Opponent::Battle(d) => d,
            Opponent::Human => return,
        };
        // best_move plans for whoever holds the turn, so the same path
        // serves both seats of a duel.
        match ai::best_move(&self.state, difficulty) {
            Some(idx) => {
                self.cursor = idx;
                if self.state.play(idx) == MoveResult::Played {
                    self.last_ai_move = Some(idx);
                } else {
                    let _ = self.state.pass(); // candidates are pre-verified; safety net
                }
            }
            None => {
                let _ = self.state.pass();
            }
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        self.buttons.clear();
        if self.setup_open {
            ui::draw_setup(
                frame,
                area,
                self.setup_sel,
                self.setup_hover,
                &mut self.menu_rects,
            );
            return;
        }
        let mode = match self.opponent {
            Opponent::Human => "two players".to_string(),
            Opponent::Ai(d) => format!("vs ai {} (white)", d.label()),
            Opponent::Battle(d) => format!("ai duel {}", d.label()),
        };
        let hud = ui::Hud {
            mode: &mode,
            thinking: self.ai_to_move().then_some(self.state.turn),
        };
        ui::draw(
            frame,
            &mut self.state,
            area,
            self.cursor,
            &hud,
            &mut self.geom,
            &mut self.buttons,
        );
    }

    fn wants_quit(&self) -> bool {
        self.quit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn click(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn draw_once(game: &mut GoGame) {
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| Game::draw(game, f, f.area())).unwrap();
    }

    const CENTER: usize = (SIZE / 2) * SIZE + SIZE / 2;

    #[test]
    fn setup_menu_blocks_board_input() {
        let mut game = GoGame::new();
        assert!(game.setup_open);

        // Keys that would move the cursor / drop a stone in play.
        game.handle_key(key(KeyCode::Right));
        game.handle_key(key(KeyCode::Enter));
        assert!(!game.setup_open, "Enter confirmed the highlighted entry");
        assert_eq!(game.opponent, Opponent::Human);
        assert!(
            game.state.board().iter().all(|&c| c == Player::Empty),
            "no stone may be placed while the menu is open"
        );

        // With the overlay up, clicks go to the menu, never to the board.
        game.handle_key(key(KeyCode::Char('m')));
        assert!(game.setup_open);
        draw_once(&mut game);
        assert!(game.geom.is_none(), "overlay replaces the board");
        game.handle_mouse(click(10, 10));
        assert!(game.state.board().iter().all(|&c| c == Player::Empty));
        assert!(game.setup_open);
    }

    #[test]
    fn selecting_vs_ai_then_tick_makes_ai_reply() {
        let mut game = GoGame::new();
        game.handle_key(key(KeyCode::Down)); // VS AI - EASY
        game.handle_key(key(KeyCode::Enter));
        assert!(!game.setup_open);
        assert_eq!(game.opponent, Opponent::Ai(Difficulty::Easy));
        assert_eq!(game.ai_side, Player::White);

        game.handle_key(key(KeyCode::Enter)); // human Black opens at centre
        assert_eq!(game.state.board()[CENTER], Player::Black);
        assert_eq!(game.state.turn, Player::White);

        game.tick();
        let whites = game
            .state
            .board()
            .iter()
            .filter(|&&c| c == Player::White)
            .count();
        assert_eq!(whites, 1, "the AI answered with exactly one stone");
        assert_eq!(game.state.turn, Player::Black);
        assert_eq!(
            game.last_ai_move.map(|idx| game.state.board()[idx]),
            Some(Player::White)
        );
    }

    #[test]
    fn human_cannot_play_for_ai() {
        let mut game = GoGame::new();
        game.handle_key(key(KeyCode::Down));
        game.handle_key(key(KeyCode::Enter)); // vs AI easy

        game.handle_key(key(KeyCode::Enter)); // black centre; now AI's turn
        assert!(game.ai_to_move());
        let before = game.state.board().to_vec();

        game.handle_key(key(KeyCode::Enter)); // ignored
        game.handle_key(key(KeyCode::Char(' '))); // ignored
        game.handle_key(key(KeyCode::Char('p'))); // no pass-by-proxy either
        assert_eq!(game.state.board(), before.as_slice());
        assert_eq!(game.state.turn, Player::White);

        // Board clicks are equally inert during the AI's turn.
        draw_once(&mut game);
        let geom = game.geom.unwrap();
        'outer: for col in 0..80u16 {
            for row in 0..24u16 {
                if geom.cell_at(col, row).is_some() {
                    game.handle_mouse(click(col, row));
                    break 'outer;
                }
            }
        }
        assert_eq!(game.state.board(), before.as_slice());

        game.tick(); // then the AI does get to move
        assert_eq!(game.state.turn, Player::Black);
    }

    #[test]
    fn duel_menu_mapping() {
        let mut game = GoGame::new();
        game.handle_key(key(KeyCode::Char('8'))); // AI DUEL - HARD
        assert_eq!(game.opponent, Opponent::Battle(Difficulty::Hard));
        assert!(!game.setup_open);

        let mut expert = GoGame::new();
        expert.handle_key(key(KeyCode::Char('9')));
        assert_eq!(expert.opponent, Opponent::Battle(Difficulty::Expert));

        // A stray confirm mid-duel places nothing and stays closed.
        game.handle_key(key(KeyCode::Enter));
        assert!(!game.setup_open);
        assert!(
            game.state.board().iter().all(|&c| c == Player::Empty),
            "no stone may appear from a leftover Enter"
        );
    }

    #[test]
    fn duel_self_play_progresses() {
        let mut game = GoGame::new();
        game.handle_key(key(KeyCode::Char('6'))); // AI DUEL - EASY
        assert_eq!(game.opponent, Opponent::Battle(Difficulty::Easy));
        assert!(!game.setup_open);

        let mut saw_black = false;
        let mut saw_white = false;
        let mut settled = false;
        for _ in 0..400 {
            game.tick(); // Black opens; both seats are automated
            saw_black |= game.state.board().contains(&Player::Black);
            saw_white |= game.state.board().contains(&Player::White);
            settled |= game.state.passes > 0 || game.state.over;
            if game.state.over {
                break;
            }
        }
        assert!(saw_black && saw_white, "both colours laid stones");
        assert!(settled, "the duel must pass out or finish within 400 plies");
    }

    #[test]
    fn duel_input_inert() {
        let mut game = GoGame::new();
        game.handle_key(key(KeyCode::Char('6'))); // AI DUEL - EASY
        game.tick(); // Black opens

        let board = game.state.board().to_vec();
        let turn = game.state.turn;

        game.handle_key(key(KeyCode::Enter)); // would place a stone
        game.handle_key(key(KeyCode::Char(' '))); // would place a stone
        game.handle_key(key(KeyCode::Char('p'))); // would pass
        game.handle_key(key(KeyCode::Char('r'))); // lowercase: no restart mid-duel

        draw_once(&mut game);
        let geom = game.geom.expect("board visible mid-duel");
        let mut target = None;
        'outer: for col in 0..80u16 {
            for row in 0..24u16 {
                if geom.cell_at(col, row).is_some() {
                    target = Some((col, row));
                    break 'outer;
                }
            }
        }
        if let Some((col, row)) = target {
            game.handle_mouse(click(col, row)); // board click ignored
        }
        if let Some(&(rect, _)) = game.buttons.first() {
            game.handle_mouse(click(rect.x + rect.width / 2, rect.y)); // button click ignored
        }

        assert_eq!(game.state.board(), board.as_slice(), "zero board delta");
        assert_eq!(game.state.turn, turn, "turn unchanged");
        assert_eq!(game.state.passes, 0, "nothing passed by proxy");
    }
}
