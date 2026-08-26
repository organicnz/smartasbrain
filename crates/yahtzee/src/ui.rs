//! Rendering and hit-testing for Yahtzee: dual scorecards, the dice tray
//! with hold markers, and the roll button.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::engine::{ALL_CATEGORIES, CATS, DICE, Side, Status};

const BG: Color = Color::Rgb(13, 15, 23);
const PANEL_BG: Color = Color::Rgb(15, 17, 27);
const BORDER_FG: Color = Color::Rgb(92, 108, 142);
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

pub(crate) const SETUP_ITEMS: [&str; 7] = [
    "TWO PLAYERS",
    "VS AI - EASY",
    "VS AI - MEDIUM",
    "VS AI - HARD",
    "AI DUEL - EASY",
    "AI DUEL - MEDIUM",
    "AI DUEL - HARD",
];

pub(crate) const CARD_W: u16 = 12 + 6 + 6 + 2;
pub(crate) const CARD_H: u16 = CATS as u16 + 5;
/// Dice tray: five fixed-width cells plus borders.
/// Dice slot widths tried from roomiest to tightest.
const DIE_WIDTHS: [u16; 3] = [7, 5, 3];

/// Tray block width for a given die-slot width (borders included).
#[must_use]
fn tray_w(die_w: u16) -> u16 {
    DICE as u16 * die_w + 2
}
pub(crate) const TRAY_H: u16 = 6;

#[derive(Clone, Copy)]
pub(crate) struct View {
    pub(crate) cursor_row: usize,
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
    card_x: u16,
    card_y: u16,
    tray_x: u16,
    tray_y: u16,
    die_w: u16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    Roll,
}

impl Geom {
    fn new(card_x: u16, card_y: u16, tray_x: u16, tray_y: u16, die_w: u16) -> Self {
        Self {
            card_x,
            card_y,
            tray_x,
            tray_y,
            die_w: die_w.max(1),
        }
    }

    /// Scorecard row clicked (category index), or None off the table.
    pub(crate) fn row_at(&self, col: u16, row: u16) -> Option<usize> {
        if col < self.card_x + 1
            || col >= self.card_x + CARD_W - 1
            || row < self.card_y + 1
            || row >= self.card_y + 1 + CATS as u16
        {
            return None;
        }
        Some((row - self.card_y - 1) as usize)
    }

    /// Die index clicked in the tray (glyph column of each fixed cell).
    pub(crate) fn die_at(&self, col: u16, row: u16) -> Option<usize> {
        if row != self.tray_y + 1 {
            return None;
        }
        let rel = col.checked_sub(self.tray_x + 1)?;
        let die = (rel / self.die_w) as usize;
        (rel % self.die_w == 0 && die < DICE).then_some(die)
    }

    pub(crate) fn roll_button(&self) -> Rect {
        Rect {
            x: self.tray_x + 1,
            y: self.tray_y + 3,
            width: DICE as u16 * self.die_w - 2 + 4,
            height: 1,
        }
    }

    #[cfg(test)]
    pub(crate) fn tray_click_column(&self, die: usize) -> u16 {
        self.tray_x + 1 + die as u16 * self.die_w
    }

    #[cfg(test)]
    pub(crate) fn tray_click_row(&self) -> u16 {
        self.tray_y + 1
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

fn die_glyph(v: u8) -> char {
    const FACES: [char; 6] = [
        '\u{2680}', '\u{2681}', '\u{2682}', '\u{2683}', '\u{2684}', '\u{2685}',
    ];
    FACES[(v.clamp(1, 6) - 1) as usize]
}

fn glyph_of(side: Side) -> &'static str {
    if side == Side::White {
        "\u{25cb}"
    } else {
        "\u{25cf}"
    }
}

pub(crate) fn draw(
    frame: &mut Frame,
    game: &crate::engine::Yahtzee,
    area: Rect,
    view: View,
    mode: ModeInfo<'_>,
    geom_out: &mut Option<Geom>,
    buttons: &mut Vec<(Rect, Button)>,
) {
    if area.width < CARD_W || area.height < CARD_H {
        *geom_out = None;
        return;
    }
    // Resizable tray: roomiest die slots that fit beside/below the card.
    let mut die_w = DIE_WIDTHS[2];
    for &w in &DIE_WIDTHS {
        let tw = tray_w(w);
        let fits_side = area.width >= CARD_W + 2 + tw && area.height >= CARD_H.max(TRAY_H);
        let fits_below = area.width >= tw && area.height >= CARD_H + 1 + TRAY_H;
        if fits_side || fits_below {
            die_w = w;
            break;
        }
    }
    let tray_width = tray_w(die_w);
    let side_by_side = area.width >= CARD_W + 2 + tray_width && area.height >= CARD_H.max(TRAY_H);
    let cluster_w = if side_by_side {
        CARD_W + 2 + tray_width
    } else {
        CARD_W.max(tray_width)
    };
    let cluster_h = if side_by_side {
        CARD_H.max(TRAY_H)
    } else {
        CARD_H + 1 + TRAY_H
    };
    let cluster = centered_rect(area, cluster_w.min(area.width), cluster_h.min(area.height));
    let cols = Layout::horizontal([
        Constraint::Length(CARD_W),
        Constraint::Length(2),
        Constraint::Length(tray_width),
    ])
    .split(cluster);

    let tray_area = if side_by_side {
        cols[2]
    } else {
        Rect {
            x: cluster.x,
            y: cluster.y.saturating_add(CARD_H + 1),
            width: tray_width.min(area.width.saturating_sub(cluster.x)),
            height: TRAY_H.min(area.height.saturating_sub(CARD_H + 1)),
        }
    };

    *geom_out = Some(Geom::new(
        cols[0].x,
        cols[0].y,
        tray_area.x,
        tray_area.y,
        die_w,
    ));

    draw_card(frame, cols[0], game, view);
    draw_tray(frame, tray_area, game, view, mode, die_w, buttons);
}

fn draw_card(frame: &mut Frame, area: Rect, game: &crate::engine::Yahtzee, view: View) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG))
        .style(Style::default().bg(PANEL_BG))
        .title(Line::from(Span::styled(
            " scorecard ",
            Style::default().fg(BORDER_FG),
        )))
        .title_bottom(
            Line::from(Span::styled(
                match game.status() {
                    Status::Ongoing => format!("{} to play", glyph_of(game.turn())),
                    Status::Won(s) => format!("{} wins!", glyph_of(s)),
                    Status::Draw => "draw".to_string(),
                },
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::with_capacity(CARD_H as usize);
    lines.push(Line::from(Span::styled(
        format!(
            "{:<12}   {}  {}",
            "category",
            glyph_of(Side::White),
            glyph_of(Side::Black)
        ),
        Style::default().fg(DIM_FG),
    )));

    for (i, cat) in ALL_CATEGORIES.iter().enumerate() {
        let cursor_here = i == view.cursor_row && game.status().is_ongoing();
        let base_bg = if cursor_here { CROSS_BG } else { PANEL_BG };
        let label_style = Style::default()
            .fg(GIVEN_FG)
            .bg(base_bg)
            .add_modifier(if cursor_here {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        let cell = |side: Side| -> Span<'static> {
            let card = game.card(side);
            let mut fg = FAINT_FG;
            let mut text = "   .".to_string();
            if let Some(v) = card.slots[cat.index()] {
                text = format!("{v:>4}");
                fg = DIM_FG;
                if game.last_assigned() == Some((side, *cat)) {
                    fg = GOLD_FG;
                }
            }
            Span::styled(
                format!(" {:>3} ", text.trim()),
                Style::default().fg(fg).bg(base_bg),
            )
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{:<12}", cat.label()), label_style),
            cell(Side::White),
            cell(Side::Black),
        ]));
    }

    let (wu, wb, _, wt) = game.card(Side::White).totals();
    let (bu, bb, _, bt) = game.card(Side::Black).totals();
    lines.push(Line::from(Span::styled(
        format!("{:<12}   {:>4}  {:>4}", "upper+bonus", wu + wb, bu + bb),
        Style::default().fg(ACCENT_FG),
    )));
    lines.push(Line::from(Span::styled(
        format!("{:<12}   {:>4}  {:>4}", "grand total", wt, bt),
        Style::default().fg(BONE).add_modifier(Modifier::BOLD),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

#[allow(clippy::too_many_arguments)]
fn draw_tray(
    frame: &mut Frame,
    area: Rect,
    game: &crate::engine::Yahtzee,
    view: View,
    mode: ModeInfo<'_>,
    die_w: u16,
    buttons: &mut Vec<(Rect, Button)>,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FIRE))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " dice ",
            Style::default().fg(FIRE),
        )))
        .title_bottom(
            Line::from(Span::styled(
                mode.label.to_string(),
                Style::default().fg(DIM_FG),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let hover_die = view.hover.and_then(|(c, r)| {
        Geom::new(
            inner.x.saturating_sub(1),
            inner.y.saturating_sub(1),
            area.x,
            area.y,
            die_w,
        )
        .die_at(c, r)
    });

    let mut dice_line: Vec<Span<'static>> = Vec::with_capacity(DICE);
    let mut holds_line: Vec<Span<'static>> = Vec::with_capacity(DICE);
    for (i, &d) in game.dice().iter().enumerate() {
        let held = game.holds()[i];
        let hot = hover_die == Some(i);
        let bg = if held || hot { SELECT_BG } else { BG };
        dice_line.push(Span::styled(
            format!("{:^width$}", die_glyph(d), width = die_w as usize),
            Style::default()
                .fg(if held { GOLD_FG } else { GIVEN_FG })
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ));
        holds_line.push(Span::styled(
            format!(
                "{:^width$}",
                if held { "\u{25b2}" } else { "" },
                width = die_w as usize
            ),
            Style::default().fg(GOLD_FG).bg(bg),
        ));
    }
    frame.render_widget(
        Paragraph::new(vec![Line::from(dice_line), Line::from(holds_line)]),
        inner,
    );

    let btn = Geom::new(0, 0, area.x, area.y, die_w).roll_button();
    buttons.push((btn, Button::Roll));
    let hot = view.hover.is_some_and(|(c, r)| in_rect(btn, c, r));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" roll ({}) ", game.rolls_left()),
            Style::default()
                .fg(if game.rolls_left() > 0 {
                    ACCENT_FG
                } else {
                    FAINT_FG
                })
                .bg(if hot { HOVER_BG } else { BUTTON_BG }),
        )))
        .alignment(Alignment::Center),
        btn,
    );

    let status_y = inner.y + 3.min(inner.height.saturating_sub(1));
    if mode.thinking {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{} thinking...", glyph_of(mode.ai_side)),
                Style::default().fg(GOLD_FG).add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            Rect {
                y: status_y,
                ..inner
            },
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "space roll \u{00b7} 1-5 hold \u{00b7} enter scores",
                Style::default().fg(ASH),
            )))
            .alignment(Alignment::Center),
            Rect {
                y: status_y,
                ..inner
            },
        );
    }
}

/// Modal opponent picker shown instead of play until confirmed.
pub(crate) fn draw_setup(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    hover: Option<(u16, u16)>,
    items_out: &mut Vec<(Rect, usize)>,
) {
    items_out.clear();
    if area.width < 24 || area.height < 13 {
        return;
    }
    let popup = centered_rect(area, 30, 13);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FIRE))
        .style(Style::default().bg(BG))
        .title(Line::from(Span::styled(
            " yahtzee setup ",
            Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    for (i, label) in SETUP_ITEMS.iter().enumerate() {
        let y = inner.y + (i as u16) * 2;
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
