//! Backgammon — rules engine and TUI for smartasbrain.
//!
//! Standard two-player race: hit blots to the bar, re-enter, bring all fifteen
//! checkers home, then bear off. Doubles grant four moves. See [`engine`] docs
//! for the exact rules model and simplifications (no max-die rule; auto-pass
//! approximated by a single-step union right after the roll).

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyEvent, MouseEvent};
use game_core::{Difficulty, Game};
use rand::SeedableRng;
use rand::rngs::StdRng;
use ratatui::Frame;
use ratatui::layout::Rect;

pub use engine::{BAR, Backgammon, Dest, OFF, Phase, Side};

use ui::View;

/// Who sits across the board: a second human, the built-in AI, or two AIs
/// playing each other while you spectate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opponent {
    Human,
    Ai(Difficulty),
    /// Both seats automated; input stays inert until the game ends.
    Battle(Difficulty),
}

/// Shell state wrapping the pure engine with cursor/selection and input.
pub struct BackgammonGame {
    state: engine::Backgammon,
    /// Cursor slot in 0..=25 using the engine's [`BAR`]/[`OFF`] pseudo-slots.
    cursor: usize,
    selected: Option<usize>,
    hover: Option<(u16, u16)>,
    geom: Option<ui::Geom>,
    buttons: Vec<(Rect, Button)>,
    quit: bool,
    /// Configured second seat; picked through the setup menu.
    opponent: Opponent,
    /// Setup menu visibility; starts open so every session picks a mode.
    setup_open: bool,
    /// Highlighted row in the setup menu.
    menu_index: usize,
    /// Screen rects of menu rows, rebuilt each draw for mouse handling.
    menu_rects: Vec<(Rect, usize)>,
    /// Seat the AI plays; black, which also opens the game.
    ai_side: Side,
    /// RNG feeding Easy's mistakes, seeded from entropy for variety.
    ai_rng: StdRng,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Button {
    Roll,
    Restart,
}

impl BackgammonGame {
    pub fn new() -> Self {
        Self {
            state: engine::Backgammon::new(),
            cursor: 23,
            selected: None,
            hover: None,
            geom: None,
            buttons: Vec::new(),
            quit: false,
            opponent: Opponent::Human,
            setup_open: true,
            menu_index: 0,
            menu_rects: Vec::new(),
            ai_side: Side::Black,
            ai_rng: StdRng::from_entropy(),
        }
    }

    /// Fresh board with identical opponent settings (the `r`/`R` restart).
    fn reset(&mut self) {
        self.state = engine::Backgammon::new();
        self.cursor = 23;
        self.selected = None;
        self.quit = false;
    }

    /// Apply the highlighted menu row and start play.
    fn confirm_menu(&mut self) {
        let count = Difficulty::ALL.len();
        self.opponent = match self.menu_index {
            0 => Opponent::Human,
            i if (1..=count).contains(&i) => Opponent::Ai(Difficulty::ALL[i - 1]),
            i if (count + 1..=2 * count).contains(&i) => {
                Opponent::Battle(Difficulty::ALL[i - count - 1])
            }
            _ => return,
        };
        self.ai_side = Side::Black;
        self.reset();
        self.setup_open = false;
    }

    /// True while an automated seat owns the action and board input must stay
    /// inert: the AI's turn in vs-AI, or any live moment of a running duel.
    fn ai_turn(&self) -> bool {
        if self.state.phase() == Phase::GameOver {
            return false;
        }
        match self.opponent {
            Opponent::Human => false,
            Opponent::Ai(_) => self.state.turn() == self.ai_side,
            Opponent::Battle(_) => true,
        }
    }

    /// Vs-AI-only busy state; stricter than [`ai_turn`] because even the
    /// restart key waits for the machine to finish its turn.
    fn vs_ai_turn(&self) -> bool {
        matches!(self.opponent, Opponent::Ai(_))
            && self.state.phase() != Phase::GameOver
            && self.state.turn() == self.ai_side
    }

    fn move_cursor(&mut self, d: isize) {
        let next = (self.cursor as isize + d).clamp(0, OFF as isize);
        self.cursor = next as usize;
    }

    /// Activate whatever the cursor sits on, shared by Enter/space/mouse.
    fn activate(&mut self) {
        match self.state.phase() {
            Phase::Roll => {
                self.selected = None;
                self.state.roll();
            }
            Phase::Move => {
                let movable = self.state.movable();
                match self.selected {
                    None => {
                        if movable.contains(&self.cursor) {
                            self.selected = Some(self.cursor);
                        }
                    }
                    Some(src) => {
                        let dest = slot_dest(self.cursor);
                        if src != self.cursor
                            && let Some(dest) = dest
                            && self.state.destinations(src).iter().any(|(d, _)| *d == dest)
                            && self.state.move_checker(src, dest).is_ok()
                        {
                            self.selected = None;
                            return;
                        }
                        // Fall through: hop to another own movable source.
                        if movable.contains(&self.cursor) {
                            self.selected = Some(self.cursor);
                        }
                    }
                }
            }
            Phase::GameOver => {}
        }
    }

    /// Panel context: mode description and whether the AI is mid-thought.
    fn panel_mode(&self) -> (String, bool) {
        let thinking = self.ai_turn();
        let mode = match self.opponent {
            Opponent::Human => "two players".to_string(),
            Opponent::Ai(difficulty) => format!(
                "vs ai {} ({})",
                difficulty.label(),
                match self.ai_side {
                    Side::White => "white",
                    Side::Black => "black",
                }
            ),
            Opponent::Battle(difficulty) => format!("ai duel {}", difficulty.label()),
        };
        (mode, thinking)
    }

    /// Seat rendered by the thinking glyph; duels flip it to whoever plays next.
    fn glyph_side(&self) -> Side {
        match self.opponent {
            Opponent::Battle(_) => self.state.turn(),
            _ => self.ai_side,
        }
    }
}

impl Default for BackgammonGame {
    fn default() -> Self {
        Self::new()
    }
}

fn slot_dest(cursor: usize) -> Option<Dest> {
    match cursor {
        OFF => Some(Dest::Off),
        i if i < 24 => Some(Dest::Point(i)),
        _ => None,
    }
}

impl Game for BackgammonGame {
    fn id(&self) -> &'static str {
        "backgammon"
    }

    fn title(&self) -> &'static str {
        "backgammon"
    }

    fn handle_key(&mut self, key: KeyEvent) {
        use crossterm::event::KeyCode;
        if key.code == KeyCode::Char('q') {
            self.quit = true;
            return;
        }
        if self.setup_open {
            match key.code {
                KeyCode::Esc => self.quit = true,
                KeyCode::Up | KeyCode::Char('k') => {
                    self.menu_index =
                        (self.menu_index + ui::MENU_ITEMS.len() - 1) % ui::MENU_ITEMS.len();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.menu_index = (self.menu_index + 1) % ui::MENU_ITEMS.len();
                }
                KeyCode::Char(c)
                    if c.to_digit(10)
                        .is_some_and(|d| (1..=ui::MENU_ITEMS.len() as u32).contains(&d)) =>
                {
                    self.menu_index = c.to_digit(10).unwrap() as usize - 1;
                }
                KeyCode::Enter | KeyCode::Char(' ') => self.confirm_menu(),
                _ => {}
            }
            return;
        }
        let ai_busy = self.ai_turn();
        match key.code {
            KeyCode::Esc => self.selected = None,
            KeyCode::Left | KeyCode::Char('h') => {
                self.cursor = if self.cursor == 0 {
                    OFF
                } else {
                    self.cursor - 1
                };
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.cursor = if self.cursor == OFF {
                    0
                } else {
                    self.cursor + 1
                };
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-12),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(12),
            KeyCode::Enter | KeyCode::Char(' ') if !ai_busy => self.activate(),
            KeyCode::Char('r') if !ai_busy && self.state.phase() == Phase::Roll => {
                self.selected = None;
                self.state.roll();
            }
            KeyCode::Char('R') if !self.vs_ai_turn() => self.reset(),
            KeyCode::Char('m') => {
                self.reset();
                self.setup_open = true;
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.setup_open {
                    for (rect, index) in &self.menu_rects {
                        if game_core::geom::in_rect(*rect, mouse.column, mouse.row) {
                            self.menu_index = *index;
                            self.confirm_menu();
                            break;
                        }
                    }
                    return;
                }
                let ai_busy = self.ai_turn();
                for (rect, button) in &self.buttons {
                    if game_core::geom::in_rect(*rect, mouse.column, mouse.row) {
                        if ai_busy {
                            return;
                        }
                        match button {
                            Button::Roll => {
                                self.selected = None;
                                self.state.roll();
                            }
                            Button::Restart => self.reset(),
                        }
                        return;
                    }
                }
                // Automated play owns the board: clicks do nothing at all.
                if !ai_busy
                    && let Some(slot) = self
                        .geom
                        .as_ref()
                        .and_then(|g| g.slot_at(mouse.column, mouse.row))
                {
                    self.cursor = slot.index();
                    self.activate();
                }
            }
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                self.hover = Some((mouse.column, mouse.row));
            }
            _ => {}
        }
    }

    /// Advances automated play one action per frame: roll when due, then apply
    /// a single `ai::step` move; leftover dice wait for the next tick. Duels
    /// drive both seats; vs-AI only acts on its own side.
    fn tick(&mut self) {
        if self.setup_open || self.state.phase() == Phase::GameOver {
            return;
        }
        let difficulty = match self.opponent {
            Opponent::Ai(d) | Opponent::Battle(d) => d,
            Opponent::Human => return,
        };
        let automated = match self.opponent {
            Opponent::Battle(_) => true,
            Opponent::Ai(_) => self.state.turn() == self.ai_side,
            Opponent::Human => false,
        };
        if !automated {
            return;
        }
        match self.state.phase() {
            Phase::Roll => {
                self.state.roll();
            }
            Phase::Move => {
                if let Some((src, dest)) = ai::step(&self.state, difficulty, &mut self.ai_rng) {
                    let _ = self.state.move_checker(src, dest);
                }
                // None: the engine's auto-pass/end_turn logic hands over.
            }
            Phase::GameOver => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        self.buttons.clear();
        self.menu_rects.clear();
        if self.setup_open {
            self.geom = None;
            ui::draw_setup(
                frame,
                area,
                self.menu_index,
                self.hover,
                &mut self.menu_rects,
            );
            return;
        }
        let (mode_line, thinking) = self.panel_mode();
        let view = View {
            cursor: self.cursor,
            selected: self.selected,
            hover: self.hover,
        };
        ui::draw(
            frame,
            &self.state,
            area,
            view,
            ui::ModeInfo {
                label: &mode_line,
                ai_side: self.glyph_side(),
                ai_thinking: thinking,
            },
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
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
    use ratatui::backend::TestBackend;

    /// A game past the setup menu with two human players.
    fn started_two_player() -> BackgammonGame {
        let mut g = BackgammonGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Enter)); // TWO PLAYERS
        g
    }

    #[test]
    fn keyboard_wraps_horizontally_and_clamps_vertically() {
        let mut g = started_two_player();
        g.cursor = 0;
        g.handle_key(KeyEvent::from(KeyCode::Left));
        assert_eq!(g.cursor, OFF);
        g.handle_key(KeyEvent::from(KeyCode::Right));
        assert_eq!(g.cursor, 0);
        g.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(g.cursor, 0, "clamped at bottom");
        g.cursor = 20;
        g.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(g.cursor, OFF);
        g.cursor = 10;
        g.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(g.cursor, 0, "-12 clamps to 0");
    }

    #[test]
    fn rolls_only_in_roll_phase_and_shift_r_restarts() {
        let mut g = started_two_player();
        assert_eq!(g.state.phase(), Phase::Roll);
        g.handle_key(KeyEvent::from(KeyCode::Char('r')));
        assert_eq!(g.state.phase(), Phase::Move);
        let dice = g.state.dice();
        g.handle_key(KeyEvent::from(KeyCode::Char('r'))); // no-op mid-move
        assert_eq!(g.state.dice(), dice);
        g.handle_key(KeyEvent::from(KeyCode::Char('R')));
        assert_eq!(g.state.phase(), Phase::Roll);
    }

    #[test]
    fn esc_deselects_but_does_not_quit_and_q_quits() {
        let mut g = started_two_player();
        g.state.roll_with(3, 1);
        g.cursor = 23;
        g.activate(); // select black checker on 23
        assert_eq!(g.selected, Some(23));
        g.handle_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(g.selected, None);
        assert!(!g.wants_quit());
        g.handle_key(KeyEvent::from(KeyCode::Char('q')));
        assert!(g.wants_quit());
    }

    #[test]
    fn human_enter_does_nothing_during_setup() {
        let mut g = BackgammonGame::new();
        assert!(g.setup_open);
        // Board-facing actions stay inert while the menu owns the screen.
        for key in [KeyCode::Char('r'), KeyCode::Char('R')] {
            g.handle_key(KeyEvent::from(key));
        }
        assert!(g.setup_open);
        assert_eq!(g.state.phase(), Phase::Roll);
        // Enter only drives the menu; it never rolls or moves checkers.
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(!g.setup_open, "Enter confirms the highlighted row");
        assert_eq!(g.opponent, Opponent::Human);
        assert_eq!(g.state.phase(), Phase::Roll, "Enter must not roll for you");
        assert_eq!(g.state.turn(), Side::Black);
    }

    #[test]
    fn setup_menu_blocks_input_and_selection_starts_game() {
        let mut g = BackgammonGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Down)); // VS AI - EASY
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(!g.setup_open);
        assert_eq!(g.opponent, Opponent::Ai(Difficulty::Easy));
        assert_eq!(g.ai_side, Side::Black);
        assert_eq!(g.state.phase(), Phase::Roll, "black (the AI) opens");

        let mut saw_ai_move = false;
        let mut saw_white_turn = false;
        for _ in 0..600 {
            if g.state.phase() == Phase::Move && g.state.turn() == Side::Black {
                saw_ai_move = true;
                assert!(!g.state.left().is_empty(), "AI move phase has dice left");
                // Humans must not be able to play the AI's dice.
                let before = g.state.left().to_vec();
                g.handle_key(KeyEvent::from(KeyCode::Enter));
                g.handle_key(KeyEvent::from(KeyCode::Char(' ')));
                g.handle_key(KeyEvent::from(KeyCode::Char('r')));
                assert_eq!(
                    g.state.left().to_vec(),
                    before,
                    "input ignored on AI's turn"
                );
            }
            if g.state.turn() == Side::White {
                saw_white_turn = true;
            }
            g.tick();
            if g.state.phase() == Phase::GameOver {
                break;
            }
        }
        assert!(saw_ai_move, "AI rolled itself into a move phase");
        assert!(
            saw_white_turn,
            "dice were rolled automatically and play alternated"
        );
        assert!(
            (1..=6).contains(&g.state.dice()[0]) && (1..=6).contains(&g.state.dice()[1]),
            "dice look like real rolls"
        );
    }

    #[test]
    fn restart_keeps_settings_while_m_reopens_setup() {
        let mut g = BackgammonGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Char('3'))); // digit select: MEDIUM
        g.handle_key(KeyEvent::from(KeyCode::Char(' '))); // confirm
        assert_eq!(g.opponent, Opponent::Ai(Difficulty::Medium));
        for _ in 0..50 {
            g.tick(); // AI plays until the human's seat comes up
            if g.state.phase() == Phase::Roll && g.state.turn() == Side::White {
                break;
            }
        }
        assert_eq!(g.state.turn(), Side::White, "AI finished its opening");

        g.handle_key(KeyEvent::from(KeyCode::Char('R')));
        assert!(!g.setup_open);
        assert_eq!(g.opponent, Opponent::Ai(Difficulty::Medium));
        assert_eq!(g.state.phase(), Phase::Roll, "same settings, fresh board");

        g.handle_key(KeyEvent::from(KeyCode::Char('m')));
        assert!(g.setup_open, "m reopens setup over a fresh game");
        assert_eq!(g.state.phase(), Phase::Roll);
        assert_eq!(g.state.points()[0], 2, "fresh start layout");
    }

    /// A running duel at the given digit ('6'..'9' pick the difficulty).
    fn started_duel(digit: char) -> BackgammonGame {
        let mut g = BackgammonGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Char(digit)));
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g
    }

    #[test]
    fn duel_menu_mapping() {
        let g = started_duel('8'); // AI DUEL - HARD
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Hard));
        assert!(!g.setup_open, "confirming a duel closes setup");
        assert_eq!(g.state.phase(), Phase::Roll);
        assert_eq!(g.state.turn(), Side::Black, "black opens the duel");

        let expert = started_duel('9');
        assert_eq!(expert.opponent, Opponent::Battle(Difficulty::Expert));

        let mut g = BackgammonGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Down));
        g.handle_key(KeyEvent::from(KeyCode::Down));
        g.handle_key(KeyEvent::from(KeyCode::Down));
        g.handle_key(KeyEvent::from(KeyCode::Down));
        g.handle_key(KeyEvent::from(KeyCode::Down)); // AI DUEL - EASY
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));
    }

    #[test]
    fn duel_self_play_runs_toward_completion() {
        let mut g = started_duel('6');
        let start_off = g.state.off();
        let mut progressed = false;
        for _ in 0..5000 {
            if g.state.phase() == Phase::Move {
                for d in g.state.dice() {
                    assert!((1..=6).contains(&d), "die {d} out of range");
                }
                for d in g.state.left() {
                    assert!((1..=6).contains(d), "left die {d} out of range");
                }
            }
            g.tick();
            if g.state.off().0 + g.state.off().1 > start_off.0 + start_off.1
                || g.state.phase() == Phase::GameOver
            {
                progressed = true;
            }
            if g.state.phase() == Phase::GameOver {
                break;
            }
        }
        assert!(
            progressed,
            "duel must bear checkers off or finish (off: {:?})",
            g.state.off()
        );
    }

    #[test]
    fn duel_input_inert_except_restart_key() {
        let mut g = started_duel('6');
        for _ in 0..50 {
            g.tick(); // reach black's first move phase
            if g.state.phase() == Phase::Move {
                break;
            }
        }
        assert_eq!(g.state.phase(), Phase::Move);

        // Render once so board clicks land on real geometry.
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        let target = g
            .geom
            .as_ref()
            .expect("geom captured at 80x24")
            .rects()
            .iter()
            .find(|(_, s)| *s == ui::Slot::Point(12))
            .copied()
            .expect("point 12 hit region");
        let click = |g: &mut BackgammonGame| {
            g.handle_mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: target.0.x + target.0.width / 2,
                row: target.0.y + target.0.height / 2,
                modifiers: KeyModifiers::empty(),
            });
        };

        let snapshot = (
            g.state.left().to_vec(),
            g.state.dice(),
            *g.state.points(),
            g.state.bar(),
            g.state.off(),
        );
        let cursor_before = g.cursor;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g.handle_key(KeyEvent::from(KeyCode::Char(' ')));
        g.handle_key(KeyEvent::from(KeyCode::Char('r')));
        click(&mut g);
        assert_eq!(g.state.left().to_vec(), snapshot.0, "dice left untouched");
        assert_eq!(g.state.dice(), snapshot.1);
        assert_eq!(*g.state.points(), snapshot.2, "board untouched");
        assert_eq!(g.state.bar(), snapshot.3);
        assert_eq!(g.state.off(), snapshot.4);
        assert_eq!(g.cursor, cursor_before, "clicks must not even steer");

        // 'R' restarts the duel on a fresh board, same settings.
        g.handle_key(KeyEvent::from(KeyCode::Char('R')));
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));
        assert_eq!(g.state.phase(), Phase::Roll);
        assert_eq!(g.state.points()[0], 2, "fresh start layout after R");

        // Lowercase 'r' is not a roll shortcut mid-duel.
        g.handle_key(KeyEvent::from(KeyCode::Char('r')));
        assert_eq!(g.state.phase(), Phase::Roll, "'r' stays inert in duels");
    }
}
