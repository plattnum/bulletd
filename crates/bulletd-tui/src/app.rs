use std::io;
use std::sync::mpsc;
use std::time::Duration;

use chrono::{Local, NaiveDate};
use color_eyre::eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use notify_debouncer_mini::new_debouncer;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};

use bulletd_core::config::Config;
use bulletd_core::config::IconsConfig;
use bulletd_core::model::{Bullet, BulletStatus};
use bulletd_core::ops::Store;

use crate::bullet_form::{BulletForm, FormMode};
use crate::theme::Theme;

/// State for grab-and-move mode.
struct GrabState {
    /// The ID of the bullet being moved.
    bullet_id: String,
    /// Current position in the list.
    position: usize,
}

/// Which view is currently active.
enum ViewMode {
    /// Daily log — the default view.
    DailyLog,
    /// Grab-and-move mode — reorder a bullet with arrows.
    Grab(GrabState),
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
        /// The bullet text (same across the chain).
        title: String,
        chain: Vec<(NaiveDate, String, BulletStatus, String)>,
    },
}

/// The main TUI application state.
pub struct App {
    pub(crate) store: Store,
    theme: Theme,
    config: Config,
    icons: IconsConfig,
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
    /// Popup form for adding/editing bullets.
    bullet_form: Option<BulletForm>,
    /// Whether the help overlay is visible.
    show_help: bool,
    /// Whether the daily log is grouped by status.
    grouped: bool,
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
            icons: IconsConfig::minimal(),
            should_quit: false,
            mode: ViewMode::DailyLog,
            current_date,
            bullets: vec![],
            selected: 0,
            status_message: None,
            bullet_form: None,
            show_help: false,
            grouped: false,
        };
        app.reload_bullets();
        app
    }

    fn reload_bullets(&mut self) {
        match self.store.list_bullets(self.current_date, None) {
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
        // Set up file watcher on the data directory
        let (fs_tx, fs_rx) = mpsc::channel();
        let _debouncer = self.start_file_watcher(fs_tx);

        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;

            // Check for file system changes (non-blocking)
            if fs_rx.try_recv().is_ok() {
                self.reload_bullets();
            }

            if event::poll(Duration::from_millis(250))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle_key(key.code, key.modifiers);
            }
        }
        Ok(())
    }

    /// Start a file watcher on the data directory. Sends a signal on any .md file change.
    fn start_file_watcher(
        &self,
        tx: mpsc::Sender<()>,
    ) -> Option<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> {
        let data_dir = bulletd_core::config::resolve_data_dir(&self.config.general.data_dir);

        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            move |result: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                if let Ok(events) = result {
                    let dominated = events.iter().any(|e| {
                        let path = e.path.to_string_lossy();
                        path.ends_with(".md") && !path.ends_with(".md.tmp")
                    });
                    if dominated {
                        let _ = tx.send(());
                    }
                }
            },
        ) {
            Ok(d) => d,
            Err(_) => return None,
        };

        let _ = debouncer
            .watcher()
            .watch(&data_dir, notify::RecursiveMode::NonRecursive);

        Some(debouncer)
    }

    // -- Key handling --

    fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) {
        // If a bullet form is open, delegate all keys to it
        if let Some(ref mut form) = self.bullet_form {
            form.handle_key(key, modifiers);

            if form.submitted {
                let result = form.result();
                match &form.mode {
                    FormMode::Add => {
                        match self.store.add_bullet(
                            result.text.clone(),
                            result.notes,
                            Some(self.current_date),
                        ) {
                            Ok(bullet) => {
                                self.status_message =
                                    Some(format!("Added {} ({})", bullet.text, bullet.id));
                                self.reload_bullets();
                                if !self.bullets.is_empty() {
                                    self.selected = self.bullets.len() - 1;
                                }
                            }
                            Err(e) => self.status_message = Some(format!("Error: {e}")),
                        }
                    }
                    FormMode::Edit { bullet_id } => {
                        let id = bullet_id.clone();
                        let notes = if result.notes.is_empty() {
                            None
                        } else {
                            Some(result.notes)
                        };
                        match self.store.update_bullet(
                            self.current_date,
                            &id,
                            Some(result.text),
                            notes,
                        ) {
                            Ok(_) => {
                                self.status_message = Some("Updated".to_string());
                                self.reload_bullets();
                            }
                            Err(e) => self.status_message = Some(format!("Error: {e}")),
                        }
                    }
                }
                self.bullet_form = None;
            } else if form.cancelled {
                self.bullet_form = None;
            }

            return;
        }

        // Help overlay — intercept before mode-specific handlers
        if self.show_help {
            match key {
                KeyCode::Char('?') | KeyCode::Esc => self.show_help = false,
                _ => {}
            }
            return;
        }
        if key == KeyCode::Char('?') {
            self.show_help = true;
            return;
        }

        match &self.mode {
            ViewMode::DailyLog => self.handle_key_daily_log(key),
            ViewMode::Grab(_) => self.handle_key_grab(key),
            ViewMode::Review { .. } => self.handle_key_review(key),
            ViewMode::OpenTasks { .. } => self.handle_key_open_tasks(key),
            ViewMode::MigrationHistory { .. } => self.handle_key_migration_history(key),
        }
    }

    fn handle_key_daily_log(&mut self, key: KeyCode) {
        // Clear status message on any key press so hints are visible again
        self.status_message = None;

        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('[') => self.prev_day(),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Char(']') => self.next_day(),
            KeyCode::Char('a') => {
                self.bullet_form = Some(BulletForm::new_add());
            }
            KeyCode::Char('e') => {
                if let Some(bullet) = self.bullets.get(self.selected) {
                    self.bullet_form = Some(BulletForm::new_edit(
                        bullet.id.clone(),
                        &bullet.text,
                        &bullet.notes,
                    ));
                }
            }
            KeyCode::Char('d') => self.action_complete(),
            KeyCode::Char('o') => self.action_reopen(),
            KeyCode::Char('x') => self.action_cancel(),
            KeyCode::Char('D') | KeyCode::Delete => self.action_delete(),
            KeyCode::Char('m') => self.action_migrate(),
            KeyCode::Char('u') => self.action_unmigrate(),
            KeyCode::Char('b') => self.action_backlog(),
            KeyCode::Char('g') => {
                self.grouped = !self.grouped;
                self.status_message = Some(
                    if self.grouped {
                        "Grouped by status"
                    } else {
                        "Flat view"
                    }
                    .to_string(),
                );
            }
            KeyCode::Enter if !self.grouped => self.start_grab(),
            KeyCode::Char('r') => self.enter_review_mode(),
            KeyCode::Char('O') => self.enter_open_tasks(),
            KeyCode::Char('H') => self.enter_migration_history(),
            KeyCode::Char('i') => self.toggle_icons(),
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

    /// Returns bullet indices in grouped display order (by status group, preserving
    /// intra-group file order). Used so that j/k navigate visually in grouped view.
    fn grouped_indices(&self) -> Vec<usize> {
        use bulletd_core::model::STATUS_GROUP_ORDER;
        let mut indices = Vec::with_capacity(self.bullets.len());
        for status in &STATUS_GROUP_ORDER {
            for (i, b) in self.bullets.iter().enumerate() {
                if b.status == *status {
                    indices.push(i);
                }
            }
        }
        indices
    }

    fn move_down(&mut self) {
        if self.bullets.is_empty() {
            return;
        }
        if self.grouped {
            let order = self.grouped_indices();
            if let Some(pos) = order.iter().position(|&i| i == self.selected) {
                if pos + 1 < order.len() {
                    self.selected = order[pos + 1];
                }
            }
        } else if self.selected < self.bullets.len() - 1 {
            self.selected += 1;
        }
    }

    fn move_up(&mut self) {
        if self.grouped {
            let order = self.grouped_indices();
            if let Some(pos) = order.iter().position(|&i| i == self.selected) {
                if pos > 0 {
                    self.selected = order[pos - 1];
                }
            }
        } else if self.selected > 0 {
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

    fn action_reopen(&mut self) {
        if let Some(id) = self.selected_bullet_id().map(|s| s.to_string()) {
            match self.store.reopen_bullet(self.current_date, &id) {
                Ok(_) => {
                    self.status_message = Some("Reopened".to_string());
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

    // -- Grab-and-move mode --

    fn start_grab(&mut self) {
        if let Some(bullet) = self.bullets.get(self.selected) {
            self.mode = ViewMode::Grab(GrabState {
                bullet_id: bullet.id.clone(),
                position: self.selected,
            });
            self.status_message = None;
        }
    }

    fn handle_key_grab(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('j') | KeyCode::Down => {
                if let ViewMode::Grab(ref mut state) = self.mode
                    && state.position < self.bullets.len().saturating_sub(1)
                {
                    let id = state.bullet_id.clone();
                    if self.store.move_bullet(self.current_date, &id, 1).is_ok() {
                        state.position += 1;
                        self.selected = state.position;
                        self.reload_bullets();
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let ViewMode::Grab(ref mut state) = self.mode
                    && state.position > 0
                {
                    let id = state.bullet_id.clone();
                    if self.store.move_bullet(self.current_date, &id, -1).is_ok() {
                        state.position -= 1;
                        self.selected = state.position;
                        self.reload_bullets();
                    }
                }
            }
            KeyCode::Enter | KeyCode::Esc => {
                self.mode = ViewMode::DailyLog;
                self.status_message = Some("Released".to_string());
            }
            _ => {}
        }
    }

    fn action_delete(&mut self) {
        if let Some(id) = self.selected_bullet_id().map(|s| s.to_string()) {
            match self.store.delete_bullet(self.current_date, &id) {
                Ok(()) => {
                    self.status_message = Some("Deleted".to_string());
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
            let title = bullet.text.clone();
            match self.store.migration_history(self.current_date, &bullet.id) {
                Ok(chain) => {
                    if chain.is_empty() {
                        self.status_message = Some("No migration history found".to_string());
                        return;
                    }
                    self.mode = ViewMode::MigrationHistory { title, chain };
                }
                Err(e) => self.status_message = Some(format!("Error: {e}")),
            }
        }
    }

    // -- Rendering --

    fn render(&mut self, frame: &mut ratatui::Frame) {
        match &self.mode {
            ViewMode::DailyLog => self.render_daily_log(frame),
            ViewMode::Grab(_) => self.render_grab(frame),
            ViewMode::Review { .. } => self.render_review(frame),
            ViewMode::OpenTasks { .. } => self.render_open_tasks(frame),
            ViewMode::MigrationHistory { .. } => self.render_migration_history(frame),
        }

        // Render bullet form popup on top if active
        if let Some(ref mut form) = self.bullet_form {
            form.render(frame, frame.area(), &self.theme);
        }

        // Render help overlay on top of everything
        if self.show_help {
            self.render_help(frame);
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
            " q:quit hjkl:nav a:add e:edit d:done o:open x:cancel D:del m:migrate b:backlog enter:grab r:review O:all-open g:group i:icons",
        );
    }

    fn render_grab(&self, frame: &mut ratatui::Frame) {
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
            " GRAB MODE — j/k:move bullet  Enter/Esc:release",
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

        // Header with context
        let header = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!(
                        "Reviewing open tasks for {}",
                        self.current_date.format("%Y-%m-%d")
                    ),
                    Style::default().fg(self.theme.muted),
                ),
            ]),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("Task {} of {}", current + 1, total),
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " — decide: done, cancel, migrate, or backlog",
                    Style::default().fg(self.theme.muted),
                ),
            ]),
        ])
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
                            format!("{} ", self.status_icon(bullet.status)),
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

        // Header with context
        let lookback = self.config.general.lookback_days;
        let today = Local::now().date_naive();
        let start_date = today - chrono::Duration::days(i64::from(lookback) - 1);
        let header = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!(
                        "Open tasks from last {} days ({} to {})",
                        lookback,
                        start_date.format("%Y-%m-%d"),
                        today.format("%Y-%m-%d"),
                    ),
                    Style::default().fg(self.theme.muted),
                ),
            ]),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("{} unresolved", tasks.len()),
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " — select and press Enter to jump to that day",
                    Style::default().fg(self.theme.muted),
                ),
            ]),
        ])
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
                    format!("{} ", self.status_icon(bullet.status)),
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

        let (title, chain) = match &self.mode {
            ViewMode::MigrationHistory { title, chain } => (title, chain),
            _ => return,
        };

        // Header — show the bullet text so you know what this chain is about
        let header = Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("Migration History — \"{title}\""),
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

        // Timeline display
        let mut lines: Vec<Line> = vec![Line::from("")];
        for (i, (date, _id, status, _text)) in chain.iter().enumerate() {
            let is_last = i == chain.len() - 1;
            let connector = if is_last { "  └─ " } else { "  ├─ " };

            lines.push(Line::from(vec![
                Span::styled(connector, Style::default().fg(self.theme.muted)),
                Span::styled(
                    format!("{} ", self.status_icon(*status)),
                    self.status_color(*status),
                ),
                Span::styled(
                    date.format("%Y-%m-%d").to_string(),
                    Style::default().fg(self.theme.foreground),
                ),
                Span::styled(
                    format!("  {}", status.display_name()),
                    Style::default().fg(self.theme.muted),
                ),
            ]));

            // Add a vertical connector line between entries (except after last)
            if !is_last {
                lines.push(Line::from(vec![Span::styled(
                    "  │",
                    Style::default().fg(self.theme.muted),
                )]));
            }
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, chunks[1]);

        self.render_status_bar(frame, chunks[2], " Esc:back");
    }

    // -- Shared rendering helpers --

    fn render_help(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        let width = 50u16.min(area.width.saturating_sub(4));
        let height = 28u16.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        let help_text = "\
 ── Navigation ──────────────────
  j / k        Move down / up
  h / l        Previous / next day
  ← / → / ↑ / ↓  Arrow key nav

 ── Bullets ─────────────────────
  a            Add new bullet
  e            Edit selected bullet
  d            Mark done
  o            Set to open
  x            Cancel
  m            Migrate to tomorrow
  u            Unmigrate
  b            Move to backlog
  D            Delete
  Enter        Grab and reorder

 ── Views ───────────────────────
  r            Review open bullets
  O            All open bullets
  H            Migration history
  i            Toggle icon style

 ── General ─────────────────────
  ?            Toggle this help
  q            Quit";

        let paragraph = Paragraph::new(help_text)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(self.theme.foreground))
            .block(
                Block::default()
                    .title(" Help (? to close) ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(self.theme.accent))
                    .style(Style::default().bg(self.theme.background)),
            );

        frame.render_widget(Clear, popup);
        frame.render_widget(paragraph, popup);
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
                "bulletd",
                Style::default()
                    .fg(self.theme.foreground)
                    .add_modifier(Modifier::BOLD),
            ),
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

    fn render_bullet_line<'a>(
        &self,
        bullet: &'a Bullet,
        bullet_idx: usize,
        is_grab_mode: bool,
        lines: &mut Vec<(usize, Line<'a>)>,
    ) {
        let is_selected = bullet_idx == self.selected;
        let is_grabbed = is_selected && is_grab_mode;
        let status_style = self.status_color(bullet.status);
        let text_style = if is_selected {
            Style::default()
                .fg(self.theme.foreground)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.foreground)
        };
        let select_indicator = if is_grabbed {
            "≡ "
        } else if is_selected {
            "▸ "
        } else {
            "  "
        };

        let line_bg = if is_grabbed {
            Style::default().bg(self.theme.accent)
        } else {
            Style::default()
        };

        let mut spans = vec![
            Span::styled(select_indicator, Style::default().fg(self.theme.accent)),
            Span::styled(
                format!("{} ", self.status_icon(bullet.status)),
                status_style,
            ),
        ];
        spans.push(Span::styled(&bullet.text, text_style));
        if self.config.display.show_ids {
            spans.push(Span::styled(
                format!(" ({})", bullet.id),
                Style::default().fg(self.theme.muted),
            ));
        }
        let mut main_line = Line::from(spans);
        if is_grabbed {
            main_line = main_line.style(line_bg);
        }
        lines.push((bullet_idx, main_line));

        for note in &bullet.notes {
            let mut note_line = Line::from(vec![
                Span::styled("     ↳ ", Style::default().fg(self.theme.muted)),
                Span::styled(
                    note,
                    Style::default()
                        .fg(self.theme.muted)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]);
            if is_grabbed {
                note_line = note_line.style(line_bg);
            }
            lines.push((bullet_idx, note_line));
        }
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

        let is_grab_mode = matches!(self.mode, ViewMode::Grab(_));

        let mut lines: Vec<(usize, Line)> = Vec::new();

        if self.grouped {
            use bulletd_core::model::STATUS_GROUP_ORDER;

            for status in &STATUS_GROUP_ORDER {
                let group_bullets: Vec<(usize, &Bullet)> = self
                    .bullets
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.status == *status)
                    .collect();

                if group_bullets.is_empty() {
                    continue;
                }

                // Section header
                let header_text = format!(
                    "── {} ({}) ──",
                    status.display_name().to_uppercase(),
                    group_bullets.len()
                );
                lines.push((
                    usize::MAX,
                    Line::from(Span::styled(
                        header_text,
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD),
                    )),
                ));

                for (bullet_idx, bullet) in group_bullets {
                    self.render_bullet_line(bullet, bullet_idx, is_grab_mode, &mut lines);
                }
            }
        } else {
            for (i, bullet) in self.bullets.iter().enumerate() {
                self.render_bullet_line(bullet, i, is_grab_mode, &mut lines);
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
        };
        Style::default().fg(color)
    }

    fn status_icon(&self, status: BulletStatus) -> &str {
        status.display_icon(&self.icons)
    }

    fn toggle_icons(&mut self) {
        if self.icons == IconsConfig::minimal() {
            self.icons = IconsConfig::emoji();
            self.status_message = Some("Icons: emoji".to_string());
        } else {
            self.icons = IconsConfig::minimal();
            self.status_message = Some("Icons: minimal".to_string());
        }
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
