//! Dominoes AI: random for Easy, heaviest-shed for Medium, and a
//! flexibility-aware heuristic for Hard (shed pips while keeping suits that
//! stay playable).

use crate::engine::Dominoes;
use game_core::Difficulty;
use rand::Rng;

pub fn best_move(game: &Dominoes, difficulty: Difficulty) -> Option<usize> {
    let legal = game.legal_moves();
    if legal.is_empty() {
        return None;
    }
    let mut rng = rand::thread_rng();
    match difficulty {
        Difficulty::Easy => {
            let pick = rng.gen_range(0..legal.len());
            legal.get(pick).map(|&(i, _, _)| i)
        }
        Difficulty::Medium => legal
            .iter()
            .copied()
            .max_by_key(|&(i, _, _)| i32::from(game.hand(game.turn())[i].pips()))
            .map(|(i, _, _)| i),
        Difficulty::Hard => legal
            .iter()
            .copied()
            .max_by_key(|&(i, _, _)| hard_score(game, i))
            .map(|(i, _, _)| i),
    }
}

/// Pips shed now, plus a bonus for keeping the remaining hand diverse
/// against the new end values, plus a small nudge for dumping doubles.
fn hard_score(game: &Dominoes, idx: usize) -> i32 {
    let mut child = game.clone();
    if !child.play(idx) {
        return i32::MIN;
    }
    let mover = game.turn();
    let hand = child.hand(mover);
    let mut score = game.hand(mover)[idx].pips() as i32 * 2;
    score += i32::from(game.hand(mover)[idx].a == game.hand(mover)[idx].b) * 3;

    // End flexibility: for each open end, count how many distinct tiles in
    // the kept hand could still answer it next turn.
    let (Some(l), Some(r)) = child.ends() else {
        return score;
    };
    let answers = |v: u8| -> i32 { hand.iter().filter(|t| t.has(v)).count() as i32 };
    score += answers(l).min(4) + answers(r).min(4);
    score
}
