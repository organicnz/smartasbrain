//! Pure Go rules: groups, liberties, captures, suicide, superko, scoring.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Player {
    Empty,
    Black,
    White,
}

impl Player {
    pub fn opponent(self) -> Self {
        match self {
            Self::Black => Self::White,
            _ => Self::Black,
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Empty => "\u{b7}",
            Self::Black => "\u{25cf}", // ●
            Self::White => "\u{25cb}", // ○
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveResult {
    Played,
    Occupied,
    Suicide,
    Ko,
    GameOver,
}

#[derive(Clone)]
pub struct GoState {
    size: usize,
    cells: Vec<Player>,
    pub turn: Player,
    pub captures_black: u32,
    pub captures_white: u32,
    pub passes: u8,
    pub over: bool,
    pub last_move: Option<usize>,
    /// Every whole-board position reached so far (superko bookkeeping).
    seen: HashSet<u64>,
}

pub(crate) fn neighbors(size: usize, idx: usize) -> impl Iterator<Item = usize> {
    let r = idx / size;
    let c = idx % size;
    let up = r.checked_sub(1).map(|r| r * size + c);
    let down = (r + 1 < size).then(|| (r + 1) * size + c);
    let left = c.checked_sub(1).map(|c| r * size + c);
    let right = (c + 1 < size).then(|| r * size + (c + 1));
    [up, down, left, right].into_iter().flatten()
}

/// Flood-fills the group containing `start`; returns its stones and liberty count.
pub(crate) fn group_and_liberties(
    cells: &[Player],
    size: usize,
    start: usize,
) -> (Vec<usize>, usize) {
    let color = cells[start];
    let mut group = vec![start];
    let mut visited = vec![false; cells.len()];
    visited[start] = true;
    let mut liberties = 0usize;

    let mut i = 0;
    while i < group.len() {
        for n in neighbors(size, group[i]) {
            match cells[n] {
                Player::Empty if !visited[n] => liberties += 1,
                c if c == color && !visited[n] => group.push(n),
                _ => {}
            }
            visited[n] = true;
        }
        i += 1;
    }
    (group, liberties)
}

fn hash_position(cells: &[Player], turn: Player) -> u64 {
    let mut hasher = DefaultHasher::new();
    for cell in cells {
        cell.hash(&mut hasher);
    }
    // The player to move distinguishes otherwise identical boards.
    turn.hash(&mut hasher);
    hasher.finish()
}

impl GoState {
    pub fn new(size: usize) -> Self {
        let cells = vec![Player::Empty; size * size];
        let mut seen = HashSet::new();
        seen.insert(hash_position(&cells, Player::Black));
        Self {
            size,
            cells,
            turn: Player::Black,
            captures_black: 0,
            captures_white: 0,
            passes: 0,
            over: false,
            last_move: None,
            seen,
        }
    }

    pub fn board(&self) -> &[Player] {
        &self.cells
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn play(&mut self, idx: usize) -> MoveResult {
        if self.over {
            return MoveResult::GameOver;
        }
        if idx >= self.cells.len() || self.cells[idx] != Player::Empty {
            return MoveResult::Occupied;
        }
        let me = self.turn;
        let opp = me.opponent();

        let mut trial = self.cells.clone();
        trial[idx] = me;

        let mut captured = 0u32;
        for n in neighbors(self.size, idx) {
            if trial[n] == opp {
                let (group, liberties) = group_and_liberties(&trial, self.size, n);
                if liberties == 0 {
                    for g in group {
                        trial[g] = Player::Empty;
                        captured += 1;
                    }
                }
            }
        }

        let (_, own_liberties) = group_and_liberties(&trial, self.size, idx);
        if own_liberties == 0 {
            return MoveResult::Suicide;
        }

        let position = hash_position(&trial, opp);
        if self.seen.contains(&position) {
            return MoveResult::Ko;
        }

        self.cells = trial;
        match me {
            Player::Black => self.captures_black += captured,
            _ => self.captures_white += captured,
        }
        self.seen.insert(position);
        self.turn = opp;
        self.passes = 0;
        self.last_move = Some(idx);
        MoveResult::Played
    }

    pub fn pass(&mut self) -> MoveResult {
        if self.over {
            return MoveResult::GameOver;
        }
        self.passes += 1;
        self.last_move = None;
        if self.passes >= 2 {
            self.over = true;
        } else {
            self.turn = self.turn.opponent();
        }
        MoveResult::Played
    }

    /// Area score. White's total includes komi.
    pub fn score(&self) -> Score {
        let mut black_stones = 0.0f32;
        let mut white_stones = 0.0f32;
        for &cell in &self.cells {
            match cell {
                Player::Black => black_stones += 1.0,
                Player::White => white_stones += 1.0,
                Player::Empty => {}
            }
        }

        let mut black_territory = 0.0f32;
        let mut white_territory = 0.0f32;
        let mut visited = vec![false; self.cells.len()];

        for start in 0..self.cells.len() {
            if self.cells[start] != Player::Empty || visited[start] {
                continue;
            }
            // Flood-fill one empty region and see whose colors border it.
            let mut region = vec![start];
            visited[start] = true;
            let mut touches_black = false;
            let mut touches_white = false;
            let mut i = 0;
            while i < region.len() {
                for n in neighbors(self.size, region[i]) {
                    match self.cells[n] {
                        Player::Black => touches_black = true,
                        Player::White => touches_white = true,
                        Player::Empty if !visited[n] => {
                            visited[n] = true;
                            region.push(n);
                        }
                        _ => {}
                    }
                }
                i += 1;
            }
            match (touches_black, touches_white) {
                (true, false) => black_territory += region.len() as f32,
                (false, true) => white_territory += region.len() as f32,
                _ => {}
            }
        }

        Score {
            black: black_stones + black_territory,
            white: white_stones + white_territory + KOMI,
        }
    }
}

pub const KOMI: f32 = 6.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score {
    pub black: f32,
    pub white: f32,
}

impl Score {
    /// e.g. `"B+3.5"` / `"W+6.5"`. Komi guarantees a winner under area rules.
    pub fn result_text(&self) -> String {
        if self.black > self.white {
            format!("B+{:.1}", self.black - self.white)
        } else {
            format!("W+{:.1}", self.white - self.black)
        }
    }
}

#[cfg(test)]
impl GoState {
    /// Test-only: build a state directly from cells; superko history holds
    /// just the given position.
    pub(crate) fn from_cells(size: usize, cells: Vec<Player>, turn: Player) -> Self {
        let mut seen = HashSet::new();
        seen.insert(hash_position(&cells, turn));
        Self {
            size,
            cells,
            turn,
            captures_black: 0,
            captures_white: 0,
            passes: 0,
            over: false,
            last_move: None,
            seen,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: usize = 9;

    const fn pt(r: usize, c: usize) -> usize {
        r * N + c
    }

    #[test]
    fn capture_single_stone_in_atari() {
        let mut s = GoState::new(N);

        // Wrap a white stone at (1,1) with black on all four sides; white
        // stalls along the bottom edge between black moves.
        let seq: &[(usize, bool)] = &[
            // (idx, is_black)
            (pt(8, 8), true),  // B filler corner
            (pt(1, 1), false), // W target stone
            (pt(0, 1), true),  // B north
            (pt(8, 7), false), // W stall
            (pt(2, 1), true),  // B south
            (pt(8, 6), false), // W stall
            (pt(1, 0), true),  // B west
            (pt(8, 5), false), // W stall
            (pt(1, 2), true),  // B east -> capture
        ];
        for &(idx, is_black) in seq {
            let expected = if is_black {
                Player::Black
            } else {
                Player::White
            };
            assert_eq!(s.turn, expected, "out of turn before move at {idx}");
            assert_eq!(s.play(idx), MoveResult::Played, "move at {idx}");
        }

        assert_eq!(s.board()[pt(1, 1)], Player::Empty, "white stone captured");
        assert_eq!(s.captures_black, 1);
        assert_eq!(s.captures_white, 0);
    }

    #[test]
    fn capture_two_stone_group() {
        let mut s = GoState::new(N);

        // White pair at (4,4),(5,4); its six outside liberties are
        // (3,4),(4,3),(4,5),(6,4),(5,3),(5,5). Black fills them across turns
        // while white stalls on the bottom edge; the sixth fill captures.
        let ring = [pt(3, 4), pt(4, 3), pt(4, 5), pt(6, 4), pt(5, 3), pt(5, 5)];
        let stalls = [pt(8, 0), pt(8, 1), pt(8, 2), pt(8, 3), pt(8, 4)];

        assert_eq!(s.play(pt(8, 8)), MoveResult::Played); // B filler
        assert_eq!(s.play(pt(4, 4)), MoveResult::Played); // W pair a
        assert_eq!(s.play(ring[0]), MoveResult::Played); // B
        assert_eq!(s.play(pt(5, 4)), MoveResult::Played); // W pair b
        assert_eq!(s.play(ring[1]), MoveResult::Played); // B
        assert_eq!(s.play(stalls[0]), MoveResult::Played); // W
        assert_eq!(s.play(ring[2]), MoveResult::Played); // B
        assert_eq!(s.play(stalls[1]), MoveResult::Played); // W
        assert_eq!(s.play(ring[3]), MoveResult::Played); // B
        assert_eq!(s.play(stalls[2]), MoveResult::Played); // W
        assert_eq!(s.play(ring[4]), MoveResult::Played); // B
        assert_eq!(s.play(stalls[3]), MoveResult::Played); // W

        // Five of six liberties gone; exactly one must remain.
        let (_, libs) = group_and_liberties(s.board(), N, pt(4, 4));
        assert_eq!(libs, 1, "one liberty must remain");

        assert_eq!(s.play(ring[5]), MoveResult::Played); // B closes the net
        assert_eq!(s.board()[pt(4, 4)], Player::Empty);
        assert_eq!(s.board()[pt(5, 4)], Player::Empty);
        assert_eq!(s.captures_black, 2);
    }

    #[test]
    fn suicide_is_forbidden() {
        let mut s = GoState::new(N);
        // Surround corner (0,0) with white on its two neighbours.
        assert_eq!(s.play(pt(8, 8)), MoveResult::Played); // B elsewhere
        assert_eq!(s.play(pt(0, 1)), MoveResult::Played); // W (0,1)
        assert_eq!(s.play(pt(8, 7)), MoveResult::Played); // B elsewhere
        assert_eq!(s.play(pt(1, 0)), MoveResult::Played); // W (1,0)

        // Both white neighbours still hold outside liberties, so nothing is
        // captured and black's stone would have zero liberties.
        assert_eq!(s.play(0), MoveResult::Suicide);
        assert_eq!(s.board()[0], Player::Empty);
    }

    #[test]
    fn simple_ko_is_blocked() {
        let mut s = GoState::new(N);
        let play = |s: &mut GoState, idx: usize| {
            assert_eq!(s.play(idx), MoveResult::Played, "move at {idx}");
        };

        // Textbook ko shape (cols 0-3, rows 0-2):
        //   . B W .
        //   B W _ W      <- white (1,1) is in atari; mouth at (1,2)
        //   . B W .
        // Black plays (1,2) capturing W(1,1); the capturing stone is then a
        // lone stone with its only liberty at (1,1), so immediate white
        // recapture would recreate the prior position -> ko.
        play(&mut s, pt(0, 1)); // B (0,1)
        play(&mut s, pt(1, 1)); // W target
        play(&mut s, pt(1, 0)); // B (1,0)
        play(&mut s, pt(0, 2)); // W (0,2)
        play(&mut s, pt(2, 1)); // B (2,1)
        play(&mut s, pt(1, 3)); // W (1,3)
        play(&mut s, pt(8, 8)); // B stall
        play(&mut s, pt(2, 2)); // W (2,2)

        let (_, libs) = group_and_liberties(s.board(), N, pt(1, 1));
        assert_eq!(libs, 1);

        play(&mut s, pt(1, 2)); // B captures
        assert_eq!(s.board()[pt(1, 1)], Player::Empty);
        assert_eq!(s.captures_black, 1);

        // The capturing stone sits alone with one liberty.
        let (group, libs) = group_and_liberties(s.board(), N, pt(1, 2));
        assert_eq!(group, vec![pt(1, 2)]);
        assert_eq!(libs, 1);

        // Immediate recapture recreates the previous position.
        assert_eq!(s.play(pt(1, 1)), MoveResult::Ko);
        assert_eq!(s.board()[pt(1, 2)], Player::Black, "ko point stays black");
    }

    #[test]
    fn double_pass_ends_game_with_komi_win_on_empty_board() {
        let mut state = GoState::new(N);
        assert_eq!(state.pass(), MoveResult::Played);
        assert!(!state.over);
        assert_eq!(state.turn, Player::White);
        assert_eq!(state.pass(), MoveResult::Played);
        assert!(state.over);

        let score = state.score();
        assert_eq!(score.black, 0.0);
        assert_eq!(score.white, KOMI);
        assert_eq!(score.result_text(), "W+6.5");

        // Play is refused after the game ends.
        assert_eq!(state.play(0), MoveResult::GameOver);
        assert_eq!(state.pass(), MoveResult::GameOver);
    }

    #[test]
    fn score_counts_stones_and_shared_board_has_no_territory() {
        let mut state = GoState::new(N);
        assert_eq!(state.play(pt(4, 4)), MoveResult::Played); // B centre
        assert_eq!(state.play(pt(4, 5)), MoveResult::Played); // W beside it
        // Both players then pass consecutively to end the game.
        assert_eq!(state.pass(), MoveResult::Played);
        assert_eq!(state.pass(), MoveResult::Played);
        assert!(state.over);

        // Every empty point touches both colours, so territory is zero.
        let score = state.score();
        assert_eq!(score.black, 1.0);
        assert_eq!(score.white, 1.0 + KOMI);
        assert_eq!(score.result_text(), "W+6.5");
    }

    #[test]
    fn occupied_point_rejected() {
        let mut state = GoState::new(N);
        assert_eq!(state.play(0), MoveResult::Played);
        assert_eq!(state.play(0), MoveResult::Occupied);
    }
}
