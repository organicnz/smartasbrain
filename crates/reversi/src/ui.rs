//! Rendering and hit-testing for the reversi board and info panel.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use super::Button;
use super::engine::{Reversi, Side, Status, sq};

const BG: Color = Color::Rgb(13, 15, 23);
/// Green-black felt the discs sit on.
const BOARD_BG: Color = Color::Rgb(26, 36, 30);
/// Faint lattice rails between cells.
const GRID_FG: Color = Color::Rgb(70, 88, 76);
const BORDER_FG: Color = Color::Rgb(86, 116, 98);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const WHITE_DISC_FG: Color = Color::Rgb(226, 230, 244);
const BLACK_DISC_FG: Color = Color::Rgb(140, 150, 175);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const CROSS_BG: Color = Color::Rgb(44, 50, 68);
/// Subtle gold wash marking the AI's previous move.
const LAST_MOVE_BG: Color = Color::Rgb(72, 62, 30);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);
const HOVER_BG: Color = Color::Rgb(48, 54, 74);

// Setup-menu palette, borrowed from the lobby's forge-fire look.
const FIRE: Color = Color::Rgb(178, 34, 34);
const BONE: Color = Color::Rgb(216, 208, 194);
const SETUP_SELECT_BG: Color = Color::Rgb(88, 16, 14);
const SETUP_HOVER_BG: Color = Color::Rgb(54, 22, 18);

/// Board occupies 16 columns (8 two-char cells) and 8 rows inside its block.
const BOARD_INNER_W: u16 = 16;
const BOARD_INNER_H: u16 = 8;
const BOARD_BLOCK_H: u16 = BOARD_INNER_H + 2;
const PANEL_W: u16 = 32;
/// score / status / mode, plus the button row along the bottom.
const PANEL_H: u16 = 6;

const DISC: char = '\u{25cf}';
const RING: char = '\u{25cb}';
/// Legal-placement marker shown for the side a human controls.
const DOT: char = '\u{00b7}';
/// Vertical rail drawn between neighbouring cells.
const RAIL: char = '\u{2502}';

/// Cursor/hover/AI-trail snapshot handed to [`draw`] each frame.
pub struct Interaction {
    pub cursor: usize,
    pub hover: Option<(u16, u16)>,
    /// Square of the AI's latest move, tinted gold.
    pub last_move: Option<usize>,
    /// Panel mode line ("two players" / "vs ai …" / "ai duel …").
    pub mode: String,
    /// Side currently deliberating, rendered as its glyph + "thinking...".
    /// During a duel this alternates with the turn every frame.
    pub thinking: Option<Side>,
    /// Legal-move dots are shown only while a human owns the turn; they are
    /// hidden during AI turns and duels to keep automated play clean.
    pub show_legal_moves: bool,
}

/// Screen geometry captured during draw so clicks map to squares.
#[derive(Clone, Copy)]
pub struct Geom {
    x0: u16,
    y0: u16,
    /// Columns per square (resizable canvas picks this per frame).
    stride: u16,
    /// Rows per square.
    cell_h: u16,
}

impl Geom {
    fn new(x0: u16, y0: u16, stride: u16, cell_h: u16) -> Self {
        Self {
            x0,
            y0,
            stride,
            cell_h: cell_h.max(1),
        }
    }

    /// Map a click to its square; every column of a wide cell agrees.
    pub fn cell_at(&self, col: u16, row: u16) -> Option<usize> {
        let stride = self.stride.max(1);
        let cell_h = self.cell_h.max(1);
        if row < self.y0 || col < self.x0 {
            return None;
        }
        let (dr, dc) = ((row - self.y0) as usize, (col - self.x0) as usize);
        let (r, c) = (dr / cell_h.max(1) as usize, dc / stride.max(1) as usize);
        if r >= BOARD_INNER_H as usize || c >= 8 {
            return None;
        }
        Some(sq(r, c))
    }
}

fn side_glyph(side: Side) -> char {
    match side {
        Side::Black => DISC,
        Side::White => RING,
    }
}

pub fn draw(
    frame: &mut Frame,
    state: &Reversi,
    area: Rect,
    inter: &Interaction,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    // Resizable canvas: roomiest near-square cells that fit, else stand
    // down (also covers 0x0 ptty startup).
    let ladder = game_core::geom::proportional_ladder(8, 8, 9, 4);
    let Some(canvas) = game_core::geom::fit_canvas(area, 8, 8, 1, &ladder) else {
        *geom_out = None;
        return;
    };
    let block_w = game_core::geom::board_width(8, canvas.cell_w) + 2;
    let block_h = game_core::geom::board_height(8, canvas.cell_h) + 2;

    let side_by_side = area.width >= block_w + 2 + PANEL_W && area.height > block_h + PANEL_H;
    let cluster_w = if side_by_side {
        block_w + 2 + PANEL_W
    } else {
        block_w.max(PANEL_W)
    };
    let cluster_h = if side_by_side {
        block_h
    } else {
        block_h + 1 + PANEL_H
    };

    let cluster =
        game_core::geom::centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
    let cols = Layout::horizontal([
        Constraint::Length(block_w),
        Constraint::Length(2),
        Constraint::Length(PANEL_W),
    ])
    .split(cluster);

    draw_board(
        frame,
        cols[0],
        state,
        inter,
        geom_out,
        canvas.cell_w,
        canvas.cell_h,
    );

    let panel = if side_by_side {
        cols[2]
    } else {
        let panel_h = PANEL_H.min(area.height.saturating_sub(BOARD_BLOCK_H));
        Rect {
            x: cluster.x,
            y: cluster.y + BOARD_BLOCK_H + 1,
            width: PANEL_W.min(area.width),
            height: panel_h,
        }
    };
    draw_panel(frame, panel, state, inter, buttons);
}

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    state: &Reversi,
    inter: &Interaction,
    geom_out: &mut Option<Geom>,
    stride: u16,
    cell_h: u16,
) {
    let footer = match state.status() {
        Status::Won(winner) => format!("{} {} wins ", side_glyph(winner), winner.name()),
        Status::Draw => "draw ".to_string(),
        Status::Ongoing => format!(
            "{} {} to play ",
            side_glyph(state.turn()),
            state.turn().name()
        ),
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " reversi ",
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(footer, Style::default().fg(DIM_FG))).right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    *geom_out = Some(Geom::new(inner.x, inner.y, stride, cell_h));

    let legal = if inter.show_legal_moves {
        state.legal_moves()
    } else {
        Vec::new()
    };

    let mut lines =
        Vec::with_capacity(game_core::geom::board_height(BOARD_INNER_H, cell_h) as usize);
    for r in 0..BOARD_INNER_H as usize {
        // Spacer lines between tall square bands keep the grid airy.
        for _ in 0..(cell_h - 1) {
            if r > 0 {
                lines.push(Line::from(vec![Span::raw(
                    " ".repeat(inner.width as usize),
                )]));
            }
        }
        let mut spans = Vec::with_capacity((BOARD_INNER_W as usize) * 2);
        for c in 0..8usize {
            let idx = sq(r, c);
            let bg = if idx == inter.cursor {
                CROSS_BG
            } else if inter.last_move == Some(idx) {
                LAST_MOVE_BG
            } else {
                BOARD_BG
            };
            let (glyph_text, mut style) = match state.board()[idx] {
                Some(side) => (
                    side_glyph(side).to_string(),
                    Style::default().fg(match side {
                        Side::Black => BLACK_DISC_FG,
                        Side::White => WHITE_DISC_FG,
                    }),
                ),
                None if legal.contains(&idx) => (DOT.to_string(), Style::default().fg(ACCENT_FG)),
                None => (" ".to_string(), Style::default().fg(DIM_FG)),
            };
            if state.board()[idx].is_some() {
                style = style.add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(
                format!("{glyph_text:^width$}", width = stride as usize),
                style.bg(bg),
            ));
            spans.push(Span::styled(
                RAIL.to_string(),
                Style::default().fg(GRID_FG).bg(bg),
            ));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    state: &Reversi,
    inter: &Interaction,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(PANEL_BORDER_FG))
        .style(Style::default().bg(PANEL_BG))
        .title(Line::from(Span::styled(
            " info ",
            Style::default().fg(ACCENT_FG),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let kv = |key: &str, value: Span<'static>| {
        Line::from(vec![
            Span::styled(format!("{key:<8}"), Style::default().fg(DIM_FG)),
            value,
        ])
    };

    let (black, white) = state.score();
    let mut score_line = vec![
        Span::styled("score    ", Style::default().fg(DIM_FG)),
        Span::styled(
            format!("{DISC}{black:02} {RING}{white:02}"),
            Style::default().fg(WHITE_DISC_FG),
        ),
    ];
    if let Some(side) = inter.thinking {
        score_line.push(Span::styled(
            format!("  {} {} thinking...", side_glyph(side), side.name()),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ));
    }

    let status_text = match state.status() {
        Status::Won(winner) => format!("{} wins", winner.name()),
        Status::Draw => "stalemate draw".to_string(),
        Status::Ongoing => "-".to_string(),
    };
    let status_style = if matches!(state.status(), Status::Ongoing) {
        Style::default().fg(DIM_FG)
    } else {
        Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD)
    };

    let lines = vec![
        Line::from(score_line),
        kv("status", Span::styled(status_text, status_style)),
        kv(
            "mode",
            Span::styled(inter.mode.clone(), Style::default().fg(DIM_FG)),
        ),
    ];
    frame.render_widget(Paragraph::new(lines), inner);

    // Button along the bottom of the panel.
    if area.height < 2 {
        return;
    }
    let rect = Rect {
        x: inner.x,
        y: area.y + area.height - 2,
        width: inner.width.max(1),
        height: 1,
    };
    let hovered = inter
        .hover
        .is_some_and(|(c, r)| game_core::geom::in_rect(rect, c, r));
    buttons.push((rect, Button::Restart));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " [ restart ] ",
            Style::default()
                .fg(ACCENT_FG)
                .bg(if hovered { HOVER_BG } else { BUTTON_BG }),
        )))
        .alignment(Alignment::Center),
        rect,
    );
}

/// Setup-menu geometry: fixed 36x11 box, one row per choice plus key hints.
const SETUP_W: u16 = 36;
const SETUP_H: u16 = 11;

/// Full-screen opponent picker shown instead of the board while active.
/// Registers each item's hit rect into `items_out` for click activation.
pub fn draw_setup(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    hover: Option<(u16, u16)>,
    items_out: &mut Vec<(Rect, usize)>,
) {
    items_out.clear();
    if area.width < SETUP_W || area.height < SETUP_H {
        return;
    }
    let popup = game_core::geom::centered_rect(area, SETUP_W, SETUP_H);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FIRE))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " reversi setup ",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    for (i, label) in super::SETUP_ITEMS.iter().enumerate() {
        let rect = Rect {
            x: inner.x,
            y: inner.y + i as u16,
            width: inner.width,
            height: 1,
        };
        items_out.push((rect, i));
        let hovered = hover.is_some_and(|(c, r)| game_core::geom::in_rect(rect, c, r));
        let style = if i == selected {
            Style::default()
                .fg(BONE)
                .bg(SETUP_SELECT_BG)
                .add_modifier(Modifier::BOLD)
        } else if hovered {
            Style::default().fg(BONE).bg(SETUP_HOVER_BG)
        } else {
            Style::default().fg(BONE)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(format!(" {label} "), style)))
                .alignment(Alignment::Center),
            rect,
        );
    }

    let hint_y = inner.y + super::SETUP_ITEMS.len() as u16 + 1;
    if hint_y < inner.bottom() {
        let hint = Rect {
            x: inner.x,
            y: hint_y,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "up/down \u{00b7} 1-7 \u{00b7} enter picks",
                Style::default().fg(DIM_FG),
            )))
            .alignment(Alignment::Center),
            hint,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::engine::{col, row};
    use super::*;
    use crate::ReversiGame;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use game_core::{Difficulty, Game};
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, h: u16) {
        for setup_open in [true, false] {
            let mut game = ReversiGame::new();
            if !setup_open {
                game.handle_key(key(KeyCode::Enter)); // TWO PLAYERS
            }
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| Game::draw(&mut game, f, f.area()))
                .unwrap();
        }
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [
            (0u16, 0u16),
            (20, 6),
            (40, 14),
            (80, 24),
            (120, 40),
            (200, 60),
        ] {
            render_at(w, h);
        }
    }

    #[test]
    fn click_mapping_round_trips() {
        let geom = Geom::new(2, 1, 2, 1);
        assert_eq!(geom.cell_at(2, 1), Some(sq(0, 0)));
        assert_eq!(geom.cell_at(3, 1), Some(sq(0, 0)), "second half of cell");
        assert_eq!(geom.cell_at(4, 1), Some(sq(0, 1)));
        assert_eq!(geom.cell_at(2, 2), Some(sq(1, 0)));
        assert_eq!(geom.cell_at(17, 8), Some(sq(7, 7)), "bottom-right cell");
        assert_eq!(geom.cell_at(18, 1), None, "past right edge");
        assert_eq!(geom.cell_at(2, 9), None, "past bottom edge");
        assert_eq!(geom.cell_at(1, 1), None, "left of origin");
        assert_eq!(geom.cell_at(2, 0), None, "above origin");
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn left_click(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    /// Screen coordinates of the left half of a square under `geom`.
    fn click_of(geom: &Geom, index: usize) -> (u16, u16) {
        (
            geom.x0 + geom.stride * col(index) as u16,
            geom.y0 + geom.cell_h * row(index) as u16,
        )
    }

    #[test]
    fn duel_menu_rows_selectable() {
        assert_eq!(
            crate::SETUP_ITEMS,
            [
                "TWO PLAYERS",
                "VS AI - EASY",
                "VS AI - MEDIUM",
                "VS AI - HARD",
                "AI DUEL - EASY",
                "AI DUEL - MEDIUM",
                "AI DUEL - HARD",
            ]
        );

        let mut game = ReversiGame::new();
        for digit in '5'..'8' {
            game.handle_key(key(KeyCode::Char(digit)));
            assert_eq!(
                game.setup_sel,
                digit.to_digit(10).unwrap() as usize - 1,
                "digit {digit} jumps to its row"
            );
        }
        assert_eq!(game.setup_sel, 6);
        game.handle_key(key(KeyCode::Enter)); // confirm AI DUEL - HARD
        assert!(!game.setup_open);
        assert_eq!(game.opponent, crate::Opponent::Battle(Difficulty::Hard));
    }

    #[test]
    fn duel_self_play_progresses_or_ends() {
        let mut game = ReversiGame::new();
        game.handle_key(key(KeyCode::Char('7'))); // AI DUEL - HARD
        game.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            game.opponent,
            crate::Opponent::Battle(Difficulty::Hard)
        ));

        let mut saw_both_colors_early = false;
        for frame in 0..600usize {
            Game::tick(&mut game);
            let (b, w) = game.state.score();
            if b > 0 && w > 0 && frame < 100 {
                saw_both_colors_early = true;
            }
            match game.state.status() {
                Status::Won(_) | Status::Draw => break,
                Status::Ongoing => {}
            }
        }
        assert!(saw_both_colors_early, "both sides placed discs early");
        assert!(
            matches!(game.state.status(), Status::Won(_) | Status::Draw),
            "duel terminated cleanly: {:?}",
            game.state.status()
        );
    }

    #[test]
    fn duel_input_inert() {
        let mut game = ReversiGame::new();
        game.handle_key(key(KeyCode::Char('5'))); // AI DUEL - EASY
        game.handle_key(key(KeyCode::Enter));
        assert!(!game.setup_open);
        Game::tick(&mut game);
        assert!(game.last_ai_move.is_some(), "black engine opened the duel");
        let snapshot = *game.state.board();

        game.cursor = sq(4, 4);
        game.handle_key(key(KeyCode::Enter)); // play attempt
        game.handle_key(key(KeyCode::Up)); // cursor drift attempt
        game.handle_key(key(KeyCode::Char(' '))); // play attempt
        game.handle_key(key(KeyCode::Char('r'))); // must NOT reset mid-duel
        assert_eq!(game.cursor, sq(4, 4), "cursor frozen while spectating");
        assert_eq!(game.state.board(), &snapshot, "keys left board alone");

        // Clicking a board square selects nothing and moves nothing.
        let backend = TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
        let geom = game.geom.expect("board drawn during duel");
        let (c, r) = click_of(&geom, sq(0, 0));
        game.handle_mouse(left_click(c, r));
        assert_eq!(game.state.board(), &snapshot, "click left board alone");
        assert_eq!(game.cursor, sq(4, 4), "click must not even steer");

        // The allowed spectate keys still respond.
        game.handle_key(key(KeyCode::Char('R')));
        assert_eq!(game.state.last_move(), None, "'R' restarts the duel");
        assert_eq!(game.state.turn(), Side::Black);
        game.handle_key(key(KeyCode::Char('m')));
        assert!(game.setup_open, "'m' reopens setup");
    }

    #[test]
    fn selecting_vs_ai_then_tick_makes_ai_reply() {
        let mut game = ReversiGame::new();
        game.handle_key(key(KeyCode::Down)); // highlight "VS AI - EASY"
        game.handle_key(key(KeyCode::Enter));
        assert!(!game.setup_open);
        assert_eq!(game.opponent, crate::Opponent::Ai(Difficulty::Easy));
        assert_eq!(game.ai_side, Side::Black);

        // Black (the AI) opens; wait for the human seat.
        for _ in 0..10 {
            Game::tick(&mut game);
            if game.state.turn() == Side::White {
                break;
            }
        }
        assert_eq!(game.state.turn(), Side::White, "AI took its opening move");

        // Human (white) steers the cursor onto a legal square via keys.
        let target = game.state.legal_moves()[0];
        let (tr, tc) = (row(target), col(target));
        let (cr, cc) = (row(game.cursor), col(game.cursor));
        for _ in cc..tc {
            game.handle_key(key(KeyCode::Right));
        }
        for _ in tc..cc {
            game.handle_key(key(KeyCode::Left));
        }
        for _ in cr..tr {
            game.handle_key(key(KeyCode::Down));
        }
        for _ in tr..cr {
            game.handle_key(key(KeyCode::Up));
        }
        assert_eq!(game.cursor, target);
        game.handle_key(key(KeyCode::Enter));
        assert_eq!(game.state.turn(), Side::Black, "human move registered");

        for _ in 0..100 {
            Game::tick(&mut game);
            if game.state.turn() == Side::White {
                break;
            }
        }
        assert_eq!(game.state.turn(), Side::White, "AI replied");
        let mv = game.last_ai_move.expect("AI reply recorded");
        assert_eq!(game.state.last_move(), Some(mv));
        assert!(game.state.board()[mv].is_some());
    }

    #[test]
    fn cursor_clamps_and_esc_quits_but_not_mid_setup() {
        let mut game = ReversiGame::new();
        game.handle_key(key(KeyCode::Enter)); // TWO PLAYERS

        game.cursor = sq(0, 0);
        game.handle_key(key(KeyCode::Up));
        game.handle_key(key(KeyCode::Left));
        assert_eq!(game.cursor, sq(0, 0), "clamped at top-left");
        game.cursor = sq(7, 7);
        game.handle_key(key(KeyCode::Down));
        game.handle_key(key(KeyCode::Right));
        assert_eq!(game.cursor, sq(7, 7), "clamped at bottom-right");
        assert!(!game.wants_quit());

        game.handle_key(key(KeyCode::Esc));
        assert!(game.wants_quit(), "esc quits when nothing special pending");

        // Digits never reach the board once the menu is closed...
        game = ReversiGame::new();
        game.handle_key(key(KeyCode::Enter));
        game.handle_key(key(KeyCode::Char('5')));
        assert!(!game.setup_open);
        // ...and 'm' brings the menu back over a fresh board.
        game.handle_key(key(KeyCode::Char('5')));
        game.handle_key(key(KeyCode::Enter));
        game.handle_key(key(KeyCode::Char('m')));
        assert!(game.setup_open);
        assert_eq!(game.setup_sel, 0);
        assert_eq!(game.state.score(), (2, 2), "fresh board underneath");
    }
}
