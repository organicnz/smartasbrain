//! Dominoes for smartasbrain — draw-variant chain building with the house
//! shell pattern: setup picker, AI seats, duels, and full mouse support.

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use game_core::{Difficulty, Game};
use rand::SeedableRng;
use ratatui::{Frame, layout::Rect};

use engine::Side;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opponent {
    Human,
    Ai(Difficulty),
    Battle(Difficulty),
}

pub struct DominoesGame {
    engine: engine::Dominoes,
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
    rng_seed: u64,
}

impl DominoesGame {
    pub fn new() -> Self {
        Self {
            engine: engine::Dominoes::default(),
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
            rng_seed: 0xD0_0001,
        }
    }

    fn next_rng(&mut self) -> rand::rngs::StdRng {
        self.rng_seed = self
            .rng_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(3);
        rand::rngs::StdRng::seed_from_u64(self.rng_seed)
    }

    fn reset(&mut self) {
        let mut rng = self.next_rng();
        self.engine = engine::Dominoes::new(&mut rng);
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

    fn difficulty(&self) -> Option<Difficulty> {
        match self.opponent {
            Opponent::Ai(d) | Opponent::Battle(d) => Some(d),
            Opponent::Human => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn hand_len(&self, side: Side) -> usize {
        self.engine.hand(side).len()
    }

    fn cycle_cursor(&mut self, forward: bool) {
        let n = self.engine.hand(self.engine.turn()).len();
        if n == 0 {
            self.cursor = None;
            return;
        }
        let cur = self.cursor.unwrap_or(0).min(n - 1);
        self.cursor = Some(if forward {
            (cur + 1) % n
        } else {
            (cur + n - 1) % n
        });
    }
}

impl Default for DominoesGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for DominoesGame {
    fn id(&self) -> &'static str {
        "dominoes"
    }

    fn title(&self) -> &'static str {
        "dominoes"
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
            Left | Char('h') if !busy => self.cycle_cursor(false),
            Right | Char('l') if !busy => self.cycle_cursor(true),
            Enter | Char(' ') if !busy => {
                if let Some(idx) = self.cursor
                    && !self.engine.play(idx)
                {
                    // Illegal pick is a no-op; drawing is the real escape.
                }
            }
            Char('d') if !busy => {
                while self.engine.must_draw() {
                    if !self.engine.draw_one() {
                        break;
                    }
                }
            }
            Char('p') if !busy => {
                let _ = self.engine.pass();
            }
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
                if matches!(self.opponent, Opponent::Battle(_)) || self.ai_to_move() {
                    return;
                }
                for (rect, button) in &self.buttons {
                    if game_core::geom::in_rect(*rect, mouse.column, mouse.row) {
                        match button {
                            ui::Button::Draw => {
                                while self.engine.must_draw() {
                                    if !self.engine.draw_one() {
                                        break;
                                    }
                                }
                            }
                            ui::Button::Pass => {
                                let _ = self.engine.pass();
                            }
                            ui::Button::Restart => self.reset(),
                        }
                        return;
                    }
                }
                if let Some(idx) = self
                    .geom
                    .as_ref()
                    .and_then(|g| g.tile_at(mouse.column, mouse.row))
                {
                    self.cursor = Some(idx);
                    self.engine.play(idx);
                }
            }
            _ => {}
        }
    }

    /// One automated action per frame: forced draws first, then plays or
    /// passes when the yard is dry.
    fn tick(&mut self) {
        if self.setup_open || !self.engine.status().is_ongoing() {
            return;
        }
        let Some(difficulty) = self.difficulty() else {
            return;
        };
        if !self.ai_to_move() {
            return;
        }
        if self.engine.must_draw() {
            let _ = self.engine.draw_one();
            return;
        }
        if self.engine.can_pass() {
            let _ = self.engine.pass();
            return;
        }
        if let Some(idx) = ai::best_move(&self.engine, difficulty) {
            self.cursor = Some(idx);
            let _ = self.engine.play(idx);
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

    fn snapshot(&self) -> Option<String> {
        build_snapshot(self)
    }

    fn restore(&mut self, blob: &str) -> bool {
        apply_restore(self, blob)
    }
}

/// Versioned encoding: hands, boneyard order, laid halves, turn.
fn side_tag(side: Side) -> &'static str {
    match side {
        Side::White => "W",
        Side::Black => "B",
    }
}

fn build_snapshot(g: &DominoesGame) -> Option<String> {
    if g.setup_open {
        return None;
    }
    let tiles = |ts: &[engine::Tile]| -> String {
        ts.iter()
            .map(|t| format!("{}{}", t.a, t.b))
            .collect::<Vec<_>>()
            .join(",")
    };
    let line = g
        .engine
        .line()
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");
    Some(format!(
        "v1|{}|{}|{}|{}|{}",
        side_tag(g.engine.turn()),
        tiles(g.engine.hand(Side::White)),
        tiles(g.engine.hand(Side::Black)),
        tiles(&g.boneyard_snapshot()),
        line,
    ))
}

fn apply_restore(g: &mut DominoesGame, blob: &str) -> bool {
    let parts: Vec<&str> = blob.split('|').collect();
    if parts.len() != 6 || parts[0] != "v1" {
        return false;
    }
    let turn = match parts[1] {
        "W" => Side::White,
        "B" => Side::Black,
        _ => return false,
    };
    let parse_tiles = |s: &str| -> Option<Vec<engine::Tile>> {
        if s.is_empty() {
            return Some(Vec::new());
        }
        s.split(',')
            .map(|t| {
                let b = t.as_bytes();
                if b.len() != 2 || b[0] > b'6' || b[1] > b'6' {
                    return None;
                }
                Some(engine::Tile {
                    a: b[0] - b'0',
                    b: b[1] - b'0',
                })
            })
            .collect()
    };
    let (Some(w), Some(b), Some(yard)) = (
        parse_tiles(parts[2]),
        parse_tiles(parts[3]),
        parse_tiles(parts[4]),
    ) else {
        return false;
    };
    let mut line = Vec::new();
    for tok in parts[5].split(',') {
        if tok.is_empty() {
            continue;
        }
        let Ok(v) = tok.parse::<u8>() else {
            return false;
        };
        if v > 6 {
            return false;
        }
        line.push(v);
    }
    g.engine.restore_state([w, b], yard, line, turn);
    g.setup_open = false;
    true
}

impl DominoesGame {
    pub(crate) fn boneyard_snapshot(&self) -> Vec<engine::Tile> {
        // Engine exposes only the count publicly; snapshot support reaches
        // through a crate-private accessor added for persistence.
        self.engine.boneyard_tiles()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn started_two_player() -> DominoesGame {
        let mut g = DominoesGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g
    }

    fn key(g: &mut DominoesGame, code: KeyCode) {
        g.handle_key(KeyEvent::from(code));
    }

    #[test]
    fn duel_menu_mapping_and_self_play_completes() {
        let mut g = DominoesGame::new();
        key(&mut g, KeyCode::Char('6')); // AI DUEL - EASY
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));

        let mut expert = DominoesGame::new();
        key(&mut expert, KeyCode::Char('9'));
        key(&mut expert, KeyCode::Enter);
        assert_eq!(expert.opponent, Opponent::Battle(Difficulty::Expert));

        for _ in 0..3000 {
            g.tick();
            if !g.engine.status().is_ongoing() {
                break;
            }
        }
        assert!(
            !g.engine.status().is_ongoing(),
            "duel must finish within 3000 ticks"
        );
    }

    #[test]
    fn vs_ai_answers_within_budget() {
        let mut g = DominoesGame::new();
        key(&mut g, KeyCode::Down);
        key(&mut g, KeyCode::Enter); // VS AI - EASY
        // Human plays whatever the cursor holds until it lands.
        // One legal human action hands the seat to black.
        while g.engine.must_draw() {
            assert!(g.engine.draw_one());
        }
        let legal = g.engine.legal_moves();
        if !legal.is_empty() {
            g.cursor = Some(legal[0].0);
            key(&mut g, KeyCode::Enter);
        } else {
            key(&mut g, KeyCode::Char('p'));
        }
        for _ in 0..200 {
            g.tick();
            if g.engine.turn() == Side::White || !g.engine.status().is_ongoing() {
                break;
            }
        }
        assert!(
            g.engine.turn() == Side::White || !g.engine.status().is_ongoing(),
            "AI must take its seat"
        );
    }

    #[test]
    fn duel_input_is_inert_except_keeper_keys() {
        let mut g = DominoesGame::new();
        key(&mut g, KeyCode::Char('6'));
        key(&mut g, KeyCode::Enter);
        for _ in 0..12 {
            g.tick();
        }
        let before = (
            g.hand_len(Side::White),
            g.hand_len(Side::Black),
            g.engine.line().len(),
        );
        for code in [KeyCode::Enter, KeyCode::Char('d'), KeyCode::Char('r')] {
            key(&mut g, code);
        }
        assert_eq!(
            before,
            (
                g.hand_len(Side::White),
                g.hand_len(Side::Black),
                g.engine.line().len()
            ),
            "duel frozen to input"
        );
        key(&mut g, KeyCode::Char('R'));
        assert_eq!(g.engine.line().len(), 0, "R restarts fresh");
        key(&mut g, KeyCode::Char('m'));
        assert!(g.setup_open);
    }

    #[test]
    fn snapshot_round_trips_mid_game_state() {
        let mut g = started_two_player();
        // Play until the line exists, then snapshot.
        for _ in 0..30 {
            if !g.engine.line().is_empty() {
                break;
            }
            let legal = g.engine.legal_moves();
            if legal.is_empty() {
                while g.engine.must_draw() {
                    g.engine.draw_one();
                }
            } else {
                g.cursor = Some(legal[0].0);
                key(&mut g, KeyCode::Enter);
            }
        }
        let blob = g.snapshot().expect("mid-game snapshot");
        let mut revived = DominoesGame::new();
        assert!(revived.restore(&blob));
        assert_eq!(revived.engine.line(), g.engine.line());
        assert_eq!(revived.engine.turn(), g.engine.turn());
        assert!(!revived.setup_open);
        let mut fresh = DominoesGame::new();
        assert!(!fresh.restore("nonsense"));
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(80u16, 24u16), (64, 16), (40, 14), (20, 8), (1, 1), (0, 0)] {
            let mut g = started_two_player();
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        }
    }
}
