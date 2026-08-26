//! Connect Four for smartasbrain — gravity drops, four-in-a-row wins, and
//! the house shell pattern: setup picker, AI seats, duels, full mouse support.

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use game_core::{Difficulty, Game};
use ratatui::{Frame, layout::Rect};

use engine::Side;

/// Who sits across the grid: a second human or the built-in AI.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opponent {
    Human,
    Ai(Difficulty),
    Battle(Difficulty),
}

pub struct Connect4Game {
    engine: engine::Connect4,
    /// Column the keyboard cursor highlights (0..COLS).
    cursor_col: usize,
    hover: Option<(u16, u16)>,
    geom: Option<ui::Geom>,
    buttons: Vec<(Rect, ui::Button)>,
    quit: bool,
    opponent: Opponent,
    /// Setup menu visibility; starts open so every session picks a mode.
    setup_open: bool,
    /// Highlighted row in the setup menu.
    menu_index: usize,
    /// Screen rects of setup rows, rebuilt each draw for mouse handling.
    menu_rects: Vec<(Rect, usize)>,
    /// Seat the AI plays in vs-AI mode (white, so the human opens).
    ai_side: Side,
}

impl Connect4Game {
    pub fn new() -> Self {
        Self {
            engine: engine::Connect4::new(),
            cursor_col: 3,
            hover: None,
            geom: None,
            buttons: Vec::new(),
            quit: false,
            opponent: Opponent::Human,
            setup_open: true,
            menu_index: 0,
            menu_rects: Vec::new(),
            ai_side: Side::White,
        }
    }

    fn reset(&mut self) {
        self.engine = engine::Connect4::new();
        self.cursor_col = 3;
        self.quit = false;
    }

    fn open_setup(&mut self) {
        self.reset();
        self.setup_open = true;
    }

    fn confirm_setup(&mut self, index: usize) {
        let all = Difficulty::ALL;
        self.opponent = match index {
            0 => Opponent::Human,
            1..=3 => Opponent::Ai(all[index - 1]),
            _ => Opponent::Battle(all[(index - 4).min(2)]),
        };
        self.ai_side = Side::White;
        self.reset();
        self.setup_open = false;
    }

    /// True while an automated seat owns the current position.
    fn ai_to_move(&self) -> bool {
        if self.setup_open || !self.engine.status().is_ongoing() {
            return false;
        }
        match self.opponent {
            Opponent::Human => false,
            Opponent::Ai(_) => self.engine.turn() == self.ai_side,
            Opponent::Battle(_) => true,
        }
    }

    fn mode_label(&self) -> String {
        match self.opponent {
            Opponent::Human => "two players".to_string(),
            Opponent::Ai(d) => format!(
                "vs ai {} ({})",
                d.label(),
                if self.ai_side == Side::White {
                    "white"
                } else {
                    "black"
                }
            ),
            Opponent::Battle(d) => format!("ai duel {}", d.label()),
        }
    }

    #[cfg(test)]
    pub(crate) fn board_origin(&self) -> Option<(u16, u16)> {
        self.geom.map(|g| (g.origin(), g.row0()))
    }

    fn move_cursor(&mut self, delta: isize) {
        let next = self.cursor_col as isize + delta;
        self.cursor_col = next.clamp(0, (engine::COLS - 1) as isize) as usize;
    }
}

impl Default for Connect4Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for Connect4Game {
    fn id(&self) -> &'static str {
        "connect4"
    }

    fn title(&self) -> &'static str {
        "connect four"
    }

    fn handle_key(&mut self, key: KeyEvent) {
        use KeyCode::*;
        if key.code == Char('q') {
            self.quit = true;
            return;
        }
        if self.setup_open {
            match key.code {
                Esc => self.quit = true,
                Up | Char('k') => {
                    self.menu_index =
                        (self.menu_index + ui::SETUP_ITEMS.len() - 1) % ui::SETUP_ITEMS.len()
                }
                Down | Char('j') => self.menu_index = (self.menu_index + 1) % ui::SETUP_ITEMS.len(),
                Char(d @ '1'..='7') => self.menu_index = (d as u8 - b'1') as usize,
                Enter | Char(' ') => self.confirm_setup(self.menu_index),
                _ => {}
            }
            return;
        }
        if matches!(self.opponent, Opponent::Battle(_)) {
            // Spectate-only: the duel answers to nobody but its keeper.
            match key.code {
                Char('R') => self.reset(),
                Char('m') => self.open_setup(),
                _ => {}
            }
            return;
        }
        match key.code {
            Left | Char('h') => self.move_cursor(-1),
            Right | Char('l') => self.move_cursor(1),
            Down | Enter | Char(' ') => {
                if !self.ai_to_move() {
                    let col = self.cursor_col;
                    if self.engine.drop(col) {
                        self.cursor_col = col;
                    }
                }
            }
            Esc => {}
            Char('R') => self.reset(),
            Char('m') => self.open_setup(),
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                self.hover = Some((mouse.column, mouse.row));
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.setup_open {
                    for (rect, index) in &self.menu_rects {
                        if game_core::geom::in_rect(*rect, mouse.column, mouse.row) {
                            self.menu_index = *index;
                            self.confirm_setup(*index);
                            break;
                        }
                    }
                    return;
                }
                if matches!(self.opponent, Opponent::Battle(_)) {
                    return;
                }
                for (rect, button) in &self.buttons {
                    if game_core::geom::in_rect(*rect, mouse.column, mouse.row)
                        && *button == ui::Button::Restart
                    {
                        self.reset();
                        return;
                    }
                }
                if self.ai_to_move() {
                    return;
                }
                if let Some(col) = self
                    .geom
                    .as_ref()
                    .and_then(|g| g.col_at(mouse.column, mouse.row))
                {
                    self.cursor_col = col;
                    self.engine.drop(col);
                }
            }
            _ => {}
        }
    }

    /// Plays one automated move per frame when an AI seat is up.
    fn tick(&mut self) {
        if self.setup_open || !self.engine.status().is_ongoing() {
            return;
        }
        let difficulty = match self.opponent {
            Opponent::Ai(d) | Opponent::Battle(d) => d,
            Opponent::Human => return,
        };
        if !self.ai_to_move() {
            return;
        }
        if let Some(col) = ai::best_move(&self.engine, difficulty) {
            self.engine.drop(col);
        }
    }

    fn poll_navigation(&mut self) -> Option<usize> {
        None
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
        let view = ui::View {
            cursor_col: self.cursor_col,
            hover: self.hover,
        };
        let label = self.mode_label();
        let ai_side = match self.opponent {
            Opponent::Battle(_) => self.engine.turn(),
            _ => self.ai_side,
        };
        let mode = ui::ModeInfo {
            label: &label,
            ai_side,
            thinking: self.ai_to_move(),
        };
        ui::draw(
            frame,
            &self.engine,
            area,
            view,
            mode,
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
    use crossterm::event::KeyModifiers;
    use ratatui::backend::TestBackend;

    /// A game past the setup menu with two human players.
    fn started_two_player() -> Connect4Game {
        let mut g = Connect4Game::new();
        g.handle_key(KeyEvent::from(KeyCode::Enter)); // TWO PLAYERS
        g
    }

    fn key(g: &mut Connect4Game, code: KeyCode) {
        g.handle_key(KeyEvent::from(code));
    }

    #[test]
    fn keyboard_drops_alternate_turns() {
        let mut g = started_two_player();
        assert!(!g.setup_open);
        key(&mut g, KeyCode::Left); // cursor col 2
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.engine.cell(0, 2), Some(Side::Black));
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.engine.cell(1, 2), Some(Side::White), "discs stack");
        assert_eq!(g.engine.col_height(2), 2);
    }

    #[test]
    fn selecting_vs_ai_then_ticks_makes_ai_reply() {
        let mut g = Connect4Game::new();
        key(&mut g, KeyCode::Down); // VS AI - EASY
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.opponent, Opponent::Ai(Difficulty::Easy));
        assert_eq!(g.ai_side, Side::White);
        // Human (black) opens at the cursor column.
        let before = total_discs(&g);
        key(&mut g, KeyCode::Enter);
        assert_eq!(total_discs(&g), before + 1);
        for _ in 0..100 {
            g.tick();
            if total_discs(&g) >= 2 {
                break;
            }
        }
        assert!(total_discs(&g) >= 2, "the AI must answer within 100 ticks");
    }

    #[test]
    fn duel_menu_rows_selectable_and_self_play_progresses() {
        let mut g = Connect4Game::new();
        key(&mut g, KeyCode::Char('5')); // AI DUEL - EASY (debug-friendly)
        key(&mut g, KeyCode::Enter);
        assert!(!g.setup_open);
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));

        let mut drops = 0;
        let mut last = total_discs(&g);
        for _ in 0..400 {
            g.tick();
            let now = total_discs(&g);
            if now > last {
                drops += 1;
                last = now;
            }
            if !g.engine.status().is_ongoing() {
                break;
            }
        }
        assert!(drops >= 2, "duel must place discs early");
    }

    #[test]
    fn duel_input_is_inert_except_keeper_keys() {
        let mut g = Connect4Game::new();
        key(&mut g, KeyCode::Char('5')); // AI DUEL - EASY
        key(&mut g, KeyCode::Enter);
        for _ in 0..10 {
            g.tick();
        }
        let discs_before = total_discs(&g);
        let turn_before = g.engine.turn();
        for code in [KeyCode::Enter, KeyCode::Char(' '), KeyCode::Char('r')] {
            key(&mut g, code);
        }
        assert_eq!(total_discs(&g), discs_before, "board frozen to input");
        assert_eq!(g.engine.turn(), turn_before);

        key(&mut g, KeyCode::Char('R'));
        assert_eq!(total_discs(&g), 0, "R restarts the duel");
        assert!(matches!(g.opponent, Opponent::Battle(_)));
        key(&mut g, KeyCode::Char('m'));
        assert!(g.setup_open, "m reopens setup");
    }

    #[test]
    fn mouse_click_on_board_drops_a_disc() {
        let mut g = started_two_player();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        let Some((x0, y0)) = g.board_origin() else {
            panic!("board drawn at 80x24");
        };
        let stride = g.geom.as_ref().expect("geom").stride();
        let target_col = 2_usize;
        g.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x0 + stride * target_col as u16,
            row: y0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(g.engine.col_height(target_col), 1);
        assert_eq!(g.engine.cell(0, 2), Some(Side::Black));
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(80u16, 24u16), (40, 14), (20, 8), (18, 7), (1, 1), (0, 0)] {
            let mut g = Connect4Game::new();
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        }
    }

    fn total_discs(g: &Connect4Game) -> usize {
        (0..engine::COLS).map(|c| g.engine.col_height(c)).sum()
    }
}
