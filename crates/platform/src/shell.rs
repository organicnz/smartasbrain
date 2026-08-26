use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use game_core::{Game, persist};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

const BG: Color = Color::Rgb(13, 15, 23);
const CHROME_BG: Color = Color::Rgb(24, 28, 42);
const CHROME_HOVER_BG: Color = Color::Rgb(48, 54, 74);
const ACCENT_FG: Color = Color::Rgb(122, 162, 247);
const QUIT_FG: Color = Color::Rgb(200, 60, 50);
const BONE_FG: Color = Color::Rgb(216, 208, 194);
const FAINT: Color = Color::Rgb(84, 92, 112);
// Warm chrome for the lobby — ember-tinged so the DOOM menu feels seamless.
const LOBBY_BG: Color = Color::Rgb(28, 14, 14);
const LOBBY_HOVER_BG: Color = Color::Rgb(54, 22, 18);
const GOLD: Color = Color::Rgb(230, 178, 78);
const HOT: Color = Color::Rgb(255, 138, 48);
const EMBER: Color = Color::Rgb(126, 44, 28);

/// How navigation moves between games.
pub enum TabNav {
    Prev,
    Next,
    Index(usize),
    /// Pop the shell's navigation history (Alt+Left, chrome button).
    Back,
}

enum ChromeAction {
    Back,
    Lobby,
    Quit,
}

/// History cap so marathon sessions can't grow it unboundedly.
const HISTORY_CAP: usize = 64;

pub struct Shell {
    games: Vec<Box<dyn Game>>,
    active: usize,
    quit_requested: bool,
    history: Vec<usize>,
    /// Chrome hit rects, rebuilt every frame.
    back_btn: Option<Rect>,
    lobby_btn: Option<Rect>,
    quit_btn: Option<Rect>,
    chrome_hover: Option<(u16, u16)>,
    /// Per-game restore bookkeeping: blobs apply exactly once per session.
    restored: Vec<bool>,
}

impl Shell {
    pub fn new(games: Vec<Box<dyn Game>>) -> Self {
        let restored = vec![false; games.len()];
        Self {
            games,
            active: 0,
            quit_requested: false,
            history: Vec::new(),
            back_btn: None,
            lobby_btn: None,
            quit_btn: None,
            chrome_hover: None,
            restored,
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
        let target = match nav {
            TabNav::Back => {
                self.go_back();
                return;
            }
            TabNav::Prev => (self.active + count - 1) % count,
            TabNav::Next => (self.active + 1) % count,
            TabNav::Index(i) if i < count => i,
            TabNav::Index(_) => return,
        };
        if target != self.active {
            if self.history.last() != Some(&self.active) {
                self.history.push(self.active);
            }
            if self.history.len() > HISTORY_CAP {
                self.history.remove(0);
            }
        }
        self.activate(target);
    }

    /// Returns to the previous screen; a no-op with an empty history.
    pub fn go_back(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.activate(prev);
        }
    }

    fn activate(&mut self, target: usize) {
        self.active = target;
        // Lazily rehydrate a saved game the first time it is opened.
        if !self.restored[target] {
            self.restored[target] = true;
            let id = self.games[target].id();
            if let Some(blob) = persist::load(id) {
                let _ = self.games[target].restore(&blob);
            }
        }
    }

    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// Consumes mouse events aimed at the shell's own chrome; everything
    /// else falls through to the active game untouched.
    pub fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            if matches!(mouse.kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) {
                self.chrome_hover = Some((mouse.column, mouse.row));
            }
            return false;
        }
        for (rect, action) in [
            (self.back_btn, ChromeAction::Back),
            (self.lobby_btn, ChromeAction::Lobby),
            (self.quit_btn, ChromeAction::Quit),
        ] {
            if let Some(r) = rect
                && game_core::geom::in_rect(r, mouse.column, mouse.row)
            {
                match action {
                    ChromeAction::Back => self.go_back(),
                    ChromeAction::Lobby => self.switch_to(TabNav::Index(0)),
                    ChromeAction::Quit => self.quit_requested = true,
                }
                return true;
            }
        }
        false
    }

    /// Persists the active tab plus every savable game snapshot. Called on
    /// graceful shutdown.
    pub fn persist_session(&self) {
        let _ = persist::save("session", &format!("v1|{}", self.active));
        for game in &self.games {
            match game.snapshot() {
                Some(blob) => {
                    let _ = persist::save(game.id(), &blob);
                }
                None => persist::clear(game.id()),
            }
        }
    }

    /// Rehydrates the last active tab before the first draw.
    pub fn resume_session(&mut self) {
        if let Some(blob) = persist::load("session")
            && let Some(idx) = blob
                .strip_prefix("v1|")
                .and_then(|n| n.parse::<usize>().ok())
        {
            self.switch_to(TabNav::Index(idx));
        }
    }

    /// Advances every game one frame (AI turns, timers, animations).
    pub fn tick_all(&mut self) {
        for game in &mut self.games {
            game.tick();
        }
    }

    /// The active game owns the screen; the shell paints only its chrome.
    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
        self.games[self.active].draw(frame, area);
        self.draw_chrome(frame, area);
    }

    fn draw_chrome(&mut self, frame: &mut Frame, area: Rect) {
        const BACK_LABEL: &str = " \u{2190} back ";
        const LOBBY_LABEL: &str = " \u{2302} lobby ";
        const QUIT_LABEL: &str = " \u{2715} quit ";
        let hover = self.chrome_hover;
        let quit_w = QUIT_LABEL.chars().count() as u16;
        let lobby_w = LOBBY_LABEL.chars().count() as u16;
        let back_w = BACK_LABEL.chars().count() as u16;
        if area.width < quit_w + lobby_w + back_w + 1 || area.height == 0 {
            self.back_btn = None;
            self.lobby_btn = None;
            self.quit_btn = None;
            return;
        }
        let q_rect = Rect {
            x: area.x + area.width - quit_w,
            y: area.y,
            width: quit_w,
            height: 1,
        };
        let l_rect = Rect {
            x: q_rect.x.saturating_sub(lobby_w),
            y: area.y,
            width: lobby_w,
            height: 1,
        };
        let b_rect = Rect {
            x: l_rect.x.saturating_sub(back_w),
            y: area.y,
            width: back_w,
            height: 1,
        };
        self.quit_btn = Some(q_rect);
        self.lobby_btn = Some(l_rect);
        // Dim when there is nowhere to go back to.
        let can_back = !self.history.is_empty();
        self.back_btn = Some(b_rect);

        // Lobby-aware palette — warm ember when on the menu, cool steel elsewhere.
        let is_lobby = self.games[self.active].id() == "lobby";
        let (chrome_bg, hover_bg) = if is_lobby {
            (LOBBY_BG, LOBBY_HOVER_BG)
        } else {
            (CHROME_BG, CHROME_HOVER_BG)
        };
        let style_for = |r: Rect, fg: Color| -> Style {
            if hover.is_some_and(|(c, row)| game_core::geom::in_rect(r, c, row)) {
                Style::default().fg(fg).bg(hover_bg)
            } else {
                Style::default().fg(fg).bg(chrome_bg)
            }
        };

        // Centered breadcrumb / title pill — only when there's room between the
        // left edge and the chrome cluster. It gives the lobby a sense of place
        // without stealing hit targets.
        let title = self.games[self.active].title().to_uppercase();
        let pill = format!(" ◆ {title} ◆ ");
        let pill_w = pill.chars().count() as u16 + 2;
        let chrome_left = b_rect.x;
        let available = chrome_left.saturating_sub(area.x + 2);
        if available >= pill_w + 4 && area.width >= 40 {
            let pill_x = area.x + (chrome_left.saturating_sub(area.x).saturating_sub(pill_w)) / 2;
            let pill_rect = Rect {
                x: pill_x,
                y: area.y,
                width: pill_w,
                height: 1,
            };
            let pill_style = if is_lobby {
                Style::default().fg(GOLD).bg(chrome_bg).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(ACCENT_FG).bg(chrome_bg).add_modifier(Modifier::BOLD)
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(" ", Style::default().bg(chrome_bg)),
                    Span::styled(pill, pill_style),
                    Span::styled(" ", Style::default().bg(chrome_bg)),
                ]))
                .alignment(Alignment::Center),
                pill_rect,
            );
        }

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                BACK_LABEL,
                style_for(b_rect, if can_back { BONE_FG } else { FAINT }).add_modifier(
                    if can_back {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    },
                ),
            ))),
            b_rect,
        );
        // On the lobby the home button reads as the ember accent.
        let lobby_fg = if is_lobby { GOLD } else { ACCENT_FG };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                LOBBY_LABEL,
                style_for(l_rect, lobby_fg).add_modifier(Modifier::BOLD),
            ))),
            l_rect,
        );
        let quit_style = if is_lobby {
            style_for(q_rect, HOT)
        } else {
            style_for(q_rect, QUIT_FG)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(QUIT_LABEL, quit_style))),
            q_rect,
        );

        // Thin ember rule under the chrome on the lobby — ties the bar into
        // the vignette backdrop without adding height.
        if is_lobby && area.height > 1 && area.width >= 20 {
            let rule_rect = Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: 1,
            };
            let rule = "─".repeat(area.width as usize);
            frame.render_widget(
                Paragraph::new(Span::styled(rule, Style::default().fg(EMBER))),
                rule_rect,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lobby::Lobby;
    use ratatui::backend::TestBackend;

    fn shell() -> Shell {
        Shell::new(vec![
            Box::new(Lobby::new()),
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
        ])
    }

    fn drawn_shell(w: u16, h: u16) -> Shell {
        let mut s = shell();
        let backend = TestBackend::new(w, h);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| s.draw(f)).unwrap();
        s
    }

    #[test]
    fn tab_nav_wraps_and_indexes() {
        let mut shell = shell();
        assert_eq!(shell.games[shell.active].id(), "lobby");
        for expected in [
            "sudoku",
            "go",
            "chess",
            "checkers",
            "backgammon",
            "reversi",
            "morris",
            "connect4",
            "mancala",
            "dots",
            "yahtzee",
            "dominoes",
            "battleship",
        ] {
            shell.switch_to(TabNav::Next);
            assert_eq!(shell.games[shell.active].id(), expected);
        }
        shell.switch_to(TabNav::Next);
        assert_eq!(shell.games[shell.active].id(), "lobby", "wraps around");
        shell.switch_to(TabNav::Prev);
        assert_eq!(shell.games[shell.active].id(), "battleship");
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
    fn back_pops_history_in_order_and_stops_when_empty() {
        let mut s = shell();
        s.switch_to(TabNav::Index(3)); // chess   (hist: lobby)
        s.switch_to(TabNav::Index(7)); // morris  (hist: lobby, chess)
        assert_eq!(s.games[s.active].id(), "morris");

        s.go_back();
        assert_eq!(s.games[s.active].id(), "chess");
        s.go_back();
        assert_eq!(s.games[s.active].id(), "lobby");
        let before = s.active;
        s.go_back();
        assert_eq!(s.active, before, "empty history is a no-op");

        // Back never records itself; forward jumps re-seed the stack.
        s.switch_to(TabNav::Index(1));
        s.go_back();
        assert_eq!(s.games[s.active].id(), "lobby", "returned to pre-jump seat");
    }

    #[test]
    fn back_button_click_navigates_and_is_consumed() {
        let mut s = drawn_shell(80, 24);
        s.switch_to(TabNav::Index(2)); // go
        // Redraw so the chrome rects reflect the new state.
        {
            let backend = TestBackend::new(80, 24);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| s.draw(f)).unwrap();
        }
        let back = s.back_btn.expect("back button drawn at 80 cols");
        let swallowed = s.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: back.x + 2,
            row: back.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert!(swallowed);
        assert_eq!(s.games[s.active].id(), "lobby", "back returns to origin");
        assert!(!s.quit_requested());
    }

    #[test]
    fn chrome_buttons_claim_their_corner_and_act() {
        let mut shell = drawn_shell(80, 24);
        let lobby = shell.lobby_btn.expect("chrome drawn at 80 cols");
        let quit = shell.quit_btn.expect("chrome drawn at 80 cols");

        shell.switch_to(TabNav::Index(3)); // sit on chess
        let swallowed = shell.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: lobby.x + 2,
            row: lobby.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert!(swallowed, "lobby click belongs to the chrome");
        assert_eq!(shell.games[shell.active].id(), "lobby");

        let swallowed = shell.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: quit.x + 2,
            row: quit.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert!(swallowed);
        assert!(shell.quit_requested());

        // Anything outside the two rects passes straight through.
        let passthrough = shell.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 12,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        assert!(!passthrough);
    }

    #[test]
    fn session_persists_active_tab_across_instances() {
        let tmp = std::env::temp_dir().join(format!("sab-shell-session-{}", std::process::id()));
        game_core::persist::set_data_dir(tmp.clone());
        {
            let mut s = shell();
            s.switch_to(TabNav::Index(3));
            s.persist_session();
        }
        let mut revived = shell();
        revived.resume_session();
        assert_eq!(revived.games[revived.active].id(), "chess");
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[test]
    fn renders_across_sizes_without_panicking() {
        for (w, h) in [(80u16, 24u16), (40, 10), (200, 55), (20, 5), (1, 1), (0, 0)] {
            let _ = drawn_shell(w, h);
        }
    }
}
