//! Rendering and hit-testing for mancala: two pit rows flanked by stores,
//! legal-move markers, capture flashes and the standard info panel.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::engine::{Mancala, PITS_PER_SIDE, Side, Status};

const BG: Color = Color::Rgb(13, 15, 23);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
const PANEL_BORDER_FG: Color = Color::Rgb(78, 92, 126);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const GIVEN_FG: Color = Color::Rgb(226, 230, 244);
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

/// Fixed geometry inside the board block: [store 4][gap][6x3 pits][gap][store 4].
pub(crate) const INNER_W: u16 = 4 + 1 + PITS_PER_SIDE as u16 * 3 + 1 + 4;
pub(crate) const INNER_H: u16 = 3; // black row / flow gap / white row
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

    /// Pit index under a click, or None for stores/gaps/outside.
    pub(crate) fn pit_at(&self, col: u16, row: u16) -> Option<usize> {
        let in_x = col >= self.x0 + 5 && col < self.x0 + 5 + PITS_PER_SIDE as u16 * 3;
        if !in_x {
            return None;
        }
        let slot = ((col - self.x0 - 5) / 3) as usize;
        match row {
            r if r == self.y0 => Some(12 - slot),
            r if r == self.y0 + 2 => Some(slot),
            _ => None,
        }
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

pub(crate) fn draw(
    frame: &mut Frame,
    game: &Mancala,
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

fn pit_cell_style(
    idx: usize,
    game: &Mancala,
    view: View,
    geom: &Geom,
    hover_pit: Option<usize>,
) -> Style {
    let mut base = Style::default();
    let is_legal = game.status().is_ongoing() && game.legal_pits().contains(&idx);
    let hovered = hover_pit == Some(idx);
    let cursored = view.cursor == Some(idx);
    if cursored || hovered && is_legal {
        base = base.bg(CROSS_BG);
    } else if hovered {
        base = base.bg(HOVER_BG);
    }
    if game.last_sown() == Some(idx) {
        base = base.fg(GOLD_FG).add_modifier(Modifier::BOLD);
    } else if let Some((src, _)) = game.captured_last()
        && src == idx
    {
        base = base.bg(SELECT_BG);
    } else if is_legal {
        base = base.fg(ACCENT_FG);
    } else {
        base = base.fg(DIM_FG);
    }
    let _ = geom;
    base
}

fn draw_board(
    frame: &mut Frame,
    area: Rect,
    game: &Mancala,
    view: View,
    geom_out: &mut Option<Geom>,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " mancala ",
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
    let hover_pit = view.hover.and_then(|(c, r)| geom.pit_at(c, r));

    // Stores flank the pit field.
    let store_line = |side: Side| {
        let n = game.store_of(side);
        Span::styled(
            format!("{} {:>2}", glyph_of(side), n),
            Style::default().fg(GIVEN_FG).add_modifier(Modifier::BOLD),
        )
    };

    let mut top = vec![Span::raw(" ")];
    top.push(store_line(Side::White));
    for slot in 0..PITS_PER_SIDE {
        let idx = 12 - slot;
        let text = format!("{:^3}", game.pit(idx));
        let style = pit_cell_style(idx, game, view, &geom, hover_pit);
        top.push(Span::styled(text, style));
    }
    top.push(Span::raw("  "));
    top.push(store_line(Side::Black));

    let bottom: Vec<Span<'static>> = (0..PITS_PER_SIDE)
        .map(|slot| {
            let style = pit_cell_style(slot, game, view, &geom, hover_pit);
            Span::styled(format!("{:^3}", game.pit(slot)), style)
        })
        .collect();

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(top),
            Line::from(Span::styled("", Style::default())),
            Line::from(bottom),
        ]),
        inner,
    );
}

fn draw_panel(
    frame: &mut Frame,
    area: Rect,
    game: &Mancala,
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
            format!("{} thinking...", glyph_of(mode.ai_side)),
            Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::raw(""))
    };

    frame.render_widget(
        Paragraph::new(vec![
            kv(
                "score",
                Span::styled(
                    format!(
                        "\u{25cb}{} \u{25cf}{}",
                        game.store_of(Side::White),
                        game.store_of(Side::Black)
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

fn glyph_of(side: Side) -> &'static str {
    if side == Side::White {
        "\u{25cb}"
    } else {
        "\u{25cf}"
    }
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
            " mancala setup ",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    render_setup_rows(frame, inner, selected, hover, items_out);
}

pub(crate) fn render_setup_rows(
    frame: &mut Frame,
    inner: Rect,
    selected: usize,
    hover: Option<(u16, u16)>,
    items_out: &mut Vec<(Rect, usize)>,
) {
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
        let number = format!("{:>02}", i + 1);
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
                Span::styled(number, Style::default().fg(FAINT_FG)),
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
        let game = Mancala::new();
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
                        cursor: Some(0),
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
            (32, 8),
            (46, 12),
            (80, 24),
            (120, 40),
        ] {
            let _ = rendered(w, h);
        }
    }

    #[test]
    fn pit_hit_testing_round_trips() {
        let g = rendered(80, 24).expect("board drawn");
        assert_eq!(g.pit_at(g.origin().0 + 5, g.origin().1 + 2), Some(0));
        assert_eq!(g.pit_at(g.origin().0 + 20, g.origin().1 + 2), Some(5));
        assert_eq!(g.pit_at(g.origin().0 + 5, g.origin().1), Some(12));
        assert_eq!(g.pit_at(g.origin().0, g.origin().1), None, "store zone");
    }
}
