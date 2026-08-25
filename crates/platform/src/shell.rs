use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use game_core::Game;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

const BG: Color = Color::Rgb(13, 15, 23);
const BAR_BG: Color = Color::Rgb(10, 12, 20);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const GIVEN_FG: Color = Color::Rgb(226, 230, 244);
const DIM_FG: Color = Color::Rgb(112, 120, 140);
const FAINT_FG: Color = Color::Rgb(84, 92, 112);
const SELECT_BG: Color = Color::Rgb(52, 66, 104);
const HOVER_BG: Color = Color::Rgb(48, 54, 74);

/// How the tab bar moves focus between games.
pub enum TabNav {
    Prev,
    Next,
    Index(usize),
}

pub struct Shell {
    games: Vec<Box<dyn Game>>,
    active: usize,
    /// Last known mouse position, for tab hover styling.
    mouse: Option<(u16, u16)>,
    /// Click target for the brand label (returns to the lobby).
    brand_rect: Option<Rect>,
    /// Click targets for tab labels, rebuilt every frame.
    tab_rects: Vec<Rect>,
}

impl Shell {
    pub fn new(games: Vec<Box<dyn Game>>) -> Self {
        Self {
            games,
            active: 0,
            mouse: None,
            brand_rect: None,
            tab_rects: Vec::new(),
        }
    }

    pub fn active(&self) -> &dyn Game {
        self.games[self.active].as_ref()
    }

    pub fn active_mut(&mut self) -> &mut dyn Game {
        self.games[self.active].as_mut()
    }

    pub fn switch_to(&mut self, nav: TabNav) {
        let count = self.games.len();
        self.active = match nav {
            TabNav::Prev => (self.active + count - 1) % count,
            TabNav::Next => (self.active + 1) % count,
            TabNav::Index(i) if i < count => i,
            TabNav::Index(_) => return,
        };
    }

    /// Consumes a mouse event when it lands on a tab label; returns true in
    /// that case so the active game never sees the click.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                self.mouse = Some((mouse.column, mouse.row));
                return false;
            }
            MouseEventKind::Down(MouseButton::Left) => {}
            _ => return false,
        }
        let hit = self
            .tab_rects
            .iter()
            .position(|r| in_rect(*r, mouse.column, mouse.row));
        match hit {
            Some(i) => {
                self.switch_to(TabNav::Index(i));
                true
            }
            None => {
                // Brand click returns to the lobby.
                if let Some(brand) = self.brand_rect
                    && in_rect(brand, mouse.column, mouse.row)
                {
                    self.switch_to(TabNav::Index(0));
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Advances every game one frame (AI turns, timers, animations).
    pub fn tick_all(&mut self) {
        for game in &mut self.games {
            game.tick();
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
        self.draw_tab_bar(frame, rows[0]);
        self.games[self.active].draw(frame, rows[1]);
    }

    fn draw_tab_bar(&mut self, frame: &mut Frame, bar: Rect) {
        frame.render_widget(Block::default().style(Style::default().bg(BAR_BG)), bar);

        // Brand on the left; clicking it returns to the lobby (tab 0).
        const BRAND: &str = " smartasbrain ";
        let brand_w = BRAND.chars().count() as u16;
        self.brand_rect = None;
        if brand_w < bar.width {
            let rect = Rect {
                x: bar.x,
                y: bar.y,
                width: brand_w,
                height: 1,
            };
            self.brand_rect = Some(rect);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    BRAND,
                    Style::default().fg(ACCENT_FG).add_modifier(Modifier::BOLD),
                ))),
                rect,
            );
        }

        const PAD: u16 = 1;
        let widths: Vec<u16> = self
            .games
            .iter()
            .map(|g| g.title().chars().count() as u16 + PAD * 2)
            .collect();

        let mut x = bar.x + brand_w.min(bar.width);
        let tabs_end = bar.x
            + bar
                .width
                .saturating_sub("[ ] tabs · alt+1-9 jump · ctrl+q quit".chars().count() as u16 + 1);
        self.tab_rects.clear();
        for (i, game) in self.games.iter().enumerate() {
            let remaining = tabs_end.saturating_sub(x);
            let width = widths[i].min(remaining).max(1);
            let rect = Rect {
                x,
                y: bar.y,
                width,
                height: 1,
            };
            self.tab_rects.push(rect);
            x = x.saturating_add(width);
            if x >= bar.x + bar.width {
                break;
            }

            let hovered = self.mouse.is_some_and(|(mx, my)| in_rect(rect, mx, my));
            let style = if i == self.active {
                Style::default()
                    .fg(GIVEN_FG)
                    .bg(SELECT_BG)
                    .add_modifier(Modifier::BOLD)
            } else if hovered {
                Style::default()
                    .fg(ACCENT_FG)
                    .bg(HOVER_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(DIM_FG)
            };
            let label = format!("{:^width$}", game.title(), width = width as usize);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(label, style))).alignment(Alignment::Center),
                rect,
            );
        }

        let hint = "[ ] tabs · alt+1-9 jump · ctrl+q quit";
        let hint_w = hint.chars().count() as u16;
        if hint_w < bar.width.saturating_sub(x.saturating_sub(bar.x)) {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    hint,
                    Style::default().fg(FAINT_FG),
                )))
                .alignment(Alignment::Right),
                Rect {
                    x: bar.x + bar.width - hint_w,
                    y: bar.y,
                    width: hint_w,
                    height: 1,
                },
            );
        }
    }
}

fn in_rect(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lobby::Lobby;
    use crossterm::event::KeyModifiers;
    use ratatui::backend::TestBackend;

    fn shell() -> Shell {
        Shell::new(vec![
            Box::new(Lobby::new()),
            Box::new(sudoku::App::new()),
            Box::new(go::GoGame::new()),
        ])
    }

    #[test]
    fn tab_nav_wraps_and_indexes() {
        let mut shell = shell();
        assert_eq!(shell.games[shell.active].id(), "lobby");
        shell.switch_to(TabNav::Next);
        assert_eq!(shell.games[shell.active].id(), "sudoku");
        shell.switch_to(TabNav::Next);
        assert_eq!(shell.games[shell.active].id(), "go");
        shell.switch_to(TabNav::Next);
        assert_eq!(shell.games[shell.active].id(), "lobby", "wraps around");
        shell.switch_to(TabNav::Prev);
        assert_eq!(shell.games[shell.active].id(), "go");
        shell.switch_to(TabNav::Index(1));
        assert_eq!(shell.games[shell.active].id(), "sudoku");
        shell.switch_to(TabNav::Index(99));
        assert_eq!(
            shell.games[shell.active].id(),
            "sudoku",
            "out-of-range ignored"
        );
    }

    #[test]
    fn brand_click_returns_to_lobby() {
        let mut shell = shell();
        shell.switch_to(TabNav::Index(2));
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| shell.draw(f)).unwrap();

        let brand = shell.brand_rect.expect("brand rendered at 80 cols");
        let swallowed = shell.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: brand.x + 2,
            row: brand.y,
            modifiers: KeyModifiers::NONE,
        });
        assert!(swallowed, "brand click belongs to the navbar");
        assert_eq!(shell.games[shell.active].id(), "lobby");
    }

    #[test]
    fn tab_click_switches_games_and_swallows_click() {
        let mut shell = shell();
        let backend = TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| shell.draw(f)).unwrap();

        assert_eq!(shell.tab_rects.len(), 3, "one click target per game tab");
        let target = shell.tab_rects[2];
        let swallowed = shell.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: target.x + 1,
            row: target.y,
            modifiers: KeyModifiers::NONE,
        });
        assert!(swallowed);
        assert_eq!(shell.games[shell.active].id(), "go");

        let outside = shell.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 79,
            row: 23,
            modifiers: KeyModifiers::NONE,
        });
        assert!(!outside, "clicks below the bar belong to the game");
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(80u16, 24u16), (40, 10), (200, 55), (20, 5)] {
            let mut s = shell();
            let backend = TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| s.draw(f)).unwrap();
        }
    }
}
