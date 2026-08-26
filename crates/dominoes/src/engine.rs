//! Pure draw-dominoes rules over a double-six set: matching ends, forced
//! draws from the boneyard, passing only when it is truly dry, domino wins
//! worth the opponent's pips, and block resolution by lower pip count.

use rand::seq::SliceRandom;

/// A tile with normalized halves (`a <= b`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tile {
    pub a: u8,
    pub b: u8,
}

impl Tile {
    pub fn pips(self) -> u8 {
        self.a + self.b
    }

    pub fn has(self, v: u8) -> bool {
        self.a == v || self.b == v
    }

    /// The half that is NOT `v` (for orienting onto an open end).
    pub fn other(self, v: u8) -> u8 {
        if self.a == v { self.b } else { self.a }
    }
}

/// The full double-six set, 28 tiles.
pub fn full_set() -> Vec<Tile> {
    let mut tiles = Vec::with_capacity(28);
    for a in 0..=6_u8 {
        for b in a..=6_u8 {
            tiles.push(Tile { a, b });
        }
    }
    tiles
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }

    pub fn symbol(self) -> char {
        if self == Self::White {
            '\u{25cb}'
        } else {
            '\u{25cf}'
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ongoing,
    /// Winner plus the points awarded (opponent's leftover pips or the
    /// block margin).
    Won(Side, u16),
    Draw,
}

impl Status {
    pub fn is_ongoing(self) -> bool {
        matches!(self, Self::Ongoing)
    }
}

#[derive(Clone)]
pub struct Dominoes {
    hands: [Vec<Tile>; 2],
    boneyard: Vec<Tile>,
    /// Laid halves in order; both ends are open for matching.
    line: Vec<u8>,
    turn: Side,
    status: Status,
    consecutive_passes: u8,
}

fn side_index(side: Side) -> usize {
    if side == Side::White { 0 } else { 1 }
}

impl Dominoes {
    /// Deals 7 + 7 and parks 14 in the boneyard. White opens (house rule:
    /// no highest-double forcing, documented simplification).
    pub fn new(rng: &mut impl rand::Rng) -> Self {
        let mut set = full_set();
        set.shuffle(rng);
        let mut hands: [Vec<Tile>; 2] = [Vec::new(), Vec::new()];
        for (i, t) in set.iter().take(14).enumerate() {
            hands[i / 7].push(*t);
        }
        Self {
            hands,
            boneyard: set[14..].to_vec(),
            line: Vec::new(),
            turn: Side::White,
            status: Status::Ongoing,
            consecutive_passes: 0,
        }
    }

    pub fn hand(&self, side: Side) -> &[Tile] {
        &self.hands[side_index(side)]
    }

    pub fn boneyard_len(&self) -> usize {
        self.boneyard.len()
    }

    /// Persistence view of the remaining yard (draw order preserved).
    pub(crate) fn boneyard_tiles(&self) -> Vec<Tile> {
        self.boneyard.clone()
    }

    /// Full-state overwrite used by session restore.
    pub fn restore_state(
        &mut self,
        hands: [Vec<Tile>; 2],
        boneyard: Vec<Tile>,
        line: Vec<u8>,
        turn: Side,
    ) {
        self.hands = hands;
        self.boneyard = boneyard;
        self.line = line;
        self.turn = turn;
        self.status = Status::Ongoing;
        self.consecutive_passes = 0;
    }

    /// Laid halves; pairs read as tiles left-to-right.
    pub fn line(&self) -> &[u8] {
        &self.line
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// Open pip values at both ends of the chain (None while empty).
    pub fn ends(&self) -> (Option<u8>, Option<u8>) {
        (self.line.first().copied(), self.line.last().copied())
    }

    fn hand_pips(&self, side: Side) -> u16 {
        self.hands[side_index(side)]
            .iter()
            .map(|t| u16::from(t.pips()))
            .sum()
    }

    /// `(hand index, fits-left, fits-right)` for every playable tile.
    pub fn legal_moves(&self) -> Vec<(usize, bool, bool)> {
        if !self.status.is_ongoing() {
            return Vec::new();
        }
        let (Some(l), Some(r)) = self.ends() else {
            // Empty board: everything plays.
            return self.hands[side_index(self.turn)]
                .iter()
                .enumerate()
                .map(|(i, _)| (i, true, true))
                .collect();
        };
        self.hands[side_index(self.turn)]
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let left = t.has(l);
                let right = t.has(r);
                (left || right).then_some((i, left, right))
            })
            .collect()
    }

    pub fn must_draw(&self) -> bool {
        self.status.is_ongoing() && self.legal_moves().is_empty() && !self.boneyard.is_empty()
    }

    pub fn can_pass(&self) -> bool {
        self.status.is_ongoing() && self.legal_moves().is_empty() && self.boneyard.is_empty()
    }

    /// Pulls one tile from the boneyard into the mover's hand; the turn does
    /// not advance on draws.
    pub fn draw_one(&mut self) -> bool {
        if !self.must_draw() {
            return false;
        }
        let mover = side_index(self.turn);
        if let Some(t) = self.boneyard.pop() {
            self.hands[mover].push(t);
            return true;
        }
        false
    }

    /// Plays hand tile `idx`, auto-orienting onto whichever end matches.
    pub fn play(&mut self, idx: usize) -> bool {
        self.play_on(idx, false)
    }

    /// Like [`play`] but prefers the LEFT end when `prefer_left`.
    pub fn play_on(&mut self, idx: usize, prefer_left: bool) -> bool {
        if !self.status.is_ongoing() {
            return false;
        }
        let legal = self.legal_moves();
        if !legal.iter().any(|&(i, _, _)| i == idx) {
            return false;
        }
        let Some(&(_, fits_left, fits_right)) = legal.iter().find(|&&(i, _, _)| i == idx) else {
            return false;
        };
        let go_left = prefer_left && fits_left || !fits_right;
        let mover_side = self.turn;
        let mover = side_index(mover_side);
        let tile = self.hands[mover].remove(idx);

        match self.line.first() {
            None => {
                self.line.push(tile.a);
                self.line.push(tile.b);
            }
            Some(_) => {
                if go_left {
                    let end = *self.line.first().unwrap();
                    if tile.has(end) {
                        self.line.insert(0, end);
                        self.line.insert(0, tile.other(end));
                    } else {
                        let r = *self.line.last().unwrap();
                        self.line.push(r);
                        self.line.push(tile.other(r));
                    }
                } else {
                    let end = *self.line.last().unwrap();
                    if tile.has(end) {
                        self.line.push(end);
                        self.line.push(tile.other(end));
                    } else {
                        let l = *self.line.first().unwrap();
                        self.line.insert(0, l);
                        self.line.insert(0, tile.other(l));
                    }
                }
            }
        }

        self.consecutive_passes = 0;
        if self.hands[mover].is_empty() {
            let pts = self.hand_pips(self.turn.opposite());
            self.status = Status::Won(mover_side, pts);
            return true;
        }
        self.turn = mover_side.opposite();
        true
    }

    /// Passes the turn; two passes in a row resolve the block.
    pub fn pass(&mut self) -> bool {
        if !self.can_pass() {
            return false;
        }
        self.consecutive_passes += 1;
        if self.consecutive_passes >= 2 {
            let w = self.hand_pips(Side::White);
            let b = self.hand_pips(Side::Black);
            self.status = match w.cmp(&b) {
                std::cmp::Ordering::Less => Status::Won(Side::White, b - w),
                std::cmp::Ordering::Greater => Status::Won(Side::Black, w - b),
                std::cmp::Ordering::Equal => Status::Draw,
            };
            return true;
        }
        self.turn = self.turn.opposite();
        true
    }

    #[cfg(test)]
    pub(crate) fn force_state(
        &mut self,
        hands: [Vec<Tile>; 2],
        boneyard: Vec<Tile>,
        line: Vec<u8>,
        turn: Side,
    ) {
        self.hands = hands;
        self.boneyard = boneyard;
        self.line = line;
        self.turn = turn;
        self.status = Status::Ongoing;
        self.consecutive_passes = 0;
    }
}

impl Default for Dominoes {
    fn default() -> Self {
        Self::new(&mut rand::thread_rng())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    fn tile(a: u8, b: u8) -> Tile {
        Tile { a, b }
    }

    #[test]
    fn deal_shapes_are_exact() {
        let g = Dominoes::new(&mut StdRng::seed_from_u64(1));
        assert_eq!(g.hand(Side::White).len(), 7);
        assert_eq!(g.hand(Side::Black).len(), 7);
        assert_eq!(g.boneyard_len(), 14);
        assert_eq!(g.turn(), Side::White);
        assert!(g.status.is_ongoing());
    }

    #[test]
    fn first_play_is_free_and_sets_both_ends() {
        let mut g = Dominoes::new(&mut StdRng::seed_from_u64(2));
        assert!(g.play(0));
        let (l, r) = g.ends();
        assert!(l.is_some() && r.is_some());
        assert_eq!(g.line().len(), 2);
        assert_eq!(g.turn(), Side::Black);
    }

    #[test]
    fn ends_must_match_to_extend() {
        let mut g = Dominoes::new(&mut StdRng::seed_from_u64(3));
        g.force_state(
            [vec![tile(3, 5)], vec![tile(6, 6), tile(0, 1)]],
            vec![],
            vec![3, 5],
            Side::Black,
        );
        // Neither 6|6 nor 0|1 touches a 3 or a 5.
        assert!(!g.play(0));
        assert!(!g.must_draw(), "yard is empty");
        assert!(g.can_pass());
    }

    #[test]
    fn forced_draws_refill_until_playable_or_dry() {
        let mut g = Dominoes::new(&mut StdRng::seed_from_u64(4));
        g.force_state(
            [vec![tile(0, 0)], vec![tile(6, 6)]],
            vec![tile(3, 5), tile(5, 5)],
            vec![1, 4],
            Side::Black,
        );
        // Black holds 6|6: nothing matches the 1/4 ends -> draw twice dry.
        assert!(g.must_draw());
        assert!(g.draw_one());
        assert!(g.draw_one());
        assert!(!g.must_draw());
        assert!(!g.play(0), "6|6 still cannot land on 1 or 4");
        assert!(g.can_pass(), "yard drained -> pass is the only move");
    }

    #[test]
    fn double_pass_resolves_block_by_pip_count() {
        let mut g = Dominoes::new(&mut StdRng::seed_from_u64(5));
        g.force_state(
            [vec![tile(0, 0)], vec![tile(1, 1)]],
            vec![],
            vec![2, 3],
            Side::White,
        );
        assert!(g.can_pass());
        assert!(g.pass());
        assert!(g.pass());
        assert!(
            matches!(g.status(), Status::Won(Side::White, 2)),
            "white holds 0 vs black's 2"
        );
    }

    #[test]
    fn dominoing_scores_opponent_leftovers() {
        let mut g = Dominoes::new(&mut StdRng::seed_from_u64(6));
        g.force_state(
            [vec![tile(2, 3)], vec![tile(4, 4), tile(6, 6)]],
            vec![],
            vec![2, 3],
            Side::White,
        );
        assert!(g.play(0));
        assert!(matches!(g.status(), Status::Won(Side::White, 20)));
    }

    #[test]
    fn ai_self_play_always_legal_and_terminates() {
        use rand::Rng;
        for seed in 0..8_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut g = Dominoes::new(&mut rng);
            let mut plies = 0_u32;
            while g.status.is_ongoing() {
                while g.must_draw() {
                    assert!(g.draw_one());
                }
                if g.can_pass() {
                    assert!(g.pass());
                } else {
                    let moves = g.legal_moves();
                    let pick = moves[rng.gen_range(0..moves.len())];
                    assert!(g.play(pick.0), "engine suggested illegal move");
                }
                plies += 1;
                assert!(plies < 400);
            }
        }
    }
}
