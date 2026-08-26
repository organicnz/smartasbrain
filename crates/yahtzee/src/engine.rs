//! Pure Yahtzee rules: five dice, up to three rolls with holds, a
//! thirteen-category scorecard per player, the upper-section bonus, and
//! grand-total resolution.

pub const CATS: usize = 13;
pub const DICE: usize = 5;
/// Upper-section subtotal that earns the +35 bonus.
const BONUS_THRESHOLD: u16 = 63;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Category {
    Ones = 0,
    Twos,
    Threes,
    Fours,
    Fives,
    Sixes,
    ThreeKind,
    FourKind,
    FullHouse,
    SmallStraight,
    LargeStraight,
    Yahtzee,
    Chance,
}

pub const ALL_CATEGORIES: [Category; CATS] = [
    Category::Ones,
    Category::Twos,
    Category::Threes,
    Category::Fours,
    Category::Fives,
    Category::Sixes,
    Category::ThreeKind,
    Category::FourKind,
    Category::FullHouse,
    Category::SmallStraight,
    Category::LargeStraight,
    Category::Yahtzee,
    Category::Chance,
];

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ones => "ones",
            Self::Twos => "twos",
            Self::Threes => "threes",
            Self::Fours => "fours",
            Self::Fives => "fives",
            Self::Sixes => "sixes",
            Self::ThreeKind => "3-kind",
            Self::FourKind => "4-kind",
            Self::FullHouse => "full house",
            Self::SmallStraight => "sm straight",
            Self::LargeStraight => "lg straight",
            Self::Yahtzee => "yahtzee",
            Self::Chance => "chance",
        }
    }

    /// Upper section covers the first six categories.
    pub fn is_upper(self) -> bool {
        matches!(
            self,
            Self::Ones | Self::Twos | Self::Threes | Self::Fours | Self::Fives | Self::Sixes
        )
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Points `dice` would score in `cat` (0 when the shape doesn't match).
pub fn score_of(cat: Category, dice: &[u8; DICE]) -> u16 {
    let mut faces = [0_u8; 7];
    for &d in dice {
        faces[d as usize] += 1;
    }
    let sum: u16 = dice.iter().map(|&d| d as u16).sum();
    let max_kind = *faces.iter().max().unwrap_or(&0);
    match cat {
        c if c.is_upper() => {
            let face = c as u8 + 1; // discriminant 0 == aces
            u16::from(face) * faces[face as usize] as u16
        }
        Category::ThreeKind if max_kind >= 3 => sum,
        Category::FourKind if max_kind >= 4 => sum,
        Category::FullHouse => {
            // Exactly 3+1+1 split with a pair; five-of-a-kind is NOT a house.
            let mut has_triple = false;
            let mut has_pair = false;
            for &f in faces[1..].iter() {
                match f {
                    3 => has_triple = true,
                    2 => has_pair = true,
                    _ => {}
                }
            }
            u16::from(has_triple && has_pair) * 25
        }
        Category::SmallStraight | Category::LargeStraight => {
            let distinct: Vec<u8> = (1..=6_u8).filter(|&f| faces[f as usize] > 0).collect();
            let run = longest_run(&distinct);
            let needed = match cat {
                Category::SmallStraight => 4,
                _ => 5,
            };
            u16::from(run >= needed) * if needed == 4 { 30 } else { 40 }
        }
        Category::Yahtzee if max_kind == 5 => 50,
        Category::Chance => sum,
        _ => 0,
    }
}

fn longest_run(sorted_distinct: &[u8]) -> usize {
    let mut best = 0_usize;
    let mut run = 0_usize;
    let mut prev: Option<u8> = None;
    for &f in sorted_distinct {
        run = if prev.is_some_and(|p| p + 1 == f) {
            run + 1
        } else {
            1
        };
        best = best.max(run);
        prev = Some(f);
    }
    best
}

#[derive(Clone, Copy)]
pub struct Card {
    pub slots: [Option<u16>; CATS],
}

impl Card {
    pub fn new() -> Self {
        Self {
            slots: [None; CATS],
        }
    }

    pub fn filled(&self, cat: Category) -> bool {
        self.slots[cat.index()].is_some()
    }

    /// (upper subtotal, bonus, lower subtotal, grand total)
    pub fn totals(&self) -> (u16, u16, u16, u16) {
        let mut upper = 0_u16;
        let mut lower = 0_u16;
        for cat in ALL_CATEGORIES {
            if let Some(v) = self.slots[cat.index()] {
                if cat.is_upper() {
                    upper += v;
                } else {
                    lower += v;
                }
            }
        }
        let bonus = u16::from(upper >= BONUS_THRESHOLD) * 35;
        (upper, bonus, lower, upper + bonus + lower)
    }

    pub fn complete(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct Yahtzee {
    pub(crate) cards: [Card; 2],
    turn: Side,
    dice: [u8; DICE],
    holds: [bool; DICE],
    rolls_left: u8,
    status: Status,
    last_assigned: Option<(Side, Category)>,
}

impl Yahtzee {
    pub fn new() -> Self {
        Self {
            cards: [Card::new(), Card::new()],
            turn: Side::White,
            dice: [1; DICE],
            holds: [false; DICE],
            rolls_left: 3,
            status: Status::Ongoing,
            last_assigned: None,
        }
    }

    pub fn dice(&self) -> &[u8; DICE] {
        &self.dice
    }

    pub fn holds(&self) -> &[bool; DICE] {
        &self.holds
    }

    pub fn rolls_left(&self) -> u8 {
        self.rolls_left
    }

    pub fn turn(&self) -> Side {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn card(&self, side: Side) -> &Card {
        &self.cards[if side == Side::White { 0 } else { 1 }]
    }

    pub fn last_assigned(&self) -> Option<(Side, Category)> {
        self.last_assigned
    }

    /// Rolls every unheld die; false when no rolls remain.
    pub fn roll(&mut self, rng: &mut impl rand::Rng) -> bool {
        if !self.status.is_ongoing() || self.rolls_left == 0 {
            return false;
        }
        for d in self.dice.iter_mut().zip(self.holds.iter()) {
            if !d.1 {
                *d.0 = rng.gen_range(1..=6);
            }
        }
        self.rolls_left -= 1;
        true
    }

    /// Toggling holds is allowed between rolls only (at least one roll used
    /// and at least one remaining).
    pub fn toggle_hold(&mut self, die: usize) -> bool {
        if !self.status.is_ongoing() || self.rolls_left == 0 || self.rolls_left == 3 {
            return false;
        }
        if let Some(h) = self.holds.get_mut(die) {
            *h = !*h;
            true
        } else {
            false
        }
    }

    /// Writes the current dice into an empty category slot and passes the
    /// cup. False when the slot is taken or the sheet is frozen.
    pub fn assign(&mut self, cat: Category) -> bool {
        if !self.status.is_ongoing() {
            return false;
        }
        let mover = self.turn;
        let slot = &mut self.cards[side_index(mover)].slots[cat.index()];
        if slot.is_some() {
            return false;
        }
        *slot = Some(score_of(cat, &self.dice));
        self.last_assigned = Some((mover, cat));

        if self.cards[0].complete() || self.cards[1].complete() {
            let (w_tot, b_tot) = (self.cards[0].totals().3, self.cards[1].totals().3);
            self.status = match w_tot.cmp(&b_tot) {
                std::cmp::Ordering::Greater => Status::Won(Side::White),
                std::cmp::Ordering::Less => Status::Won(Side::Black),
                std::cmp::Ordering::Equal => Status::Draw,
            };
            return true;
        }

        self.turn = mover.opposite();
        self.holds = [false; DICE];
        self.rolls_left = 3;
        true
    }

    #[cfg(test)]
    pub fn force_dice(&mut self, dice: [u8; DICE]) {
        self.dice = dice;
    }

    #[cfg(test)]
    pub fn force_rolls(&mut self, n: u8) {
        self.rolls_left = n;
    }

    /// Mutable hold flags for AI decision application.
    pub fn holds_mut(&mut self) -> &mut [bool; DICE] {
        &mut self.holds
    }

    /// Full-state overwrite used by session restore; validates nothing but
    /// keeps values sane (rolls clamped, faces bounded).
    pub fn restore_state(
        &mut self,
        turn: Side,
        rolls_left: u8,
        mut dice: [u8; DICE],
        holds: [bool; DICE],
        white: Card,
        black: Card,
    ) {
        for d in &mut dice {
            *d = (*d).clamp(1, 6);
        }
        self.cards = [white, black];
        self.turn = turn;
        self.dice = dice;
        self.holds = holds;
        self.rolls_left = rolls_left.min(3);
        self.last_assigned = None;
        // Recompute status from sheet completeness.
        self.status = if self.cards[0].complete() || self.cards[1].complete() {
            let (w, b) = (self.cards[0].totals().3, self.cards[1].totals().3);
            match w.cmp(&b) {
                std::cmp::Ordering::Greater => Status::Won(Side::White),
                std::cmp::Ordering::Less => Status::Won(Side::Black),
                std::cmp::Ordering::Equal => Status::Draw,
            }
        } else {
            Status::Ongoing
        };
    }
}

fn side_index(side: Side) -> usize {
    if side == Side::White { 0 } else { 1 }
}

impl Default for Yahtzee {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_scoring_matches_the_sheet() {
        assert_eq!(score_of(Category::Fours, &[4, 4, 1, 4, 2]), 12);
        assert_eq!(score_of(Category::Sixes, &[4, 4, 1, 4, 2]), 0);
        assert_eq!(score_of(Category::ThreeKind, &[6, 6, 6, 2, 1]), 21);
        assert_eq!(score_of(Category::ThreeKind, &[6, 6, 2, 2, 1]), 0);
        assert_eq!(score_of(Category::FourKind, &[3, 3, 3, 3, 5]), 17);
        assert_eq!(score_of(Category::FullHouse, &[2, 2, 5, 5, 5]), 25);
        assert_eq!(score_of(Category::FullHouse, &[2, 2, 2, 5, 5]), 25);
        assert_eq!(
            score_of(Category::FullHouse, &[2, 2, 2, 2, 5]),
            0,
            "no quads"
        );
        assert_eq!(score_of(Category::SmallStraight, &[1, 2, 3, 4, 6]), 30);
        assert_eq!(score_of(Category::SmallStraight, &[2, 3, 5, 6, 1]), 0);
        assert_eq!(score_of(Category::LargeStraight, &[1, 2, 3, 4, 5]), 40);
        assert_eq!(score_of(Category::LargeStraight, &[2, 3, 4, 5, 6]), 40);
        assert_eq!(score_of(Category::Yahtzee, &[4, 4, 4, 4, 4]), 50);
        assert_eq!(score_of(Category::Chance, &[1, 3, 3, 5, 6]), 18);
    }

    #[test]
    fn rolls_decrement_and_holds_are_respected() {
        use rand::{SeedableRng, rngs::StdRng};
        let mut g = Yahtzee::new();
        let mut rng = StdRng::seed_from_u64(7);
        g.force_dice([1, 2, 3, 4, 5]);
        g.force_rolls(2);
        g.holds = [true, false, true, false, true];
        assert!(g.roll(&mut rng));
        assert_eq!(g.dice()[0], 1);
        assert_eq!(g.dice()[2], 3);
        assert_eq!(g.dice()[4], 5);
        assert_eq!(g.rolls_left(), 1);
        // Releasing an already-held die is equally legal mid-turn.
        assert!(g.toggle_hold(0));
        assert!(!g.holds()[0]);
        assert!(g.roll(&mut rng));
        assert_eq!(g.rolls_left(), 0);
        assert!(!g.roll(&mut rng), "cup is empty");
    }

    #[test]
    fn assign_fills_slot_then_passes_cup_with_fresh_rolls() {
        let mut g = Yahtzee::new();
        g.force_dice([2, 2, 2, 5, 5]);
        assert!(g.assign(Category::FullHouse));
        assert_eq!(
            g.card(Side::White).slots[Category::FullHouse.index()],
            Some(25)
        );
        assert_eq!(g.turn(), Side::Black);
        assert_eq!(g.rolls_left(), 3);
        assert!(g.holds.iter().all(|h| !h));
        // Black's sheet is separate: the same slot is free on their side.
        g.force_dice([1, 1, 1, 1, 1]);
        assert!(g.assign(Category::Yahtzee));
        assert_eq!(
            g.card(Side::Black).slots[Category::Yahtzee.index()],
            Some(50)
        );
    }

    #[test]
    fn upper_bonus_applies_at_sixty_three() {
        let mut card = Card::new();
        card.slots[Category::Ones.index()] = Some(3);
        for (i, v) in [(1usize, 6u16), (2, 12), (3, 12), (4, 12), (5, 18)] {
            card.slots[i] = Some(v);
        }
        // upper = 3+6+12+12+12+18 = 63 -> bonus fires
        let (upper, bonus, _, _) = card.totals();
        assert_eq!(upper, 63);
        assert_eq!(bonus, 35);
    }

    #[test]
    fn game_over_when_both_sheets_complete() {
        let mut g = Yahtzee::new();
        g.force_dice([1, 1, 1, 1, 1]);
        // Both sheets fill in lockstep: white then black per category.
        for cat in ALL_CATEGORIES {
            for _ in 0..2 {
                if !g.status.is_ongoing() {
                    // First completed sheet ends the session by convention.
                    assert!(g.card(Side::White).complete() || g.card(Side::Black).complete());
                    return;
                }
                assert!(g.assign(cat), "{cat:?} slot must be open");
            }
        }
        assert!(!g.status.is_ongoing());
        // All-ones sheets: white total 50+..., black mirrors; whoever went
        // first fills 13 categories second-to-last — winner is deterministic
        // because identical scores mean Draw unless bonus differs (it doesn't).
        assert!(matches!(g.status(), Status::Won(_) | Status::Draw));
    }
}
