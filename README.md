# smartasbrain

A terminal arcade for games — and eventually, for the AIs that play them.
Built with [ratatui](https://ratatui.rs) at a steady 60 FPS.

```
┌─[ lobby ]──[ sudoku ]─────────────────────────────────────┐
│                                                           │
│                   S M A R T A S B R A I N                 │
│        an arcade for games - and for the ais that         │
│                      play them                            │
│                                                           │
```

## Games

| Game      | Status  |
| --------- | ------- |
| Lobby     | shipped |
| Sudoku    | shipped — generator with MRV uniqueness prover, pencil marks, hints, undo/redo, pause, personal bests |
| Go        | planned |
| AI Battleground | planned — AIs competing against each other in real time |

## Run

```sh
cargo run --release -p platform
```

Requires a terminal with mouse support. macOS/Linux; Windows Terminal works too.

## Shell controls

| Key                | Action                    |
| ------------------ | ------------------------- |
| `[` / `]`          | previous / next game tab  |
| `Alt+1`..`Alt+9`   | jump to tab               |
| click a tab        | switch games              |
| `Ctrl+Q` / `Ctrl+C`| quit                      |

### Sudoku controls

| Key                  | Action                          |
| -------------------- | ------------------------------- |
| arrows / `h j k l`   | move cursor                     |
| `1-9` / click numpad | place digit                     |
| `z` or `Tab`         | toggle notes (pencil marks)     |
| `0` / Backspace      | erase                           |
| `Space`              | hint                            |
| `u` / `Ctrl+R`       | undo / redo                     |
| `r`                  | restart same puzzle             |
| `p`                  | pause (hides your entries)      |
| `n`                  | new puzzle menu                 |
| right-click          | erase cell under cursor         |

Personal best times persist per difficulty in
`~/Library/Application Support/sudoku/` (macOS) or `$XDG_DATA_HOME/sudoku/`.

## Architecture

Cargo workspace, three crates:

```
crates/
├── game-core   Game trait + shell contracts
├── sudoku      the game as a plugin (pure lib)
└── platform    smartasbrain binary: tab bar, input router, 60 FPS loop
```

Every game implements `game_core::Game`:

```rust
pub trait Game {
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn handle_key(&mut self, key: KeyEvent);
    fn handle_mouse(&mut self, mouse: MouseEvent);
    fn tick(&mut self);                       // AI turns, animations, timers
    fn draw(&mut self, frame: &mut Frame, area: Rect);
    fn wants_quit(&self) -> bool;
}
```

The shell runs a fixed-timestep loop (~60 FPS): drain all pending input per
frame without blocking past the deadline, tick every game, then draw once.
Ratatui's diffing backend writes only changed cells, so idle frames are nearly
free. Quit intent is sampled only from the game that just received input, so
switching tabs never inherits a stale request.

The future AI battleground is "just" another plugin whose `tick()` advances
AI players — the shell already gives it a real-time frame budget.

## Development

```sh
cargo test --workspace       # 28 tests incl. frame-cost regression guard
cargo clippy --workspace --all-targets
```
