use std::sync::Arc;

use chrono::{Local, NaiveDate};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use serde_json::json;

use bulletd_core::config::Config;
use bulletd_core::model::BulletStatus;
use bulletd_core::ops::Store;

use crate::params::*;

pub struct McpState {
    pub store: Store,
    pub config: Config,
}

#[derive(Clone)]
pub struct BulletdMcpServer {
    #[allow(dead_code)] // used by tool_router macro
    tool_router: ToolRouter<Self>,
    state: Arc<McpState>,
}

const SERVER_INSTRUCTIONS: &str = "\
bulletd is a digital bullet journal. A bullet is a single line entry in a daily log \
with a status (open, done, migrated, cancelled, backlogged) and optional context notes. \
Daily logs are stored as markdown files, one per day. \
The typical workflow: add bullets throughout the day, then review what's still open \
and decide to complete, cancel, migrate to tomorrow, or shelve to the backlog. \
Use list_bullets with status=open to review a day. \
Dates are always YYYY-MM-DD. Bullet IDs are short (letter + digit, e.g. \"a3\"). \
When displaying bullets to the user, put the ID at the end in parentheses, e.g.: \
\"Fix flaky auth test (b7)\". Notes go in parentheses after the text, before the ID.";

#[tool_handler]
impl ServerHandler for BulletdMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(SERVER_INSTRUCTIONS.to_string())
    }
}

#[tool_router]
impl BulletdMcpServer {
    pub fn new(state: Arc<McpState>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            state,
        }
    }

    #[tool(description = "Add a bullet to a day's log. Returns the new bullet's id.")]
    fn add_bullet(&self, Parameters(params): Parameters<AddBulletParams>) -> String {
        let date = match parse_optional_date(params.date.as_deref()) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        let notes = params.notes.unwrap_or_default();

        match self.state.store.add_bullet(params.text, notes, Some(date)) {
            Ok(bullet) => json!({
                "id": bullet.id,
                "date": date.to_string(),
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(
        description = "List bullets for a date. Filter by status: open, done, migrated, cancelled, backlogged. Set group_by=\"status\" to get results grouped by status."
    )]
    fn list_bullets(&self, Parameters(params): Parameters<ListBulletsParams>) -> String {
        let date = match parse_optional_date(params.date.as_deref()) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        if params.group_by.as_deref() == Some("status") {
            return match self.state.store.list_bullets_grouped(date) {
                Ok(groups) => {
                    let mut grouped = json!({});
                    let mut total = 0;
                    for (status, bullets) in &groups {
                        let items: Vec<_> = bullets.iter().map(bullet_to_json).collect();
                        total += items.len();
                        grouped[status.display_name()] = json!(items);
                    }
                    json!({"date": date.to_string(), "count": total, "grouped": grouped})
                        .to_string()
                }
                Err(e) => json!({"error": e.to_string()}).to_string(),
            };
        }

        let status_filter = params.status.as_deref().and_then(parse_bullet_status);

        match self.state.store.list_bullets(date, status_filter) {
            Ok(bullets) => {
                let items: Vec<_> = bullets.iter().map(bullet_to_json).collect();
                json!({"date": date.to_string(), "count": items.len(), "bullets": items})
                    .to_string()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Update a bullet's text.")]
    fn update_bullet(&self, Parameters(params): Parameters<UpdateBulletParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self
            .state
            .store
            .update_bullet(date, &params.id, Some(params.text), None)
        {
            Ok(bullet) => json!({
                "id": bullet.id,
                "text": bullet.text,
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Append a note line to a bullet.")]
    fn append_note(&self, Parameters(params): Parameters<AppendNoteParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.append_note(date, &params.id, params.note) {
            Ok(bullet) => json!({
                "ok": true,
                "notes_count": bullet.notes.len(),
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Replace all notes on a bullet.")]
    fn update_notes(&self, Parameters(params): Parameters<UpdateNotesParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self
            .state
            .store
            .update_notes(date, &params.id, params.notes)
        {
            Ok(bullet) => json!({
                "ok": true,
                "notes_count": bullet.notes.len(),
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Clear all notes from a bullet.")]
    fn clear_notes(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.clear_notes(date, &params.id) {
            Ok(_) => json!({"ok": true}).to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Mark a bullet as done.")]
    fn complete_bullet(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.complete_task(date, &params.id) {
            Ok(_) => json!({"ok": true}).to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Mark a bullet as cancelled.")]
    fn cancel_bullet(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.cancel_task(date, &params.id) {
            Ok(_) => json!({"ok": true}).to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Set a bullet back to open.")]
    fn open_bullet(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.reopen_bullet(date, &params.id) {
            Ok(_) => json!({"ok": true}).to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Migrate a bullet to another day. Defaults to tomorrow.")]
    fn migrate_bullet(&self, Parameters(params): Parameters<MigrateBulletParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        let target = match params.target_date.as_deref() {
            Some(s) => match parse_date(s) {
                Ok(d) => Some(d),
                Err(e) => return json!({"error": e}).to_string(),
            },
            None => None,
        };

        match self.state.store.migrate_task(date, &params.id, target) {
            Ok((_, target_bullet)) => json!({
                "ok": true,
                "target_id": target_bullet.id,
                "target_date": target.map_or_else(
                    || (date.succ_opt().unwrap_or(date)).to_string(),
                    |d| d.to_string()
                ),
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Reverse a migration. Reopens the source bullet.")]
    fn unmigrate_bullet(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.unmigrate_task(date, &params.id) {
            Ok(outcome) => json!({
                "ok": true,
                "outcome": format!("{outcome:?}"),
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Move a bullet to the backlog.")]
    fn backlog_bullet(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.backlog_task(date, &params.id) {
            Ok((_, backlog_bullet)) => json!({
                "ok": true,
                "backlog_id": backlog_bullet.id,
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(
        description = "Move a bullet to a new position in the day's list. Position: \"top\", \"bottom\", or a 0-based index."
    )]
    fn move_bullet(&self, Parameters(params): Parameters<MoveBulletParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        let result = match params.position.as_str() {
            "top" => self.state.store.move_bullet_to(date, &params.id, 0),
            "bottom" => self.state.store.move_bullet_to(date, &params.id, usize::MAX),
            s => match s.parse::<usize>() {
                Ok(pos) => self.state.store.move_bullet_to(date, &params.id, pos),
                Err(_) => return json!({"error": format!("invalid position: {s} (use \"top\", \"bottom\", or a number)")}).to_string(),
            },
        };

        match result {
            Ok(()) => json!({"ok": true}).to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "List all open bullets across recent days.")]
    fn list_open_bullets(&self, Parameters(params): Parameters<ListOpenBulletsParams>) -> String {
        let lookback = params
            .lookback_days
            .unwrap_or(self.state.config.general.lookback_days);

        match self.state.store.list_open_tasks(lookback) {
            Ok(tasks) => {
                let items: Vec<_> = tasks
                    .iter()
                    .map(|(date, bullet)| {
                        json!({
                            "date": date.to_string(),
                            "id": bullet.id,
                            "text": bullet.text,
                        })
                    })
                    .collect();
                json!({"count": items.len(), "bullets": items}).to_string()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Trace a bullet's migration chain across days.")]
    fn migration_history(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.migration_history(date, &params.id) {
            Ok(chain) => {
                let steps: Vec<_> = chain
                    .iter()
                    .map(|(date, id, status, text)| {
                        json!({
                            "date": date.to_string(),
                            "id": id,
                            "status": status.display_name(),
                            "text": text,
                        })
                    })
                    .collect();
                json!({"chain": steps}).to_string()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(
        description = "Change the status of multiple bullets at once. Status: \"done\", \"open\", \"cancelled\"."
    )]
    fn batch_set_status(&self, Parameters(params): Parameters<BatchStatusParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        let mut ok = Vec::new();
        let mut errors = Vec::new();

        for id in &params.ids {
            let result = match params.status.as_str() {
                "done" => self.state.store.complete_task(date, id),
                "open" => self.state.store.reopen_bullet(date, id),
                "cancelled" => self.state.store.cancel_task(date, id),
                other => {
                    errors.push(json!({"id": id, "error": format!("unsupported status: {other}")}));
                    continue;
                }
            };
            match result {
                Ok(_) => ok.push(id.as_str()),
                Err(e) => errors.push(json!({"id": id, "error": e.to_string()})),
            }
        }

        let mut resp = json!({"updated": ok, "count": ok.len()});
        if !errors.is_empty() {
            resp["errors"] = json!(errors);
        }
        resp.to_string()
    }
}

// -- Helpers --

fn bullet_to_json(b: &bulletd_core::model::Bullet) -> serde_json::Value {
    let mut entry = json!({
        "id": b.id,
        "status": b.status.display_name(),
        "text": b.text,
    });
    if !b.notes.is_empty() {
        entry["notes"] = json!(b.notes);
    }
    entry
}

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("invalid date: {s}"))
}

fn parse_optional_date(s: Option<&str>) -> Result<NaiveDate, String> {
    match s {
        Some(s) => parse_date(s),
        None => Ok(Local::now().date_naive()),
    }
}

fn parse_bullet_status(s: &str) -> Option<BulletStatus> {
    match s {
        "open" => Some(BulletStatus::Open),
        "done" => Some(BulletStatus::Done),
        "migrated" => Some(BulletStatus::Migrated),
        "cancelled" => Some(BulletStatus::Cancelled),
        "backlogged" => Some(BulletStatus::Backlogged),
        _ => None,
    }
}
