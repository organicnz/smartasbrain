//! The smartasbrain shell: owns the terminal, the 60 FPS loop and input
//! routing. Games are plugins implementing [`game_core::Game`]; the lobby
//! doubles as the central DOOM-style menu and opens games via
//! [`Game::poll_navigation`].

mod lobby;
mod shell;

use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::shell::{Shell, TabNav};

/// Target frame period (~60 FPS). The terminal only receives cell diffs, so
/// steady-state redraws are cheap; the budget exists to keep input latency low
/// and AI-driven animation smooth.
const FRAME_BUDGET: Duration = Duration::from_millis(16);

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = run_app(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    let mut shell = Shell::new(vec![
        Box::new(lobby::Lobby::new()),
        Box::new(sudoku::App::new()),
        Box::new(go::GoGame::new()),
        Box::new(chess::ChessGame::new()),
        Box::new(checkers::CheckersGame::new()),
        Box::new(backgammon::BackgammonGame::new()),
        Box::new(reversi::ReversiGame::new()),
        Box::new(morris::MorrisGame::new()),
        Box::new(connect4::Connect4Game::new()),
        Box::new(mancala::MancalaGame::new()),
        Box::new(dots::DotsGame::new()),
        Box::new(yahtzee::YahtzeeGame::new()),
        Box::new(dominoes::DominoesGame::new()),
        Box::new(battleship::BattleshipGame::new()),
    ]);
    shell.resume_session();

    // Paint once immediately so startup never waits a frame.
    terminal.draw(|frame| shell.draw(frame))?;
    let mut frame_start = Instant::now();

    loop {
        // Phase 1: drain every pending input event without blocking past the
        // frame deadline. Bursts (e.g. mouse drags) collapse into one redraw.
        loop {
            let remaining = FRAME_BUDGET.saturating_sub(frame_start.elapsed());
            if !event::poll(remaining)? {
                break;
            }
            let quit_via_game = route_event(&mut shell, event::read()?);
            if quit_via_game || shell.quit_requested() {
                shell.persist_session();
                return Ok(());
            }
        }

        // Phase 2: tick every game, then draw once. Ratatui diffs the buffers
        // and writes only what changed.
        shell.tick_all();
        terminal.draw(|frame| shell.draw(frame))?;
        frame_start = Instant::now();
    }
}

/// Returns true when the application should exit. Quit intent is only ever
/// sampled from the game that just received input, so switching tabs never
/// inherits a stale request.
fn route_event(shell: &mut Shell, event: Event) -> bool {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            // Global bindings first.
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c' | 'q' | 'Q'))
            {
                return true;
            }
            if let Some(nav) = tab_nav_key(&key) {
                shell.switch_to(nav);
                return false;
            }
            shell.active_mut().handle_key(key);
            apply_navigation(shell);
            shell.active().wants_quit()
        }
        Event::Mouse(mouse) => {
            // Chrome first: the corner buttons outrank any game.
            if !shell.handle_mouse(mouse) {
                shell.active_mut().handle_mouse(mouse);
                apply_navigation(shell);
                return shell.active().wants_quit();
            }
            false
        }
        _ => false,
    }
}

/// Applies an out-of-band navigation request from the active game (the lobby
/// menu uses this to open a game).
fn apply_navigation(shell: &mut Shell) {
    if let Some(i) = shell.active_mut().poll_navigation() {
        shell.switch_to(TabNav::Index(i));
    }
}

/// `[` / `]` cycle tabs; Alt+1..9 jumps directly; Alt+Left pops history.
fn tab_nav_key(key: &crossterm::event::KeyEvent) -> Option<TabNav> {
    match key.code {
        KeyCode::Char('[') => Some(TabNav::Prev),
        KeyCode::Char(']') => Some(TabNav::Next),
        KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => Some(TabNav::Back),
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::ALT) => ('1'..='9')
            .contains(&c)
            .then(|| TabNav::Index((c as u8 - b'1') as usize)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lobby::Lobby;
    use crossterm::event::{MouseButton, MouseEventKind};
    use game_core::Game;
    use ratatui::backend::TestBackend;

    #[test]
    fn clicking_a_lobby_row_routes_to_that_game_tab() {
        // Measure the menu geometry on a standalone lobby first: the shell
        // hands the active game the exact same full-screen area.
        let mut lobby = Lobby::new();
        let backend = TestBackend::new(80, 24);
        let mut measure = ratatui::Terminal::new(backend).unwrap();
        measure.draw(|f| lobby.draw(f, f.area())).unwrap();
        let row = lobby.row_rect(2).expect("menu drawn at 80x24");

        let mut shell = Shell::new(vec![
            Box::new(lobby),
            Box::new(sudoku::App::new()),
            Box::new(go::GoGame::new()),
            Box::new(chess::ChessGame::new()),
            Box::new(checkers::CheckersGame::new()),
            Box::new(backgammon::BackgammonGame::new()),
        ]);
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| shell.draw(f)).unwrap();

        let quit = route_event(
            &mut shell,
            Event::Mouse(crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: row.x + 2,
                row: row.y,
                modifiers: KeyModifiers::NONE,
            }),
        );
        assert!(!quit);
        assert_eq!(
            shell.active().id(),
            "chess",
            "third menu entry opens the chess tab"
        );
    }

    #[test]
    fn clicks_off_menu_reach_the_active_game_without_navigating() {
        let mut shell = Shell::new(vec![
            Box::new(Lobby::new()),
            Box::new(sudoku::App::new()),
            Box::new(go::GoGame::new()),
            Box::new(chess::ChessGame::new()),
            Box::new(checkers::CheckersGame::new()),
            Box::new(backgammon::BackgammonGame::new()),
        ]);
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| shell.draw(f)).unwrap();

        let quit = route_event(
            &mut shell,
            Event::Mouse(crossterm::event::MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 79,
                row: 23,
                modifiers: KeyModifiers::NONE,
            }),
        );
        assert!(!quit);
        assert_eq!(
            shell.active().id(),
            "lobby",
            "a click outside every hit rect never switches tabs"
        );
    }
}
