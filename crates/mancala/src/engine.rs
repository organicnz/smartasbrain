//! Pure Kalah mancala: six pits and a store per side, counter-clockwise
//! sowing that skips the opponent's store, extra turns on store landings,
//! captures on empty-own-pit landings, and an end-of-game sweep.

pub const PITS_PER_SIDE: usize = 6;
/// Pit indexing: 0..6 = White pits, 6 = White store, 7..13 = Black pits,
/// 13 = Black store.
pub const STORES: usize = 14;

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

    /// First pit index owned by this side.
    pub fn pit_range(self) -> std::ops::Range<usize> {
        match self {
            Self::White => 0..PITS_PER_SIDE,
            Self::Black => 7..7 + PITS_PER_SIDE,
        }
    }

    pub fn store(self) -> usize {
        match self {
            Self::White => 6,
            Self::Black => 13,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Ongoing,
    Won(Side),
    Draw,
}

impl Status {
    pub fn is_ongoing(self) -> bool {
        matches!(self, Self::Ongoing)
    }
}

#[derive(Clone)]
pub struct Mancala {
    pits: [u8; STORES],
    turn: Side,
    status: Status,
    last_sown: Option<usize>,
    captured_last: Option<(usize, usize)>,
}

/// Mirror pit across the board (undefined for stores).
fn mirror(i: usize) -> usize {
    12 - i
}

impl Mancala {
    pub fn new() -> Self {
        Self {
            pits: {
                let mut p = [0_u8; STORES];
                for slot in p.iter_mut().enumerate() {
                    let (i, v) = slot;
                    *v = if i == 6 || i == 13 { 0 } else { 4 };
                }
                p
            },
            turn: Side::White,
            status: Status::Ongoing,
            last_sown: None,
            captured_last: None,
        }
    }

    pub fn pit(&self, i: usize) -> u8 {
        self.pits.get(i).copied().unwrap_or(0)
    }

    pub fn store_of(&self, side: Side) -> u8 {
        self.pits[side.store()]
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn last_sown(&self) -> Option<usize> {
        self.last_sown
    }

    pub fn captured_last(&self) -> Option<(usize, usize)> {
        self.captured_last
    }

    /// Own pits holding at least one seed.
    pub fn legal_pits(&self) -> Vec<usize> {
        if !self.status.is_ongoing() {
            return Vec::new();
        }
        self.turn
            .pit_range()
            .filter(|&i| self.pits[i] > 0)
            .collect()
    }

    /// Sows from `pit`; false for wrong side, empty pit, or finished game.
    /// Handles store-extra-turn, captures, and the terminal sweep.
    pub fn sow(&mut self, pit: usize) -> bool {
        if !self.status.is_ongoing() || !self.turn.pit_range().contains(&pit) || self.pits[pit] == 0
        {
            return false;
        }
        self.captured_last = None;
        let mover = self.turn;
        let mut hand = self.pits[pit];
        self.pits[pit] = 0;

        let skip = mover.opposite().store();
        let mut i = pit;
        while hand > 0 {
            i = (i + 1) % STORES;
            if i == skip {
                continue;
            }
            self.pits[i] += 1;
            hand -= 1;
        }
        self.last_sown = Some(i);

        // Capture when the last seed lands in a formerly-empty own pit.
        let own_pits = mover.pit_range();
        if own_pits.contains(&i) && self.pits[i] == 1 && self.pits[mirror(i)] > 0 {
            let taken = self.pits[mirror(i)] + 1;
            self.pits[mirror(i)] = 0;
            self.pits[i] = 0;
            self.pits[mover.store()] += taken;
            self.captured_last = Some((mirror(i), taken as usize));
        }

        let extra_turn = i == mover.store();

        // Sweep when either side's pits run dry.
        let white_dry = (0..PITS_PER_SIDE).all(|p| self.pits[p] == 0);
        let black_dry = (7..7 + PITS_PER_SIDE).all(|p| self.pits[p] == 0);
        if white_dry || black_dry {
            for side in [Side::White, Side::Black] {
                let sweep: u32 = side.pit_range().map(|p| self.pits[p] as u32).sum();
                for p in side.pit_range() {
                    self.pits[p] = 0;
                }
                self.pits[side.store()] += sweep as u8;
            }
            let (w, b) = (self.pits[6], self.pits[13]);
            self.status = match w.cmp(&b) {
                std::cmp::Ordering::Greater => Status::Won(Side::White),
                std::cmp::Ordering::Less => Status::Won(Side::Black),
                std::cmp::Ordering::Equal => Status::Draw,
            };
            return true;
        }

        if !extra_turn {
            self.turn = mover.opposite();
        }
        true
    }
}

impl Default for Mancala {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl Mancala {
    /// Directly sets a pit for constructing tactical positions.
    pub fn force_pit(&mut self, i: usize, seeds: u8) {
        if i < STORES {
            self.pits[i] = seeds;
        }
    }

    /// Declares a finished result without touching stores.
    pub fn finish_into_win(&mut self, winner: Side) {
        self.status = Status::Won(winner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_board_has_four_seeds_per_pit() {
        let g = Mancala::new();
        assert_eq!(g.pit(0), 4);
        assert_eq!(g.pit(12), 4);
        assert_eq!(g.store_of(Side::White), 0);
        assert_eq!(g.turn(), Side::White);
    }

    #[test]
    fn sowing_wraps_and_skips_opponent_store() {
        // White sows pit 5 with 4 seeds: lands 6(store),7,8,9 — never 13.
        let mut g = Mancala::new();
        g.pits[5] = 8; // reach past black's store region
        assert!(g.sow(5));
        assert_eq!(g.pit(13), 0, "opponent store skipped");
        assert_eq!(g.store_of(Side::White), 1);
    }

    #[test]
    fn store_landing_grants_extra_turn() {
        let mut g = Mancala::new();
        // Pit 2 holds 4 seeds: 3,4,5,store -> extra turn.
        assert!(g.sow(2));
        assert_eq!(g.turn(), Side::White);
        assert_eq!(g.last_sown(), Some(6));
    }

    #[test]
    fn capture_takes_mirror_pair_into_store() {
        // Mirror of white pit 4 is black pit 8 (mirror(i) == 12 - i).
        let mut g = Mancala::new();
        g.pits = [4, 0, 0, 0, 0, 0, 0, 4, 1, 4, 4, 4, 4, 0];
        // Sow pit 0 (4 seeds): lands 1,2,3,4 -> pit 4 was empty, mirror 8
        // holds 1 -> capture takes 2 into White's store.
        assert!(g.sow(0));
        assert_eq!(g.captured_last(), Some((8, 2)));
        assert_eq!(g.store_of(Side::White), 2);
        assert_eq!(g.pit(4), 0);
        assert_eq!(g.pit(8), 0);
    }

    #[test]
    fn illegal_sources_are_rejected() {
        let mut g = Mancala::new();
        assert!(!g.sow(6), "store not sowable");
        assert!(!g.sow(9), "opponent pit");
        assert!(!g.sow(13), "opponent store");
    }

    #[test]
    fn sweep_ends_game_with_majority_winner() {
        // White's last six seeds march through black territory; when white's
        // pits empty, black's two remaining seeds sweep into their store.
        let mut g = Mancala::new();
        g.pits = [0, 0, 0, 0, 0, 6, 10, 0, 0, 0, 0, 0, 2, 3];
        assert!(g.sow(5));
        assert!(!g.status.is_ongoing());
        // Black sweeps its five freshly-sown seeds plus the two originals.
        assert_eq!(g.store_of(Side::Black), 3 + 5 + 2);
        // White edges it 11-10 via the store landing mid-sow.
        assert!(matches!(g.status(), Status::Won(Side::White)));
    }

    #[test]
    fn legal_pits_track_side_and_seeds() {
        let mut g = Mancala::new();
        let legal = g.legal_pits();
        assert_eq!(legal.len(), 6);
        assert!(legal.iter().all(|&i| i < 6));
        g.sow(2); // extra turn keeps white
        assert_eq!(g.turn(), Side::White);
    }
}
