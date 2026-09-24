//! Nine Men's Morris — rules engine and TUI for smartasbrain.
//!
//! Place nine stones apiece, close mills to knock enemy stones off, then
//! slide (or fly, at three stones) toward the same goal. See [`engine`] for
//! the exact rules model and [`ui::POINT_XY`] for screen geometry.

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyEvent, MouseEvent};
use game_core::{Difficulty, Game};
use ratatui::Frame;
use ratatui::layout::Rect;

pub use engine::{ADJACENCY, Action, MILLS, Morris, POINTS, Phase, Side, Status};
pub use ui::POINT_XY;

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
pub struct MorrisGame {
    state: Morris,
    /// Highlighted point in 0..[`POINTS`]; arrows cycle with wraparound.
    cursor: usize,
    /// Picked-up stone awaiting a landing point during the moving phase.
    selected: Option<usize>,
    hover: Option<(u16, u16)>,
    pub(crate) geom: Option<ui::Geom>,
    buttons: Vec<(Rect, Button)>,
    quit: bool,
    opponent: Opponent,
    /// Setup menu visibility; starts open so every session picks a mode.
    setup_open: bool,
    menu_index: usize,
    menu_rects: Vec<(Rect, usize)>,
    /// Seat the AI plays; black answers the human's white opening.
    ai_side: Side,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Button {
    Restart,
}

/// Cursor starts on a mid-ring junction with room to slide in every phase.
const START_CURSOR: usize = 4;

impl MorrisGame {
    pub fn new() -> Self {
        Self {
            state: Morris::new(),
            cursor: START_CURSOR,
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
        }
    }

    /// Fresh board with identical opponent settings (the `R` restart).
    fn reset(&mut self) {
        self.state = Morris::new();
        self.cursor = START_CURSOR;
        self.selected = None;
        self.quit = false;
    }

    /// Reopen the setup menu over a fresh game.
    fn open_setup(&mut self) {
        self.setup_open = true;
        self.menu_index = 0;
        self.selected = None;
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
    /// inert: the AI's turn in vs-AI (captures included), or any live moment
    /// of a running duel.
    fn ai_turn(&self) -> bool {
        if self.setup_open || self.state.status() != Status::Ongoing {
            return false;
        }
        match self.opponent {
            Opponent::Human => false,
            Opponent::Ai(_) => self.state.turn() == self.ai_side,
            Opponent::Battle(_) => true,
        }
    }

    fn move_cursor(&mut self, delta: isize) {
        let next = (self.cursor as isize + delta).rem_euclid(POINTS as isize);
        self.cursor = next as usize;
    }

    /// Activate whatever the cursor sits on, shared by Enter/space/mouse:
    /// capture when one is owed, otherwise place / pick up / land.
    fn activate(&mut self) {
        if self.ai_turn() {
            return;
        }
        let at = self.cursor;
        if at >= POINTS {
            return;
        }
        if self.state.removal_pending() {
            self.state.remove(at);
            return;
        }
        let own = self.state.point(at) == Some(self.state.turn());
        match self.selected {
            None => {
                if own {
                    if !self.state.legal_moves_from(at).is_empty() {
                        self.selected = Some(at);
                    }
                } else {
                    self.state.place(at);
                }
            }
            Some(src) => {
                if src != at
                    && self.state.legal_moves_from(src).contains(&at)
                    && self.state.move_stone(src, at)
                {
                    self.selected = None;
                    return;
                }
                // Not a landing dot: hop to another own stone or toggle off.
                if at == src {
                    self.selected = None;
                } else if own && !self.state.legal_moves_from(at).is_empty() {
                    self.selected = Some(at);
                }
            }
        }
    }

    /// Panel context: mode description and whether the AI is mid-thought.
    fn panel_mode(&self) -> (String, bool) {
        let thinking = self.ai_turn();
        let mode = match self.opponent {
            Opponent::Human => "two players".to_string(),
            Opponent::Ai(difficulty) => {
                format!("vs ai {} ({})", difficulty.label(), self.ai_side.name())
            }
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

#[cfg(test)]
impl MorrisGame {
    /// Test hook: skip the setup overlay without touching engine state.
    pub(crate) fn dismiss_setup_for_test(&mut self) {
        self.setup_open = false;
    }
}

impl Default for MorrisGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for MorrisGame {
    fn id(&self) -> &'static str {
        "morris"
    }

    fn title(&self) -> &'static str {
        "morris"
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
        // Automated play owns the board (AI turns include their captures):
        // only quit / restart / menu get through.
        if self.ai_turn() {
            match key.code {
                KeyCode::Char('R') => self.reset(),
                KeyCode::Char('m') => self.open_setup(),
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Esc => self.selected = None,
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Up | KeyCode::Char('k') => {
                self.move_cursor(-1);
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Down | KeyCode::Char('j') => {
                self.move_cursor(1);
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
            KeyCode::Char('R') => self.reset(),
            KeyCode::Char('m') => self.open_setup(),
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
                            self.menu_index = *index;
                            self.confirm_menu();
                            break;
                        }
                    }
                    return;
                }
                for (rect, button) in &self.buttons {
                    if game_core::geom::in_rect(*rect, mouse.column, mouse.row) {
                        if !self.ai_turn() && *button == Button::Restart {
                            self.reset();
                        }
                        return;
                    }
                }
                // Automated play owns the board: clicks do nothing at all.
                if !self.ai_turn()
                    && let Some(point) = self
                        .geom
                        .as_ref()
                        .and_then(|g| g.slot_at(mouse.column, mouse.row))
                {
                    self.cursor = point;
                    self.activate();
                }
            }
            _ => {}
        }
    }

    /// Advances automated play one action per frame: a place/move first, then
    /// the owed capture on the following tick. Duels drive both seats; vs-AI
    /// only acts on its own side.
    fn tick(&mut self) {
        if self.setup_open || self.state.status() != Status::Ongoing {
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
        if self.state.removal_pending() {
            if let Some(target) = ai::best_removal(&self.state, difficulty) {
                self.state.remove(target);
            }
        } else if let Some(action) = ai::best_action(&self.state, difficulty) {
            match action {
                Action::Place(i) => {
                    self.state.place(i);
                }
                Action::Move(from, to) => {
                    self.state.move_stone(from, to);
                }
                Action::Remove(_) => {}
            }
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
        let interactive = !self.ai_turn();
        let view = View {
            cursor: self.cursor,
            selected: self.selected,
            hover: self.hover,
            interactive,
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
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;

    /// A game past the setup menu with two human players.
    fn started_two_player() -> MorrisGame {
        let mut g = MorrisGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Enter)); // TWO PLAYERS
        g
    }

    /// A running duel at the given digit ('5'..'7' pick the difficulty).
    fn started_duel(digit: char) -> MorrisGame {
        let mut g = MorrisGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Char(digit)));
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g
    }

    fn stones_on_board(g: &MorrisGame, side: Side) -> usize {
        (0..POINTS)
            .filter(|&i| g.state.point(i) == Some(side))
            .count()
    }

    #[test]
    fn cursor_walks_all_points_with_wraparound() {
        let mut g = started_two_player();
        assert_eq!(g.cursor, START_CURSOR);
        g.cursor = 0;
        g.handle_key(KeyEvent::from(KeyCode::Left));
        assert_eq!(g.cursor, 23, "left wraps past point zero");
        g.handle_key(KeyEvent::from(KeyCode::Right));
        assert_eq!(g.cursor, 0);
        g.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(g.cursor, 23, "up walks the same ring");
        g.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(g.cursor, 0);
        for _ in 0..25 {
            g.handle_key(KeyEvent::from(KeyCode::Char('l')));
        }
        assert_eq!(g.cursor, 1, "24 rights land one past the start");
    }

    #[test]
    fn esc_deselects_without_quitting_and_q_quits() {
        let mut g = started_two_player();
        g.handle_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(g.selected, None);
        assert!(!g.wants_quit(), "esc never quits here");
        g.handle_key(KeyEvent::from(KeyCode::Char('q')));
        assert!(g.wants_quit());
    }

    #[test]
    fn human_places_alternately_via_keyboard() {
        let mut g = started_two_player();
        g.cursor = 4;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(g.state.point(4), Some(Side::White));
        assert_eq!(g.state.turn(), Side::Black);
        g.cursor = 19;
        g.handle_key(KeyEvent::from(KeyCode::Char(' ')));
        assert_eq!(g.state.point(19), Some(Side::Black));
        assert_eq!(stones_on_board(&g, Side::White), 1);
    }

    #[test]
    fn human_mill_leads_to_keyboard_capture() {
        let mut g = started_two_player();
        // White closes 0-1-2 while black sprinkles 23, 22.
        g.cursor = 0;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g.cursor = 23;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g.cursor = 1;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g.cursor = 22;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g.cursor = 2;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(g.state.removal_pending(), "white closed the top rail mill");
        assert_eq!(g.state.turn(), Side::White);

        // Enter on a non-capturable point is refused silently.
        g.cursor = 0;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(g.state.removal_pending(), "own stone is not removable");

        g.cursor = 23;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(!g.state.removal_pending());
        assert_eq!(g.state.point(23), None);
        assert_eq!(g.state.last_action(), Some(Action::Remove(23)));
        assert_eq!(g.state.turn(), Side::Black);
    }

    #[test]
    fn setup_menu_blocks_play_until_confirmed() {
        let mut g = MorrisGame::new();
        assert!(g.setup_open);
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(!g.setup_open);
        assert_eq!(g.opponent, Opponent::Human);
        assert_eq!(g.state.turn(), Side::White);
        assert_eq!(g.state.stones_left(Side::White), 9);
        // Digits jump straight to a row before confirming.
        let mut g = MorrisGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Char('3')));
        g.handle_key(KeyEvent::from(KeyCode::Char(' ')));
        assert_eq!(g.opponent, Opponent::Ai(Difficulty::Medium));
        assert!(!g.setup_open);
    }

    #[test]
    fn restart_keeps_settings_while_m_reopens_setup() {
        let mut g = started_two_player();
        g.cursor = 4;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(g.state.point(4), Some(Side::White));

        g.handle_key(KeyEvent::from(KeyCode::Char('R')));
        assert!(!g.setup_open);
        assert_eq!(g.opponent, Opponent::Human);
        assert_eq!(g.state.point(4), None, "fresh board after R");
        assert_eq!(g.state.stones_left(Side::White), 9);

        g.handle_key(KeyEvent::from(KeyCode::Char('m')));
        assert!(g.setup_open, "m reopens setup over a fresh game");
        g.handle_key(KeyEvent::from(KeyCode::Char('7'))); // AI DUEL - MEDIUM
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Medium));
    }

    #[test]
    fn duel_menu_mapping() {
        let g = started_duel('8'); // AI DUEL - HARD
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Hard));
        assert!(!g.setup_open, "confirming a duel closes setup");
        assert_eq!(g.state.status(), Status::Ongoing);
        assert_eq!(g.state.turn(), Side::White, "white opens every duel");

        let expert = started_duel('9');
        assert_eq!(expert.opponent, Opponent::Battle(Difficulty::Expert));

        let mut g = MorrisGame::new();
        for _ in 0..5 {
            g.handle_key(KeyEvent::from(KeyCode::Down)); // AI DUEL - EASY
        }
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));
    }

    #[test]
    fn duel_self_play_progresses_or_ends() {
        let mut g = started_duel('6');
        let mut early_stones = 0usize;
        let mut ended = false;
        let mut turns_seen = std::collections::HashSet::new();
        for tick in 0..800 {
            g.tick();
            let stones = (0..POINTS).filter(|&i| g.state.point(i).is_some()).count();
            if tick < 60 {
                early_stones = early_stones.max(stones);
            }
            turns_seen.insert(g.state.turn());
            if g.state.status() != Status::Ongoing {
                ended = true;
                break;
            }
        }
        assert!(
            early_stones >= 4 || ended,
            "placements must start immediately (saw {early_stones})"
        );
        let stones = stones_on_board(&g, Side::White) + stones_on_board(&g, Side::Black);
        assert!(
            ended || (4..=18).contains(&stones),
            "board holds {stones} stones without ending"
        );
        assert!(
            ended || turns_seen.len() == 2,
            "both duel seats took turns: {turns_seen:?}"
        );
    }

    #[test]
    fn duel_input_inert_mid_duel() {
        let mut g = started_duel('6');
        for _ in 0..60 {
            g.tick();
            if g.state.removal_pending()
                || stones_on_board(&g, Side::White) + stones_on_board(&g, Side::Black) >= 2
            {
                break;
            }
        }
        assert!(stones_on_board(&g, Side::White) + stones_on_board(&g, Side::Black) >= 2);

        // Render once so board clicks land on real geometry.
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();

        let snapshot: Vec<Option<Side>> = (0..POINTS).map(|i| g.state.point(i)).collect();
        let cursor_before = g.cursor;
        for key in [
            KeyCode::Enter,
            KeyCode::Char(' '),
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Char('h'),
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('l'),
            KeyCode::Char('r'),
            KeyCode::Esc,
        ] {
            g.handle_key(KeyEvent::from(key));
        }
        assert_eq!(
            (0..POINTS).map(|i| g.state.point(i)).collect::<Vec<_>>(),
            snapshot,
            "keys must not touch the board mid-duel"
        );

        let geom = g.geom.expect("geom captured at 80x24");
        let inner = geom.inner();
        // At 80x24 the ladder keeps base scale (x1).
        g.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: inner.x + POINT_XY[12].0,
            row: inner.y + POINT_XY[12].1,
            modifiers: KeyModifiers::empty(),
        });
        assert_eq!(
            (0..POINTS).map(|i| g.state.point(i)).collect::<Vec<_>>(),
            snapshot,
            "clicks must not touch the board mid-duel"
        );
        assert_eq!(g.cursor, cursor_before, "clicks must not even steer");

        // 'R' restarts the duel on a fresh board, same settings.
        g.handle_key(KeyEvent::from(KeyCode::Char('R')));
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));
        assert_eq!(g.state.status(), Status::Ongoing);
        assert_eq!(stones_on_board(&g, Side::White), 0, "fresh start after R");

        // 'm' reopens setup; lowercase 'r' never rolls anything here.
        g.handle_key(KeyEvent::from(KeyCode::Char('r')));
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));
        g.handle_key(KeyEvent::from(KeyCode::Char('m')));
        assert!(g.setup_open);
    }

    #[test]
    fn vs_ai_full_exchange() {
        let mut g = started_duel('2'); // VS AI - EASY plays black
        assert_eq!(g.opponent, Opponent::Ai(Difficulty::Easy));
        assert_eq!(g.state.turn(), Side::White, "the human opens");

        g.cursor = 4;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(g.state.point(4), Some(Side::White));

        // The AI owes a place plus any follow-up capture within 50 ticks.
        for _ in 0..50 {
            if g.state.turn() == Side::White || g.state.status() != Status::Ongoing {
                break;
            }
            g.tick();
        }
        assert_eq!(g.state.turn(), Side::White, "AI replied promptly");
        assert_eq!(g.state.status(), Status::Ongoing);
        assert!(stones_on_board(&g, Side::Black) >= 1, "black answered");
        assert_eq!(stones_on_board(&g, Side::White), 1, "no theft this early");

        // Second exchange: keys pressed while black thinks do nothing.
        g.cursor = 19;
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(stones_on_board(&g, Side::White), 2);
        for _ in 0..50 {
            if g.state.turn() == Side::White || g.state.status() != Status::Ongoing {
                break;
            }
            let pre = stones_on_board(&g, Side::White);
            g.handle_key(KeyEvent::from(KeyCode::Enter));
            g.handle_key(KeyEvent::from(KeyCode::Char(' ')));
            g.handle_key(KeyEvent::from(KeyCode::Char('r')));
            assert_eq!(
                stones_on_board(&g, Side::White),
                pre,
                "input inert while the AI thinks"
            );
            g.tick();
        }
        assert_eq!(g.state.turn(), Side::White, "AI finished its second reply");
    }
}
