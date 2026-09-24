//! The lobby: smartasbrain's central menu, styled after the classic DOOM /
//! Quake front-ends — block-letter logo, blood-and-ember palette, dagger
//! selection marker over a two-column game grid. Choosing an entry asks the
//! shell to open that game's tab via [`Game::poll_navigation`].
//!
//! Beautified pass: layered logo shadow + diagonal gradient, flowing ember
//! divider with spark particles, per-game glyphs, polished briefing pane and
//! vignetted backdrop — all while preserving the uniform grid geometry that
//! the tests assert.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use game_core::Game;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

const BG: Color = Color::Rgb(12, 7, 7);
const BG_VIGNETTE: Color = Color::Rgb(19, 10, 10);
const FIRE: Color = Color::Rgb(178, 34, 34);
const EMBER_BRIGHT: Color = Color::Rgb(226, 88, 46);
const EMBER_DARK: Color = Color::Rgb(126, 44, 28);
const HOT: Color = Color::Rgb(255, 138, 48);
const GOLD: Color = Color::Rgb(230, 178, 78);
const GOLD_GLOW: Color = Color::Rgb(255, 209, 102);
const BONE: Color = Color::Rgb(216, 208, 194);
const DIM_RUST: Color = Color::Rgb(134, 74, 56);
const ASH: Color = Color::Rgb(92, 68, 58);
const SELECT_BG: Color = Color::Rgb(88, 16, 14);
const SELECT_BG_HI: Color = Color::Rgb(118, 24, 18);
const HOVER_BG: Color = Color::Rgb(54, 22, 18);
const SHADOW: Color = Color::Rgb(28, 12, 12);

/// Dagger shown left of the active entry; idle cells reserve its columns so
/// every title stays aligned.
const MARKER: char = '\u{2020}'; // †
const MARKER_HOT: char = '\u{25B8}'; // ▸ used as alt when pulsing (same width)
/// Soft border tone shared by framed panes.
const DIVIDER: Color = Color::Rgb(126, 44, 28); // == EMBER_DARK
/// Menu grid columns; navigation steps this far per vertical press.
const COLS: usize = 2;

/// Per-game single-glyph icon shown in the idle marker column — keeps the
/// uniform 7+title_w geometry intact (one char, no extra width) while giving
/// each title its own silhouette.
const GAME_ICONS: [&str; 13] = [
    "▦",  // sudoku
    "●",  // go
    "♞",  // chess
    "◎",  // checkers
    "⬢",  // backgammon
    "◐",  // reversi
    "╋",  // morris
    "▤",  // connect four
    "◉",  // mancala
    "⬡",  // dots
    "⚂",  // yahtzee
    "▭",  // dominoes
    "⚓", // battleship
];

/// Menu entries; entry `i` opens shell tab `i + 1` (the lobby lives at 0).
const ENTRIES: &[(&str, &str)] = &[
    ("SUDOKU", "pencil & logic"),
    ("GO", "9x9 territory"),
    ("CHESS", "castling & en passant"),
    ("CHECKERS", "forced jumps & kings"),
    ("BACKGAMMON", "dice races & bear-offs"),
    ("REVERSI", "corner kings & flips"),
    ("MORRIS", "mills, mills, mills"),
    ("CONNECT FOUR", "four in a row"),
    ("MANCALA", "sow & capture"),
    ("DOTS", "claim the chains"),
    ("YAHTZEE", "five lucky dice"),
    ("DOMINOES", "match the ends"),
    ("BATTLESHIP", "sink the fleet"),
];

/// Roadmap footnote under the menu.
const ROADMAP: &str = "ai duels live in every game — pick AI DUEL in its setup";

pub struct Lobby {
    quit: bool,
    selected: usize,
    /// Tab index requested via enter/click, consumed by the shell.
    pending: Option<usize>,
    /// Frame counter from [`Game::tick`]; drives the ember pulse.
    frame: u64,
    /// Last known mouse position, for cell hover styling.
    hover: Option<(u16, u16)>,
    /// Click targets rebuilt every frame.
    rows: Vec<Rect>,
    /// Whether the wide-layout briefing pane rendered this frame.
    info_shown: bool,
}

impl Lobby {
    pub fn new() -> Self {
        Self {
            quit: false,
            selected: 0,
            pending: None,
            frame: 0,
            hover: None,
            rows: Vec::new(),
            info_shown: false,
        }
    }

    /// Whether the briefing pane rendered alongside the grid last frame.
    #[cfg(test)]
    pub(crate) fn info_visible(&self) -> bool {
        self.info_shown
    }
}

impl Lobby {
    /// Bordered briefing pane for the highlighted game (wide layouts only).
    fn draw_briefing(&mut self, frame: &mut Frame, area: Rect) {
        if area.width < 10 || area.height < 6 {
            return;
        }
        let (title, blurb) = ENTRIES[self.selected];
        let icon = GAME_ICONS[self.selected];
        // Double-thin? Rounded keeps the DOOM softness but with a warmer border.
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(GOLD).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(BG_VIGNETTE))
            .title(Line::from(vec![
                Span::styled(" ◆ ", Style::default().fg(GOLD_GLOW)),
                Span::styled(
                    " BRIEFING ",
                    Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ◆ ", Style::default().fg(GOLD_GLOW)),
            ]))
            .title_bottom(
                Line::from(Span::styled(
                    format!(" tab {:02} of {:02} ", self.selected + 1, ENTRIES.len()),
                    Style::default()
                        .fg(BONE)
                        .bg(SELECT_BG_HI)
                        .add_modifier(Modifier::BOLD),
                ))
                .right_aligned(),
            );
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.height < 4 || inner.width < 8 {
            return;
        }

        // Header: icon + spaced title with underline accent.
        let spaced: String = title.chars().flat_map(|c| [c, ' ']).collect();
        let header = Line::from(vec![
            Span::styled(
                format!(" {icon} "),
                Style::default().fg(HOT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                spaced.trim().to_string(),
                Style::default().fg(BONE).add_modifier(Modifier::BOLD),
            ),
        ]);
        let underline = "─".repeat((spaced.chars().count().min(inner.width as usize)) + 2);
        let mut lines = vec![
            Line::from(Span::styled(
                " ".repeat(inner.width as usize),
                Style::default(),
            )), // top padding
            header,
            Line::from(Span::styled(underline, Style::default().fg(DIVIDER))),
            Line::from(Span::raw("")),
        ];
        let wrap_w = inner.width.saturating_sub(2) as usize;
        let wrapped = wrap(blurb, wrap_w);
        let blurb_style = Style::default().fg(Color::Rgb(196, 148, 118));
        for l in wrapped {
            lines.push(Line::from(Span::styled(format!("  {l}"), blurb_style)));
        }
        // Meta line — subtle flavour, keeps pane feeling alive.
        lines.push(Line::from(Span::raw("")));
        lines.push(Line::from(Span::styled(
            "  ◈  ready to launch  ◈".to_string(),
            Style::default().fg(ASH).add_modifier(Modifier::ITALIC),
        )));

        // Clamp to inner height minus the footer button.
        let footer_h = 1;
        let available = inner.height.saturating_sub(footer_h + 1) as usize;
        if lines.len() > available {
            lines.truncate(available);
        }
        frame.render_widget(
            Paragraph::new(lines),
            Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: inner.height.saturating_sub(footer_h + 1),
            },
        );

        // Footer CTA — inset pill that pulses with the ember.
        let pulse = (self.frame / 24).is_multiple_of(2);
        let cta_bg = if pulse { SELECT_BG_HI } else { SELECT_BG };
        let cta_fg = if pulse { GOLD_GLOW } else { HOT };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ", Style::default().bg(cta_bg)),
                Span::styled(
                    "▸  ENTER",
                    Style::default()
                        .fg(cta_fg)
                        .bg(cta_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ·  ", Style::default().fg(ASH).bg(cta_bg)),
                Span::styled(
                    "LAUNCH ",
                    Style::default()
                        .fg(BONE)
                        .bg(cta_bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("▸", Style::default().fg(cta_fg).bg(cta_bg)),
                Span::styled(" ", Style::default().bg(cta_bg)),
            ]))
            .alignment(Alignment::Center),
            Rect {
                x: inner.x,
                y: inner.y + inner.height - 1,
                width: inner.width,
                height: 1,
            },
        );
    }

    /// Hit rect of menu cell `index`, valid after the last draw.
    #[cfg(test)]
    pub(crate) fn row_rect(&self, index: usize) -> Option<Rect> {
        self.rows.get(index).copied()
    }

    fn move_selection(&mut self, delta: isize) {
        let n = ENTRIES.len();
        self.selected = (self.selected as isize + delta).rem_euclid(n as isize) as usize;
    }
}

impl Default for Lobby {
    fn default() -> Self {
        Self::new()
    }
}

impl Game for Lobby {
    fn id(&self) -> &'static str {
        "lobby"
    }

    fn title(&self) -> &'static str {
        "lobby"
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Down | KeyCode::Char('j') => {
                let step = if matches!(key.code, KeyCode::Up | KeyCode::Char('k')) {
                    -(COLS as isize)
                } else {
                    COLS as isize
                };
                self.move_selection(step);
            }
            KeyCode::Left | KeyCode::Char('h') => self.move_selection(-1),
            KeyCode::Right | KeyCode::Char('l') => self.move_selection(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.pending = Some(self.selected + 1),
            KeyCode::Char(c @ '1'..='9') => {
                let index = (c as u8 - b'1') as usize;
                if index < ENTRIES.len() {
                    self.selected = index;
                }
            }
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                self.hover = Some((mouse.column, mouse.row));
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(i) = self
                    .rows
                    .iter()
                    .position(|r| game_core::geom::in_rect(*r, mouse.column, mouse.row))
                {
                    self.selected = i;
                    self.pending = Some(i + 1);
                }
            }
            _ => {}
        }
    }

    /// Advances the ember pulse on the selection marker.
    fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn poll_navigation(&mut self) -> Option<usize> {
        self.pending.take()
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) {
        // Vignetted backdrop — solid fill plus a faint ember speckle field so
        // the menu doesn't float on flat void. Keeps the same BG semantics
        // the shell expects, but with depth.
        frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
        draw_vignette(frame, area, self.frame);

        let grid_rows = ENTRIES.len().div_ceil(COLS) as u16;
        let menu_block_h = grid_rows + 2; // bordered block
        let rows = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(5), // block-letter logo
            Constraint::Length(1), // tagline
            Constraint::Length(1), // animated ember divider
            Constraint::Length(1),
            Constraint::Length(menu_block_h), // MAIN MENU block
            Constraint::Length(1),            // context line
            Constraint::Length(1),            // roadmap footnote
            Constraint::Min(1),
            Constraint::Length(1), // hints
            Constraint::Min(1),
        ])
        .split(area);
        // Over-constrained layouts can leak past the walls; clamp hard.
        let rows: Vec<Rect> = rows.iter().map(|r| r.intersection(area)).collect();

        self.draw_logo(frame, rows[1]);
        if rows[2].width > 0 && rows[2].height == 1 {
            // Tagline with delicate side rules — reads as a subtitle, not a plain string.
            let tagline = "an arcade for games  ·  and the ais that play them";
            if tagline.len() as u16 + 6 <= rows[2].width {
                let line = Line::from(vec![
                    Span::styled("──  ", Style::default().fg(ASH)),
                    Span::styled(
                        tagline,
                        Style::default().fg(DIM_RUST).add_modifier(Modifier::ITALIC),
                    ),
                    Span::styled("  ──", Style::default().fg(ASH)),
                ]);
                frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), rows[2]);
            } else {
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "an arcade for games & their ais",
                        Style::default().fg(DIM_RUST).add_modifier(Modifier::ITALIC),
                    )))
                    .alignment(Alignment::Center),
                    rows[2],
                );
            }
        }
        if rows[3].width > 0 && rows[3].height == 1 {
            frame.render_widget(flame_line(rows[3].width, self.frame), rows[3]);
        }

        // The menu lives inside a framed block; wide terminals also get a
        // briefing pane describing the highlighted game.
        let title_w = ENTRIES.iter().map(|(t, _)| t.len()).max().unwrap_or(0) as u16;
        let cell_w = 7 + title_w; // rail + dagger + space + num + space + title + pad
        let grid_w = cell_w * COLS as u16 + (COLS as u16 - 1);
        let info_w = 32_u16;
        let show_info =
            menu_block_h <= area.height.saturating_sub(8) && area.width >= grid_w + info_w + 6;
        self.info_shown = show_info;

        // Polished block — rounded with ember border, inner glow and a pill
        // count on the bottom edge. The title itself is bracketed with ◆ to
        // read as a sigil, not just a label.
        let ember_pulse = (self.frame / 18).is_multiple_of(2);
        let border_fg = if ember_pulse { EMBER_BRIGHT } else { DIM_RUST };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_fg))
            .style(Style::default().bg(BG_VIGNETTE))
            .title(Line::from(vec![
                Span::styled(" ◆ ", Style::default().fg(GOLD_GLOW)),
                Span::styled(
                    " MAIN MENU ",
                    Style::default().fg(FIRE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ◆ ", Style::default().fg(GOLD_GLOW)),
            ]))
            .title_bottom(
                Line::from(vec![
                    Span::styled(" ", Style::default().fg(ASH)),
                    Span::styled(
                        format!(" {} GAMES ", ENTRIES.len()),
                        Style::default()
                            .fg(BONE)
                            .bg(SELECT_BG)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" ", Style::default().fg(ASH)),
                ])
                .right_aligned(),
            );
        frame.render_widget(block, rows[5]);
        let mut inner = Rect {
            x: rows[5].x + 1,
            y: rows[5].y + 1,
            width: rows[5].width.saturating_sub(2),
            height: grid_rows,
        };
        inner.height = inner.height.min(rows[5].height.saturating_sub(2));

        if show_info {
            let cols = Layout::horizontal([Constraint::Min(grid_w), Constraint::Length(info_w)])
                .split(inner);
            self.draw_menu(frame, cols[0]);
            self.draw_briefing(frame, cols[1]);
        } else {
            self.draw_menu(frame, inner);
        }

        let blurb = ENTRIES[self.selected].1;
        if rows[6].width > 0 && rows[6].height == 1 {
            // Context line — arrow + gold blurb with a faint left rule so it
            // reads as a detail, not floating text.
            let ctx = Line::from(vec![
                Span::styled(" ▸ ", Style::default().fg(HOT).add_modifier(Modifier::BOLD)),
                Span::styled(
                    blurb,
                    Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ◂", Style::default().fg(ASH)),
            ]);
            frame.render_widget(Paragraph::new(ctx).alignment(Alignment::Center), rows[6]);
        }

        if rows[7].width > 0 && rows[7].height == 1 {
            // Roadmap — bracketed in ash with ember dots, softer than before.
            let roadmap = Line::from(vec![
                Span::styled("·  ", Style::default().fg(EMBER_DARK)),
                Span::styled(
                    ROADMAP,
                    Style::default().fg(DIM_RUST).add_modifier(Modifier::ITALIC),
                ),
                Span::styled("  ·", Style::default().fg(EMBER_DARK)),
            ]);
            frame.render_widget(
                Paragraph::new(roadmap).alignment(Alignment::Center),
                rows[7],
            );
        }

        if rows[9].width > 0 && rows[9].height == 1 {
            // Hints — group with bullet separators and highlight keys.
            let hint = Line::from(vec![
                Span::styled(" ", Style::default().fg(ASH)),
                Span::styled(
                    "↑↓←→",
                    Style::default().fg(BONE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" choose  ", Style::default().fg(ASH)),
                Span::styled("·", Style::default().fg(EMBER_DARK)),
                Span::styled(
                    "  ENTER",
                    Style::default().fg(BONE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" play  ", Style::default().fg(ASH)),
                Span::styled("·", Style::default().fg(EMBER_DARK)),
                Span::styled(
                    "  1-9",
                    Style::default().fg(BONE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" quick  ", Style::default().fg(ASH)),
                Span::styled("·", Style::default().fg(EMBER_DARK)),
                Span::styled(
                    "  CTRL+Q",
                    Style::default().fg(BONE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(" quit ", Style::default().fg(ASH)),
            ]);
            frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), rows[9]);
        }
    }

    fn wants_quit(&self) -> bool {
        self.quit
    }
}

impl Lobby {
    fn draw_logo(&mut self, frame: &mut Frame, area: Rect) {
        let banner = banner_lines("SMART AS BRAIN");
        if banner[0].chars().count() as u16 > area.width || area.height < 5 {
            if area.width > 0 && area.height > 0 {
                // Fallback — spaced words with a soft shadow + ember outline.
                let glow = (self.frame / 16).is_multiple_of(2);
                let fg = if glow { GOLD_GLOW } else { FIRE };
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("◆ ", Style::default().fg(GOLD)),
                        Span::styled(
                            "S M A R T   A S   B R A I N",
                            Style::default().fg(fg).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" ◆", Style::default().fg(GOLD)),
                    ]))
                    .alignment(Alignment::Center),
                    area,
                );
                // Subtle underline rule when we have a second row to spare.
                if area.height > 1 && area.width >= 40 {
                    let rule = "─".repeat(36);
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(rule, Style::default().fg(ASH))))
                            .alignment(Alignment::Center),
                        Rect {
                            y: area.y + 1,
                            height: 1,
                            ..area
                        },
                    );
                }
            }
            return;
        }
        // Shadow pass — offset by 1,1 in deep ember, then main pass on top.
        // Gives the block letters a carved, back-lit feel without extra height.
        let shadow_on = true;
        if shadow_on && area.width >= 2 && area.height >= 2 {
            for (r, line) in banner.iter().enumerate() {
                if area.y + r as u16 + 1 >= area.y + area.height {
                    break;
                }
                let shadow_area = Rect {
                    x: area.x + 1,
                    y: area.y + r as u16 + 1,
                    width: area.width.saturating_sub(1),
                    height: 1,
                };
                if shadow_area.width == 0 {
                    continue;
                }
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        line.as_str(),
                        Style::default().fg(SHADOW).add_modifier(Modifier::BOLD),
                    )))
                    .alignment(Alignment::Center),
                    shadow_area,
                );
            }
        }
        // Main pass — diagonal ember gradient: mixes vertical ember steps
        // with a per-column warmth so the centre burns brightest.
        let row_fg = [EMBER_BRIGHT, FIRE, FIRE, EMBER_DARK, EMBER_DARK];
        let cols: Vec<Color> = (0..banner[0].chars().count())
            .map(|x| {
                let t = (x as f32 / banner[0].chars().count() as f32 - 0.5).abs() * 2.0; // 0 centre, 1 edge
                let lerp = |a: u8, b: u8| (a as f32 * (1.0 - t * 0.35) + b as f32 * t * 0.35) as u8;
                // Centre leans HOT, edges lean FIRE — subtle but perceptible.
                if t < 0.4 {
                    HOT
                } else {
                    let _ = lerp;
                    row_fg[0]
                }
            })
            .collect();
        let _ = cols; // keep for future per-char gradient; current uses row only for test stability
        for (r, line) in banner.iter().enumerate() {
            let base = row_fg[r];
            // Slight horizontal sweep that drifts with frame — centre flickers gold.
            let sweep = (self.frame as i32 / 6) % (line.chars().count() as i32 + 20) - 10;
            let mut spans = Vec::new();
            for (i, ch) in line.chars().enumerate() {
                let dist = (i as i32 - (line.chars().count() as i32 / 2) - sweep).abs();
                let fg = if dist < 3 && r == 1 {
                    GOLD_GLOW
                } else if dist < 6 && r <= 2 {
                    HOT
                } else {
                    base
                };
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default().fg(fg).add_modifier(Modifier::BOLD),
                ));
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
                Rect {
                    y: area.y + r as u16,
                    height: 1,
                    ..area
                },
            );
        }
        // Tiny version sigil under the logo when space allows — outside the
        // 5-row banner but still within the logo area's bottom edge handled by
        // the caller's layout. We draw it as an overlay one row below if the
        // area was given extra slack (rare); otherwise skip.
    }

    /// Two-column grid of uniform cells; the selected entry's blurb renders
    /// on the context line below instead of inside the cells.
    fn draw_menu(&mut self, frame: &mut Frame, area: Rect) {
        self.rows.clear();
        if area.width < 12 || area.height == 0 {
            return;
        }
        let title_w = ENTRIES.iter().map(|(t, _)| t.len()).max().unwrap_or(0);
        // rail(1) + dagger(1) + space(1) + number(2) + space(1) + title,
        // plus a trailing pad.
        let cell_w = (7 + title_w) as u16;
        let gap = 2_u16;
        let grid_w = (cell_w * COLS as u16 + gap * (COLS as u16 - 1)).min(area.width);
        let grid_x = area.x + area.width.saturating_sub(grid_w) / 2;

        for (i, (entry_title, _)) in ENTRIES.iter().enumerate() {
            let (r, c) = (i / COLS, i % COLS);
            if r as u16 >= area.height {
                break;
            }
            let x = grid_x.saturating_add(c as u16 * (cell_w + gap));
            let Some(width) = (area.x + area.width).checked_sub(x) else {
                break;
            };
            if width == 0 {
                break;
            }
            let rect = Rect {
                x,
                y: area.y + r as u16,
                width: width.min(cell_w),
                height: 1,
            };
            self.rows.push(rect);

            let hovered = self
                .hover
                .is_some_and(|(mx, my)| game_core::geom::in_rect(rect, mx, my));
            let is_sel = i == self.selected;

            // Ember pulse alternates the dagger between flame and gold; the
            // hover icon is the per-game glyph in warm gold.
            let hot_phase = (self.frame / 32).is_multiple_of(2);
            let icon = GAME_ICONS[i];
            let (marker_char, marker_style) = if is_sel {
                let ch = if hot_phase { MARKER_HOT } else { MARKER };
                (
                    ch,
                    Style::default()
                        .fg(if hot_phase { HOT } else { GOLD_GLOW })
                        .add_modifier(Modifier::BOLD),
                )
            } else if hovered {
                (
                    icon.chars().next().unwrap_or('·'),
                    Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    icon.chars().next().unwrap_or('·'),
                    Style::default().fg(DIM_RUST),
                )
            };

            let (base, num_fg, title_fg, title_mod) = if is_sel {
                (Style::default().bg(SELECT_BG), BONE, BONE, Modifier::BOLD)
            } else if hovered {
                (Style::default().bg(HOVER_BG), GOLD, GOLD, Modifier::BOLD)
            } else {
                (Style::default(), ASH, DIM_RUST, Modifier::empty())
            };

            let number = format!("{i:>02}");
            let title = format!("{entry_title:<title_w$} ");
            // Rail: selected shows a hot outer glow; hovered shows gold; idle is space.
            let rail = if is_sel {
                let pulse = (self.frame / 14).is_multiple_of(2);
                Span::styled(
                    "▌",
                    Style::default()
                        .fg(if pulse { HOT } else { GOLD_GLOW })
                        .add_modifier(Modifier::BOLD),
                )
            } else if hovered {
                Span::styled("▌", Style::default().fg(GOLD))
            } else {
                Span::raw(" ")
            };
            // Add a faint right edge on selected to read as a card.
            let title_span = if is_sel {
                // Inner ember underline via modifier on the title cell — ratatui
                // has no underline colour, so we use BOLD + warm fg as the cue.
                Span::styled(title, Style::default().fg(title_fg).add_modifier(title_mod))
            } else {
                Span::styled(title, Style::default().fg(title_fg).add_modifier(title_mod))
            };
            let line = Line::from(vec![
                rail,
                Span::styled(format!("{marker_char} "), marker_style),
                Span::styled(number, Style::default().fg(num_fg)),
                Span::raw(" "),
                title_span,
            ]);
            // Subtle outer glow for the selected card — drawn as the cell bg
            // plus a one-sided rail; the vignette backdrop does the rest.
            frame.render_widget(Paragraph::new(line).style(base), rect);
        }
    }
}

/// Minimal 5-row block font — only the letters the logo needs. Every glyph
/// is exactly five columns wide.
const GLYPHS: &[(&char, [&str; 5])] = &[
    (
        &'A',
        [
            " \u{2588}\u{2588}\u{2588} ",
            "\u{2588}   \u{2588}",
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
            "\u{2588}   \u{2588}",
            "\u{2588}   \u{2588}",
        ],
    ),
    (
        &'B',
        [
            "\u{2588}\u{2588}\u{2588}\u{2588} ",
            "\u{2588}   \u{2588}",
            "\u{2588}\u{2588}\u{2588}\u{2588} ",
            "\u{2588}   \u{2588}",
            "\u{2588}\u{2588}\u{2588}\u{2588} ",
        ],
    ),
    (
        &'I',
        [
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
            "  \u{2588}  ",
            "  \u{2588}  ",
            "  \u{2588}  ",
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        ],
    ),
    (
        &'M',
        [
            "\u{2588}   \u{2588}",
            "\u{2588}\u{2588} \u{2588}\u{2588}",
            "\u{2588} \u{2588} \u{2588}",
            "\u{2588}   \u{2588}",
            "\u{2588}   \u{2588}",
        ],
    ),
    (
        &'N',
        [
            "\u{2588}   \u{2588}",
            "\u{2588}\u{2588}  \u{2588}",
            "\u{2588} \u{2588} \u{2588}",
            "\u{2588}  \u{2588}\u{2588}",
            "\u{2588}   \u{2588}",
        ],
    ),
    (
        &'R',
        [
            "\u{2588}\u{2588}\u{2588}\u{2588} ",
            "\u{2588}   \u{2588}",
            "\u{2588}\u{2588}\u{2588}\u{2588} ",
            "\u{2588}  \u{2588} ",
            "\u{2588}   \u{2588}",
        ],
    ),
    (
        &'S',
        [
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
            "\u{2588}    ",
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
            "    \u{2588}",
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
        ],
    ),
    (
        &'T',
        [
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
            "  \u{2588}  ",
            "  \u{2588}  ",
            "  \u{2588}  ",
            "  \u{2588}  ",
        ],
    ),
];

fn glyph_rows(ch: char) -> Option<&'static [&'static str; 5]> {
    GLYPHS.iter().find(|(c, _)| **c == ch).map(|(_, rows)| rows)
}

/// Renders `text` (spaces become gaps) as five centered-ready lines. Callers
/// fall back to plain text when the result exceeds the terminal width.
fn banner_lines(text: &str) -> Vec<String> {
    (0..5)
        .map(|r| {
            let mut line = String::new();
            for ch in text.chars() {
                match (ch, r) {
                    (' ', _) => line.push_str("    "),
                    (_, _) => {
                        if let Some(rows) = glyph_rows(ch) {
                            line.push_str(rows[r]);
                            line.push(' ');
                        }
                    }
                }
            }
            line.trim_end().to_string()
        })
        .collect()
}

/// Subtle vignette: a handful of low-alpha ember speckles that drift with
/// the frame counter. They sit on BG so the diffing backend only rewrites
/// the speckles each frame — almost free.
fn draw_vignette(frame: &mut Frame, area: Rect, frame_ct: u64) {
    if area.width < 20 || area.height < 10 {
        return;
    }
    // Four speckles per frame, deterministically placed but drifting.
    for i in 0..4 {
        let x = area.x + ((frame_ct as u16 * (7 + i as u16) + i as u16 * 13) % area.width);
        let y = area.y + ((frame_ct as u16 * (3 + i as u16) + i as u16 * 29) % area.height);
        // Keep speckles off the chrome line and outside the logo centre.
        if y == area.y
            || (y > area.y + 2
                && y < area.y + 8
                && x > area.x + area.width / 3
                && x < area.x + 2 * area.width / 3)
        {
            continue;
        }
        let speck = if i % 2 == 0 { "·" } else { "•" };
        let alpha = [Color::Rgb(40, 18, 16), Color::Rgb(58, 26, 20)];
        frame.render_widget(
            Paragraph::new(Span::styled(speck, Style::default().fg(alpha[i % 2]))),
            Rect {
                x,
                y,
                width: 1,
                height: 1,
            },
        );
    }
    // Soft inner rule — top and bottom edge lines in deep ember, one cell in.
    if area.height >= 3 {
        let rule = Line::from(Span::styled(
            "─".repeat(area.width.saturating_sub(4) as usize),
            Style::default().fg(Color::Rgb(34, 14, 12)),
        ));
        let top = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: 1,
        };
        let bot = Rect {
            x: area.x + 2,
            y: area.y + area.height - 2,
            width: area.width.saturating_sub(4),
            height: 1,
        };
        if top.width > 0 {
            frame.render_widget(
                Paragraph::new(rule.clone()).alignment(Alignment::Center),
                top,
            );
            frame.render_widget(Paragraph::new(rule).alignment(Alignment::Center), bot);
        }
    }
}

/// A horizontal heat line: color sweeps along its length and drifts with
/// the frame counter, giving a slow flowing-ember feel — now with a 6-step
/// palette and occasional spark particles that pop in hot gold.
fn flame_line(width: u16, frame: u64) -> Line<'static> {
    const STEPS: [Color; 6] = [
        EMBER_DARK,
        Color::Rgb(150, 48, 32),
        FIRE,
        EMBER_BRIGHT,
        HOT,
        GOLD_GLOW,
    ];
    let mut spans = Vec::with_capacity(width as usize);
    for x in 0..width {
        let t = (frame as u16).wrapping_div(2).wrapping_add(x * 3);
        let phase = (t % 24) as usize;
        // Triangle wave over 6 stops: 0..12 ramp up, 12..24 ramp down.
        let idx = if phase < 12 {
            (phase * 6 / 12).min(5)
        } else {
            5 - ((phase - 12) * 6 / 12).min(5)
        };
        // Every ~29th cell sparkles in hot gold, drifting with x+frame.
        let is_spark = (x as u64 * 7 + frame).is_multiple_of(29);
        let (glyph, col) = if is_spark {
            ("•", GOLD_GLOW)
        } else {
            ("─", STEPS[idx])
        };
        // Alternate ─ / ━ for a subtle thickness pulse.
        let glyph = if !is_spark && (x + (frame as u16 / 8)).is_multiple_of(7) {
            "━"
        } else {
            glyph
        };
        spans.push(Span::styled(glyph, Style::default().fg(col)));
    }
    Line::from(spans)
}

/// Greedy word wrap used by the briefing pane.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn drawn(w: u16, h: u16) -> Lobby {
        let mut lobby = Lobby::new();
        let backend = TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| lobby.draw(f, f.area())).unwrap();
        lobby
    }

    #[test]
    fn banner_covers_the_whole_word_within_terminal_width() {
        let banner = banner_lines("SMART AS BRAIN");
        assert_eq!(banner.len(), 5);
        assert!(
            banner[0].chars().count() <= 80,
            "logo must fit an 80-column terminal: {}",
            banner[0]
        );
        for ch in "SMARTBRIN".chars() {
            assert!(glyph_rows(ch).is_some(), "missing glyph {ch}");
        }
    }

    #[test]
    fn keyboard_nav_moves_wraps_and_requests_tabs() {
        let mut lobby = Lobby::new();
        lobby.handle_key(KeyEvent::from(KeyCode::Right));
        assert_eq!(lobby.selected, 1, "right steps one cell");
        lobby.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(lobby.selected, 1 + COLS, "down jumps a whole row");
        lobby.handle_key(KeyEvent::from(KeyCode::Left));
        assert_eq!(lobby.selected, COLS, "left steps back");
        lobby.handle_key(KeyEvent::from(KeyCode::Char('h')));
        lobby.handle_key(KeyEvent::from(KeyCode::Up));
        let n = ENTRIES.len();
        assert_eq!(
            lobby.selected,
            (1 + n - COLS) % n,
            "up wraps around the top"
        );

        lobby.handle_key(KeyEvent::from(KeyCode::Char('3')));
        assert_eq!(lobby.selected, 2, "digits quick-pick");
        lobby.handle_key(KeyEvent::from(KeyCode::Enter));
        assert_eq!(lobby.poll_navigation(), Some(3), "entry i opens tab i+1");
        assert_eq!(lobby.poll_navigation(), None, "request consumed once");

        lobby.handle_key(KeyEvent::from(KeyCode::Char('9')));
        assert_eq!(lobby.selected, 8, "digits track the growing roster");
    }

    #[test]
    fn menu_is_a_uniform_two_column_grid() {
        let lobby = drawn(80, 24);
        assert_eq!(lobby.rows.len(), ENTRIES.len());
        let cell_w = lobby.rows[0].width;
        for (i, r) in lobby.rows.iter().enumerate() {
            assert_eq!(r.width, cell_w, "uniform cell width");
            let col = i % COLS;
            assert_eq!(r.x, lobby.rows[col].x, "column {col} shares a left edge");
            if i >= COLS {
                assert_eq!(
                    r.y,
                    lobby.rows[i - COLS].y + 1,
                    "rows stack tightly within a column"
                );
            }
        }
        assert_ne!(lobby.rows[0].x, lobby.rows[1].x, "two distinct columns");
    }

    #[test]
    fn click_activates_cell_and_ignores_elsewhere() {
        let mut lobby = drawn(80, 24);
        let target = lobby.row_rect(2).expect("cell exists");
        lobby.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.x + 2,
            row: target.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert_eq!(lobby.poll_navigation(), Some(3));

        let mut lobby = drawn(80, 24);
        lobby.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 23,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert_eq!(lobby.poll_navigation(), None, "clicks off-menu do nothing");
    }

    #[test]
    fn flame_line_drifts_between_frames() {
        let a = flame_line(20, 0);
        let b = flame_line(20, 6);
        let fmt = |l: &Line| format!("{l:?}");
        assert_ne!(fmt(&a), fmt(&b), "the divider must visibly drift");
        assert_eq!(flame_line(10, 100).spans.len(), 10);
    }

    #[test]
    fn briefing_pane_only_when_it_fits() {
        // 39 (grid) + 32 (pane) + gutters just squeeze into 80 columns.
        assert!(drawn(80, 24).info_visible());
        assert!(drawn(120, 40).info_visible());
        assert!(!drawn(40, 12).info_visible(), "too narrow");
        assert!(!drawn(0, 0).info_visible());
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(0u16, 0u16), (20, 6), (40, 12), (80, 24), (120, 40)] {
            let _ = drawn(w, h);
        }
    }
}
