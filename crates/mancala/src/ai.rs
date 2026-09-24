//! Mancala AI: random-with-instinct for Easy, greedy sowing for Medium,
//! shallow minimax with extra-turn recursion for Hard.

use crate::engine::{Mancala, Side, Status};
use game_core::Difficulty;

pub fn best_move(game: &Mancala, difficulty: Difficulty) -> Option<usize> {
    let legal = game.legal_pits();
    if legal.is_empty() {
        return None;
    }
    match difficulty {
        Difficulty::Easy => easy(game, &legal),
        Difficulty::Medium => greedy(game, &legal),
        Difficulty::Hard => search_root(game, &legal),
        Difficulty::Expert => search_root(game, &legal),
    }
}

/// Random, but takes a visible free extra turn half the time.
fn easy(game: &Mancala, legal: &[usize]) -> Option<usize> {
    let extra: Vec<usize> = legal
        .iter()
        .copied()
        .filter(|&p| {
            let landing = (p + game.pit(p) as usize) % crate::engine::STORES;
            landing == side_store_for_landing(game.turn())
        })
        .collect();
    let mut rng = rand::thread_rng();
    use rand::Rng;
    if !extra.is_empty() && rng.gen_bool(0.5) {
        let pick = rng.gen_range(0..extra.len());
        return extra.get(pick).copied();
    }
    let pick = rng.gen_range(0..legal.len());
    legal.get(pick).copied()
}

/// Landing on your own store index accounting for the skip handled by sow.
fn side_store_for_landing(side: Side) -> usize {
    side.store()
}

/// Greedy one-ply: store delta + captures*2 + extra-turn bonus.
fn greedy(game: &Mancala, legal: &[usize]) -> Option<usize> {
    legal.iter().copied().max_by_key(|&p| {
        let mut child = game.clone();
        let before = child.store_of(game.turn());
        let _ = child.sow(p);
        let after = child.store_of(game.turn());
        let captured = child.captured_last().map(|(_, n)| n).unwrap_or(0);
        (after - before) as i32
            + captured as i32 * 2
            + if child.turn() == game.turn() { 8 } else { 0 }
    })
}

/// Depth-limited minimax from `me`'s store perspective; extra turns keep
/// the same player at the node (no negation).
fn search_root(game: &Mancala, legal: &[usize]) -> Option<usize> {
    const DEPTH: u8 = 3;
    let me = game.turn();
    let mut best: Option<(usize, i32)> = None;
    for &p in legal {
        let mut child = game.clone();
        if !child.sow(p) {
            continue;
        }
        let score = -negamax(&child, DEPTH - 1, me);
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((p, score));
        }
    }
    best.map(|(p, _)| p)
}

fn negamax(game: &Mancala, depth: u8, me: Side) -> i32 {
    if !game.status().is_ongoing() || depth == 0 {
        return terminal_or_eval(game, me);
    }
    // Score is always reported from `me`; recursion sign depends on who acts.
    let acting_is_me = game.turn() == me;
    let legal = game.legal_pits();
    if legal.is_empty() {
        return terminal_or_eval(game, me);
    }
    let mut best = if acting_is_me { i32::MIN } else { i32::MAX };
    for &p in &legal {
        let mut child = game.clone();
        if !child.sow(p) {
            continue;
        }
        let score = negamax(&child, depth - 1, me);
        best = if acting_is_me {
            best.max(score)
        } else {
            best.min(score)
        };
    }
    best
}

fn terminal_or_eval(game: &Mancala, me: Side) -> i32 {
    let mine = game.store_of(me) as i32;
    let theirs = game.store_of(me.opposite()) as i32;
    let diff = (mine - theirs) * 3;
    if game.status().is_ongoing() {
        return diff;
    }
    match game.status() {
        Status::Won(w) if w == me => diff + 10_000,
        Status::Draw => diff,
        _ => diff - 10_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{STORES, Side as S};

    /// Plays Easy-vs-Easy until finished; every sow must be legal.
    #[test]
    fn easy_self_play_stays_legal_until_finished() {
        use rand::{SeedableRng, rngs::StdRng};
        for seed in 0..6_u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut g = Mancala::new();
            let mut plies = 0_u32;
            while g.status().is_ongoing() {
                let legal = g.legal_pits();
                let Some(pick) = best_move(&g, Difficulty::Easy) else {
                    break;
                };
                assert!(legal.contains(&pick), "illegal ai sow {pick}");
                assert!(g.sow(pick));
                plies += 1;
                assert!(plies < 400, "game must terminate");
            }
            let _ = &mut rng;
        }
    }

    #[test]
    fn medium_takes_free_extra_turn() {
        // Pit 2 with exactly 4 seeds lands in White's store.
        let mut g = Mancala::new();
        for i in 0..STORES {
            g.force_pit(i, if i == 2 { 4 } else { 0 });
        }
        assert_eq!(best_move(&g, Difficulty::Medium), Some(2));
    }

    #[test]
    fn finished_game_yields_no_move() {
        let mut g = Mancala::new();
        for i in 0..STORES {
            g.force_pit(i, 0);
        }
        g.finish_into_win(S::White);
        assert_eq!(best_move(&g, Difficulty::Hard), None);
    }
}
