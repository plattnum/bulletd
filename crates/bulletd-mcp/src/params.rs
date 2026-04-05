use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddBulletParams {
    /// Bullet type: "task", "event", or "note"
    #[serde(rename = "type")]
    pub bullet_type: String,
    /// The bullet text
    pub text: String,
    /// Target date (YYYY-MM-DD). Defaults to today.
    pub date: Option<String>,
    /// Optional notes
    pub notes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListBulletsParams {
    /// Date to list bullets for (YYYY-MM-DD). Defaults to today.
    pub date: Option<String>,
    /// Filter by type: "task", "event", or "note"
    #[serde(rename = "type")]
    pub bullet_type: Option<String>,
    /// Filter by status: "open", "done", "migrated", "cancelled", "backlogged"
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateBulletParams {
    /// Date of the bullet (YYYY-MM-DD)
    pub date: String,
    /// Bullet ID (8-char hex)
    pub id: String,
    /// New text (optional)
    pub text: Option<String>,
    /// New notes (optional, replaces existing)
    pub notes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BulletRefParams {
    /// Date of the bullet (YYYY-MM-DD)
    pub date: String,
    /// Bullet ID (8-char hex)
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MigrateTaskParams {
    /// Source date (YYYY-MM-DD)
    pub date: String,
    /// Bullet ID (8-char hex)
    pub id: String,
    /// Target date (YYYY-MM-DD). Defaults to tomorrow.
    pub target_date: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListOpenTasksParams {
    /// Number of days to look back. Defaults to config value.
    pub lookback_days: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DailyReviewParams {
    /// Date to review (YYYY-MM-DD). Defaults to today.
    pub date: Option<String>,
}
