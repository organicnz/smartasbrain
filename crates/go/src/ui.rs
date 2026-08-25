use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use super::Button;
use super::engine::{GoState, Player};

const BG: Color = Color::Rgb(13, 15, 23);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const FAINT_FG: Color = Color::Rgb(84, 92, 112);
const GIVEN_FG: Color = Color::Rgb(226, 230, 244);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const CROSS_BG: Color = Color::Rgb(38, 44, 60);
const SELECT_BG: Color = Color::Rgb(52, 66, 104);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);

pub struct Geom {
    /// Screen position of the top-left intersection.
    x0: u16,
    y0: u16,
}

impl Geom {
    fn new(x0: u16, y0: u16) -> Self {
        Self { x0, y0 }
    }

    pub fn cell_at(&self, col: u16, row: u16) -> Option<usize> {
        if row < self.y0 || col < self.x0 {
            return None;
        }
        let (dr, dc) = ((row - self.y0), (col - self.x0));
        if dc % 2 != 0 {
            return None;
        }
        let (r, c) = (dr as usize, (dc / 2) as usize);
        if r >= super::SIZE || c >= super::SIZE {
            return None;
        }
        Some(r * super::SIZE + c)
    }
}

impl Clone for Geom {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Geom {}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

pub fn in_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Board occupies `2*SIZE-1` columns and SIZE rows inside its block.
fn board_block_w() -> u16 {
    2 * super::SIZE as u16 - 1 + 2
}
fn board_block_h() -> u16 {
    super::SIZE as u16 + 2
}
const PANEL_W: u16 = 26;

pub fn draw(
    frame: &mut Frame,
    state: &mut GoState,
    area: Rect,
    cursor: usize,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    // Too small to draw anything safely (also covers 0x0 ptty startup).
    if area.width < board_block_w() || area.height <= board_block_h() {
        *geom_out = None;
        return;
    }

    let side = area.width >= board_block_w() + 2 + PANEL_W && area.height > board_block_h() + 7;
    let cluster_w = if side {
        board_block_w() + 2 + PANEL_W
    } else {
        board_block_w().max(PANEL_W)
    };
    let cluster_h = if side {
        board_block_h()
    } else {
        board_block_h() + 1 + 7
    };

    let cluster = centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
    let cols = Layout::horizontal([
        Constraint::Length(board_block_w()),
        Constraint::Length(2),
        Constraint::Length(PANEL_W),
    ])
    .split(cluster);

    draw_board(frame, cols[0], state, cursor, geom_out);

    let panel = if side {
        cols[2]
    } else {
        let panel_h = 7.min(area.height - board_block_h());
        Rect {
            x: cluster.x,
            y: cluster.y + board_block_h(),
            width: PANEL_W.min(area.width),
            height: panel_h,
        }
    };
    draw_panel(frame, panel, state, buttons);
}

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    state: &GoState,
    cursor: usize,
    geom_out: &mut Option<Geom>,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            format!(" go {}x{} ", state.size(), state.size()),
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(
                match state.turn {
                    Player::Black => "\u{25cf} to play ",
                    Player::White => "\u{25cb} to play ",
                    Player::Empty => "",
                },
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    *geom_out = Some(Geom::new(inner.x, inner.y));

    let size = state.size();
    let (cr, cc) = (cursor / size, cursor % size);
    let mut lines = Vec::with_capacity(size);
    for r in 0..size {
        let mut spans = Vec::with_capacity(size * 2 - 1);
        for c in 0..size {
            if c > 0 {
                spans.push(Span::raw(" "));
            }
            let idx = r * size + c;
            let stone = state.board()[idx];
            let is_cursor = idx == cursor;
            let on_cross = r == cr || c == cc;

            let fg = match stone {
                Player::Black => GIVEN_FG,
                Player::White => ACCENT_FG,
                Player::Empty => FAINT_FG,
            };
            let last = state.last_move == Some(idx);
            let mut style = Style::default().fg(fg);
            if stone != Player::Empty && last {
                style = style.fg(GOLD_FG).add_modifier(Modifier::BOLD);
            } else if is_cursor || (stone == Player::Empty && on_cross) {
                style = style.bg(if is_cursor { SELECT_BG } else { CROSS_BG });
                if is_cursor {
                    style = style.add_modifier(Modifier::BOLD);
                }
            }
            spans.push(Span::styled(stone.symbol(), style));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_panel(frame: &mut Frame, area: Rect, state: &GoState, buttons: &mut Vec<(Rect, Button)>) {
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
            Span::styled(format!("{key:<9}"), Style::default().fg(DIM_FG)),
            value,
        ])
    };

    let turn_span = Span::styled(
        match state.turn {
            Player::Black => "\u{25cf} black".to_string(),
            Player::White => "\u{25cb} white".to_string(),
            Player::Empty => "-".to_string(),
        },
        Style::default().fg(GIVEN_FG).add_modifier(Modifier::BOLD),
    );

    let result_line = if state.over {
        vec![kv(
            "result",
            Span::styled(
                state.score().result_text(),
                Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
            ),
        )]
    } else {
        vec![]
    };

    let mut lines = vec![
        kv("to play", turn_span),
        kv(
            "captures",
            Span::styled(
                format!(
                    "\u{25cf} {} \u{25cb} {}",
                    state.captures_black, state.captures_white
                ),
                Style::default().fg(GIVEN_FG),
            ),
        ),
        kv(
            "passes",
            Span::styled(format!("{} / 2", state.passes), Style::default().fg(DIM_FG)),
        ),
    ];
    lines.extend(result_line);

    frame.render_widget(Paragraph::new(lines), inner);

    // Buttons along the bottom of the panel.
    if area.height < 2 {
        return;
    }
    let btn_y = area.y + area.height - 2;
    let half = inner.width / 2;
    let specs = [("pass", Button::Pass), ("restart", Button::Restart)];
    for (i, (label, button)) in specs.iter().enumerate() {
        let rect = Rect {
            x: inner.x + i as u16 * half,
            y: btn_y,
            width: half.max(4),
            height: 1,
        };
        buttons.push((rect, *button));
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {label} "),
                Style::default().fg(ACCENT_FG).bg(BUTTON_BG),
            )))
            .alignment(Alignment::Center),
            rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GoGame, SIZE};
    use game_core::Game;
    use ratatui::backend::TestBackend;

    fn render_at(w: u16, h: u16) {
        let mut game = GoGame::new();
        let backend = TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| Game::draw(&mut game, f, f.area()))
            .unwrap();
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [
            (0u16, 0u16),
            (10, 3),
            (21, 11),
            (40, 14),
            (60, 20),
            (120, 40),
        ] {
            render_at(w, h);
        }
    }

    #[test]
    fn click_mapping_round_trips() {
        let geom = Geom::new(2, 1);
        assert_eq!(geom.cell_at(2, 1), Some(0));
        assert_eq!(geom.cell_at(4, 1), Some(1));
        assert_eq!(geom.cell_at(2, 2), Some(SIZE));
        assert_eq!(geom.cell_at(3, 1), None, "gap column");
        assert_eq!(geom.cell_at(100, 1), None);
    }
}
