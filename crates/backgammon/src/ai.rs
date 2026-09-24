//! Heuristic opponent brain: proposes one legal checker move at a time.
//!
//! [`step`] is the whole API. Given the live engine state it enumerates legal
//! `(source, destination, die)` triples via `movable()`/`destinations()`,
//! scores each candidate by evaluating the position after applying it on a
//! clone, and returns the pick for the caller to feed back through
//! `move_checker`. Returning `None` means "no progress possible" — the caller
//! then lets the engine's auto-pass logic end the turn.
//!
//! Difficulty tiers:
//! - Easy occasionally plays a uniformly random legal move, otherwise samples
//!   uniformly from the top-5 scored moves;
//! - Medium plays greedy argmax every step;
//! - Hard brute-forces all legal move sequences for the whole remaining roll
//!   (DFS over clones) and plays the first move of the best line, falling back
//!   to Medium when its node budget is exhausted.
//!
//! No ratatui/crossterm here; `rand` is only used for Easy's mistakes.

use crate::engine::{Backgammon, Dest, Phase, Side};
use game_core::Difficulty;
use rand::Rng;

/// Hard stops expanding its search tree after this many visited states and
/// falls back to Medium's greedy pick.
const HARD_NODE_CAP: usize = 4000;
/// Chance Easy ignores its evaluation entirely and plays any legal move.
const EASY_BLUNDER_RATE: f64 = 0.3;
/// Outside blunders Easy samples uniformly from its best-N scored moves.
const EASY_SHORTLIST: usize = 5;

// Evaluation weights.
/// Owning a point (2+ checkers), anywhere.
const W_MADE_POINT: i64 = 4;
/// Extra for a made point on the side's own home board.
const W_MADE_HOME_EXTRA: i64 = 8;
/// A lone checker deep in the opponent's home board is prime hit bait.
const W_BLOT_OPP_HOME: i64 = 18;
/// Blots in the opponent's outer quadrant; flat cost, no shot analysis.
const W_BLOT_OPP_OUTER: i64 = 12;
/// Blots elsewhere (own outer + own home).
const W_BLOT_ELSEWHERE: i64 = 9;
/// Per opponent checker knocked to the bar by this move.
const W_HIT: i64 = 14;
/// Per own checker borne off by this move.
const W_BEAR_OFF: i64 = 22;

/// Next single checker move for the side to play, or `None` when nothing is
/// playable. Called once per AI action slot; apply the result with
/// `move_checker` and call again until the turn ends or this returns `None`.
pub fn step(
    state: &Backgammon,
    difficulty: Difficulty,
    rng: &mut impl Rng,
) -> Option<(usize, Dest)> {
    if state.phase() != Phase::Move {
        return None;
    }
    match difficulty {
        Difficulty::Easy => easy_step(state, rng),
        Difficulty::Medium => greedy_step(state),
        // Cap hit / no line found: greedy is always available as a floor.
        Difficulty::Hard => hard_step(state).or_else(|| greedy_step(state)),
        Difficulty::Expert => hard_step(state).or_else(|| greedy_step(state)),
    }
}

// -- move selection ---------------------------------------------------------

/// All `(source, destination, die)` triples currently playable.
fn candidates(state: &Backgammon) -> Vec<(usize, Dest, u8)> {
    let mut out = Vec::new();
    for src in state.movable() {
        for (dest, die) in state.destinations(src) {
            out.push((src, dest, die));
        }
    }
    out
}

/// Every legal candidate with the evaluation of the position after playing it.
fn scored(state: &Backgammon) -> Vec<(i64, usize, Dest)> {
    let mut out = Vec::new();
    for (src, dest, _) in candidates(state) {
        let mut next = state.clone();
        if next.move_checker(src, dest).is_err() {
            continue;
        }
        let value = positional_eval(&next, state.turn()) + step_delta(state, &next, state.turn());
        out.push((value, src, dest));
    }
    out
}

fn easy_step(state: &Backgammon, rng: &mut impl Rng) -> Option<(usize, Dest)> {
    let mut list = scored(state);
    if list.is_empty() {
        return None;
    }
    if rng.gen_bool(EASY_BLUNDER_RATE) {
        let i = rng.gen_range(0..list.len());
        return Some((list[i].1, list[i].2));
    }
    list.sort_by_key(|a| std::cmp::Reverse(a.0));
    let top = list.len().min(EASY_SHORTLIST);
    let i = rng.gen_range(0..top);
    Some((list[i].1, list[i].2))
}

fn greedy_step(state: &Backgammon) -> Option<(usize, Dest)> {
    let list = scored(state);
    list.into_iter()
        .max_by_key(|(value, _, _)| *value)
        .map(|(_, src, dest)| (src, dest))
}

/// First move of the best full-roll line, or `None` when the node cap was hit
/// before every root candidate got a fair look.
fn hard_step(state: &Backgammon) -> Option<(usize, Dest)> {
    let side = state.turn();
    let mut nodes = 0usize;
    let mut best: Option<(i64, usize, Dest)> = None;
    for (src, dest, _) in candidates(state) {
        let mut next = state.clone();
        if next.move_checker(src, dest).is_err() {
            continue;
        }
        let delta = step_delta(state, &next, side);
        let value = search_remaining(&next, side, delta, &mut nodes);
        if best.is_none_or(|(b, _, _)| value > b) {
            best = Some((value, src, dest));
        }
        if nodes > HARD_NODE_CAP {
            return None;
        }
    }
    best.map(|(_, src, dest)| (src, dest))
}

/// Best total score reachable from `state` by playing out the remaining dice
/// (`acc` carries hit/bear-off bonuses already banked along the way). Only
/// `side` moves until the turn ends, so this is a plain max over sequences.
/// Returns `i64::MIN` as an inert "budget blown" sentinel that never enters
/// arithmetic — [`hard_step`] discards the whole search when the cap trips.
fn search_remaining(state: &Backgammon, side: Side, acc: i64, nodes: &mut usize) -> i64 {
    *nodes += 1;
    if *nodes > HARD_NODE_CAP {
        return i64::MIN;
    }
    let cands = candidates(state);
    if state.phase() != Phase::Move || cands.is_empty() {
        return acc + positional_eval(state, side);
    }
    let mut best = i64::MIN;
    for (src, dest, _) in cands {
        let mut next = state.clone();
        if next.move_checker(src, dest).is_err() {
            continue;
        }
        let child = search_remaining(&next, side, acc + step_delta(state, &next, side), nodes);
        if child > best {
            best = child;
        }
        if *nodes > HARD_NODE_CAP {
            return i64::MIN;
        }
    }
    best
}

// -- evaluation -------------------------------------------------------------

/// Static evaluation from `side`'s perspective: race lead plus structure.
fn positional_eval(state: &Backgammon, side: Side) -> i64 {
    let pip_edge = state.pip(side) as i64 - state.pip(side.opposite()) as i64;
    let mut s = -pip_edge;
    for idx in 0..24 {
        let v = state.points()[idx];
        if v == 0 || owner(v) != side {
            continue;
        }
        if v.unsigned_abs() >= 2 {
            s += W_MADE_POINT;
            if quadrant(side, idx) == 3 {
                s += W_MADE_HOME_EXTRA;
            }
        } else {
            s -= blot_cost(side, idx);
        }
    }
    s
}

/// Instant rewards of one applied move: hits and bear-offs. Compared between
/// the pre-move and post-move states so they stack with the static eval.
fn step_delta(before: &Backgammon, after: &Backgammon, side: Side) -> i64 {
    let opp = side.opposite();
    let hits = i64::from(bar_of(after, opp)) - i64::from(bar_of(before, opp));
    let my_offs = match side {
        Side::White => i64::from(after.off().0) - i64::from(before.off().0),
        Side::Black => i64::from(after.off().1) - i64::from(before.off().1),
    };
    W_HIT * hits.max(0) + W_BEAR_OFF * my_offs.max(0)
}

// -- geometry helpers -------------------------------------------------------

/// Quadrants counted along `side`'s direction of travel:
/// 0 = opponent's home board, 1 = opponent's outer board,
/// 2 = own outer board, 3 = own home board.
fn quadrant(side: Side, idx: usize) -> usize {
    match side {
        Side::White => idx / 6,
        Side::Black => 3 - idx / 6,
    }
}

fn blot_cost(side: Side, idx: usize) -> i64 {
    match quadrant(side, idx) {
        0 => W_BLOT_OPP_HOME,
        1 => W_BLOT_OPP_OUTER,
        _ => W_BLOT_ELSEWHERE,
    }
}

fn owner(v: i8) -> Side {
    if v > 0 { Side::White } else { Side::Black }
}

fn bar_of(state: &Backgammon, side: Side) -> u8 {
    match side {
        Side::White => state.bar().0,
        Side::Black => state.bar().1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn ai_steps_are_always_legal() {
        for seed in 1..=3u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut g = Backgammon::new();
            let mut applied = 0usize;
            for i in 0..2000u32 {
                match g.phase() {
                    Phase::Roll => {
                        let d1 = (i % 6) as u8 + 1;
                        let d2 = ((i * 3 + 1) % 6) as u8 + 1;
                        g.roll_with(d1, d2);
                    }
                    Phase::Move => {
                        let Some((src, dest)) = step(&g, Difficulty::Easy, &mut rng) else {
                            break;
                        };
                        assert!(
                            g.move_checker(src, dest).is_ok(),
                            "seed {seed} step {i}: {src} -> {dest:?} must be legal"
                        );
                        applied += 1;
                    }
                    Phase::GameOver => break,
                }
            }
            assert!(applied > 0, "seed {seed}: AI never moved");
        }
    }

    #[test]
    fn hard_beats_greedy_on_obvious_bearoff() {
        let mut pts = [0i8; 24];
        pts[20] = 3;
        pts[21] = 2;
        let run = |difficulty| {
            let mut g = Backgammon::staged(pts, (0, 0), (10, 0), Side::White, 4, 3);
            while g.phase() == Phase::Move {
                let mut rng = StdRng::seed_from_u64(7);
                let Some((src, dest)) = step(&g, difficulty, &mut rng) else {
                    break;
                };
                assert_eq!(
                    dest,
                    Dest::Off,
                    "must bear off instead of shuffling inside: {src} -> {dest:?}"
                );
                assert!(g.move_checker(src, dest).is_ok());
            }
            g.off().0
        };
        let hard_off = run(Difficulty::Hard);
        let medium_off = run(Difficulty::Medium);
        assert_eq!(hard_off, 12, "both dice must bear off under Hard");
        assert!(hard_off >= medium_off, "Hard never does worse than Medium");
    }

    #[test]
    fn easy_returns_something_when_moves_exist() {
        for seed in 0..32u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut g = Backgammon::new();
            g.roll_with(3, 1);
            assert!(!g.movable().is_empty(), "start position has black moves");
            assert!(
                step(&g, Difficulty::Easy, &mut rng).is_some(),
                "seed {seed}"
            );
            assert!(step(&g, Difficulty::Medium, &mut rng).is_some());
            assert!(step(&g, Difficulty::Hard, &mut rng).is_some());
            assert!(step(&g, Difficulty::Expert, &mut rng).is_some());
        }
    }
}
