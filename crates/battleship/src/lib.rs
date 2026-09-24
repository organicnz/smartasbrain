//! Battleship for smartasbrain — hidden fleets, salvo exchanges, and the
//! house shell pattern: setup picker, AI seats, duels, session persistence.

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

pub struct BattleshipGame {
    engine: engine::Battleship,
    cursor: usize,
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

impl BattleshipGame {
    pub fn new() -> Self {
        Self {
            engine: engine::Battleship::default(),
            cursor: 0,
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
        self.engine = engine::Battleship::default();
        self.cursor = 0;
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
    pub(crate) fn shot_count(&self, side: Side) -> usize {
        self.engine.shots_by(side).iter().filter(|&&v| v).count()
    }

    /// Human fires at the cursor's cell when it is their turn.
    fn fire_at_cursor(&mut self) {
        if self.ai_to_move() {
            return;
        }
        let cell = self.cursor;
        let _ = self.engine.fire(cell);
    }
}

impl Default for BattleshipGame {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for BattleshipGame {
    fn id(&self) -> &'static str {
        "battleship"
    }

    fn title(&self) -> &'static str {
        "battleship"
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
            Left | Char('h') if !busy => {
                self.cursor = (self.cursor + engine::CELLS - 1) % engine::CELLS
            }
            Right | Char('l') if !busy => self.cursor = (self.cursor + 1) % engine::CELLS,
            Up | Char('k') if !busy => {
                self.cursor = (self.cursor + engine::CELLS - engine::GRID) % engine::CELLS
            }
            Down | Char('j') if !busy => self.cursor = (self.cursor + engine::GRID) % engine::CELLS,
            Enter | Char(' ') if !busy => self.fire_at_cursor(),
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
                    if game_core::geom::in_rect(*rect, mouse.column, mouse.row)
                        && *button == ui::Button::Restart
                    {
                        self.reset();
                        return;
                    }
                }
                if let Some(cell) = self
                    .geom
                    .as_ref()
                    .and_then(|g| g.cell_at(mouse.column, mouse.row))
                {
                    self.cursor = cell;
                    self.fire_at_cursor();
                }
            }
            _ => {}
        }
    }

    /// One automated salvo per frame when an AI seat is up.
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
        if let Some(cell) = ai::best_shot(&self.engine, difficulty) {
            let _ = self.engine.fire(cell);
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

    /// `v1|turn|shotsW hex|shotsB hex|ships W kind,csv;...|ships B ...`
    fn snapshot(&self) -> Option<String> {
        build_snapshot(self)
    }

    fn restore(&mut self, blob: &str) -> bool {
        apply_restore(self, blob)
    }
}

fn mask_hex(mask: &[bool; crate::engine::CELLS]) -> String {
    let mut out = String::new();
    for chunk in mask.chunks(4) {
        let mut nibble = 0_u8;
        for (bit, &v) in chunk.iter().enumerate() {
            if v {
                nibble |= 1 << bit;
            }
        }
        out.push_str(&format!("{nibble:x}"));
    }
    out
}

fn unmask_hex(s: &str) -> Option<[bool; crate::engine::CELLS]> {
    if s.len() != crate::engine::CELLS / 4 {
        return None;
    }
    let mut mask = [false; crate::engine::CELLS];
    for (ni, ch) in s.bytes().enumerate() {
        let nibble = (ch as char).to_digit(16)? as u8;
        for bit in 0..4 {
            mask[ni * 4 + bit] = nibble & (1 << bit) != 0;
        }
    }
    Some(mask)
}

fn build_snapshot(g: &BattleshipGame) -> Option<String> {
    if g.setup_open {
        return None;
    }
    let masks = g.engine.incoming_masks();
    let side_blob = |side: Side| -> String {
        let cells = g
            .engine
            .fleet_cells(side)
            .into_iter()
            .map(|(kind, cells)| {
                let ki = crate::engine::FLEET
                    .iter()
                    .position(|&k| k == kind)
                    .unwrap_or(0);
                format!(
                    "{}:{}",
                    ki,
                    cells
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        format!(
            "{}|{}",
            mask_hex(&masks[if side == Side::White { 0 } else { 1 }]),
            cells
        )
    };
    Some(format!(
        "v1|{}|{}|{}",
        side_tag(g.engine.turn()),
        side_blob(Side::White),
        side_blob(Side::Black),
    ))
}

fn apply_restore(g: &mut BattleshipGame, blob: &str) -> bool {
    let parts: Vec<&str> = blob.split('|').collect();
    // header | turn | whiteMask | whiteShips | blackMask | blackShips
    if parts.len() != 6 || parts[0] != "v1" {
        return false;
    }
    let turn = match parts[1] {
        "W" => Side::White,
        "B" => Side::Black,
        _ => return false,
    };
    let white_blob = format!("{}|{}", parts[2], parts[3]);
    let black_blob = format!("{}|{}", parts[4], parts[5]);
    let parse_side = |blob: &str| -> Option<BattleshipSideSave> {
        let mut halves = blob.splitn(2, '|');
        let mask = unmask_hex(halves.next()?)?;
        let mut ships = Vec::new();
        for group in halves.next()?.split(';') {
            let mut it = group.splitn(2, ':');
            let ki: usize = it.next()?.parse().ok()?;
            let kind = *crate::engine::FLEET.get(ki)?;
            let cells = it
                .next()?
                .split(',')
                .map(|c| c.parse::<usize>())
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            ships.push((kind, cells));
        }
        Some((ships, mask))
    };
    let Some((w_ships, w_mask)) = parse_side(&white_blob) else {
        eprintln!("restore: white side failed");
        return false;
    };
    let Some((b_ships, b_mask)) = parse_side(&black_blob) else {
        eprintln!("restore: black side failed");
        return false;
    };
    g.engine
        .restore_state(turn, [w_ships, b_ships], [w_mask, b_mask]);
    g.setup_open = false;
    true
}

/// One seat's persisted fleet + incoming-shot mask.
type BattleshipSideSave = (
    Vec<(crate::engine::ShipKind, Vec<usize>)>,
    [bool; crate::engine::CELLS],
);

fn side_tag(side: Side) -> &'static str {
    match side {
        Side::White => "W",
        Side::Black => "B",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::backend::TestBackend;

    fn started_two_player() -> BattleshipGame {
        let mut g = BattleshipGame::new();
        g.handle_key(KeyEvent::from(KeyCode::Enter));
        g
    }

    fn key(g: &mut BattleshipGame, code: KeyCode) {
        g.handle_key(KeyEvent::from(code));
    }

    #[test]
    fn duel_menu_mapping_and_self_play_fires() {
        let mut g = BattleshipGame::new();
        key(&mut g, KeyCode::Char('6')); // AI DUEL - EASY
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Easy));
        let mut expert = BattleshipGame::new();
        key(&mut expert, KeyCode::Char('9'));
        key(&mut expert, KeyCode::Enter);
        assert_eq!(expert.opponent, Opponent::Battle(Difficulty::Expert));
        let before = g.shot_count(Side::White) + g.shot_count(Side::Black);
        for _ in 0..300 {
            g.tick();
            if !g.engine.status().is_ongoing() {
                break;
            }
        }
        assert!(
            g.shot_count(Side::White) + g.shot_count(Side::Black) > before,
            "duel must exchange fire"
        );
    }

    #[test]
    fn vs_ai_replies_after_human_shot() {
        let mut g = BattleshipGame::new();
        key(&mut g, KeyCode::Down); // VS AI - EASY
        key(&mut g, KeyCode::Enter);
        assert_eq!(g.ai_side, Side::Black);
        key(&mut g, KeyCode::Enter); // white fires cell 0
        assert_eq!(g.shot_count(Side::White), 1);
        for _ in 0..200 {
            g.tick();
            if g.shot_count(Side::Black) > 0 || !g.engine.status().is_ongoing() {
                break;
            }
        }
        assert!(
            g.shot_count(Side::Black) > 0,
            "AI must answer within 200 ticks"
        );
    }

    #[test]
    fn duel_input_is_inert_except_keeper_keys() {
        let mut g = BattleshipGame::new();
        key(&mut g, KeyCode::Char('6'));
        key(&mut g, KeyCode::Enter);
        for _ in 0..12 {
            g.tick();
        }
        let total_before = g.shot_count(Side::White) + g.shot_count(Side::Black);
        for code in [KeyCode::Enter, KeyCode::Char(' '), KeyCode::Char('r')] {
            key(&mut g, code);
        }
        assert_eq!(
            g.shot_count(Side::White) + g.shot_count(Side::Black),
            total_before,
            "duel frozen to input"
        );
        key(&mut g, KeyCode::Char('R'));
        assert_eq!(total_fleet_cells(&g), 34, "R redeploys fresh fleets");
        key(&mut g, KeyCode::Char('m'));
        assert!(g.setup_open);
    }

    #[test]
    fn snapshot_round_trips_mid_battle() {
        let mut g = started_two_player();
        key(&mut g, KeyCode::Enter); // white fires cursor cell 0
        let blob = g.snapshot().expect("mid-battle snapshot");
        let mut revived = BattleshipGame::new();
        assert!(revived.restore(&blob));
        assert_eq!(revived.engine.turn(), g.engine.turn());
        assert_eq!(revived.shot_count(Side::White), g.shot_count(Side::White));
        assert!(!revived.setup_open);
        let mut fresh = BattleshipGame::new();
        assert!(!fresh.restore("junk"));
    }

    fn total_fleet_cells(g: &BattleshipGame) -> usize {
        let mut n = 0;
        for side in [Side::White, Side::Black] {
            for r in 0..engine::GRID {
                for c in 0..engine::GRID {
                    if g.engine.own_cell(side, r * engine::GRID + c).0.is_some() {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    #[test]
    fn mouse_click_fires_at_target_sea() {
        let mut g = started_two_player();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        let Some(geom) = g.geom else {
            panic!("enemy sea drawn")
        };
        let before = g.shot_count(Side::White);
        g.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: geom.sea_x(),
            row: geom.sea_y(),
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(g.shot_count(Side::White), before + 1);
    }

    #[test]
    fn setup_exposes_and_accepts_the_expert_mouse_row() {
        let mut g = BattleshipGame::new();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        assert_eq!(g.menu_rects.len(), ui::SETUP_ITEMS.len());
        let (rect, index) = *g.menu_rects.last().unwrap();
        assert_eq!(index, 8);
        g.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(g.opponent, Opponent::Battle(Difficulty::Expert));
        g.tick();
        assert!(g.shot_count(Side::White) + g.shot_count(Side::Black) > 0);
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(80u16, 24u16), (56, 22), (30, 14), (14, 10), (1, 1), (0, 0)] {
            let mut g = started_two_player();
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| Game::draw(&mut g, f, f.area())).unwrap();
        }
    }
}
