//! Mancala for smartasbrain — Kalah sowing with the house shell pattern:
//! setup picker, AI seats, duels, and full mouse support.

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use game_core::{Difficulty, Game};
use ratatui::{Frame, layout::Rect};

use engine::Side;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opponent {
    Human,
    Ai(Difficulty),
    Battle(Difficulty),
}

pub struct MancalaGame {
    engine: engine::Mancala,
    /// Pit the keyboard cursor rests on; normalized to the mover each turn.
    cursor: Option<usize>,
    hover: Option<(u16, u16)>,
    geom: Option<ui::Geom>,
    buttons: Vec<(Rect, ui::Button)>,
    quit: bool,
    opponent: Opponent,
    setup_open: bool,
    menu_index: usize,
    menu_rects: Vec<(Rect, usize)>,
    ai_side: Side,
}

impl MancalaGame {
    pub fn new() -> Self {
        Self {
            engine: engine::Mancala::new(),
            cursor: Some(0),
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

    fn reset(&mut self) {
        self.engine = engine::Mancala::new();
        self.cursor = Some(0);
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
        self.ai_side = Side::Black;
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
    pub(crate) fn board_origin(&self) -> (u16, u16) {
        self.geom.map(|g| g.origin()).unwrap_or((0, 0))
    }

    /// Steps the cursor to the next legal pit in `direction` within the
    /// mover's half of the board.
    fn cycle_cursor(&mut self, forward: bool) {
        let range: Vec<usize> = if self.engine.turn() == Side::White {
            (0..engine::PITS_PER_SIDE).collect()
        } else {
            (7..7 + engine::PITS_PER_SIDE).collect()
        };
        let legal = self.engine.legal_pits();
        if legal.is_empty() {
            self.cursor = None;
            return;
        }
        let start = self
            .cursor
            .and_then(|c| range.iter().position(|&r| r == c))
            .unwrap_or(0);
        for step in 1..=range.len() {
            let probe = if forward {
                (start + step) % range.len()
            } else {
                (start + range.len() - step) % range.len()
            };
            let pit = range[probe];
            if legal.contains(&pit) {
                self.cursor = Some(pit);
                return;
            }
        }
    }

    fn sow_at_cursor(&mut self) {
        if self.ai_to_move() {
            return;
        }
        let Some(pit) = self.cursor else { return };
        if !self.engine.legal_pits().contains(&pit) {
            // Snap to the first legal pit so Enter always does something.
            if let Some(first) = self.engine.legal_pits().first() {
                self.cursor = Some(*first);
                let _ = self.engine.sow(*first);
            }
            return;
        }
        let _ = self.engine.sow(pit);
        self.normalize_cursor();
    }

    fn normalize_cursor(&mut self) {
        if let Some(c) = self.cursor
            && !self.engine.legal_pits().contains(&c)
        {
            self.cursor = self.engine.legal_pits().first().copied();
        }
    }
}

impl Default for MancalaGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for MancalaGame {
    fn id(&self) -> &'static str {
        "mancala"
    }

    fn title(&self) -> &'static str {
        "mancala"
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
            match key.code {
                Char('R') => self.reset(),
                Char('m') => self.open_setup(),
                _ => {}
            }
            return;
        }
        let busy = self.ai_to_move();
        match key.code {
            Left | Char('h') if !busy => self.cycle_cursor(false),
            Right | Char('l') if !busy => self.cycle_cursor(true),
            Enter | Char(' ') if !busy => self.sow_at_cursor(),
            Esc => {}
            Char('R') if !busy => self.reset(),
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
                if let Some(pit) = self
                    .geom
                    .as_ref()
                    .and_then(|g| g.pit_at(mouse.column, mouse.row))
                {
                    self.cursor = Some(pit);
                    self.sow_at_cursor();
                }
            }
            _ => {}
        }
    }

    /// One automated sow per frame when an AI seat is up.
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
        if let Some(pit) = ai::best_move(&self.engine, difficulty) {
            let _ = self.engine.sow(pit);
            self.cursor = Some(pit);
        }
        self.normalize_cursor();
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
            cursor: self.cursor,
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

    fn started_two_player() -> MancalaGame {
        let mut g = MancalaGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g
    }

    fn key(g: &mut MancalaGame, code: KeyCode) {
        g.handle_key(KeyEvent::from(code));
    }

    fn seeds_on_board(g: &MancalaGame) -> u32 {
        (0..engine::STORES).map(|i| g.engine.pit(i) as u32).sum()
    }

    #[test]
    fn enter_sows_and_extra_turn_keeps_mover() {
        let mut g = started_two_player();
        // Cursor starts at pit 2? It starts at 0; sow(0): 4 seeds land
        // 1,2,3,4 — no store, so black is next.
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.engine.turn(), Side::Black);
        assert_eq!(g.engine.pit(0), 0);
    }

    #[test]
    fn selecting_vs_ai_then_ticks_makes_ai_reply() {
        let mut g = MancalaGame::new();
        key(&mut g, KeyCode::Down); // VS AI - EASY
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.opponent, Opponent::Ai(Difficulty::Easy));
        assert_eq!(g.ai_side, Side::Black);
        key(&mut g, KeyCode::Enter); // human white sows cursor pit
        assert_eq!(g.engine.turn(), Side::Black);
        for _ in 0..100 {
            g.tick();
            if g.engine.turn() == Side::White || !g.engine.status().is_ongoing() {
                break;
            }
        }
        assert!(
            g.engine.turn() == Side::White || !g.engine.status().is_ongoing(),
            "AI must answer within 100 ticks"
        );
    }

    #[test]
    fn duel_menu_mapping_and_self_play_progresses() {
        let mut g = MancalaGame::new();
        key(&mut g, KeyCode::Char('5')); // AI DUEL - EASY
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));
        let before = seeds_on_board(&g);
        for _ in 0..300 {
            g.tick();
            if !g.engine.status().is_ongoing() {
                break;
            }
        }
        assert!(
            seeds_on_board(&g) != before || !g.engine.status().is_ongoing(),
            "duel must move seeds early"
        );
    }

    #[test]
    fn duel_input_is_inert_except_keeper_keys() {
        let mut g = MancalaGame::new();
        key(&mut g, KeyCode::Char('5'));
        key(&mut g, KeyCode::Enter);
        for _ in 0..10 {
            g.tick();
        }
        let snapshot = seeds_on_board(&g);
        let turn_before = g.engine.turn();
        for code in [KeyCode::Enter, KeyCode::Char(' '), KeyCode::Char('r')] {
            key(&mut g, code);
        }
        assert_eq!(seeds_on_board(&g), snapshot, "board frozen to input");
        assert_eq!(g.engine.turn(), turn_before);
        key(&mut g, KeyCode::Char('R'));
        assert_eq!(g.engine.pit(0), 4, "R restarts the duel fresh");
        key(&mut g, KeyCode::Char('m'));
        assert!(g.setup_open);
    }

    #[test]
    fn mouse_click_sows_via_board_hit() {
        let mut g = started_two_player();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        let (x0, y0) = g.board_origin();
        g.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x0 + 6, // bottom pit band, slot 0
            row: y0 + 2,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(g.engine.pit(0), 0, "clicked pit was sown");
        assert_eq!(g.engine.turn(), Side::Black);
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(80u16, 24u16), (46, 12), (32, 8), (20, 5), (1, 1), (0, 0)] {
            let mut g = MancalaGame::new();
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        }
    }
}
