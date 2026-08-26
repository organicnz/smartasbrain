//! Yahtzee AI: instinct for Easy, mode-face holds for Medium, sampled
//! expected-value hold selection for Hard.

use crate::engine::{ALL_CATEGORIES, CATS, Category, DICE, Yahtzee};
use game_core::Difficulty;
use rand::Rng;

/// Holds to apply, then roll.
pub fn holds_before_roll(game: &Yahtzee, difficulty: Difficulty) -> [bool; DICE] {
    match difficulty {
        Difficulty::Easy => [false; DICE],
        Difficulty::Medium => mode_face_holds(game),
        Difficulty::Hard => sampled_holds(game, 200, &mut rand::thread_rng()),
    }
}

/// Category to write the current dice into.
pub fn best_category(game: &Yahtzee, difficulty: Difficulty) -> Option<Category> {
    let open: Vec<Category> = open_slots(game);
    if open.is_empty() {
        return None;
    }
    match difficulty {
        // Random slot — throws points away freely.
        Difficulty::Easy => {
            let mut rng = rand::thread_rng();
            use rand::Rng;
            let pick = rng.gen_range(0..open.len());
            open.get(pick).copied()
        }
        _ => {
            let me = game.turn();
            open.into_iter().max_by_key(|&c| {
                crate::engine::score_of(c, game.dice())
                    + u16::from(
                        c.is_upper()
                            && game.card(me).totals().0 + crate::engine::score_of(c, game.dice())
                                >= 63,
                    ) * 20 // nudge toward securing the bonus
            })
        }
    }
}

fn open_slots(game: &Yahtzee) -> Vec<Category> {
    let card = game.card(game.turn());
    ALL_CATEGORIES
        .iter()
        .copied()
        .filter(|c| !card.filled(*c))
        .collect()
}

/// Keep every die matching the most common face (pairs upward).
fn mode_face_holds(game: &Yahtzee) -> [bool; DICE] {
    let mut counts = [0_u8; 7];
    for &d in game.dice() {
        counts[d as usize] += 1;
    }
    let best_face = (1..=6).max_by_key(|f| counts[*f as usize]).unwrap_or(1) as u8;
    let keep_straight = straight_progress(game.dice());
    let mut holds = [false; DICE];
    if counts[best_face as usize] >= 2 || keep_straight {
        for (i, &d) in game.dice().iter().enumerate() {
            if (d == best_face && counts[best_face as usize] >= 2)
                || (keep_straight && counts[d as usize] == 1)
            {
                holds[i] = true;
            }
        }
    }
    holds
}

/// True when the dice form four distinct ascending faces (chase big straits).
fn straight_progress(dice: &[u8; DICE]) -> bool {
    let mut seen = [false; 7];
    for &d in dice {
        seen[d as usize] = true;
    }
    (1..=6).filter(|&f| seen[f]).count() >= 4
}

/// Candidate hold patterns: all-dice-of-one-face, pairs of faces, and the
/// straight chase; scored by sampling fresh rolls for the rest.
fn sampled_holds(game: &Yahtzee, samples: u32, rng: &mut impl Rng) -> [bool; DICE] {
    let mut candidates: Vec<[bool; DICE]> = Vec::new();
    candidates.push([false; DICE]);
    for face in 1..=6_u8 {
        let mut h = [false; DICE];
        for (i, &d) in game.dice().iter().enumerate() {
            h[i] = d == face;
        }
        candidates.push(h);
    }
    for a in 1..=6_u8 {
        for b in (a + 1)..=6_u8 {
            let mut h = [false; DICE];
            for (i, &d) in game.dice().iter().enumerate() {
                h[i] = d == a || d == b;
            }
            candidates.push(h);
        }
    }
    candidates.push(mode_face_holds(game));

    let mut best = [false; DICE];
    let mut best_ev = -1_i32;
    for cand in candidates {
        if cand.iter().all(|h| *h) {
            continue; // never hold everything mid-turn
        }
        let ev = sample_expected_value(game, &cand, samples, rng);
        if ev > best_ev {
            best_ev = ev;
            best = cand;
        }
    }
    best
}

fn sample_expected_value(
    game: &Yahtzee,
    holds: &[bool; DICE],
    samples: u32,
    rng: &mut impl Rng,
) -> i32 {
    let mut total = 0_i64;
    for _ in 0..samples {
        let mut trial = *game.dice();
        for (i, &held) in holds.iter().enumerate() {
            if !held {
                trial[i] = rng.gen_range(1..=6);
            }
        }
        let best_slot = open_slots(game)
            .iter()
            .map(|c| crate::engine::score_of(*c, &trial) as i64)
            .max()
            .unwrap_or(0);
        total += best_slot;
    }
    (total / samples.max(1) as i64) as i32
}

/// Number of open slots — handy for glue assertions.
#[allow(dead_code)]
pub fn open_count(game: &Yahtzee) -> usize {
    CATS - open_slots(game).len()
}
