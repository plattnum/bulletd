use std::io;
use std::time::Duration;

use chrono::{Local, NaiveDate};
use color_eyre::eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use bulletd_core::config::Config;
use bulletd_core::model::{Bullet, BulletStatus};
use bulletd_core::ops::Store;

use crate::theme::Theme;

/// The main TUI application state.
pub struct App {
    pub(crate) store: Store,
    theme: Theme,
    should_quit: bool,
    /// The date currently being viewed.
    current_date: NaiveDate,
    /// Bullets loaded for the current date.
    bullets: Vec<Bullet>,
    /// Currently selected bullet index.
    selected: usize,
    /// Status message shown at the bottom (e.g., error or confirmation).
    status_message: Option<String>,
}

impl App {
    pub fn new(config: &Config) -> Self {
        let data_dir = bulletd_core::config::resolve_data_dir(&config.general.data_dir);
        let store = Store::new(data_dir);
        let theme = Theme::from_config(&config.theme);
        let current_date = Local::now().date_naive();

        let mut app = Self {
            store,
            theme,
            should_quit: false,
            current_date,
            bullets: vec![],
            selected: 0,
            status_message: None,
        };
        app.reload_bullets();
        app
    }

    /// Reload bullets from disk for the current date.
    fn reload_bullets(&mut self) {
        match self.store.list_bullets(self.current_date, None, None) {
            Ok(bullets) => {
                self.bullets = bullets;
                self.status_message = None;
                // Clamp selection
                if self.selected >= self.bullets.len() && !self.bullets.is_empty() {
                    self.selected = self.bullets.len() - 1;
                }
            }
            Err(e) => {
                self.bullets = vec![];
                self.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    /// Navigate to a different date.
    fn go_to_date(&mut self, date: NaiveDate) {
        self.current_date = date;
        self.selected = 0;
        self.reload_bullets();
    }

    /// Run the TUI event loop.
    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;

        // From here on, always restore the terminal even on error
        let result = (|| -> Result<()> {
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen)?;
            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend)?;
            self.event_loop(&mut terminal)
        })();

        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);

        result
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;

            if event::poll(Duration::from_millis(250))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle_key(key.code);
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') => self.should_quit = true,

            // Navigation
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Char('[') => self.prev_day(),
            KeyCode::Char(']') => self.next_day(),

            // Actions on selected bullet
            KeyCode::Char('d') => self.action_complete(),
            KeyCode::Char('x') => self.action_cancel(),
            KeyCode::Char('m') => self.action_migrate(),
            KeyCode::Char('u') => self.action_unmigrate(),
            KeyCode::Char('b') => self.action_backlog(),

            _ => {}
        }
    }

    fn move_down(&mut self) {
        if !self.bullets.is_empty() && self.selected < self.bullets.len() - 1 {
            self.selected += 1;
        }
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn prev_day(&mut self) {
        if let Some(prev) = self.current_date.pred_opt() {
            self.go_to_date(prev);
        }
    }

    fn next_day(&mut self) {
        if let Some(next) = self.current_date.succ_opt() {
            self.go_to_date(next);
        }
    }

    // -- Actions --

    fn selected_bullet_id(&self) -> Option<&str> {
        self.bullets.get(self.selected).map(|b| b.id.as_str())
    }

    fn action_complete(&mut self) {
        if let Some(id) = self.selected_bullet_id().map(|s| s.to_string()) {
            match self.store.complete_task(self.current_date, &id) {
                Ok(_) => {
                    self.status_message = Some("Task completed".to_string());
                    self.reload_bullets();
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    fn action_cancel(&mut self) {
        if let Some(id) = self.selected_bullet_id().map(|s| s.to_string()) {
            match self.store.cancel_task(self.current_date, &id) {
                Ok(_) => {
                    self.status_message = Some("Task cancelled".to_string());
                    self.reload_bullets();
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    fn action_migrate(&mut self) {
        if let Some(id) = self.selected_bullet_id().map(|s| s.to_string()) {
            let target_date = self.current_date.succ_opt().unwrap_or(self.current_date);
            match self
                .store
                .migrate_task(self.current_date, &id, Some(target_date))
            {
                Ok((_, target)) => {
                    self.status_message =
                        Some(format!("Migrated to {target_date} ({})", target.id));
                    self.reload_bullets();
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    fn action_unmigrate(&mut self) {
        if let Some(id) = self.selected_bullet_id().map(|s| s.to_string()) {
            match self.store.unmigrate_task(self.current_date, &id) {
                Ok(outcome) => {
                    self.status_message = Some(format!("Unmigrated ({outcome:?})"));
                    self.reload_bullets();
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    fn action_backlog(&mut self) {
        if let Some(id) = self.selected_bullet_id().map(|s| s.to_string()) {
            match self.store.backlog_task(self.current_date, &id) {
                Ok(_) => {
                    self.status_message = Some("Moved to backlog".to_string());
                    self.reload_bullets();
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    // -- Rendering --

    fn render(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();

        // Layout: header (3 lines) + bullet list (remaining) + status bar (1 line)
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        self.render_header(frame, chunks[0]);
        self.render_bullet_list(frame, chunks[1]);
        self.render_status_bar(frame, chunks[2]);
    }

    fn render_header(&self, frame: &mut ratatui::Frame, area: Rect) {
        let today = Local::now().date_naive();
        let date_str = self.current_date.format("%Y-%m-%d").to_string();
        let day_label = if self.current_date == today {
            format!("{date_str} (today)")
        } else {
            date_str
        };

        let header = Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                day_label,
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {} bullets", self.bullets.len()),
                Style::default().fg(self.theme.muted),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(self.theme.muted)),
        );

        frame.render_widget(header, area);
    }

    fn render_bullet_list(&self, frame: &mut ratatui::Frame, area: Rect) {
        if self.bullets.is_empty() {
            let empty = Paragraph::new(Line::from(Span::styled(
                "  No bullets for this day. Press 'a' to add one.",
                Style::default().fg(self.theme.muted),
            )));
            frame.render_widget(empty, area);
            return;
        }

        // Build lines for all bullets (each bullet may take multiple lines due to notes)
        let mut lines: Vec<(usize, Line)> = Vec::new(); // (bullet_index, line)
        for (i, bullet) in self.bullets.iter().enumerate() {
            let is_selected = i == self.selected;
            let status_style = self.status_color(bullet.status);
            let text_style = if is_selected {
                Style::default()
                    .fg(self.theme.foreground)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.foreground)
            };
            let select_indicator = if is_selected { "▸ " } else { "  " };

            // Main bullet line
            lines.push((
                i,
                Line::from(vec![
                    Span::styled(select_indicator, Style::default().fg(self.theme.accent)),
                    Span::styled(format!("{} ", bullet.status.as_emoji()), status_style),
                    Span::styled(&bullet.text, text_style),
                ]),
            ));

            // Note lines (indented)
            for note in &bullet.notes {
                lines.push((
                    i,
                    Line::from(vec![
                        Span::raw("      "),
                        Span::styled(note, Style::default().fg(self.theme.muted)),
                    ]),
                ));
            }
        }

        // Calculate scroll to keep selected bullet visible
        let visible_height = area.height as usize;
        let selected_first_line = lines
            .iter()
            .position(|(idx, _)| *idx == self.selected)
            .unwrap_or(0);
        let scroll = selected_first_line.saturating_sub(visible_height.saturating_sub(1)) as u16;

        let display_lines: Vec<Line> = lines.into_iter().map(|(_, line)| line).collect();
        let total_lines = display_lines.len();

        let paragraph = Paragraph::new(display_lines).scroll((scroll, 0));
        frame.render_widget(paragraph, area);

        // Scrollbar
        if total_lines > visible_height {
            let mut scrollbar_state = ScrollbarState::new(total_lines).position(scroll as usize);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area,
                &mut scrollbar_state,
            );
        }
    }

    fn render_status_bar(&self, frame: &mut ratatui::Frame, area: Rect) {
        let content = if let Some(msg) = &self.status_message {
            Span::styled(format!(" {msg}"), Style::default().fg(self.theme.warning))
        } else {
            Span::styled(
                " q:quit  j/k:nav  [/]:day  d:done  x:cancel  m:migrate  u:unmigrate  b:backlog",
                Style::default().fg(self.theme.muted),
            )
        };

        let bar =
            Paragraph::new(Line::from(content)).style(Style::default().bg(self.theme.background));
        frame.render_widget(bar, area);
    }

    fn status_color(&self, status: BulletStatus) -> Style {
        let color = match status {
            BulletStatus::Open => self.theme.foreground,
            BulletStatus::Done => self.theme.success,
            BulletStatus::Migrated => self.theme.accent,
            BulletStatus::Cancelled => self.theme.error,
            BulletStatus::Backlogged => self.theme.warning,
            BulletStatus::Event => self.theme.accent,
            BulletStatus::Note => self.theme.muted,
        };
        Style::default().fg(color)
    }
}

/// Install a panic handler that restores the terminal before printing the panic.
pub fn install_panic_handler() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}
