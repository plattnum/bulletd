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

/// Which view is currently active.
enum ViewMode {
    /// Daily log — the default view.
    DailyLog,
    /// Review mode — step through open tasks one at a time.
    Review {
        /// Open tasks to review (IDs captured at review start).
        task_ids: Vec<String>,
        /// Current position in the review list.
        current: usize,
        /// Total count at start (for progress display).
        total: usize,
    },
    /// Open tasks across all days.
    OpenTasks {
        /// (date, bullet) pairs.
        tasks: Vec<(NaiveDate, Bullet)>,
        selected: usize,
    },
    /// Migration history for a specific bullet.
    MigrationHistory {
        chain: Vec<(NaiveDate, String, BulletStatus)>,
    },
}

/// The main TUI application state.
pub struct App {
    pub(crate) store: Store,
    theme: Theme,
    config: Config,
    should_quit: bool,
    mode: ViewMode,
    /// The date currently being viewed (daily log).
    current_date: NaiveDate,
    /// Bullets loaded for the current date.
    bullets: Vec<Bullet>,
    /// Currently selected bullet index in daily log.
    selected: usize,
    /// Status message shown at the bottom.
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
            config: config.clone(),
            should_quit: false,
            mode: ViewMode::DailyLog,
            current_date,
            bullets: vec![],
            selected: 0,
            status_message: None,
        };
        app.reload_bullets();
        app
    }

    fn reload_bullets(&mut self) {
        match self.store.list_bullets(self.current_date, None, None) {
            Ok(bullets) => {
                self.bullets = bullets;
                self.status_message = None;
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

    fn go_to_date(&mut self, date: NaiveDate) {
        self.current_date = date;
        self.selected = 0;
        self.reload_bullets();
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;

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

    // -- Key handling --

    fn handle_key(&mut self, key: KeyCode) {
        match &self.mode {
            ViewMode::DailyLog => self.handle_key_daily_log(key),
            ViewMode::Review { .. } => self.handle_key_review(key),
            ViewMode::OpenTasks { .. } => self.handle_key_open_tasks(key),
            ViewMode::MigrationHistory { .. } => self.handle_key_migration_history(key),
        }
    }

    fn handle_key_daily_log(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Char('[') => self.prev_day(),
            KeyCode::Char(']') => self.next_day(),
            KeyCode::Char('d') => self.action_complete(),
            KeyCode::Char('x') => self.action_cancel(),
            KeyCode::Char('m') => self.action_migrate(),
            KeyCode::Char('u') => self.action_unmigrate(),
            KeyCode::Char('b') => self.action_backlog(),
            KeyCode::Char('r') => self.enter_review_mode(),
            KeyCode::Char('o') => self.enter_open_tasks(),
            KeyCode::Char('h') => self.enter_migration_history(),
            _ => {}
        }
    }

    fn handle_key_review(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('d') => self.review_action(BulletStatus::Done),
            KeyCode::Char('x') => self.review_action(BulletStatus::Cancelled),
            KeyCode::Char('m') => self.review_migrate(),
            KeyCode::Char('b') => self.review_backlog(),
            KeyCode::Esc => {
                self.mode = ViewMode::DailyLog;
                self.reload_bullets();
                self.status_message = Some("Review cancelled".to_string());
            }
            _ => {}
        }
    }

    fn handle_key_open_tasks(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = ViewMode::DailyLog;
                self.reload_bullets();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let ViewMode::OpenTasks { tasks, selected } = &mut self.mode
                    && *selected < tasks.len().saturating_sub(1)
                {
                    *selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let ViewMode::OpenTasks { selected, .. } = &mut self.mode
                    && *selected > 0
                {
                    *selected -= 1;
                }
            }
            KeyCode::Enter => {
                if let ViewMode::OpenTasks { tasks, selected } = &self.mode
                    && let Some((date, _)) = tasks.get(*selected)
                {
                    let date = *date;
                    self.mode = ViewMode::DailyLog;
                    self.go_to_date(date);
                }
            }
            _ => {}
        }
    }

    fn handle_key_migration_history(&mut self, key: KeyCode) {
        match key {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = ViewMode::DailyLog;
                self.reload_bullets();
            }
            _ => {}
        }
    }

    // -- Navigation --

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

    // -- Daily log actions --

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

    // -- Review mode --

    fn enter_review_mode(&mut self) {
        match self.store.daily_review(self.current_date) {
            Ok(open_tasks) => {
                if open_tasks.is_empty() {
                    self.status_message = Some("No open tasks to review for this day".to_string());
                    return;
                }
                let total = open_tasks.len();
                let task_ids: Vec<String> = open_tasks.iter().map(|b| b.id.clone()).collect();
                self.mode = ViewMode::Review {
                    task_ids,
                    current: 0,
                    total,
                };
            }
            Err(e) => self.status_message = Some(format!("Error: {e}")),
        }
    }

    fn review_action(&mut self, status: BulletStatus) {
        let (id, current, total) = match &self.mode {
            ViewMode::Review {
                task_ids,
                current,
                total,
            } => {
                if let Some(id) = task_ids.get(*current) {
                    (id.clone(), *current, *total)
                } else {
                    return;
                }
            }
            _ => return,
        };

        let result = match status {
            BulletStatus::Done => self.store.complete_task(self.current_date, &id),
            BulletStatus::Cancelled => self.store.cancel_task(self.current_date, &id),
            _ => return,
        };

        match result {
            Ok(_) => self.advance_review(current, total),
            Err(e) => self.status_message = Some(format!("Error: {e}")),
        }
    }

    fn review_migrate(&mut self) {
        let (id, current, total) = match &self.mode {
            ViewMode::Review {
                task_ids,
                current,
                total,
            } => {
                if let Some(id) = task_ids.get(*current) {
                    (id.clone(), *current, *total)
                } else {
                    return;
                }
            }
            _ => return,
        };

        let target_date = self.current_date.succ_opt().unwrap_or(self.current_date);
        match self
            .store
            .migrate_task(self.current_date, &id, Some(target_date))
        {
            Ok(_) => self.advance_review(current, total),
            Err(e) => self.status_message = Some(format!("Error: {e}")),
        }
    }

    fn review_backlog(&mut self) {
        let (id, current, total) = match &self.mode {
            ViewMode::Review {
                task_ids,
                current,
                total,
            } => {
                if let Some(id) = task_ids.get(*current) {
                    (id.clone(), *current, *total)
                } else {
                    return;
                }
            }
            _ => return,
        };

        match self.store.backlog_task(self.current_date, &id) {
            Ok(_) => self.advance_review(current, total),
            Err(e) => self.status_message = Some(format!("Error: {e}")),
        }
    }

    fn advance_review(&mut self, current: usize, total: usize) {
        let next = current + 1;
        if let ViewMode::Review {
            current: ref mut c, ..
        } = self.mode
        {
            if next >= total {
                self.mode = ViewMode::DailyLog;
                self.reload_bullets();
                self.status_message = Some(format!("Review complete — {total} tasks resolved"));
            } else {
                *c = next;
            }
        }
    }

    // -- Open tasks view --

    fn enter_open_tasks(&mut self) {
        let lookback = self.config.general.lookback_days;
        match self.store.list_open_tasks(lookback) {
            Ok(tasks) => {
                if tasks.is_empty() {
                    self.status_message = Some("No open tasks found".to_string());
                    return;
                }
                self.mode = ViewMode::OpenTasks { tasks, selected: 0 };
            }
            Err(e) => self.status_message = Some(format!("Error: {e}")),
        }
    }

    // -- Migration history view --

    fn enter_migration_history(&mut self) {
        if let Some(bullet) = self.bullets.get(self.selected) {
            if bullet.migrated_to.is_none() && bullet.migrated_from.is_none() {
                self.status_message = Some("No migration history for this bullet".to_string());
                return;
            }
            match self.store.migration_history(self.current_date, &bullet.id) {
                Ok(chain) => {
                    if chain.is_empty() {
                        self.status_message = Some("No migration history found".to_string());
                        return;
                    }
                    self.mode = ViewMode::MigrationHistory { chain };
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    // -- Rendering --

    fn render(&self, frame: &mut ratatui::Frame) {
        match &self.mode {
            ViewMode::DailyLog => self.render_daily_log(frame),
            ViewMode::Review { .. } => self.render_review(frame),
            ViewMode::OpenTasks { .. } => self.render_open_tasks(frame),
            ViewMode::MigrationHistory { .. } => self.render_migration_history(frame),
        }
    }

    fn render_daily_log(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        self.render_header(frame, chunks[0]);
        self.render_bullet_list(frame, chunks[1]);
        self.render_status_bar(
            frame,
            chunks[2],
            " q:quit  j/k:nav  [/]:day  d:done  x:cancel  m:migrate  u:unmigrate  b:backlog  r:review  o:open  h:history",
        );
    }

    fn render_review(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        let (task_ids, current, total) = match &self.mode {
            ViewMode::Review {
                task_ids,
                current,
                total,
            } => (task_ids, *current, *total),
            _ => return,
        };

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("Review — Task {} of {}", current + 1, total),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", self.current_date),
                Style::default().fg(self.theme.muted),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(self.theme.muted)),
        );
        frame.render_widget(header, chunks[0]);

        // Current task
        if let Some(id) = task_ids.get(current) {
            if let Some(bullet) = self.bullets.iter().find(|b| b.id == *id) {
                let mut lines = vec![
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            format!("{} ", bullet.status.as_emoji()),
                            self.status_color(bullet.status),
                        ),
                        Span::styled(
                            &bullet.text,
                            Style::default()
                                .fg(self.theme.foreground)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                ];
                for note in &bullet.notes {
                    lines.push(Line::from(vec![
                        Span::raw("      "),
                        Span::styled(note, Style::default().fg(self.theme.muted)),
                    ]));
                }
                let paragraph = Paragraph::new(lines);
                frame.render_widget(paragraph, chunks[1]);
            } else {
                // Task may have been resolved already; reload and skip
                let msg = Paragraph::new(Line::from(Span::styled(
                    "  Task not found — it may have been resolved",
                    Style::default().fg(self.theme.muted),
                )));
                frame.render_widget(msg, chunks[1]);
            }
        }

        self.render_status_bar(
            frame,
            chunks[2],
            " d:done  x:cancel  m:migrate  b:backlog  Esc:exit review",
        );
    }

    fn render_open_tasks(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        let (tasks, selected) = match &self.mode {
            ViewMode::OpenTasks { tasks, selected } => (tasks, *selected),
            _ => return,
        };

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("Open Tasks — {} total", tasks.len()),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(self.theme.muted)),
        );
        frame.render_widget(header, chunks[0]);

        // Task list grouped by date
        let mut lines: Vec<Line> = Vec::new();
        let mut last_date: Option<NaiveDate> = None;
        for (i, (date, bullet)) in tasks.iter().enumerate() {
            if last_date != Some(*date) {
                if last_date.is_some() {
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    format!("  {date}"),
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::UNDERLINED),
                )));
                last_date = Some(*date);
            }
            let is_selected = i == selected;
            let indicator = if is_selected { "▸ " } else { "  " };
            let text_style = if is_selected {
                Style::default()
                    .fg(self.theme.foreground)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.foreground)
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    {indicator}"),
                    Style::default().fg(self.theme.accent),
                ),
                Span::styled(
                    format!("{} ", bullet.status.as_emoji()),
                    self.status_color(bullet.status),
                ),
                Span::styled(&bullet.text, text_style),
            ]));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, chunks[1]);

        self.render_status_bar(frame, chunks[2], " j/k:nav  Enter:go to day  Esc:back");
    }

    fn render_migration_history(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        let chain = match &self.mode {
            ViewMode::MigrationHistory { chain } => chain,
            _ => return,
        };

        // Header
        let header = Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("Migration History — {} steps", chain.len()),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(self.theme.muted)),
        );
        frame.render_widget(header, chunks[0]);

        // Chain display
        let mut lines: Vec<Line> = vec![Line::from("")];
        for (i, (date, id, status)) in chain.iter().enumerate() {
            let arrow = if i < chain.len() - 1 { " →" } else { "" };
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("{} ", status.as_emoji()),
                    self.status_color(*status),
                ),
                Span::styled(
                    format!("{date}/{id}"),
                    Style::default().fg(self.theme.foreground),
                ),
                Span::styled(
                    format!("  ({}){arrow}", status.display_name()),
                    Style::default().fg(self.theme.muted),
                ),
            ]));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, chunks[1]);

        self.render_status_bar(frame, chunks[2], " Esc:back");
    }

    // -- Shared rendering helpers --

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

        let mut lines: Vec<(usize, Line)> = Vec::new();
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

            lines.push((
                i,
                Line::from(vec![
                    Span::styled(select_indicator, Style::default().fg(self.theme.accent)),
                    Span::styled(format!("{} ", bullet.status.as_emoji()), status_style),
                    Span::styled(&bullet.text, text_style),
                ]),
            ));

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

        if total_lines > visible_height {
            let mut scrollbar_state = ScrollbarState::new(total_lines).position(scroll as usize);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area,
                &mut scrollbar_state,
            );
        }
    }

    fn render_status_bar(&self, frame: &mut ratatui::Frame, area: Rect, default_hint: &str) {
        let content = if let Some(msg) = &self.status_message {
            Span::styled(format!(" {msg}"), Style::default().fg(self.theme.warning))
        } else {
            Span::styled(default_hint, Style::default().fg(self.theme.muted))
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
