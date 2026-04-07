use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddBulletParams {
    /// The bullet text
    pub text: String,
    /// Target date (YYYY-MM-DD). Defaults to today.
    pub date: Option<String>,
    /// Optional context notes
    pub notes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListBulletsParams {
    /// Date to list bullets for (YYYY-MM-DD). Defaults to today.
    pub date: Option<String>,
    /// Filter by status: "open", "done", "migrated", "cancelled", "backlogged"
    pub status: Option<String>,
    /// Group results by "status". When set, returns grouped object instead of flat list.
    pub group_by: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateBulletParams {
    /// Date of the bullet (YYYY-MM-DD)
    pub date: String,
    /// Bullet ID (e.g. "a3")
    pub id: String,
    /// New text
    pub text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppendNoteParams {
    /// Date of the bullet (YYYY-MM-DD)
    pub date: String,
    /// Bullet ID (e.g. "a3")
    pub id: String,
    /// Note line to append
    pub note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateNotesParams {
    /// Date of the bullet (YYYY-MM-DD)
    pub date: String,
    /// Bullet ID (e.g. "a3")
    pub id: String,
    /// Full replacement notes (overwrites all existing notes)
    pub notes: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BulletRefParams {
    /// Date of the bullet (YYYY-MM-DD)
    pub date: String,
    /// Bullet ID (e.g. "a3")
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MigrateBulletParams {
    /// Source date (YYYY-MM-DD)
    pub date: String,
    /// Bullet ID (e.g. "a3")
    pub id: String,
    /// Target date (YYYY-MM-DD). Defaults to tomorrow.
    pub target_date: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveBulletParams {
    /// Date of the bullet (YYYY-MM-DD)
    pub date: String,
    /// Bullet ID (e.g. "a3")
    pub id: String,
    /// Target position: "top", "bottom", or a 0-based index number
    pub position: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListOpenBulletsParams {
    /// Number of days to look back. Defaults to config value.
    pub lookback_days: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchStatusParams {
    /// Date of the bullets (YYYY-MM-DD)
    pub date: String,
    /// Bullet IDs to update (e.g. ["a3", "b7", "f4"])
    pub ids: Vec<String>,
    /// Target status: "done", "open", "cancelled"
    pub status: String,
}
