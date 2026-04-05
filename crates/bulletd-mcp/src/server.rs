use std::sync::Arc;

use chrono::{Local, NaiveDate};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_router};
use serde_json::json;

use bulletd_core::config::Config;
use bulletd_core::model::{BulletStatus, BulletType};
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

impl ServerHandler for BulletdMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "bulletd structured bullet logging server. Manages daily logs stored as GFM markdown tables."
                    .to_string(),
            )
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

    #[tool(description = "Add a bullet (task, event, or note) to a day's log")]
    fn add_bullet(&self, Parameters(params): Parameters<AddBulletParams>) -> String {
        let bullet_type = match params.bullet_type.as_str() {
            "task" => BulletType::Task,
            "event" => BulletType::Event,
            "note" => BulletType::Note,
            other => return json!({"error": format!("invalid type: {other}")}).to_string(),
        };

        let date = match parse_optional_date(params.date.as_deref()) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        let notes = params.notes.unwrap_or_default();

        match self
            .state
            .store
            .add_bullet(bullet_type, params.text, notes, Some(date))
        {
            Ok(bullet) => json!({
                "id": bullet.id,
                "status": bullet.status.as_emoji(),
                "text": bullet.text,
                "date": date.to_string(),
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "List bullets for a date with optional filters")]
    fn list_bullets(&self, Parameters(params): Parameters<ListBulletsParams>) -> String {
        let date = match parse_optional_date(params.date.as_deref()) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        let type_filter = params.bullet_type.as_deref().and_then(parse_bullet_type);
        let status_filter = params.status.as_deref().and_then(parse_bullet_status);

        match self
            .state
            .store
            .list_bullets(date, type_filter, status_filter)
        {
            Ok(bullets) => {
                let items: Vec<_> = bullets
                    .iter()
                    .map(|b| {
                        json!({
                            "id": b.id,
                            "status": b.status.as_emoji(),
                            "text": b.text,
                            "notes": b.notes,
                        })
                    })
                    .collect();
                json!({"date": date.to_string(), "bullets": items}).to_string()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Update a bullet's text and/or notes")]
    fn update_bullet(&self, Parameters(params): Parameters<UpdateBulletParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self
            .state
            .store
            .update_bullet(date, &params.id, params.text, params.notes)
        {
            Ok(bullet) => json!({
                "id": bullet.id,
                "status": bullet.status.as_emoji(),
                "text": bullet.text,
                "notes": bullet.notes,
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Mark a task as done")]
    fn complete_task(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.complete_task(date, &params.id) {
            Ok(bullet) => json!({
                "id": bullet.id,
                "status": bullet.status.as_emoji(),
                "text": bullet.text,
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Mark a task as cancelled")]
    fn cancel_task(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.cancel_task(date, &params.id) {
            Ok(bullet) => json!({
                "id": bullet.id,
                "status": bullet.status.as_emoji(),
                "text": bullet.text,
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Migrate a task to another day (defaults to tomorrow)")]
    fn migrate_task(&self, Parameters(params): Parameters<MigrateTaskParams>) -> String {
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
            Ok((source, target_bullet)) => json!({
                "source": {
                    "id": source.id,
                    "status": source.status.as_emoji(),
                    "text": source.text,
                },
                "target": {
                    "id": target_bullet.id,
                    "status": target_bullet.status.as_emoji(),
                    "text": target_bullet.text,
                },
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Reverse a migration — revert source to open, clean up target")]
    fn unmigrate_task(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.unmigrate_task(date, &params.id) {
            Ok(outcome) => json!({
                "outcome": format!("{outcome:?}"),
                "id": params.id,
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Move a task to the backlog")]
    fn backlog_task(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.backlog_task(date, &params.id) {
            Ok((source, backlog_bullet)) => json!({
                "source": {
                    "id": source.id,
                    "status": source.status.as_emoji(),
                },
                "backlog": {
                    "id": backlog_bullet.id,
                    "text": backlog_bullet.text,
                },
            })
            .to_string(),
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "List all open tasks across recent days")]
    fn list_open_tasks(&self, Parameters(params): Parameters<ListOpenTasksParams>) -> String {
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
                json!({"open_tasks": items}).to_string()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Get open tasks needing review for a date")]
    fn daily_review(&self, Parameters(params): Parameters<DailyReviewParams>) -> String {
        let date = match parse_optional_date(params.date.as_deref()) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.daily_review(date) {
            Ok(bullets) => {
                let items: Vec<_> = bullets
                    .iter()
                    .map(|b| {
                        json!({
                            "id": b.id,
                            "text": b.text,
                            "notes": b.notes,
                        })
                    })
                    .collect();
                json!({"date": date.to_string(), "open_tasks": items}).to_string()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }

    #[tool(description = "Trace a task's migration chain across days")]
    fn migration_history(&self, Parameters(params): Parameters<BulletRefParams>) -> String {
        let date = match parse_date(&params.date) {
            Ok(d) => d,
            Err(e) => return json!({"error": e}).to_string(),
        };

        match self.state.store.migration_history(date, &params.id) {
            Ok(chain) => {
                let steps: Vec<_> = chain
                    .iter()
                    .map(|(date, id, status)| {
                        json!({
                            "date": date.to_string(),
                            "id": id,
                            "status": status.as_emoji(),
                        })
                    })
                    .collect();
                json!({"chain": steps}).to_string()
            }
            Err(e) => json!({"error": e.to_string()}).to_string(),
        }
    }
}

// -- Helpers --

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("invalid date: {s}"))
}

fn parse_optional_date(s: Option<&str>) -> Result<NaiveDate, String> {
    match s {
        Some(s) => parse_date(s),
        None => Ok(Local::now().date_naive()),
    }
}

fn parse_bullet_type(s: &str) -> Option<BulletType> {
    match s {
        "task" => Some(BulletType::Task),
        "event" => Some(BulletType::Event),
        "note" => Some(BulletType::Note),
        _ => None,
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
