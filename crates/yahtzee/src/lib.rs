//! Yahtzee for smartasbrain — the house shell pattern plus session
//! persistence: every state transition is snapshot-able and restorable.

mod ai;
mod engine;
mod ui;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use game_core::{Difficulty, Game};
use ratatui::{Frame, layout::Rect};

use engine::{ALL_CATEGORIES, CATS, Category, Side};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Opponent {
    Human,
    Ai(Difficulty),
    Battle(Difficulty),
}

pub struct YahtzeeGame {
    engine: engine::Yahtzee,
    cursor_row: usize,
    hover: Option<(u16, u16)>,
    geom: Option<ui::Geom>,
    buttons: Vec<(Rect, ui::Button)>,
    quit: bool,
    opponent: Opponent,
    setup_open: bool,
    menu_index: usize,
    menu_rects: Vec<(Rect, usize)>,
    ai_side: Side,
    /// RNG carried across ticks so AI rolls stay varied but saveable.
    rng_seed: u64,
}

impl YahtzeeGame {
    pub fn new() -> Self {
        Self {
            engine: engine::Yahtzee::new(),
            cursor_row: 0,
            hover: None,
            geom: None,
            buttons: Vec::new(),
            quit: false,
            opponent: Opponent::Human,
            setup_open: true,
            menu_index: 0,
            menu_rects: Vec::new(),
            ai_side: Side::Black,
            rng_seed: 0x5EED_0000_0001,
        }
    }

    fn reset(&mut self) {
        self.engine = engine::Yahtzee::new();
        self.cursor_row = 0;
        self.quit = false;
        self.rng_seed = self.rng_seed.wrapping_add(1);
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
    pub(crate) fn card_filled_count(&self, side: Side) -> usize {
        self.engine
            .card(side)
            .slots
            .iter()
            .filter(|s| s.is_some())
            .count()
    }

    fn next_rng(&mut self) -> rand::rngs::StdRng {
        use rand::SeedableRng;
        self.rng_seed = self
            .rng_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        rand::rngs::StdRng::seed_from_u64(self.rng_seed)
    }
}

impl Default for YahtzeeGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for YahtzeeGame {
    fn id(&self) -> &'static str {
        "yahtzee"
    }

    fn title(&self) -> &'static str {
        "yahtzee"
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
            Up | Char('k') if !busy => self.cursor_row = (self.cursor_row + CATS - 1) % CATS,
            Down | Char('j') if !busy => self.cursor_row = (self.cursor_row + 1) % CATS,
            Char('r') | Char(' ') if !busy => {
                let mut rng = self.next_rng();
                let _ = self.engine.roll(&mut rng);
            }
            Enter
                if !busy
                    && !self
                        .engine
                        .card(self.engine.turn())
                        .filled(category_at(self.cursor_row)) =>
            {
                let _ = self.engine.assign(category_at(self.cursor_row));
            }
            Char(d @ '1'..='5') if !busy => {
                let _ = self.engine.toggle_hold((d as u8 - b'1') as usize);
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
                    if !game_core::geom::in_rect(*rect, mouse.column, mouse.row) {
                        continue;
                    }
                    match button {
                        ui::Button::Roll => {
                            let mut rng = self.next_rng();
                            let _ = self.engine.roll(&mut rng);
                        }
                    }
                    return;
                }
                if let Some(die) = self
                    .geom
                    .as_ref()
                    .and_then(|g| g.die_at(mouse.column, mouse.row))
                {
                    let _ = self.engine.toggle_hold(die);
                    return;
                }
                if let Some(row) = self
                    .geom
                    .as_ref()
                    .and_then(|g| g.row_at(mouse.column, mouse.row))
                    && !self
                        .engine
                        .card(self.engine.turn())
                        .filled(category_at(row))
                {
                    self.cursor_row = row;
                    let _ = self.engine.assign(category_at(row));
                }
            }
            _ => {}
        }
    }

    /// One automated action per frame: roll phases advance, then assignment.
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
        if self.engine.rolls_left() > 0 {
            let holds = ai::holds_before_roll(&self.engine, difficulty);
            self.engine.holds_mut().copy_from_slice(&holds);
            let mut rng = self.next_rng();
            let _ = self.engine.roll(&mut rng);
            return;
        }
        if let Some(cat) = ai::best_category(&self.engine, difficulty) {
            let _ = self.engine.assign(cat);
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
            cursor_row: self.cursor_row,
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

    fn snapshot(&self) -> Option<String> {
        build_snapshot(self)
    }

    fn restore(&mut self, blob: &str) -> bool {
        apply_restore(self, blob)
    }

    fn wants_quit(&self) -> bool {
        self.quit
    }
}

fn build_snapshot(g: &YahtzeeGame) -> Option<String> {
    if g.setup_open {
        return None;
    }
    let dice: String = g.engine.dice().iter().map(|d| format!("{d}")).collect();
    let holds: String = g
        .engine
        .holds()
        .iter()
        .map(|&h| if h { "1" } else { "0" })
        .collect();
    let sheet = |side: Side| -> String {
        ALL_CATEGORIES
            .iter()
            .map(|c| {
                g.engine.card(side).slots[c.index()]
                    .map_or_else(|| ".".to_string(), |v| format!("{v}"))
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    Some(format!(
        "v1|{}|{}|{}|{}|{}|{}",
        side_tag(g.engine.turn()),
        g.engine.rolls_left(),
        dice,
        holds,
        sheet(Side::White),
        sheet(Side::Black),
    ))
}

/// Inverse of [`build_snapshot`]; rejects malformed or out-of-range blobs.
fn apply_restore(g: &mut YahtzeeGame, blob: &str) -> bool {
    let parts: Vec<&str> = blob.split('|').collect();
    if parts.len() != 7 || parts[0] != "v1" {
        return false;
    }
    let Some(turn) = untag_side(parts[1]) else {
        return false;
    };
    let Ok(rolls) = parts[2].parse::<u8>() else {
        return false;
    };
    let mut dice = [0_u8; crate::engine::DICE];
    if parts[3].len() != crate::engine::DICE
        || !parts[3].bytes().all(|b| (b'1'..=b'6').contains(&b))
    {
        return false;
    }
    for (i, b) in parts[3].bytes().enumerate() {
        dice[i] = b - b'0';
    }
    if parts[4].len() != crate::engine::DICE {
        return false;
    }
    let mut holds = [false; crate::engine::DICE];
    for (i, b) in parts[4].bytes().enumerate() {
        holds[i] = b == b'1';
    }
    let parse_sheet = |s: &str| -> Option<engine::Card> {
        let mut card = engine::Card::new();
        let cells: Vec<&str> = s.split(',').collect();
        if cells.len() != crate::engine::CATS {
            return None;
        }
        for (i, cell) in cells.iter().enumerate() {
            if *cell != "." {
                card.slots[i] = Some(cell.parse::<u16>().ok()?);
            }
        }
        Some(card)
    };
    let (Some(w), Some(b)) = (parse_sheet(parts[5]), parse_sheet(parts[6])) else {
        return false;
    };
    g.engine
        .restore_state(turn, rolls.min(3), dice, holds, w, b);
    g.setup_open = false;
    true
}

fn side_tag(side: Side) -> &'static str {
    if side == Side::White { "W" } else { "B" }
}

fn untag_side(tag: &str) -> Option<Side> {
    match tag {
        "W" => Some(Side::White),
        "B" => Some(Side::Black),
        _ => None,
    }
}

fn category_at(row: usize) -> Category {
    ALL_CATEGORIES[row % ALL_CATEGORIES.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::backend::TestBackend;

    fn started_two_player() -> YahtzeeGame {
        let mut g = YahtzeeGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g
    }

    fn key(g: &mut YahtzeeGame, code: KeyCode) {
        g.handle_key(KeyEvent::from(code));
    }

    #[test]
    fn duel_menu_mapping_and_self_play_completes() {
        let mut g = YahtzeeGame::new();
        key(&mut g, KeyCode::Char('6')); // AI DUEL - EASY
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));

        let mut expert = YahtzeeGame::new();
        key(&mut expert, KeyCode::Char('9'));
        key(&mut expert, KeyCode::Enter);
        assert_eq!(expert.opponent, Opponent::Battle(Difficulty::Expert));

        for _ in 0..6000 {
            g.tick();
            if !g.engine.status().is_ongoing() {
                break;
            }
        }
        assert!(
            !g.engine.status().is_ongoing(),
            "duel must finish within 6000 ticks"
        );
    }

    #[test]
    fn vs_ai_assigns_after_human_scores() {
        let mut g = YahtzeeGame::new();
        key(&mut g, KeyCode::Down); // VS AI - EASY
        key(&mut g, KeyCode::Enter);
        // Human burns rolls then scores; AI must fill a slot in reply.
        for _ in 0..3 {
            key(&mut g, KeyCode::Char(' '));
        }
        assert!(g.card_filled_count(Side::White) <= 1);
        key(&mut g, KeyCode::Enter); // assign at cursor row 0 (ones)
        assert_eq!(g.engine.turn(), Side::Black);
        for _ in 0..200 {
            g.tick();
            if g.engine.turn() == Side::White {
                break;
            }
        }
        assert_eq!(
            g.engine.turn(),
            Side::White,
            "AI must finish its turn within 200 ticks"
        );
    }

    #[test]
    fn snapshot_round_trips_through_restore() {
        let mut g = started_two_player();
        g.force_test_dice([1, 2, 2, 4, 6]);
        key(&mut g, KeyCode::Char('2')); // hold die 1
        key(&mut g, KeyCode::Char(' ')); // roll
        key(&mut g, KeyCode::Enter); // score ones row
        let blob = g.snapshot().expect("mid-game snapshot");
        let mut revived = YahtzeeGame::new();
        assert!(revived.restore(&blob));
        assert_eq!(revived.engine.turn(), g.engine.turn());
        assert_eq!(*revived.engine.dice(), *g.engine.dice());
        assert_eq!(
            revived.card_filled_count(Side::White),
            g.card_filled_count(Side::White)
        );
        // Garbage never restores.
        let mut fresh = YahtzeeGame::new();
        assert!(!fresh.restore("bogus"));
    }

    #[test]
    fn mouse_clicks_roll_button_and_die_holds() {
        let mut g = started_two_player();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        // Tray geometry lives right of the card; find the roll button via
        // the stored geom through a second draw + known offsets is fragile —
        // instead click the dice row band across several columns until one
        // toggles a hold.
        let Some(geom) = g.geom else { panic!("drawn") };
        // Holds unlock only after the opening roll.
        key(&mut g, KeyCode::Char(' '));
        assert_eq!(g.engine.rolls_left(), 2);
        let die_x = geom.tray_click_column(0);
        let die_y = geom.tray_click_row();
        let before = *g.engine.holds();
        g.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: die_x,
            row: die_y,
            modifiers: KeyModifiers::NONE,
        });
        assert_ne!(before, *g.engine.holds(), "die hold toggled");
    }

    impl YahtzeeGame {
        pub fn force_test_dice(&mut self, dice: [u8; crate::engine::DICE]) {
            self.engine.force_dice(dice);
        }
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(80u16, 24u16), (60, 22), (30, 12), (20, 8), (1, 1), (0, 0)] {
            let mut g = started_two_player();
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        }
    }
}
