use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, MouseEvent};
use game_core::Game;
use ratatui::{Frame, layout::Rect};

use crate::{
    board::{Board, SIZE, conflict_map},
    generator,
    records::Records,
};

pub const DIFFICULTIES: [(&str, usize, &str); 3] = [
    ("easy", 45, "a gentle warm-up"),
    ("medium", 34, "a solid challenge"),
    ("hard", 27, "for the fearless"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Menu,
    Playing,
    Won,
}

#[derive(Clone, Copy)]
pub enum ClickAction {
    Cell(usize),
    Digit(u8),
    Undo,
    Erase,
    Hint,
    NewGame,
    Restart,
    ToggleNotes,
    TogglePause,
    PickDifficulty(usize),
}

#[derive(Clone, Copy)]
pub struct Geom {
    pub grid: Rect,
    pub cell_w: u16,
    pub label_w: u16,
}

impl Geom {
    fn row_y(&self, r: usize) -> u16 {
        self.grid.y + 2 + r as u16 + (r / 3) as u16
    }

    fn cell_x(&self, c: usize) -> u16 {
        let step = 3 * self.cell_w + 1;
        self.grid.x + self.label_w + 1 + ((c / 3) as u16) * step + ((c % 3) as u16) * self.cell_w
    }

    pub fn cell_at(&self, col: u16, row: u16) -> Option<usize> {
        let g = self;
        let (rx, ry) = (col.checked_sub(g.grid.x)?, row.checked_sub(g.grid.y)?);
        if rx >= g.grid.width || ry >= g.grid.height {
            return None;
        }
        let r = (0..9).find(|&r| g.row_y(r) == row)?;
        let c = (0..9).find(|&c| {
            let x = g.cell_x(c);
            rx >= x - g.grid.x && rx < x - g.grid.x + g.cell_w
        })?;
        Some(r * 9 + c)
    }
}

/// Value plus pencil-mark bitmask for one cell.
#[derive(Clone, Copy)]
struct Snapshot {
    value: u8,
    notes: u16,
}

/// One reversible edit: the cell it touched and any peer notes it cleared.
#[derive(Clone)]
struct Edit {
    idx: usize,
    before: Snapshot,
    after: Snapshot,
    /// Peer indices whose `before` notes were cleared by a placement.
    peers: Vec<(usize, u16)>,
}

#[derive(Clone, Copy)]
pub struct WinInfo {
    pub new_record: bool,
    pub prev_best: Option<Duration>,
}

pub struct App {
    pub state: State,
    pub difficulty: usize,
    pub board: Board,
    pub initial: [u8; SIZE],
    pub solution: [u8; SIZE],
    pub notes: [u16; SIZE],
    pub conflicts: [bool; SIZE],
    pub cursor: usize,
    history: Vec<Edit>,
    redo_stack: Vec<Edit>,
    pub notes_mode: bool,
    pub paused: bool,
    pub paused_at: Instant,
    pub paused_total: Duration,
    pub started: Instant,
    pub elapsed: Duration,
    pub mistakes: usize,
    pub hints: usize,
    pub win_info: Option<WinInfo>,
    pub records: Records,
    /// Set to false in tests so runs never touch the user's real save file.
    pub(crate) persist_records: bool,
    pub quit: bool,
    pub geom: Option<Geom>,
    pub clickables: Vec<(Rect, ClickAction)>,
    pub hover: Option<(u16, u16)>,
    pub hover_cell: Option<usize>,
}

fn snapshot(board: &Board, notes: &[u16; SIZE], idx: usize) -> Snapshot {
    Snapshot {
        value: board.cells[idx],
        notes: notes[idx],
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            state: State::Menu,
            difficulty: 0,
            board: Board::new([0; SIZE]),
            initial: [0; SIZE],
            solution: [0; SIZE],
            notes: [0; SIZE],
            conflicts: [false; SIZE],
            cursor: 0,
            history: Vec::new(),
            redo_stack: Vec::new(),
            notes_mode: false,
            paused: false,
            paused_at: Instant::now(),
            paused_total: Duration::ZERO,
            started: Instant::now(),
            elapsed: Duration::ZERO,
            mistakes: 0,
            hints: 0,
            win_info: None,
            records: Records::load_default(),
            persist_records: true,
            quit: false,
            geom: None,
            clickables: Vec::new(),
            hover: None,
            hover_cell: None,
        }
    }

    pub fn start(&mut self) {
        let puzzle = generator::generate(DIFFICULTIES[self.difficulty].1);
        self.initial = puzzle.board.cells;
        self.begin_puzzle(puzzle.board, puzzle.solution);
        self.records.record_start(self.difficulty);
        if self.persist_records {
            self.records.save_default();
        }
    }

    /// Reset progress on the current puzzle without generating a new one.
    pub fn restart(&mut self) {
        if self.state == State::Menu {
            return;
        }
        let cells = self.initial;
        self.begin_puzzle(Board::new(cells), self.solution);
    }

    pub(crate) fn begin_puzzle(&mut self, board: Board, solution: [u8; SIZE]) {
        self.board = board;
        self.solution = solution;
        self.conflicts = conflict_map(&self.board.cells);
        self.notes = [0; SIZE];
        self.cursor = self.board.cells.iter().position(|&v| v == 0).unwrap_or(0);
        self.history.clear();
        self.redo_stack.clear();
        self.started = Instant::now();
        self.paused_at = self.started;
        self.paused_total = Duration::ZERO;
        self.elapsed = Duration::ZERO;
        self.mistakes = 0;
        self.hints = 0;
        self.win_info = None;
        self.paused = false;
        self.notes_mode = false;
        self.state = State::Playing;
    }

    pub fn open_menu(&mut self) {
        self.state = State::Menu;
        self.paused = false;
    }

    pub fn toggle_notes_mode(&mut self) {
        if self.state == State::Playing && !self.paused {
            self.notes_mode = !self.notes_mode;
        }
    }

    pub fn toggle_pause(&mut self) {
        if self.state != State::Playing {
            return;
        }
        if self.paused {
            self.paused_total += self.paused_at.duration_since(self.started);
            self.paused = false;
        } else {
            self.paused_at = Instant::now();
            self.paused = true;
        }
    }

    /// Time on the clock right now, pauses excluded.
    pub fn live_elapsed(&self) -> Duration {
        let end = if self.paused {
            self.paused_at
        } else {
            Instant::now()
        };
        end.checked_duration_since(self.started)
            .unwrap_or(Duration::ZERO)
            .saturating_sub(self.paused_total)
    }

    pub fn move_by(&mut self, dr: isize, dc: isize) {
        let r = (self.cursor / 9) as isize + dr;
        let c = (self.cursor % 9) as isize + dc;
        self.cursor = (r.clamp(0, 8) * 9 + c.clamp(0, 8)) as usize;
    }

    pub fn place(&mut self, value: u8) {
        if !self.editable() || self.board.cells[self.cursor] == value {
            return;
        }
        if self.notes_mode {
            // Notes only apply to empty cells; never clobber an entry.
            if self.board.cells[self.cursor] == 0 {
                self.toggle_note(self.cursor, value);
            }
            return;
        }
        self.apply_placement(self.cursor, value, true);
    }

    pub fn erase(&mut self) {
        if !self.editable() || self.board.cells[self.cursor] == 0 {
            return;
        }
        if self.notes_mode {
            // In notes mode erasing wipes all pencil marks in the cell.
            self.clear_notes(self.cursor);
            return;
        }
        self.apply_placement(self.cursor, 0, false);
    }

    pub fn undo(&mut self) {
        if self.state != State::Playing || self.paused {
            return;
        }
        if let Some(edit) = self.history.pop() {
            self.restore_edit(&edit);
            self.redo_stack.push(edit);
            self.after_change(false);
        }
    }

    pub fn redo(&mut self) {
        if self.state != State::Playing || self.paused {
            return;
        }
        if let Some(edit) = self.redo_stack.pop() {
            self.replay_edit(&edit);
            self.history.push(edit.clone());
            self.after_change(edit.after.value != 0);
        }
    }

    pub fn hint(&mut self) {
        if self.state != State::Playing || self.paused || self.solution.iter().all(|&v| v == 0) {
            return;
        }
        let target = if !self.board.is_fixed(self.cursor)
            && self.board.cells[self.cursor] != self.solution[self.cursor]
        {
            // Fix the cell under the cursor.
            self.cursor
        } else {
            // Already correct (or a given): help with the first empty cell.
            match self.board.cells.iter().position(|&v| v == 0) {
                Some(idx) => idx,
                None => return,
            }
        };
        self.cursor = target;
        if self.board.is_fixed(target) {
            return;
        }
        self.hints += 1;
        self.apply_placement(target, self.solution[target], false);
    }

    fn editable(&self) -> bool {
        self.state == State::Playing && !self.paused && !self.board.is_fixed(self.cursor)
    }

    fn toggle_note(&mut self, idx: usize, digit: u8) {
        let before = snapshot(&self.board, &self.notes, idx);
        self.notes[idx] ^= 1 << (digit - 1);
        let edit = Edit {
            idx,
            before,
            after: snapshot(&self.board, &self.notes, idx),
            peers: Vec::new(),
        };
        self.push_and_apply(edit);
    }

    fn clear_notes(&mut self, idx: usize) {
        if self.notes[idx] == 0 {
            return;
        }
        let before = snapshot(&self.board, &self.notes, idx);
        self.notes[idx] = 0;
        let edit = Edit {
            idx,
            before,
            after: snapshot(&self.board, &self.notes, idx),
            peers: Vec::new(),
        };
        self.push_and_apply(edit);
    }

    /// Writes `value` into `idx`, clearing that digit from peer pencil marks.
    fn apply_placement(&mut self, idx: usize, value: u8, count_mistakes: bool) {
        let before = snapshot(&self.board, &self.notes, idx);
        self.board.cells[idx] = value;
        self.notes[idx] = 0;

        let mut peers = Vec::new();
        if value != 0 {
            let clear_mask = !(1 << (value - 1));
            let digit_bit = !clear_mask;
            for p in peers_of(idx) {
                if self.board.cells[p] == 0 && self.notes[p] & digit_bit != 0 {
                    peers.push((p, self.notes[p]));
                    self.notes[p] &= clear_mask;
                }
            }
        }

        let edit = Edit {
            idx,
            before,
            after: Snapshot { value, notes: 0 },
            peers,
        };
        self.history.push(edit);
        self.redo_stack.clear();
        self.after_change(count_mistakes);
    }

    fn push_and_apply(&mut self, edit: Edit) {
        self.history.push(edit);
        self.redo_stack.clear();
        self.after_change(false);
    }

    fn restore_edit(&mut self, edit: &Edit) {
        self.board.cells[edit.idx] = edit.before.value;
        self.notes[edit.idx] = edit.before.notes;
        for &(p, notes) in &edit.peers {
            self.notes[p] = notes;
        }
    }

    fn replay_edit(&mut self, edit: &Edit) {
        self.board.cells[edit.idx] = edit.after.value;
        self.notes[edit.idx] = edit.after.notes;
        if edit.after.value != 0 {
            let bit = !(1 << (edit.after.value - 1));
            for &(p, _) in &edit.peers {
                self.notes[p] &= bit;
            }
        }
    }

    pub(crate) fn after_change(&mut self, placed_value: bool) {
        self.conflicts = conflict_map(&self.board.cells);
        if placed_value && self.board.cells[self.cursor] != 0 && self.conflicts[self.cursor] {
            self.mistakes += 1;
        }
        if self.board.cells.iter().all(|&v| v != 0) && !self.conflicts.iter().any(|&c| c) {
            self.elapsed = self.live_elapsed();
            let prev_best = self.records.best_ms[self.difficulty].map(Duration::from_millis);
            let new_record = self
                .records
                .record_win(self.difficulty, self.elapsed.as_millis() as u64);
            if self.persist_records {
                self.records.save_default();
            }
            self.win_info = Some(WinInfo {
                new_record,
                prev_best,
            });
            self.paused = false;
            self.state = State::Won;
        }
    }

    pub fn cell_at(&self, col: u16, row: u16) -> Option<usize> {
        self.geom?.cell_at(col, row)
    }

    pub fn activate(&mut self, action: ClickAction) {
        match action {
            ClickAction::Cell(idx) => self.cursor = idx,
            ClickAction::Digit(d) => self.place(d),
            ClickAction::Undo => self.undo(),
            ClickAction::Erase => self.erase(),
            ClickAction::Hint => self.hint(),
            ClickAction::NewGame => self.open_menu(),
            ClickAction::Restart => self.restart(),
            ClickAction::ToggleNotes => self.toggle_notes_mode(),
            ClickAction::TogglePause => self.toggle_pause(),
            ClickAction::PickDifficulty(i) => {
                self.difficulty = i;
                self.start();
            }
        }
    }

    pub fn clickable_at(&self, col: u16, row: u16) -> Option<ClickAction> {
        self.clickables
            .iter()
            .rev()
            .find(|(r, _)| in_rect(*r, col, row))
            .map(|(_, a)| *a)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for App {
    fn id(&self) -> &'static str {
        "sudoku"
    }

    fn title(&self) -> &'static str {
        "sudoku"
    }

    fn handle_key(&mut self, key: KeyEvent) {
        crate::input::route_key(self, key);
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        crate::input::on_mouse(self, mouse);
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        crate::ui::draw(frame, self, area);
    }

    fn wants_quit(&self) -> bool {
        self.quit
    }
}

fn peers_of(idx: usize) -> impl Iterator<Item = usize> {
    let (r, c) = (idx / 9, idx % 9);
    let (br, bc) = ((r / 3) * 3, (c / 3) * 3);
    (0..SIZE).filter(move |&p| {
        p != idx && (p / 9 == r || p % 9 == c || ((p / 27) * 3 + (p % 9) / 3) == br * 3 + bc)
    })
}

pub fn in_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing_app() -> App {
        let mut app = App::new();
        app.persist_records = false;
        let puzzle = generator::generate_seeded(1234 + app.difficulty as u64, DIFFICULTIES[0].1);
        app.initial = puzzle.board.cells;
        app.begin_puzzle(puzzle.board, puzzle.solution);
        app
    }

    #[test]
    fn placement_updates_conflicts_and_mistakes() {
        let mut app = playing_app();

        // Find two empty peers of cell 0's row.
        let first = (0..SIZE).find(|&i| app.board.cells[i] == 0).unwrap();
        let second = (first + 1..SIZE)
            .find(|&i| app.board.cells[i] == 0 && i / 9 == first / 9)
            .expect("row needs two empties");

        app.cursor = first;
        app.place(4);
        assert_eq!(app.board.cells[first], 4);

        app.cursor = second;
        app.place(4);
        assert!(app.conflicts[first] && app.conflicts[second]);
        assert_eq!(app.mistakes, 1);

        app.cursor = second;
        app.erase();
        assert_eq!(app.board.cells[second], 0);
        assert!(!app.conflicts[first]);

        // Undo the erase, then the placement on `second`, then `first`.
        app.undo();
        assert_eq!(app.board.cells[second], 4);
        app.undo();
        assert_eq!(app.board.cells[second], 0);
        assert_eq!(app.mistakes, 1, "mistakes never decrease");
        app.undo();
        assert_eq!(app.board.cells[first], 0);
    }

    #[test]
    fn undo_redo_roundtrip_restores_state_exactly() {
        let mut app = playing_app();
        let empty: Vec<usize> = (0..SIZE).filter(|&i| app.board.cells[i] == 0).collect();

        app.cursor = empty[0];
        app.place(3);

        app.notes_mode = true;
        let note_cell = empty[1];
        app.cursor = note_cell;
        app.place(5); // pencil mark
        app.place(6); // another pencil mark
        assert_eq!(app.notes[note_cell], (1 << 4) | (1 << 5));

        let mid_cells = app.board.cells;
        let mid_notes = app.notes;

        app.undo(); // remove note bit 6
        app.undo(); // remove note bit 5
        app.undo(); // remove value 3
        assert_eq!(app.board.cells[empty[0]], 0);
        assert_eq!(app.notes[note_cell], 0);

        while !app.redo_stack.is_empty() {
            app.redo();
        }
        assert_eq!(app.board.cells, mid_cells);
        assert_eq!(app.notes, mid_notes);
    }

    #[test]
    fn notes_toggle_adds_and_removes_marks() {
        let mut app = playing_app();
        let idx = (0..SIZE).find(|&i| app.board.cells[i] == 0).unwrap();

        app.notes_mode = true;
        app.cursor = idx;
        app.place(7);
        assert_eq!(app.notes[idx], 1 << 6);
        assert_eq!(app.board.cells[idx], 0);

        app.place(7);
        assert_eq!(app.notes[idx], 0);

        // Notes cannot be toggled onto filled cells.
        app.place(2);
        app.place(3);
        assert_eq!(app.notes[idx], (1 << 1) | (1 << 2));
        app.board.cells[idx] = 9;
        app.board.fixed[idx] = false;
        app.place(4);
        assert_eq!(
            app.notes[idx],
            (1 << 1) | (1 << 2),
            "no notes on filled cell"
        );
    }

    #[test]
    fn placement_clears_peer_notes_and_undo_restores_them() {
        let mut app = playing_app();
        let target = (0..SIZE).find(|&i| app.board.cells[i] == 0).unwrap();
        let peer = (0..SIZE)
            .filter(|&p| p != target && peers_of(target).any(|q| q == p))
            .find(|&p| app.board.cells[p] == 0)
            .unwrap();

        app.notes_mode = true;
        app.cursor = peer;
        app.place(8);
        assert_eq!(app.notes[peer], 1 << 7);

        app.notes_mode = false;
        app.cursor = target;
        app.place(8);
        assert_eq!(app.notes[peer], 0, "placement clears peer note");

        app.undo();
        assert_eq!(app.notes[peer], 1 << 7, "undo restores peer note");
    }

    #[test]
    fn hint_fills_correct_value_and_counts() {
        let mut app = playing_app();
        let wrong: Vec<usize> = (0..SIZE).filter(|&i| app.board.cells[i] == 0).collect();
        assert!(!wrong.is_empty());

        app.cursor = wrong[0];
        app.hint();
        assert_eq!(app.board.cells[wrong[0]], app.solution[wrong[0]]);
        assert_eq!(app.hints, 1);

        // Hinting again on a correct cell fills the next empty one.
        app.hint();
        assert_eq!(app.hints, 2);
        let second = wrong[1];
        assert_eq!(app.board.cells[second], app.solution[second]);
    }

    #[test]
    fn pause_freezes_the_clock() {
        let mut app = playing_app();
        std::thread::sleep(Duration::from_millis(15));
        app.toggle_pause();
        let frozen = app.live_elapsed();
        assert!(app.paused);

        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(
            app.live_elapsed(),
            frozen,
            "clock must not advance while paused"
        );

        app.toggle_pause();
        assert!(!app.paused);
        std::thread::sleep(Duration::from_millis(10));
        assert!(app.live_elapsed() >= frozen);
    }

    #[test]
    fn restart_resets_progress_on_same_puzzle() {
        let mut app = playing_app();
        let initial = app.initial;
        let idx = (0..SIZE).find(|&i| app.board.cells[i] == 0).unwrap();

        app.cursor = idx;
        app.place(9);
        app.mistakes = 3;
        app.hints = 2;
        std::thread::sleep(Duration::from_millis(12));
        assert!(app.live_elapsed() > Duration::ZERO);

        app.restart();
        assert_eq!(app.board.cells, initial, "same givens come back");
        assert_eq!(app.board.cells[idx], 0);
        assert_eq!(app.history.len(), 0);
        assert_eq!(app.mistakes, 0);
        assert_eq!(app.hints, 0);
        assert!(!app.paused);
    }

    #[test]
    fn fixed_cells_are_never_editable() {
        let mut app = playing_app();
        let given = (0..SIZE).find(|&i| app.board.fixed[i]).unwrap();
        app.cursor = given;
        app.place(1);
        app.erase();
        assert_eq!(app.history.len(), 0);
        assert_eq!(app.board.cells[given], app.solution[given]);
    }

    #[test]
    fn winning_sets_records_and_win_info() {
        let mut app = playing_app();
        app.difficulty = 0;
        for i in 0..SIZE {
            app.board.cells[i] = app.solution[i];
        }
        app.after_change(true);
        assert_eq!(app.state, State::Won);
        let info = app.win_info.expect("win info recorded");
        assert!(info.new_record, "first ever win is a record");
        assert_eq!(app.records.wins[0], 1);
        assert!(app.elapsed > Duration::ZERO);
    }
}
