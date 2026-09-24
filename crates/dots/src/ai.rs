//! Dots & Boxes AI: gift-avoiding heuristics with a shallow lookahead for
//! Hard. Full chain-parity theory is deliberately out of scope.

use crate::engine::{Dots, Side, Status, box_edges, decompose};
use game_core::Difficulty;

pub fn best_edge(game: &Dots, difficulty: Difficulty) -> Option<usize> {
    let open = game.open_edges();
    if open.is_empty() {
        return None;
    }
    match difficulty {
        Difficulty::Easy => easy(game, &open),
        Difficulty::Medium => medium(game, &open),
        Difficulty::Hard => hard(game, &open),
        Difficulty::Expert => hard(game, &open),
    }
}

/// Edges that complete at least one box right now.
fn completions(game: &Dots, open: &[usize]) -> Vec<usize> {
    open.iter()
        .copied()
        .filter(|&e| would_complete(game, e))
        .collect()
}

fn would_complete(game: &Dots, e: usize) -> bool {
    adjacent_boxes(e).into_iter().flatten().any(|(r, c)| {
        box_edges(r, c)
            .iter()
            .filter(|&&x| x != e)
            .all(|&x| game.claimed(x).is_some())
    })
}

/// Boxes that this edge would bring to three sides (a future gift).
fn boxes_at_three(game: &Dots, e: usize) -> Vec<(usize, usize)> {
    adjacent_boxes(e)
        .into_iter()
        .flatten()
        .filter(|&(r, c)| {
            let claimed = box_edges(r, c)
                .iter()
                .filter(|&&x| x != e && game.claimed(x).is_some())
                .count();
            claimed == 2 // after taking `e` the box sits at three sides
        })
        .collect()
}

/// Up to two boxes touching an edge; None entries skipped.
fn adjacent_boxes(e: usize) -> [Option<(usize, usize)>; 2] {
    let (vert, row, col) = decompose(e);
    if vert {
        // Lazy closures keep the subtractions from running on the edge row.
        let a = (col > 0).then(|| (row, col - 1));
        let b = (col < crate::engine::BOXES).then_some((row, col));
        [a, b]
    } else {
        let a = (row > 0).then(|| (row - 1, col));
        let b = (row < crate::engine::BOXES).then_some((row, col));
        [a, b]
    }
}

fn safe_open(game: &Dots, open: &[usize]) -> Vec<usize> {
    open.iter()
        .copied()
        .filter(|&e| boxes_at_three(game, e).is_empty())
        .collect()
}

fn easy(game: &Dots, open: &[usize]) -> Option<usize> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let safe = safe_open(game, open);
    if !safe.is_empty() && rng.gen_bool(0.7) {
        let pick = rng.gen_range(0..safe.len());
        return safe.get(pick).copied();
    }
    let pick = rng.gen_range(0..open.len());
    open.get(pick).copied()
}

fn medium(game: &Dots, open: &[usize]) -> Option<usize> {
    let free = completions(game, open);
    if !free.is_empty() {
        return free.first().copied();
    }
    let safe = safe_open(game, open);
    if !safe.is_empty() {
        return safe.first().copied();
    }
    open.first().copied()
}

/// Hard = Medium plus a depth-2 minimax among the top candidates so it
/// avoids handing over chains when alternatives exist.
fn hard(game: &Dots, open: &[usize]) -> Option<usize> {
    let free = completions(game, open);
    if !free.is_empty() {
        return free.first().copied();
    }
    let candidates = {
        let safe = safe_open(game, open);
        if safe.is_empty() { open.to_vec() } else { safe }
    };
    let me = game.turn();
    let mut best: Option<(usize, i32)> = None;
    for &e in &candidates {
        let mut child = game.clone();
        if !child.play(e) {
            continue;
        }
        let score = -negamax(&child, 2, me);
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((e, score));
        }
    }
    best.map(|(e, _)| e).or_else(|| medium(game, open))
}

fn negamax(game: &Dots, depth: u8, me: Side) -> i32 {
    if !game.status().is_ongoing() || depth == 0 {
        return eval(game, me);
    }
    let acting_is_me = game.turn() == me;
    let open = game.open_edges();
    if open.is_empty() {
        return eval(game, me);
    }
    let mut best = if acting_is_me { i32::MIN } else { i32::MAX };
    for &e in &open {
        let mut child = game.clone();
        if !child.play(e) {
            continue;
        }
        let score = negamax(&child, depth - 1, me);
        best = if acting_is_me {
            best.max(score)
        } else {
            best.min(score)
        };
        if acting_is_me && best > 5_000 {
            break; // found a forced win line; no need to keep scanning
        }
    }
    if best == i32::MIN || best == i32::MAX {
        eval(game, me)
    } else {
        best
    }
}

fn eval(game: &Dots, me: Side) -> i32 {
    let diff = (game.score(me) as i32 - game.score(me.opposite()) as i32) * 10;
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
    use crate::engine::{EDGES, h_id, v_id};

    #[test]
    fn medium_takes_free_box_when_available() {
        let mut g = Dots::new();
        // Three sides of box (0,0): the completing edge is v_id(0,1).
        g.play(h_id(0, 0));
        g.play(h_id(1, 0));
        g.play(v_id(0, 0));
        assert_eq!(best_edge(&g, Difficulty::Medium), Some(v_id(0, 1)));
    }

    #[test]
    fn easy_self_play_never_claims_taken_edges() {
        use rand::{SeedableRng, rngs::StdRng};
        for seed in 0..6_u64 {
            let _ = StdRng::seed_from_u64(seed);
            let mut g = Dots::new();
            let mut plies = 0_u32;
            while g.status().is_ongoing() {
                let Some(e) = best_edge(&g, Difficulty::Easy) else {
                    break;
                };
                assert!(g.claimed(e).is_none(), "claimed edge replayed");
                assert!(g.play(e));
                plies += 1;
                assert!(plies <= EDGES as u32);
            }
        }
    }

    #[test]
    fn hard_avoids_handing_a_three_sided_box() {
        // One safe edge exists; the alternative gifts box (0,0) at once.
        let mut g = Dots::new();
        g.play(h_id(0, 0));
        g.play(h_id(1, 0));
        g.play(v_id(0, 0)); // box (0,0) now has three sides
        // Completing edge is v_id(0,1); Hard must take the free box itself.
        assert_eq!(best_edge(&g, Difficulty::Hard), Some(v_id(0, 1)));
    }

    #[test]
    fn finished_game_yields_no_move() {
        let mut g = Dots::new();
        while g.status().is_ongoing() {
            let open = g.open_edges();
            g.play(open[0]);
        }
        assert_eq!(best_edge(&g, Difficulty::Hard), None);
    }
}
