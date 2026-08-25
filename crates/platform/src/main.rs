//! The smartasbrain shell: owns the terminal, the 60 FPS loop, the tab bar
//! and input routing. Games are plugins implementing [`game_core::Game`].

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
        Box::new(sudoku::App::new()),
        Box::new(lobby::Lobby::new()),
    ]);

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
            if route_event(&mut shell, event::read()?) {
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
            shell.active().wants_quit()
        }
        Event::Mouse(mouse) => {
            if shell.handle_mouse(mouse) {
                return false;
            }
            shell.active_mut().handle_mouse(mouse);
            shell.active().wants_quit()
        }
        _ => false,
    }
}

/// `[` / `]` cycle tabs; Alt+1..9 jumps directly.
fn tab_nav_key(key: &crossterm::event::KeyEvent) -> Option<TabNav> {
    match key.code {
        KeyCode::Char('[') => Some(TabNav::Prev),
        KeyCode::Char(']') => Some(TabNav::Next),
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::ALT) => ('1'..='9')
            .contains(&c)
            .then(|| TabNav::Index((c as u8 - b'1') as usize)),
        _ => None,
    }
}
