use chrono::NaiveDate;
use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::theme::Theme;

/// Modal picker for choosing a future work day to migrate to.
pub struct MigratePicker {
    /// The bullet ID being migrated (captured at open time).
    pub bullet_id: String,
    /// Pre-computed list of upcoming work days.
    pub dates: Vec<NaiveDate>,
    /// Currently highlighted index.
    cursor: usize,
    /// Set on Enter.
    pub submitted: bool,
    /// Set on Esc.
    pub cancelled: bool,
}

impl MigratePicker {
    pub fn new(bullet_id: String, dates: Vec<NaiveDate>) -> Self {
        Self {
            bullet_id,
            dates,
            cursor: 0,
            submitted: false,
            cancelled: false,
        }
    }

    pub fn selected_date(&self) -> Option<NaiveDate> {
        self.dates.get(self.cursor).copied()
    }

    /// Handle a key event. Returns true if the picker consumed it.
    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Esc => {
                self.cancelled = true;
                true
            }
            KeyCode::Enter => {
                if self.dates.is_empty() {
                    self.cancelled = true;
                } else {
                    self.submitted = true;
                }
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.dates.is_empty() && self.cursor + 1 < self.dates.len() {
                    self.cursor += 1;
                }
                true
            }
            KeyCode::Home => {
                self.cursor = 0;
                true
            }
            KeyCode::End => {
                if !self.dates.is_empty() {
                    self.cursor = self.dates.len() - 1;
                }
                true
            }
            KeyCode::PageDown => {
                let max = self.dates.len().saturating_sub(1);
                self.cursor = (self.cursor + 10).min(max);
                true
            }
            KeyCode::PageUp => {
                self.cursor = self.cursor.saturating_sub(10);
                true
            }
            _ => false,
        }
    }

    pub fn render(&self, frame: &mut ratatui::Frame, area: Rect, theme: &Theme) {
        let max_height = area.height.saturating_sub(4);
        let desired_height = (self.dates.len() as u16).saturating_add(4).min(20);
        let height = desired_height.min(max_height);
        let width = 36u16.min(area.width.saturating_sub(4));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        frame.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent))
            .title(Span::styled(
                " Migrate to which work day? ",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(theme.background));

        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(inner);

        let items: Vec<ListItem> = self
            .dates
            .iter()
            .map(|d| {
                let label = format!(" {} {}", d.format("%a"), d.format("%Y-%m-%d"));
                ListItem::new(Line::from(Span::raw(label)))
            })
            .collect();

        let highlight = Style::default()
            .fg(theme.background)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD);

        let list = List::new(items)
            .highlight_style(highlight)
            .style(Style::default().fg(theme.foreground));

        let mut state = ListState::default();
        state.select(Some(self.cursor));
        frame.render_stateful_widget(list, chunks[0], &mut state);

        let footer = Line::from(vec![
            Span::styled(" ↑/↓", Style::default().fg(theme.accent)),
            Span::styled(" select  ", Style::default().fg(theme.muted)),
            Span::styled("Enter", Style::default().fg(theme.accent)),
            Span::styled(" confirm  ", Style::default().fg(theme.muted)),
            Span::styled("Esc", Style::default().fg(theme.accent)),
            Span::styled(" cancel", Style::default().fg(theme.muted)),
        ]);
        frame.render_widget(Paragraph::new(footer), chunks[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn picker_with_n(n: usize) -> MigratePicker {
        let dates = (1..=n).map(|i| ymd(2026, 5, i as u32)).collect();
        MigratePicker::new("BL-001".to_string(), dates)
    }

    #[test]
    fn cursor_starts_at_zero_and_moves() {
        let mut p = picker_with_n(5);
        assert_eq!(p.cursor, 0);
        p.handle_key(KeyCode::Down);
        assert_eq!(p.cursor, 1);
        p.handle_key(KeyCode::Down);
        assert_eq!(p.cursor, 2);
        p.handle_key(KeyCode::Up);
        assert_eq!(p.cursor, 1);
    }

    #[test]
    fn cursor_clamps_at_boundaries() {
        let mut p = picker_with_n(3);
        p.handle_key(KeyCode::Up); // already at 0
        assert_eq!(p.cursor, 0);
        p.handle_key(KeyCode::End);
        assert_eq!(p.cursor, 2);
        p.handle_key(KeyCode::Down); // already at last
        assert_eq!(p.cursor, 2);
    }

    #[test]
    fn enter_submits_and_selected_date_is_correct() {
        let mut p = picker_with_n(5);
        p.handle_key(KeyCode::Down);
        p.handle_key(KeyCode::Down);
        p.handle_key(KeyCode::Enter);
        assert!(p.submitted);
        assert!(!p.cancelled);
        assert_eq!(p.selected_date(), Some(ymd(2026, 5, 3)));
    }

    #[test]
    fn esc_cancels() {
        let mut p = picker_with_n(5);
        p.handle_key(KeyCode::Esc);
        assert!(p.cancelled);
        assert!(!p.submitted);
    }

    #[test]
    fn empty_dates_treats_enter_as_cancel() {
        let mut p = MigratePicker::new("BL-001".to_string(), vec![]);
        p.handle_key(KeyCode::Enter);
        assert!(p.cancelled);
        assert!(!p.submitted);
    }

    #[test]
    fn page_keys_jump_by_ten() {
        let mut p = picker_with_n(30);
        p.handle_key(KeyCode::PageDown);
        assert_eq!(p.cursor, 10);
        p.handle_key(KeyCode::PageDown);
        assert_eq!(p.cursor, 20);
        p.handle_key(KeyCode::PageUp);
        assert_eq!(p.cursor, 10);
    }
}
