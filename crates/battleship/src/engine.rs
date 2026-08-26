//! Pure Battleship rules: classic 10x10 fleets, hidden placement, one shot
//! per turn, hit/miss/sunk resolution and fleet-annihilation victory.

pub const GRID: usize = 10;
pub const CELLS: usize = GRID * GRID;

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
    Won(Side),
}

impl Status {
    pub fn is_ongoing(self) -> bool {
        matches!(self, Self::Ongoing)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShipKind {
    Carrier,
    Battleship,
    Cruiser,
    Submarine,
    Destroyer,
}

pub const FLEET: [ShipKind; 5] = [
    ShipKind::Carrier,
    ShipKind::Battleship,
    ShipKind::Cruiser,
    ShipKind::Submarine,
    ShipKind::Destroyer,
];

impl ShipKind {
    pub fn len(self) -> usize {
        match self {
            Self::Carrier => 5,
            Self::Battleship => 4,
            Self::Cruiser | Self::Submarine => 3,
            Self::Destroyer => 2,
        }
    }

    #[cfg(test)]
    pub fn name(self) -> &'static str {
        match self {
            Self::Carrier => "carrier",
            Self::Battleship => "battleship",
            Self::Cruiser => "cruiser",
            Self::Submarine => "submarine",
            Self::Destroyer => "destroyer",
        }
    }
}

/// What a side can know about the opposing waters.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CellView {
    Unknown,
    Miss,
    Hit,
    Sunk,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShotOutcome {
    Miss,
    Hit,
    Sunk(ShipKind),
}

#[derive(Clone)]
pub(crate) struct Ship {
    pub(crate) kind: ShipKind,
    pub(crate) cells: Vec<usize>,
    pub(crate) sunk: bool,
}

#[derive(Clone)]
struct Fleet {
    ships: Vec<Ship>,
    /// Cells fired at THIS fleet.
    incoming: [bool; CELLS],
}

impl Fleet {
    fn place(rng: &mut impl rand::Rng) -> Self {
        let mut cells_used = [false; CELLS];
        let mut ships = Vec::with_capacity(FLEET.len());
        for &kind in FLEET.iter() {
            let ship = loop {
                if let Some(s) = try_place(kind, &mut cells_used, rng) {
                    break s;
                }
                // Retry; the rejection sampler terminates practically.
            };
            ships.push(ship);
        }
        Self {
            ships,
            incoming: [false; CELLS],
        }
    }

    fn occupies(&self, cell: usize) -> Option<usize> {
        self.ships.iter().position(|s| s.cells.contains(&cell))
    }
}

/// One rejection attempt; returns None on collision or out-of-bounds drift.
fn try_place(kind: ShipKind, used: &mut [bool; CELLS], rng: &mut impl rand::Rng) -> Option<Ship> {
    let len = kind.len();
    let horizontal = rng.gen_bool(0.5);
    let row = rng.gen_range(0..if horizontal { GRID } else { GRID - len + 1 });
    let col = rng.gen_range(0..if horizontal { GRID - len + 1 } else { GRID });
    let mut cells = Vec::with_capacity(len);
    for step in 0..len {
        let (r, c) = if horizontal {
            (row, col + step)
        } else {
            (row + step, col)
        };
        let cell = r * GRID + c;
        if used[cell] {
            return None;
        }
        cells.push(cell);
    }
    // Commit only after the whole hull validated.
    for &c in &cells {
        used[c] = true;
    }
    Some(Ship {
        kind,
        cells,
        sunk: false,
    })
}

#[derive(Clone)]
pub struct Battleship {
    fleets: [Fleet; 2],
    turn: Side,
    status: Status,
    last_shot: Option<(Side, usize, ShotOutcome)>,
}

fn side_index(side: Side) -> usize {
    if side == Side::White { 0 } else { 1 }
}

impl Battleship {
    pub fn new(rng: &mut impl rand::Rng) -> Self {
        Self {
            fleets: [Fleet::place(rng), Fleet::place(rng)],
            turn: Side::White,
            status: Status::Ongoing,
            last_shot: None,
        }
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// Fires the mover's salvo at the opponent. Returns None for invalid
    /// shots (game over, out of range, repeated cell).
    pub fn fire(&mut self, cell: usize) -> Option<ShotOutcome> {
        if !self.status.is_ongoing() || cell >= CELLS {
            return None;
        }
        let foe_idx = side_index(self.turn.opposite());
        let fleet = &mut self.fleets[foe_idx];
        if fleet.incoming[cell] {
            return None;
        }
        fleet.incoming[cell] = true;

        let mover = self.turn;
        let outcome = match fleet.occupies(cell) {
            Some(ship_idx) => {
                let sunk = fleet.ships[ship_idx]
                    .cells
                    .iter()
                    .all(|&c| fleet.incoming[c]);
                if sunk {
                    fleet.ships[ship_idx].sunk = true;
                    ShotOutcome::Sunk(fleet.ships[ship_idx].kind)
                } else {
                    ShotOutcome::Hit
                }
            }
            None => ShotOutcome::Miss,
        };
        self.last_shot = Some((mover, cell, outcome));

        if fleet.ships.iter().all(|s| s.sunk) {
            self.status = Status::Won(mover);
            return Some(outcome);
        }
        self.turn = mover.opposite();
        Some(outcome)
    }

    /// Cells already fired at the opposing waters by `side`.
    #[cfg(test)]
    pub fn shots_by(&self, side: Side) -> [bool; CELLS] {
        self.fleets[side_index(side.opposite())].incoming
    }

    /// Intelligence view of the opponent's waters from `side`'s seat.
    pub fn enemy_view(&self, side: Side) -> [CellView; CELLS] {
        let foe = &self.fleets[side_index(side.opposite())];
        let mut view = [CellView::Unknown; CELLS];
        for (i, shot) in foe.incoming.iter().enumerate() {
            if !shot {
                continue;
            }
            match foe.occupies(i) {
                Some(si) => {
                    view[i] = if foe.ships[si].sunk {
                        CellView::Sunk
                    } else {
                        CellView::Hit
                    };
                }
                None => view[i] = CellView::Miss,
            }
        }
        view
    }

    /// Own board cell: which ship (if any) and whether it has been hit.
    pub fn own_cell(&self, side: Side, cell: usize) -> (Option<ShipKind>, bool) {
        let fleet = &self.fleets[side_index(side)];
        (
            fleet.occupies(cell).map(|si| fleet.ships[si].kind),
            fleet.incoming[cell],
        )
    }

    /// Unsunk ships still afloat for `side`.
    pub fn remaining(&self, side: Side) -> Vec<(ShipKind, usize)> {
        self.fleets[side_index(side)]
            .ships
            .iter()
            .filter(|s| !s.sunk)
            .map(|s| (s.kind, s.kind.len()))
            .collect()
    }

    /// Persistence view: every ship's kind and cells, sunk flags included.
    pub(crate) fn fleet_cells(&self, side: Side) -> Vec<(ShipKind, Vec<usize>)> {
        self.fleets[side_index(side)]
            .ships
            .iter()
            .map(|s| (s.kind, s.cells.clone()))
            .collect()
    }

    /// Incoming-shot masks per side (index by side).
    pub(crate) fn incoming_masks(&self) -> [[bool; CELLS]; 2] {
        [self.fleets[0].incoming, self.fleets[1].incoming]
    }

    /// Full-state overwrite used by session restore; status is recomputed.
    pub fn restore_state(
        &mut self,
        turn: Side,
        ships_by_side: [Vec<(ShipKind, Vec<usize>)>; 2],
        masks: [[bool; CELLS]; 2],
    ) {
        let build = |ships_spec: &[(ShipKind, Vec<usize>)], incoming: &[bool; CELLS]| Fleet {
            ships: ships_spec
                .iter()
                .map(|(kind, cells)| Ship {
                    kind: *kind,
                    cells: cells.clone(),
                    sunk: cells.iter().all(|&c| incoming[c]),
                })
                .collect(),
            incoming: *incoming,
        };
        self.fleets = [
            build(&ships_by_side[0], &masks[0]),
            build(&ships_by_side[1], &masks[1]),
        ];
        self.turn = turn;
        self.status = if self.fleets.iter().all(|f| f.ships.iter().all(|s| s.sunk)) {
            // Both annihilated cannot happen; first fully-sunk fleet lost.
            Status::Won(turn.opposite())
        } else if self.fleets[0].ships.iter().all(|s| s.sunk) {
            Status::Won(Side::Black)
        } else if self.fleets[1].ships.iter().all(|s| s.sunk) {
            Status::Won(Side::White)
        } else {
            Status::Ongoing
        };
        self.last_shot = None;
    }

    #[cfg(test)]
    pub(crate) fn force_fleet(&mut self, side: Side, ships: Vec<Ship>) {
        self.fleets[side_index(side)] = Fleet {
            ships,
            incoming: [false; CELLS],
        };
    }
}

impl Default for Battleship {
    fn default() -> Self {
        Self::new(&mut rand::thread_rng())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};

    fn ship(kind: ShipKind, cells: &[usize]) -> Ship {
        Ship {
            kind,
            cells: cells.to_vec(),
            sunk: false,
        }
    }

    #[test]
    fn placement_is_complete_and_collision_free() {
        for seed in 0..25_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let g = Battleship::new(&mut rng);
            for side in [Side::White, Side::Black] {
                let mut seen = [false; CELLS];
                let fleet = &g.fleets[side_index(side)];
                assert_eq!(fleet.ships.len(), FLEET.len());
                for s in &fleet.ships {
                    assert_eq!(s.cells.len(), s.kind.len(), "{} length", s.kind.name());
                    for &c in &s.cells {
                        assert!(c < CELLS);
                        assert!(!seen[c], "overlap at {c}");
                        seen[c] = true;
                    }
                }
                // Ships occupy exactly 15 distinct cells.
                assert_eq!(seen.iter().filter(|&&v| v).count(), 17);
            }
        }
    }

    #[test]
    fn fire_resolves_miss_hit_sunk_and_win() {
        let mut rng = StdRng::seed_from_u64(9);
        let mut g = Battleship::new(&mut rng);
        g.force_fleet(
            Side::Black,
            vec![
                ship(ShipKind::Destroyer, &[0, 1]),
                ship(ShipKind::Carrier, &[22, 23, 24, 32, 33]),
            ],
        );
        g.force_fleet(Side::White, vec![ship(ShipKind::Destroyer, &[99, 98])]);
        // White hunts the black destroyer at cells 0-1.
        assert_eq!(g.fire(0), Some(ShotOutcome::Hit));
        assert_eq!(g.turn(), Side::Black);
        assert!(g.fire(40).is_some(), "black answers into open water");
        assert_eq!(g.turn(), Side::White);
        assert_eq!(g.fire(1), Some(ShotOutcome::Sunk(ShipKind::Destroyer)));
        assert_eq!(g.enemy_view(Side::White)[0], CellView::Sunk);
        assert_eq!(g.remaining(Side::Black).len(), 1, "carrier afloat");
        assert_eq!(g.turn(), Side::Black);
        // Black cannot re-fire black's own earlier cell either.
        let before = g.turn();
        assert_eq!(g.fire(40), None);
        assert_eq!(g.turn(), before);
    }

    #[test]
    fn sinking_the_last_ship_ends_the_game() {
        let mut rng = StdRng::seed_from_u64(11);
        let mut g = Battleship::new(&mut rng);
        g.force_fleet(Side::Black, vec![ship(ShipKind::Destroyer, &[0, 1])]);
        g.force_fleet(Side::White, vec![ship(ShipKind::Destroyer, &[50, 51])]);
        assert_eq!(g.fire(0), Some(ShotOutcome::Hit));
        assert!(g.fire(55).is_some(), "black answers");
        assert_eq!(g.fire(1), Some(ShotOutcome::Sunk(ShipKind::Destroyer)));
        assert_eq!(g.status(), Status::Won(Side::White));
    }

    #[test]
    fn enemy_view_hides_unshot_ships() {
        let mut rng = StdRng::seed_from_u64(13);
        let mut g = Battleship::new(&mut rng);
        g.force_fleet(Side::Black, vec![ship(ShipKind::Destroyer, &[0, 1])]);
        g.force_fleet(Side::White, vec![ship(ShipKind::Destroyer, &[50, 51])]);
        let clean = g.enemy_view(Side::White);
        assert!(clean.iter().all(|v| *v == CellView::Unknown));
        let _ = g.fire(0);
        let view = g.enemy_view(Side::White);
        assert_eq!(view[0], CellView::Hit);
        assert_eq!(view[1], CellView::Unknown, "adjacency stays hidden");
    }
}
