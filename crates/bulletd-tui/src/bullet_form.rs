use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use tui_textarea::TextArea;

use crate::theme::Theme;

/// Which field is focused in the form.
#[derive(Debug, Clone, Copy, PartialEq)]
enum FormField {
    Text,
    Notes,
}

impl FormField {
    fn next(self) -> Self {
        match self {
            Self::Text => Self::Notes,
            Self::Notes => Self::Text,
        }
    }
}

/// The popup form for adding or editing a bullet.
pub struct BulletForm {
    /// Whether this is a new bullet or editing an existing one.
    pub mode: FormMode,
    /// Single-line bullet text.
    text_buffer: String,
    /// Cursor position within text_buffer (byte offset).
    cursor_pos: usize,
    /// Multi-line notes textarea.
    notes: TextArea<'static>,
    /// Which field is focused.
    focused: FormField,
    /// Whether the user submitted (Ctrl+S).
    pub submitted: bool,
    /// Whether the user cancelled (Esc).
    pub cancelled: bool,
}

pub enum FormMode {
    Add,
    Edit { bullet_id: String },
}

/// Result of the form after submission.
pub struct FormResult {
    pub text: String,
    pub notes: Vec<String>,
}

impl BulletForm {
    /// Create a new empty form for adding a bullet.
    pub fn new_add() -> Self {
        let mut notes = TextArea::default();
        notes.set_cursor_line_style(Style::default());
        notes.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        Self {
            mode: FormMode::Add,
            text_buffer: String::new(),
            cursor_pos: 0,
            notes,
            focused: FormField::Text,
            submitted: false,
            cancelled: false,
        }
    }

    /// Create a form pre-filled for editing an existing bullet.
    pub fn new_edit(bullet_id: String, text: &str, existing_notes: &[String]) -> Self {
        let mut notes = TextArea::new(
            existing_notes
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        );
        notes.set_cursor_line_style(Style::default());
        notes.set_cursor_style(Style::default().add_modifier(Modifier::REVERSED));
        let cursor_pos = text.len();
        Self {
            mode: FormMode::Edit { bullet_id },
            text_buffer: text.to_string(),
            cursor_pos,
            notes,
            focused: FormField::Text,
            submitted: false,
            cancelled: false,
        }
    }

    /// Get the form result after submission.
    pub fn result(&self) -> FormResult {
        let notes: Vec<String> = self
            .notes
            .lines()
            .iter()
            .map(|s| s.to_string())
            .filter(|s| !s.trim().is_empty())
            .collect();
        FormResult {
            text: self.text_buffer.trim().to_string(),
            notes,
        }
    }

    /// Handle a key event. Returns true if the form consumed the event.
    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        // Global form keys
        match (code, modifiers) {
            (KeyCode::Esc, _) => {
                self.cancelled = true;
                return true;
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                if !self.text_buffer.trim().is_empty() {
                    self.submitted = true;
                }
                return true;
            }
            (KeyCode::Tab, _) | (KeyCode::BackTab, _) => {
                self.focused = self.focused.next();
                return true;
            }
            _ => {}
        }

        // Field-specific handling
        match self.focused {
            FormField::Text => match code {
                KeyCode::Char(c) => {
                    self.text_buffer.insert(self.cursor_pos, c);
                    self.cursor_pos += c.len_utf8();
                    true
                }
                KeyCode::Backspace => {
                    if self.cursor_pos > 0 {
                        // Find the previous char boundary
                        let prev = self.text_buffer[..self.cursor_pos]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        self.text_buffer.remove(prev);
                        self.cursor_pos = prev;
                    }
                    true
                }
                KeyCode::Delete => {
                    if self.cursor_pos < self.text_buffer.len() {
                        self.text_buffer.remove(self.cursor_pos);
                    }
                    true
                }
                KeyCode::Left => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos = self.text_buffer[..self.cursor_pos]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                    }
                    true
                }
                KeyCode::Right => {
                    if self.cursor_pos < self.text_buffer.len() {
                        self.cursor_pos = self.text_buffer[self.cursor_pos..]
                            .char_indices()
                            .nth(1)
                            .map(|(i, _)| self.cursor_pos + i)
                            .unwrap_or(self.text_buffer.len());
                    }
                    true
                }
                KeyCode::Home => {
                    self.cursor_pos = 0;
                    true
                }
                KeyCode::End => {
                    self.cursor_pos = self.text_buffer.len();
                    true
                }
                _ => false,
            },
            FormField::Notes => {
                // Delegate to textarea — it handles Enter, arrows, etc.
                self.notes
                    .input(crossterm::event::KeyEvent::new(code, modifiers));
                true
            }
        }
    }

    /// Render the popup form.
    pub fn render(&mut self, frame: &mut ratatui::Frame, area: Rect, theme: &Theme) {
        // Calculate popup size — centered, use 80% of terminal width (min 60)
        let ideal_width = area.width * 80 / 100;
        let width = ideal_width.max(60).min(area.width.saturating_sub(4));
        let height = 14.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        // Clear background
        frame.render_widget(Clear, popup);

        // Popup border
        let title = match &self.mode {
            FormMode::Add => " Add Bullet ",
            FormMode::Edit { .. } => " Edit Bullet ",
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(theme.background));

        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        // Layout: text label + text input + notes label + notes textarea + help
        let chunks = Layout::vertical([
            Constraint::Length(1), // text label
            Constraint::Length(1), // text input
            Constraint::Length(1), // spacing
            Constraint::Length(1), // notes label
            Constraint::Min(3),    // notes textarea
            Constraint::Length(1), // help bar
        ])
        .split(inner);

        // Text label
        let text_label_style = if self.focused == FormField::Text {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(" Bullet:", text_label_style)),
            chunks[0],
        );

        // Text input with horizontal scrolling
        let text_style = if self.focused == FormField::Text {
            Style::default().fg(theme.foreground).bg(theme.muted)
        } else {
            Style::default().fg(theme.foreground)
        };

        // Available width for text (1 char padding on left)
        let input_width = chunks[1].width.saturating_sub(2) as usize; // 1 pad + 1 cursor

        // Count character position of cursor for display
        let cursor_char_pos = self.text_buffer[..self.cursor_pos].chars().count();
        let total_chars = self.text_buffer.chars().count();

        // Calculate scroll offset to keep cursor visible
        let scroll_offset = if input_width == 0 {
            0
        } else if cursor_char_pos >= input_width {
            cursor_char_pos - input_width + 1
        } else {
            0
        };

        // Build visible text with cursor
        let chars: Vec<char> = self.text_buffer.chars().collect();
        let visible_end = total_chars.min(scroll_offset + input_width);
        let before_cursor: String = chars[scroll_offset..cursor_char_pos].iter().collect();
        let after_cursor: String = chars[cursor_char_pos..visible_end].iter().collect();

        let display = if self.focused == FormField::Text {
            format!(" {before_cursor}▏{after_cursor}")
        } else {
            let visible: String = chars[scroll_offset..visible_end].iter().collect();
            format!(" {visible}")
        };

        frame.render_widget(Paragraph::new(Span::styled(display, text_style)), chunks[1]);

        // Notes label
        let notes_label_style = if self.focused == FormField::Notes {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(" Notes:", notes_label_style)),
            chunks[3],
        );

        // Notes textarea
        if self.focused == FormField::Notes {
            self.notes
                .set_style(Style::default().fg(theme.foreground).bg(theme.muted));
        } else {
            self.notes.set_style(Style::default().fg(theme.foreground));
        }
        frame.render_widget(&self.notes, chunks[4]);

        // Help bar
        let help = Line::from(vec![
            Span::styled(" ^S", Style::default().fg(theme.accent)),
            Span::styled(" Save  ", Style::default().fg(theme.muted)),
            Span::styled("Esc", Style::default().fg(theme.accent)),
            Span::styled(" Cancel  ", Style::default().fg(theme.muted)),
            Span::styled("Tab", Style::default().fg(theme.accent)),
            Span::styled(" Next field", Style::default().fg(theme.muted)),
        ]);
        frame.render_widget(Paragraph::new(help), chunks[5]);
    }
}
