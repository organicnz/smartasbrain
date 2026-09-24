//! Keyboard and mouse handling for the sudoku game, routed by the platform
//! shell via the [`game_core::Game`] trait.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent};

use crate::app::{App, ClickAction, DIFFICULTIES, State};

/// Entry points used by the platform's [`game_core::Game`] dispatch.
pub fn route_key(app: &mut App, key: crossterm::event::KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('r' | 'R'))
    {
        app.redo();
        return;
    }
    match app.state {
        State::Menu => on_menu_key(app, key.code),
        State::Playing => on_play_key(app, key.code),
        State::Won => on_won_key(app, key.code),
    }
}

/// Entry point used by the platform's [`game_core::Game`] dispatch.
pub fn on_mouse(app: &mut App, mouse: MouseEvent) {
    let (col, row) = (mouse.column, mouse.row);
    match mouse.kind {
        crossterm::event::MouseEventKind::Down(MouseButton::Left) => {
            // Any left click while paused resumes the game.
            if app.state == State::Playing && app.paused {
                app.toggle_pause();
                return;
            }
            app.hover = Some((col, row));
            if let Some(action) = app.clickable_at(col, row) {
                app.activate(action);
            } else if let Some(cell) = playable_cell(app, col, row) {
                app.activate(ClickAction::Cell(cell));
            }
        }
        crossterm::event::MouseEventKind::Down(MouseButton::Right) => {
            if let Some(cell) = playable_cell(app, col, row) {
                app.cursor = cell;
                app.erase();
            }
        }
        crossterm::event::MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(cell) = playable_cell(app, col, row) {
                app.cursor = cell;
            }
        }
        crossterm::event::MouseEventKind::Moved => app.hover = Some((col, row)),
        // The wheel steers the cursor only during live play.
        crossterm::event::MouseEventKind::ScrollUp
            if app.state == State::Playing && !app.paused =>
        {
            app.move_by(-1, 0)
        }
        crossterm::event::MouseEventKind::ScrollDown
            if app.state == State::Playing && !app.paused =>
        {
            app.move_by(1, 0)
        }
        crossterm::event::MouseEventKind::ScrollLeft
            if app.state == State::Playing && !app.paused =>
        {
            app.move_by(0, -1)
        }
        crossterm::event::MouseEventKind::ScrollRight
            if app.state == State::Playing && !app.paused =>
        {
            app.move_by(0, 1)
        }
        _ => {}
    }
}

fn playable_cell(app: &App, col: u16, row: u16) -> Option<usize> {
    if app.state == State::Menu {
        return None;
    }
    app.cell_at(col, row)
}

fn on_menu_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q' | 'Q') | KeyCode::Esc => app.quit = true,
        KeyCode::Up | KeyCode::Left | KeyCode::Char('k' | 'h') => {
            app.difficulty = (app.difficulty + DIFFICULTIES.len() - 1) % DIFFICULTIES.len();
        }
        KeyCode::Down | KeyCode::Right | KeyCode::Char('j' | 'l') => {
            app.difficulty = (app.difficulty + 1) % DIFFICULTIES.len();
        }
        KeyCode::Enter | KeyCode::Char(' ') => app.start(),
        KeyCode::Char(c)
            if c.to_digit(10)
                .is_some_and(|d| (1..=DIFFICULTIES.len() as u32).contains(&d)) =>
        {
            app.difficulty = c.to_digit(10).unwrap() as usize - 1;
            app.start();
        }
        _ => {}
    }
}

fn on_play_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q' | 'Q') | KeyCode::Esc => app.quit = true,
        KeyCode::Left | KeyCode::Char('h') => app.move_by(0, -1),
        KeyCode::Right | KeyCode::Char('l') => app.move_by(0, 1),
        KeyCode::Up | KeyCode::Char('k') => app.move_by(-1, 0),
        KeyCode::Down | KeyCode::Char('j') => app.move_by(1, 0),
        KeyCode::Char(c) if ('1'..='9').contains(&c) => {
            app.place(c.to_digit(10).unwrap() as u8);
        }
        KeyCode::Char('0') | KeyCode::Backspace | KeyCode::Delete => app.erase(),
        KeyCode::Char('u' | 'U') => app.undo(),
        KeyCode::Char('r' | 'R') => app.restart(),
        KeyCode::Char('z' | 'Z') | KeyCode::Tab => app.toggle_notes_mode(),
        KeyCode::Char(' ') => app.hint(),
        KeyCode::Char('p' | 'P') => app.toggle_pause(),
        KeyCode::Char('n' | 'N') => app.open_menu(),
        _ => {}
    }
}

fn on_won_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q' | 'Q') | KeyCode::Esc => app.quit = true,
        KeyCode::Char('n' | 'N') => app.open_menu(),
        KeyCode::Char('r' | 'R') => app.start(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseEventKind};

    fn mouse(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn menu_accepts_expert_and_rejects_out_of_range_shortcuts() {
        let mut expert = App::new();
        expert.persist_records = false;
        on_menu_key(&mut expert, KeyCode::Char('4'));
        assert_eq!(expert.difficulty, 3);
        assert_eq!(expert.state, State::Playing);

        let mut invalid = App::new();
        invalid.persist_records = false;
        on_menu_key(&mut invalid, KeyCode::Char('5'));
        assert_eq!(invalid.difficulty, 0);
        assert_eq!(invalid.state, State::Menu);
    }

    #[test]
    fn scroll_moves_cursor_only_while_playing_and_awake() {
        let mut app = App::new();
        let before = app.cursor;
        on_mouse(&mut app, mouse(MouseEventKind::ScrollDown));
        assert_eq!(app.cursor, before, "menu state ignores the wheel");

        app.start();
        on_mouse(&mut app, mouse(MouseEventKind::ScrollDown));
        assert_ne!(app.cursor, before, "playing state steers with the wheel");

        app.toggle_pause();
        let frozen = app.cursor;
        for kind in [
            MouseEventKind::ScrollUp,
            MouseEventKind::ScrollDown,
            MouseEventKind::ScrollLeft,
            MouseEventKind::ScrollRight,
        ] {
            on_mouse(&mut app, mouse(kind));
        }
        assert_eq!(app.cursor, frozen, "paused freezes the wheel");
    }
}
