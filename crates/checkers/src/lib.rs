//! Checkers (English draughts) — mandatory-capture rules engine, search AI
//! and TUI for smartasbrain.

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use game_core::Difficulty;
use game_core::Game;
use game_core::geom::in_rect;
use ratatui::{Frame, layout::Rect};

pub use engine::{Checkers, PC, SIZE, Side, Status};

/// Cursor start square: a central dark square.
const START_CURSOR: usize = 3 * SIZE + 4;

/// The two seats around the board: two humans, one human vs the machine,
/// or a spectated machine-vs-machine duel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Opponent {
    Human,
    Ai(Difficulty),
    /// Both seats automated; input is inert while the duel runs.
    Battle(Difficulty),
}

pub struct CheckersGame {
    state: Checkers,
    cursor: usize,
    selected: Option<usize>,
    hover: Option<(u16, u16)>,
    /// Screen geometry captured during draw so clicks map to squares.
    geom: Option<ui::Geom>,
    buttons: Vec<(Rect, Button)>,
    quit: bool,
    opponent: Opponent,
    /// While open, the setup overlay replaces the board and swallows input.
    setup_open: bool,
    /// Colour commanded by the AI; humans play White and move first.
    ai_side: Side,
    last_ai_move: Option<(usize, usize)>,
    /// Highlighted entry while the setup menu is open.
    menu_index: usize,
    /// Setup item hitboxes, rebuilt on every draw of the overlay.
    menu_rects: Vec<(Rect, usize)>,
}

#[derive(Clone, Copy)]
enum Button {
    Restart,
}

impl CheckersGame {
    pub fn new() -> Self {
        Self {
            state: Checkers::new(),
            cursor: START_CURSOR,
            selected: None,
            hover: None,
            geom: None,
            buttons: Vec::new(),
            quit: false,
            opponent: Opponent::Human,
            setup_open: true,
            ai_side: Side::Black,
            last_ai_move: None,
            menu_index: 0,
            menu_rects: Vec::new(),
        }
    }

    /// Fresh board for the current opponent settings.
    fn reset(&mut self) {
        self.state = Checkers::new();
        self.cursor = START_CURSOR;
        self.selected = None;
        self.last_ai_move = None;
        self.quit = false;
    }

    fn move_cursor(&mut self, dr: isize, dc: isize) {
        let r = (self.cursor / SIZE) as isize + dr;
        let c = (self.cursor % SIZE) as isize + dc;
        if (0..SIZE as isize).contains(&r) && (0..SIZE as isize).contains(&c) {
            self.cursor = (r * SIZE as isize + c) as usize;
        }
    }

    fn menu_up(&mut self) {
        let n = ui::SETUP_ITEMS.len();
        self.menu_index = (self.menu_index + n - 1) % n;
    }

    fn menu_down(&mut self) {
        self.menu_index = (self.menu_index + 1) % ui::SETUP_ITEMS.len();
    }

    /// Apply a setup entry and start a fresh game under it.
    fn confirm_setup(&mut self, index: usize) {
        self.opponent = match index {
            0 => Opponent::Human,
            1..=3 => Opponent::Ai(Difficulty::ALL[index - 1]),
            _ => Opponent::Battle(Difficulty::ALL[index - 4]),
        };
        self.ai_side = Side::Black;
        self.setup_open = false;
        self.reset();
    }

    /// True while an automated seat is on turn in a running game. During a
    /// duel both seats qualify, so this stays true until the game ends.
    fn ai_to_move(&self) -> bool {
        if self.state.status() != Status::Ongoing {
            return false;
        }
        match self.opponent {
            Opponent::Human => false,
            Opponent::Ai(_) => self.state.turn() == self.ai_side,
            Opponent::Battle(_) => true,
        }
    }

    /// Run one AI move (or chain step) for whichever seat is automated.
    fn run_ai(&mut self) {
        if !self.ai_to_move() {
            return;
        }
        let difficulty = match self.opponent {
            Opponent::Ai(difficulty) | Opponent::Battle(difficulty) => difficulty,
            Opponent::Human => return,
        };
        if let Some((from, to)) = ai::best_move(&self.state, difficulty)
            && self.state.play(from, to)
        {
            self.last_ai_move = Some((from, to));
        }
    }

    /// Enter/space: play the selected piece to the cursor, or select the own
    /// piece under it. During a chain only the chain piece is selectable.
    /// Humans may not act while it is the AI's turn.
    fn activate(&mut self) {
        if self.setup_open || matches!(self.opponent, Opponent::Battle(_)) || self.ai_to_move() {
            return;
        }
        let cur = self.cursor;
        if let Some(from) = self.selected
            && from != cur
            && self.state.play(from, cur)
        {
            self.selected = self.state.chain_from();
            if let Some(next) = self.selected {
                self.cursor = next;
            }
            return;
        }
        let locked_to = self.state.chain_from();
        if locked_to.is_none_or(|from| from == cur)
            && matches!(self.state.piece_at(cur), Some(pc) if pc.side == self.state.turn())
        {
            self.selected = Some(cur);
        }
    }
}

impl Default for CheckersGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for CheckersGame {
    fn id(&self) -> &'static str {
        "checkers"
    }

    fn title(&self) -> &'static str {
        "checkers"
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.setup_open {
            match key.code {
                KeyCode::Char('q' | 'Q') | KeyCode::Esc => self.quit = true,
                KeyCode::Up | KeyCode::Char('k') => self.menu_up(),
                KeyCode::Down | KeyCode::Char('j') => self.menu_down(),
                KeyCode::Enter | KeyCode::Char(' ') => self.confirm_setup(self.menu_index),
                KeyCode::Char(c) => {
                    // Digits confirm their entry directly.
                    if let Some(digit) = c.to_digit(10)
                        && (1..=ui::SETUP_ITEMS.len() as u32).contains(&digit)
                    {
                        self.confirm_setup(digit as usize - 1);
                    }
                }
                _ => {}
            }
            return;
        }
        // Spectating an AI duel: board input is inert, the machine plays
        // both seats. Only quit / setup / capital-R restart respond —
        // lowercase 'r' stays reserved for human seats.
        if matches!(self.opponent, Opponent::Battle(_)) {
            match key.code {
                KeyCode::Char('q' | 'Q') => self.quit = true,
                KeyCode::Char('R') => self.reset(),
                KeyCode::Char('m' | 'M') => {
                    self.reset();
                    self.setup_open = true;
                }
                KeyCode::Esc => self.selected = None,
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('q' | 'Q') => self.quit = true,
            KeyCode::Esc => {
                // Mid-chain the selection is locked; Esc neither deselects
                // nor quits there.
                if self.state.chain_from().is_some() {
                } else if self.selected.is_some() {
                    self.selected = None;
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => self.move_cursor(0, -1),
            KeyCode::Right | KeyCode::Char('l') => self.move_cursor(0, 1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1, 0),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1, 0),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate(),
            KeyCode::Char('r' | 'R') => self.reset(),
            KeyCode::Char('m' | 'M') => {
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
                        if in_rect(*rect, mouse.column, mouse.row) {
                            self.confirm_setup(*index);
                            break;
                        }
                    }
                    return;
                }
                if matches!(self.opponent, Opponent::Battle(_)) {
                    // Clicks are inert while spectating a duel.
                    return;
                }
                for (rect, button) in &self.buttons {
                    if in_rect(*rect, mouse.column, mouse.row) {
                        if matches!(button, Button::Restart) {
                            self.reset();
                        }
                        return;
                    }
                }
                if let Some(square) = self.geom.and_then(|g| g.cell_at(mouse.column, mouse.row)) {
                    self.cursor = square;
                    self.activate();
                }
            }
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                self.hover = Some((mouse.column, mouse.row));
            }
            _ => {}
        }
    }

    fn tick(&mut self) {
        self.run_ai();
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
        let mode = match self.opponent {
            Opponent::Human => "two players".to_string(),
            Opponent::Ai(d) => format!("vs ai {} ({})", d.label(), side_name(self.ai_side)),
            Opponent::Battle(d) => format!("ai duel {}", d.label()),
        };
        ui::draw(
            frame,
            &self.state,
            area,
            ui::View {
                cursor: self.cursor,
                selected: self.selected,
                hover: self.hover,
                last_ai_move: self.last_ai_move,
                thinking: self.ai_to_move(),
                mode,
            },
            &mut self.geom,
            &mut self.buttons,
        );
    }

    fn wants_quit(&self) -> bool {
        self.quit
    }
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::White => "white",
        Side::Black => "black",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::backend::TestBackend;

    fn sq(r: usize, c: usize) -> usize {
        r * SIZE + c
    }

    fn key(game: &mut CheckersGame, code: KeyCode) {
        game.handle_key(KeyEvent::new(code, KeyModifiers::empty()));
    }

    fn draw_once(game: &mut CheckersGame) {
        let mut terminal = ratatui::Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| Game::draw(game, f, f.area())).unwrap();
    }

    fn click(game: &mut CheckersGame, col: u16, row: u16) {
        game.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        });
    }

    /// Piece layout snapshot for byte-identical board comparisons.
    fn board_fingerprint(game: &CheckersGame) -> Vec<Option<(Side, bool)>> {
        (0..SIZE * SIZE)
            .map(|sq| game.state.piece_at(sq).map(|pc| (pc.side, pc.king)))
            .collect()
    }

    #[test]
    fn duel_menu_selection_maps_correctly() {
        for (digit, difficulty) in [
            ('5', Difficulty::Easy),
            ('6', Difficulty::Medium),
            ('7', Difficulty::Hard),
        ] {
            let mut game = CheckersGame::new();
            key(&mut game, KeyCode::Char(digit));
            assert!(!game.setup_open);
            assert_eq!(game.opponent, Opponent::Battle(difficulty));
            assert_eq!(game.state.counts(), (12, 12), "fresh duel board");
        }

        // Navigating onto a duel row and confirming maps the same way.
        let mut game = CheckersGame::new();
        for _ in 0..4 {
            key(&mut game, KeyCode::Down);
        }
        key(&mut game, KeyCode::Enter);
        assert!(!game.setup_open);
        assert_eq!(game.opponent, Opponent::Battle(Difficulty::Easy));
    }

    #[test]
    fn duel_self_play_runs_and_can_end() {
        let mut game = CheckersGame::new();
        key(&mut game, KeyCode::Char('7'));
        assert_eq!(game.opponent, Opponent::Battle(Difficulty::Hard));
        assert_eq!(game.state.counts(), (12, 12));

        // Early ticks mutate the board immediately, White first.
        game.tick();
        assert!(game.last_ai_move.is_some(), "first tick moved for White");
        assert_eq!(game.state.turn(), Side::Black);

        let mut finished = false;
        for _ in 0..2000 {
            game.tick();
            if game.state.status() != Status::Ongoing {
                finished = true;
                break;
            }
        }
        let (white, black) = game.state.counts();
        assert!(
            finished || white + black < 24,
            "duel must capture pieces or reach a result"
        );
        if finished {
            assert!(matches!(game.state.status(), Status::Won(_)));
        }
    }

    #[test]
    fn duel_input_inert() {
        let mut game = CheckersGame::new();
        key(&mut game, KeyCode::Char('6'));
        draw_once(&mut game);
        game.tick();

        let before_board = board_fingerprint(&game);
        let before_counts = game.state.counts();
        let before_turn = game.state.turn();
        let before_last = game.last_ai_move;
        let before_cursor = game.cursor;

        // Select/play, navigation, lowercase restart and esc: all inert.
        key(&mut game, KeyCode::Enter);
        key(&mut game, KeyCode::Char(' '));
        for code in [
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Char('h'),
            KeyCode::Char('j'),
            KeyCode::Char('k'),
            KeyCode::Char('l'),
        ] {
            key(&mut game, code);
        }
        key(&mut game, KeyCode::Char('r'));
        key(&mut game, KeyCode::Esc);
        assert!(!game.wants_quit(), "esc never quits mid-duel");

        // Clicks anywhere — board squares included — change nothing.
        let geom = game.geom.unwrap();
        click(&mut game, geom.x0, geom.y0 + 5);
        click(&mut game, 40, 12);

        assert_eq!(board_fingerprint(&game), before_board);
        assert_eq!(game.selected, None);
        assert_eq!(game.cursor, before_cursor);
        assert_eq!(game.state.counts(), before_counts, "'r' must not reset");
        assert_eq!(game.last_ai_move, before_last, "'r' must not reset");
        assert_eq!(game.state.turn(), before_turn, "only tick advances a duel");

        // The machines carry on regardless.
        game.tick();
        if game.state.status() == Status::Ongoing {
            assert_ne!(game.state.turn(), before_turn, "tick drove the next seat");
        }

        // 'm' reopens setup over a freshly reset duel.
        key(&mut game, KeyCode::Char('m'));
        assert!(game.setup_open);
        assert_eq!(game.state.counts(), (12, 12));

        // 'q' from the board quits.
        let mut game = CheckersGame::new();
        key(&mut game, KeyCode::Char('5'));
        key(&mut game, KeyCode::Char('q'));
        assert!(game.wants_quit());
    }

    #[test]
    fn setup_menu_blocks_board_input() {
        let mut game = CheckersGame::new();
        assert!(game.setup_open);
        draw_once(&mut game);
        assert!(game.geom.is_none(), "overlay replaces the board");

        // Clicking where the board would be must not touch any piece.
        click(&mut game, 10, 5);
        assert_eq!(game.selected, None);
        assert_eq!(game.state.counts(), (12, 12));
        assert_eq!(game.state.turn(), Side::White);

        // Menu keys navigate without touching the game.
        key(&mut game, KeyCode::Down);
        assert_eq!(game.menu_index, 1);

        key(&mut game, KeyCode::Esc);
        assert!(game.wants_quit(), "esc quits from the setup menu");
    }

    #[test]
    fn selecting_vs_ai_then_tick_makes_ai_reply() {
        let mut game = CheckersGame::new();
        key(&mut game, KeyCode::Down);
        key(&mut game, KeyCode::Enter);
        assert!(!game.setup_open);
        assert!(matches!(game.opponent, Opponent::Ai(Difficulty::Easy)));
        assert_eq!(game.ai_side, Side::Black);

        // Human White opens (5,2)->(4,3) via cursor keys.
        key(&mut game, KeyCode::Down);
        key(&mut game, KeyCode::Down);
        key(&mut game, KeyCode::Left);
        key(&mut game, KeyCode::Left);
        key(&mut game, KeyCode::Enter);
        assert_eq!(game.selected, Some(sq(5, 2)));
        key(&mut game, KeyCode::Up);
        key(&mut game, KeyCode::Left);
        key(&mut game, KeyCode::Enter);
        assert_eq!(game.state.turn(), Side::Black);
        assert_eq!(game.last_ai_move, None);

        for _ in 0..200 {
            game.tick();
            if game.last_ai_move.is_some() {
                break;
            }
        }
        assert!(game.last_ai_move.is_some(), "AI replied within 200 ticks");
        assert_eq!(game.state.turn(), Side::White, "back to the human");
    }

    #[test]
    fn human_cannot_move_for_ai() {
        let mut game = CheckersGame::new();
        key(&mut game, KeyCode::Down);
        key(&mut game, KeyCode::Enter);

        // Black king to move with a double jump waiting from (2,3).
        game.state = Checkers::test_position(
            Side::Black,
            &[
                (sq(2, 3), Side::Black, true),
                (sq(3, 4), Side::White, false),
                (sq(3, 6), Side::White, false),
            ],
        );
        game.tick();
        assert_eq!(
            game.state.chain_from(),
            Some(sq(4, 5)),
            "AI opened the chain"
        );
        assert_eq!(game.state.turn(), Side::Black);

        // Park the cursor on the jumping piece and try to hijack the chain.
        key(&mut game, KeyCode::Down);
        key(&mut game, KeyCode::Right);
        assert_eq!(game.cursor, sq(4, 5));
        key(&mut game, KeyCode::Enter);
        key(&mut game, KeyCode::Char(' '));
        assert_eq!(game.selected, None, "AI's piece cannot be selected");
        assert_eq!(game.state.chain_from(), Some(sq(4, 5)));
        assert_eq!(
            game.state.piece_at(sq(4, 5)).map(|pc| pc.side),
            Some(Side::Black)
        );
    }
}
