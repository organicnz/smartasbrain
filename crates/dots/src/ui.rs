//! Rendering and hit-testing for dots-and-boxes: a dot lattice with
//! clickable edge segments, owner-colored claims and the info panel.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::engine::{BOXES, Dots, Side, Status, decompose, h_id, v_id};

const BG: Color = Color::Rgb(13, 15, 23);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const GIVEN_FG: Color = Color::Rgb(226, 230, 244);
const BLACK_FG: Color = Color::Rgb(140, 150, 175);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const FAINT_FG: Color = Color::Rgb(84, 92, 112);
const ASH: Color = Color::Rgb(92, 68, 58);
const GOLD_FG: Color = Color::Rgb(235, 205, 130);
const CROSS_BG: Color = Color::Rgb(44, 50, 68);
const HOVER_BG: Color = Color::Rgb(48, 54, 74);
const SELECT_BG: Color = Color::Rgb(52, 66, 104);
const BUTTON_BG: Color = Color::Rgb(24, 28, 42);
const FIRE: Color = Color::Rgb(178, 34, 34);
const BONE: Color = Color::Rgb(216, 208, 194);
const PICK_SELECT_BG: Color = Color::Rgb(88, 16, 14);
const PICK_HOVER_BG: Color = Color::Rgb(54, 22, 18);

pub(crate) const SETUP_ITEMS: [&str; 9] = [
    "TWO PLAYERS",
    "VS AI - EASY",
    "VS AI - MEDIUM",
    "VS AI - HARD",
    "VS AI - EXPERT",
    "AI DUEL - EASY",
    "AI DUEL - MEDIUM",
    "AI DUEL - HARD",
    "AI DUEL - EXPERT",
];

/// Dot spacing inside the board block.
pub(crate) const DOT_DX: u16 = 4;
pub(crate) const DOT_DY: u16 = 2;
pub(crate) const INNER_W: u16 = BOXES as u16 * DOT_DX + 1;
pub(crate) const INNER_H: u16 = BOXES as u16 * DOT_DY + 1;
pub(crate) const BOARD_W: u16 = INNER_W + 2;
pub(crate) const BOARD_H: u16 = INNER_H + 2;
const PANEL_W: u16 = 26;

#[derive(Clone, Copy)]
pub(crate) struct View {
    pub(crate) cursor: Option<usize>,
    pub(crate) hover: Option<(u16, u16)>,
}

#[derive(Clone, Copy)]
pub(crate) struct ModeInfo<'a> {
    pub(crate) label: &'a str,
    pub(crate) ai_side: Side,
    pub(crate) thinking: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct Geom {
    x0: u16,
    y0: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    Restart,
}

impl Geom {
    fn new(x0: u16, y0: u16) -> Self {
        Self { x0, y0 }
    }

    #[allow(dead_code)]
    pub(crate) fn origin(&self) -> (u16, u16) {
        (self.x0, self.y0)
    }

    /// Edge under `(col,row)` regardless of claim state; None off-lattice.
    /// Horizontal segments span the 3 cells right of a dot on even rows;
    /// verticals sit exactly on a dot column one row below it.
    pub(crate) fn edge_at(&self, col: u16, row: u16) -> Option<usize> {
        let (dx, dy) = (col.checked_sub(self.x0)?, row.checked_sub(self.y0)?);
        if dx > BOXES as u16 * DOT_DX || dy > BOXES as u16 * DOT_DY {
            return None;
        }
        if dy % DOT_DY == 0 && dx % DOT_DX != 0 {
            // horizontal candidate between dots
            let r = (dy / DOT_DY) as usize;
            let c = ((dx - 1) / DOT_DX) as usize;
            if dx % DOT_DX >= 1 && c < BOXES {
                return Some(h_id(r, c));
            }
        }
        if dy % DOT_DY == 1 && dx % DOT_DX == 0 {
            let r = ((dy - 1) / DOT_DY) as usize;
            let c = (dx / DOT_DX) as usize;
            if r < BOXES && c <= BOXES {
                return Some(v_id(r, c));
            }
        }
        None
    }
}

fn in_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn side_fg(side: Side) -> Color {
    if side == Side::White {
        GIVEN_FG
    } else {
        BLACK_FG
    }
}

pub(crate) fn draw(
    frame: &mut Frame,
    game: &Dots,
    area: Rect,
    view: View,
    mode: ModeInfo<'_>,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    if area.width < BOARD_W || area.height < BOARD_H.max(11) {
        *geom_out = None;
        return;
    }
    let side = area.width >= BOARD_W + 2 + PANEL_W && area.height >= BOARD_H.max(11);
    let cluster_w = if side { BOARD_W + 2 + PANEL_W } else { BOARD_W };
    let cluster_h = if side { BOARD_H } else { BOARD_H + 10 };
    let cluster = centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
    let cols = Layout::horizontal([
        Constraint::Length(BOARD_W),
        Constraint::Length(2),
        Constraint::Length(PANEL_W),
    ])
    .split(cluster);

    let panel = if side {
        cols[2]
    } else {
        Rect {
            x: cluster.x,
            y: cluster.y.saturating_add(BOARD_H + 1),
            width: PANEL_W.min(area.width),
            height: 9_u16.min(area.height.saturating_sub(BOARD_H + 1)),
        }
    };

    draw_board(frame, cols[0], game, view, geom_out);
    draw_panel(frame, panel, game, view.hover, mode, buttons);
}

fn edge_span(
    game: &Dots,
    e: usize,
    view: View,
    geom: &Geom,
    hover_edge: Option<usize>,
) -> Span<'static> {
    let (vert, row, col) = decompose(e);
    let claimed_by = game.claimed(e);
    let glyph: &'static str = if vert { "\u{2502}" } else { "\u{2500}" };
    let is_cursor = view.cursor == Some(e);
    let is_hovered = hover_edge == Some(e);
    let mut fg = FAINT_FG;
    let mut bg = BG;
    let mut modifier = Modifier::empty();
    match claimed_by {
        Some(s) => fg = side_fg(s),
        None => glyph_fallback(&mut fg, &mut modifier),
    }
    if claimed_by.is_none() && (is_cursor || is_hovered) {
        fg = ACCENT_FG;
        modifier.insert(Modifier::BOLD);
    }
    if is_cursor {
        bg = CROSS_BG;
    } else if claimed_by.is_none() && is_hovered {
        bg = HOVER_BG;
    }
    let _ = (row, col, geom);
    Span::styled(
        glyph.to_string(),
        Style::default().fg(fg).bg(bg).add_modifier(modifier),
    )
}

fn glyph_fallback(fg: &mut Color, modifier: &mut Modifier) {
    *modifier = Modifier::empty();
    let _ = fg;
}

fn draw_board(frame: &mut Frame, area: Rect, game: &Dots, view: View, geom_out: &mut Option<Geom>) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " dots ",
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(
                match game.turn() {
                    Side::White => "\u{25cb} to play ",
                    Side::Black => "\u{25cf} to play ",
                },
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let geom = Geom::new(inner.x, inner.y);
    *geom_out = Some(geom);
    let hover_edge = view.hover.and_then(|(c, r)| geom.edge_at(c, r));

    let mut lines: Vec<Line> = Vec::with_capacity(INNER_H as usize);
    for gy in 0..INNER_H {
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(INNER_W as usize);
        for gx in 0..INNER_W {
            let cell = match (gy % DOT_DY, gx % DOT_DX) {
                (0, 0) => Span::styled("+", Style::default().fg(DIM_FG)), // dot
                (0, _) => {
                    let r = (gy / DOT_DY) as usize;
                    let c = ((gx - 1) / DOT_DX) as usize;
                    if c < BOXES {
                        edge_span(game, h_id(r, c), view, &geom, hover_edge)
                    } else {
                        Span::raw(" ")
                    }
                }
                (_, 0) => {
                    let r = ((gy - 1) / DOT_DY) as usize;
                    let c = (gx / DOT_DX) as usize;
                    if r < BOXES {
                        edge_span(game, v_id(r, c), view, &geom, hover_edge)
                    } else {
                        Span::raw(" ")
                    }
                }
                _ => {
                    // Box interior: owner glyph or recently-completed flash.
                    let r = ((gy - 1) / DOT_DY) as usize;
                    let c = ((gx - 1) / DOT_DX) as usize;
                    match game.box_owner(r, c) {
                        Some(owner) => Span::styled(
                            owner.symbol().to_string(),
                            Style::default()
                                .fg(side_fg(owner))
                                .add_modifier(Modifier::BOLD),
                        ),
                        None if game.boxes_completed_last().contains(&(r, c)) => {
                            Span::styled("+", Style::default().bg(SELECT_BG).fg(GOLD_FG))
                        }
                        None => Span::raw(" "),
                    }
                }
            };
            spans.push(cell);
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    game: &Dots,
    hover: Option<(u16, u16)>,
    mode: ModeInfo<'_>,
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
    let status_text = match game.status() {
        Status::Ongoing => "-".to_string(),
        Status::Won(s) => format!("{} wins!", if s == Side::White { "white" } else { "black" }),
        Status::Draw => "draw".to_string(),
    };
    let thinking_line = if mode.thinking {
        Line::from(Span::styled(
            format!(
                "{} thinking...",
                if mode.ai_side == Side::White {
                    "\u{25cb}"
                } else {
                    "\u{25cf}"
                }
            ),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::raw(""))
    };

    frame.render_widget(
        Paragraph::new(vec![
            kv(
                "boxes",
                Span::styled(
                    format!(
                        "\u{25cb}{} \u{25cf}{}",
                        game.score(Side::White),
                        game.score(Side::Black)
                    ),
                    Style::default().fg(GIVEN_FG),
                ),
            ),
            kv(
                "status",
                Span::styled(status_text, Style::default().fg(GIVEN_FG)),
            ),
            kv(
                "mode",
                Span::styled(mode.label.to_string(), Style::default().fg(BONE)),
            ),
            thinking_line,
        ]),
        inner,
    );

    if area.height < 2 || inner.width < 4 {
        return;
    }
    let rect = Rect {
        x: inner.x,
        y: area.y + area.height - 2,
        width: inner.width,
        height: 1,
    };
    buttons.push((rect, Button::Restart));
    let hot = hover.is_some_and(|(c, r)| in_rect(rect, c, r));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " restart ",
            Style::default()
                .fg(ACCENT_FG)
                .bg(if hot { HOVER_BG } else { BUTTON_BG }),
        )))
        .alignment(Alignment::Center),
        rect,
    );
}

/// Modal opponent picker shown instead of the board until confirmed.
pub(crate) fn draw_setup(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    hover: Option<(u16, u16)>,
    items_out: &mut Vec<(Rect, usize)>,
) {
    items_out.clear();
    if area.width < 24 || area.height < 12 {
        return;
    }
    let popup = centered_rect(area, 30, 12);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FIRE))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " dots setup ",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    for (i, label) in SETUP_ITEMS.iter().enumerate() {
        let y = inner.y + i as u16;
        if i > 0 && y >= inner.y + inner.height.saturating_sub(1) {
            break;
        }
        let rect = Rect {
            x: inner.x + 1,
            y,
            width: inner.width.saturating_sub(2),
            height: 1,
        };
        items_out.push((rect, i));
        let hot = selected == i;
        let hovered = hover.is_some_and(|(c, r)| in_rect(rect, c, r));
        let (base, fg, modifier) = if hot {
            (PICK_SELECT_BG, BONE, Modifier::BOLD)
        } else if hovered {
            (PICK_HOVER_BG, GOLD_FG, Modifier::BOLD)
        } else {
            (BG, FAINT_FG, Modifier::empty())
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "\u{2020} ",
                    if hot {
                        Style::default().fg(FIRE).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(format!("{:>02}", i + 1), Style::default().fg(FAINT_FG)),
                Span::raw(" "),
                Span::styled(
                    (*label).to_string(),
                    Style::default().fg(fg).add_modifier(modifier),
                ),
            ]))
            .style(Style::default().bg(base)),
            rect,
        );
    }

    let hints_y = inner.y + inner.height - 1;
    if hints_y > inner.y {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "\u{2191}\u{2193} choose \u{00b7} enter start \u{00b7} esc quit",
                Style::default().fg(ASH),
            )))
            .alignment(Alignment::Center),
            Rect {
                x: inner.x,
                y: hints_y,
                width: inner.width,
                height: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn rendered(w: u16, h: u16) -> Option<Geom> {
        let game = Dots::new();
        let mut geom = None;
        let backend = TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                draw(
                    f,
                    &game,
                    f.area(),
                    View {
                        cursor: None,
                        hover: None,
                    },
                    ModeInfo {
                        label: "two players",
                        ai_side: Side::Black,
                        thinking: false,
                    },
                    &mut geom,
                    &mut Vec::new(),
                );
            })
            .unwrap();
        geom
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [
            (0u16, 0u16),
            (20, 6),
            (26, 12),
            (40, 14),
            (80, 24),
            (120, 40),
        ] {
            let _ = rendered(w, h);
        }
    }

    #[test]
    fn edge_hit_testing_round_trips() {
        let g = rendered(80, 24).expect("board drawn");
        let (x0, y0) = g.origin();
        assert_eq!(g.edge_at(x0 + 1, y0), Some(h_id(0, 0)));
        assert_eq!(g.edge_at(x0 + 3, y0), Some(h_id(0, 0)));
        assert_eq!(g.edge_at(x0 + 5, y0), Some(h_id(0, 1)));
        assert_eq!(g.edge_at(x0, y0 + 1), Some(v_id(0, 0)));
        assert_eq!(g.edge_at(x0 + 4, y0 + 3), Some(v_id(1, 1)));
        assert_eq!(g.edge_at(x0 + 2, y0 + 1), None, "box interior");
    }
}
