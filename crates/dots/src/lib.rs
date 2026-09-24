//! Dots & Boxes for smartasbrain — chain-claiming with the house shell
//! pattern: setup picker, AI seats, duels, and full mouse support.

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

pub struct DotsGame {
    engine: engine::Dots,
    /// Edge id under the keyboard cursor; None until the first cycle.
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

impl DotsGame {
    pub fn new() -> Self {
        Self {
            engine: engine::Dots::new(),
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
        self.engine = engine::Dots::new();
        self.cursor = Some(0);
        self.quit = false;
    }

    fn open_setup(&mut self) {
        self.reset();
        self.setup_open = true;
    }

    fn confirm_setup(&mut self, index: usize) {
        let count = Difficulty::ALL.len();
        self.opponent = match index {
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

    /// Steps the cursor through open edges in list order.
    fn cycle_cursor(&mut self, forward: bool) {
        let open = self.engine.open_edges();
        if open.is_empty() {
            self.cursor = None;
            return;
        }
        let next = match (self.cursor, forward) {
            (Some(c), true) => open.iter().position(|&e| e > c).unwrap_or(0),
            (Some(c), false) => open
                .iter()
                .rev()
                .position(|&e| e < c)
                .map(|p| open.len() - 1 - p)
                .unwrap_or(open.len() - 1),
            (None, _) => 0,
        };
        self.cursor = open.get(next).copied().or(Some(open[0]));
    }

    fn claim_at_cursor(&mut self) {
        if self.ai_to_move() {
            return;
        }
        if let Some(e) = self.cursor
            && self.engine.claimed(e).is_none()
        {
            let _ = self.engine.play(e);
        }
    }
}

impl Default for DotsGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for DotsGame {
    fn id(&self) -> &'static str {
        "dots"
    }

    fn title(&self) -> &'static str {
        "dots"
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
                Char(c)
                    if c.to_digit(10)
                        .is_some_and(|d| (1..=ui::SETUP_ITEMS.len() as u32).contains(&d)) =>
                {
                    self.menu_index = c.to_digit(10).unwrap() as usize - 1;
                }
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
            Left | Up | Char('h') | Char('k') if !busy => self.cycle_cursor(false),
            Right | Down | Char('l') | Char('j') if !busy => self.cycle_cursor(true),
            Enter | Char(' ') if !busy => self.claim_at_cursor(),
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
                if let Some(e) = self
                    .geom
                    .as_ref()
                    .and_then(|g| g.edge_at(mouse.column, mouse.row))
                {
                    self.cursor = Some(e);
                    self.claim_at_cursor();
                }
            }
            _ => {}
        }
    }

    /// One automated claim per frame when an AI seat is up.
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
        if let Some(e) = ai::best_edge(&self.engine, difficulty) {
            let _ = self.engine.play(e);
            self.cursor = Some(e);
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

    fn started_two_player() -> DotsGame {
        let mut g = DotsGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g
    }

    fn key(g: &mut DotsGame, code: KeyCode) {
        g.handle_key(KeyEvent::from(code));
    }

    #[test]
    fn enter_claims_edges_and_alternates_turns() {
        let mut g = started_two_player();
        key(&mut g, KeyCode::Enter); // claims edge 0 (White)
        assert_eq!(g.engine.turn(), Side::Black);
        assert_eq!(g.engine.claimed(0), Some(Side::White));
        key(&mut g, KeyCode::Right);
        key(&mut g, KeyCode::Enter); // claims edge 1 (Black)
        assert_eq!(g.engine.turn(), Side::White);
        assert_eq!(g.engine.claimed(1), Some(Side::Black));
    }

    #[test]
    fn selecting_vs_ai_then_ticks_makes_ai_reply() {
        let mut g = DotsGame::new();
        key(&mut g, KeyCode::Down);
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.opponent, Opponent::Ai(Difficulty::Easy));
        // Human (white) claims the cursor's edge to hand the seat over.
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.engine.turn(), Side::Black);
        let claimed_before = count_claimed(&g);
        for _ in 0..100 {
            g.tick();
            if count_claimed(&g) > claimed_before {
                break;
            }
        }
        assert!(
            count_claimed(&g) > claimed_before,
            "AI must answer within 100 ticks"
        );
    }

    #[test]
    fn duel_menu_mapping_and_self_play_completes() {
        let mut g = DotsGame::new();
        key(&mut g, KeyCode::Char('6')); // AI DUEL - EASY
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));

        let mut expert = DotsGame::new();
        key(&mut expert, KeyCode::Char('9'));
        key(&mut expert, KeyCode::Enter);
        assert_eq!(expert.opponent, Opponent::Battle(Difficulty::Expert));

        for _ in 0..2000 {
            g.tick();
            if !g.engine.status().is_ongoing() {
                break;
            }
        }
        assert!(
            !g.engine.status().is_ongoing(),
            "duel must finish within 2000 ticks"
        );
    }

    #[test]
    fn duel_input_is_inert_except_keeper_keys() {
        let mut g = DotsGame::new();
        key(&mut g, KeyCode::Char('6'));
        key(&mut g, KeyCode::Enter);
        for _ in 0..10 {
            g.tick();
        }
        let snapshot = count_claimed(&g);
        let turn_before = g.engine.turn();
        for code in [KeyCode::Enter, KeyCode::Char(' '), KeyCode::Char('r')] {
            key(&mut g, code);
        }
        assert_eq!(count_claimed(&g), snapshot, "board frozen to input");
        assert_eq!(g.engine.turn(), turn_before);
        key(&mut g, KeyCode::Char('R'));
        assert_eq!(count_claimed(&g), 0, "R restarts fresh");
        key(&mut g, KeyCode::Char('m'));
        assert!(g.setup_open);
    }

    #[test]
    fn mouse_click_claims_an_edge_via_hit_test() {
        let mut g = started_two_player();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        let (x0, y0) = g.board_origin();
        // First horizontal segment sits at (x0+1, y0).
        g.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x0 + 2,
            row: y0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(g.engine.claimed(engine::h_id(0, 0)), Some(Side::White));
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(80u16, 24u16), (40, 14), (26, 12), (20, 6), (1, 1), (0, 0)] {
            let mut g = DotsGame::new();
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        }
    }

    fn count_claimed(g: &DotsGame) -> usize {
        (0..engine::EDGES)
            .filter(|&e| g.engine.claimed(e).is_some())
            .count()
    }
}
