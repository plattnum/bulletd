//! Business operations for bulletd — CRUD, migration, review, queries.
//!
//! All operations work through a `Store` that owns the data directory path
//! and handles file I/O via the parser and serializer.

use std::fs;
use std::path::PathBuf;

use chrono::{Local, NaiveDate};

use crate::error::Error;
use crate::error::UnmigrateOutcome;
use crate::id::generate_id;
use crate::model::{
    BacklogLog, Bullet, BulletStatus, BulletType, DailyLog, MigrationFrom, MigrationTarget,
    MigrationTo,
};
use crate::parser::{parse_backlog, parse_daily_log};
use crate::serializer::{write_backlog, write_daily_log};

/// The data store — wraps a directory path and provides all operations.
pub struct Store {
    data_dir: PathBuf,
}

impl Store {
    /// Create a new store pointing at the given data directory.
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Path to a daily log file for a given date.
    fn daily_log_path(&self, date: NaiveDate) -> PathBuf {
        self.data_dir.join(format!("{date}.md"))
    }

    /// Path to the backlog file.
    #[allow(dead_code)] // Used by upcoming backlog operations
    fn backlog_path(&self) -> PathBuf {
        self.data_dir.join("backlog.md")
    }

    /// Load a daily log from disk. Returns an empty log if the file doesn't exist.
    fn load_daily_log(&self, date: NaiveDate) -> crate::error::Result<DailyLog> {
        let path = self.daily_log_path(date);
        if !path.exists() {
            return Ok(DailyLog {
                date,
                bullets: vec![],
            });
        }
        let content = fs::read_to_string(&path).map_err(|source| Error::ReadFile {
            path: path.clone(),
            source,
        })?;
        parse_daily_log(&content, &path)
    }

    /// Save a daily log to disk atomically.
    fn save_daily_log(&self, log: &DailyLog) -> crate::error::Result<()> {
        let path = self.daily_log_path(log.date);
        write_daily_log(log, &path)
    }

    /// Load the backlog from disk. Returns an empty backlog if the file doesn't exist.
    #[allow(dead_code)]
    fn load_backlog(&self) -> crate::error::Result<BacklogLog> {
        let path = self.backlog_path();
        if !path.exists() {
            return Ok(BacklogLog { bullets: vec![] });
        }
        let content = fs::read_to_string(&path).map_err(|source| Error::ReadFile {
            path: path.clone(),
            source,
        })?;
        parse_backlog(&content, &path)
    }

    /// Save the backlog to disk atomically.
    #[allow(dead_code)]
    fn save_backlog(&self, backlog: &BacklogLog) -> crate::error::Result<()> {
        let path = self.backlog_path();
        write_backlog(backlog, &path)
    }

    /// Generate a unique ID that doesn't collide with existing IDs in a bullet list.
    fn generate_unique_id(&self, existing: &[Bullet]) -> String {
        loop {
            let id = generate_id();
            if !existing.iter().any(|b| b.id == id) {
                return id;
            }
        }
    }

    // -- Operations --

    /// Add a new bullet to a day's log.
    ///
    /// If `date` is None, defaults to today.
    /// Returns the created bullet (including its generated ID).
    pub fn add_bullet(
        &self,
        bullet_type: BulletType,
        text: String,
        notes: Vec<String>,
        date: Option<NaiveDate>,
    ) -> crate::error::Result<Bullet> {
        let date = date.unwrap_or_else(|| Local::now().date_naive());
        let mut log = self.load_daily_log(date)?;

        let id = self.generate_unique_id(&log.bullets);
        let status = bullet_type.default_status();

        let bullet = Bullet {
            id,
            status,
            text,
            notes,
            migrated_to: None,
            migrated_from: None,
        };

        log.bullets.push(bullet.clone());
        self.save_daily_log(&log)?;

        Ok(bullet)
    }

    /// Update a bullet's text and/or notes. Addressed by (date, id).
    /// Does not allow changing status — use complete/cancel/migrate instead.
    pub fn update_bullet(
        &self,
        date: NaiveDate,
        id: &str,
        new_text: Option<String>,
        new_notes: Option<Vec<String>>,
    ) -> crate::error::Result<Bullet> {
        let mut log = self.load_daily_log(date)?;

        let bullet =
            log.bullets
                .iter_mut()
                .find(|b| b.id == id)
                .ok_or_else(|| Error::BulletNotFound {
                    location: date.to_string(),
                    id: id.to_string(),
                })?;

        // Check immutability
        if bullet.status.is_immutable() {
            return Err(Error::ImmutableBullet {
                location: date.to_string(),
                id: id.to_string(),
                bullet_type: bullet.bullet_type().display_name().to_string(),
            });
        }

        if let Some(text) = new_text {
            bullet.text = text;
        }
        if let Some(notes) = new_notes {
            bullet.notes = notes;
        }

        let updated = bullet.clone();
        self.save_daily_log(&log)?;
        Ok(updated)
    }

    /// Mark a task as done.
    pub fn complete_task(&self, date: NaiveDate, id: &str) -> crate::error::Result<Bullet> {
        self.set_task_status(date, id, BulletStatus::Done)
    }

    /// Mark a task as cancelled.
    pub fn cancel_task(&self, date: NaiveDate, id: &str) -> crate::error::Result<Bullet> {
        self.set_task_status(date, id, BulletStatus::Cancelled)
    }

    /// Internal: set a task's status, validating it's a task in Open state.
    fn set_task_status(
        &self,
        date: NaiveDate,
        id: &str,
        new_status: BulletStatus,
    ) -> crate::error::Result<Bullet> {
        let mut log = self.load_daily_log(date)?;

        let bullet =
            log.bullets
                .iter_mut()
                .find(|b| b.id == id)
                .ok_or_else(|| Error::BulletNotFound {
                    location: date.to_string(),
                    id: id.to_string(),
                })?;

        if !bullet.is_task() {
            return Err(Error::NotATask {
                date: date.to_string(),
                id: id.to_string(),
                bullet_type: bullet.bullet_type().display_name().to_string(),
            });
        }

        if bullet.status != BulletStatus::Open {
            return Err(Error::InvalidStatusTransition {
                from: bullet.status.display_name().to_string(),
                to: new_status.display_name().to_string(),
            });
        }

        bullet.status = new_status;
        let updated = bullet.clone();
        self.save_daily_log(&log)?;
        Ok(updated)
    }

    /// Migrate a task to a target date.
    /// Returns (updated source bullet, new target bullet).
    pub fn migrate_task(
        &self,
        source_date: NaiveDate,
        source_id: &str,
        target_date: Option<NaiveDate>,
    ) -> crate::error::Result<(Bullet, Bullet)> {
        let target_date =
            target_date.unwrap_or_else(|| source_date.succ_opt().unwrap_or(source_date));

        let mut source_log = self.load_daily_log(source_date)?;
        let mut target_log = self.load_daily_log(target_date)?;

        // Find and validate source bullet
        let source_bullet = source_log
            .bullets
            .iter()
            .find(|b| b.id == source_id)
            .ok_or_else(|| Error::BulletNotFound {
                location: source_date.to_string(),
                id: source_id.to_string(),
            })?;

        if !source_bullet.is_task() {
            return Err(Error::NotATask {
                date: source_date.to_string(),
                id: source_id.to_string(),
                bullet_type: source_bullet.bullet_type().display_name().to_string(),
            });
        }
        if source_bullet.status != BulletStatus::Open {
            return Err(Error::InvalidStatusTransition {
                from: source_bullet.status.display_name().to_string(),
                to: "migrated".to_string(),
            });
        }

        let source_text = source_bullet.text.clone();

        // Create target bullet
        let target_id = self.generate_unique_id(&target_log.bullets);
        let target_bullet = Bullet {
            id: target_id.clone(),
            status: BulletStatus::Open,
            text: source_text,
            notes: vec![],
            migrated_to: None,
            migrated_from: Some(MigrationFrom {
                source_date,
                source_id: source_id.to_string(),
            }),
        };

        // Update source bullet
        let source_bullet_mut = source_log
            .bullets
            .iter_mut()
            .find(|b| b.id == source_id)
            .unwrap(); // safe: we already found it
        source_bullet_mut.status = BulletStatus::Migrated;
        source_bullet_mut.migrated_to = Some(MigrationTo {
            target_date: MigrationTarget::Date(target_date),
            target_id: target_id.clone(),
        });
        let updated_source = source_bullet_mut.clone();

        // Append target bullet
        target_log.bullets.push(target_bullet.clone());

        // Save both files
        self.save_daily_log(&source_log)?;
        self.save_daily_log(&target_log)?;

        Ok((updated_source, target_bullet))
    }

    /// Reverse a migration. Operates on the source bullet (the one marked ➡️).
    pub fn unmigrate_task(
        &self,
        source_date: NaiveDate,
        source_id: &str,
    ) -> crate::error::Result<UnmigrateOutcome> {
        let mut source_log = self.load_daily_log(source_date)?;

        // Find source bullet and extract migration info
        let source_bullet = source_log
            .bullets
            .iter()
            .find(|b| b.id == source_id)
            .ok_or_else(|| Error::BulletNotFound {
                location: source_date.to_string(),
                id: source_id.to_string(),
            })?;

        if source_bullet.status != BulletStatus::Migrated {
            return Err(Error::InvalidStatusTransition {
                from: source_bullet.status.display_name().to_string(),
                to: "open (unmigrate)".to_string(),
            });
        }

        let (target_date, target_id) = match &source_bullet.migrated_to {
            Some(MigrationTo {
                target_date: MigrationTarget::Date(d),
                target_id,
            }) => (*d, target_id.clone()),
            _ => {
                return Err(Error::InvalidMigrationLink {
                    value: format!(
                        "source bullet {source_date}/{source_id} has no valid migration-to link"
                    ),
                });
            }
        };

        // Load target and check its state
        let mut target_log = self.load_daily_log(target_date)?;
        let target_bullet = target_log
            .bullets
            .iter()
            .find(|b| b.id == target_id)
            .ok_or_else(|| Error::BulletNotFound {
                location: target_date.to_string(),
                id: target_id.clone(),
            })?;

        // If target has been migrated onward, block
        if target_bullet.status == BulletStatus::Migrated {
            return Err(Error::UnmigrateBlockedByChain {
                date: source_date.to_string(),
                id: source_id.to_string(),
            });
        }

        // Determine outcome based on whether target was modified
        let target_modified = !target_bullet.notes.is_empty();
        let outcome = if target_modified {
            // Cancel the target instead of deleting
            let tb = target_log
                .bullets
                .iter_mut()
                .find(|b| b.id == target_id)
                .unwrap();
            tb.status = BulletStatus::Cancelled;
            UnmigrateOutcome::TargetCancelled
        } else {
            // Delete the target
            target_log.bullets.retain(|b| b.id != target_id);
            UnmigrateOutcome::TargetDeleted
        };

        // Revert source
        let sb = source_log
            .bullets
            .iter_mut()
            .find(|b| b.id == source_id)
            .unwrap();
        sb.status = BulletStatus::Open;
        sb.migrated_to = None;
        sb.migrated_from = None;

        // Save both
        self.save_daily_log(&source_log)?;
        self.save_daily_log(&target_log)?;

        Ok(outcome)
    }

    /// Move a task to the backlog.
    pub fn backlog_task(
        &self,
        source_date: NaiveDate,
        source_id: &str,
    ) -> crate::error::Result<(Bullet, Bullet)> {
        let mut source_log = self.load_daily_log(source_date)?;
        let mut backlog = self.load_backlog()?;

        // Find and validate source bullet
        let source_bullet = source_log
            .bullets
            .iter()
            .find(|b| b.id == source_id)
            .ok_or_else(|| Error::BulletNotFound {
                location: source_date.to_string(),
                id: source_id.to_string(),
            })?;

        if !source_bullet.is_task() {
            return Err(Error::NotATask {
                date: source_date.to_string(),
                id: source_id.to_string(),
                bullet_type: source_bullet.bullet_type().display_name().to_string(),
            });
        }
        if source_bullet.status != BulletStatus::Open {
            return Err(Error::InvalidStatusTransition {
                from: source_bullet.status.display_name().to_string(),
                to: "backlogged".to_string(),
            });
        }

        let source_text = source_bullet.text.clone();

        // Create backlog bullet
        let backlog_id = self.generate_unique_id(&backlog.bullets);
        let backlog_bullet = Bullet {
            id: backlog_id.clone(),
            status: BulletStatus::Open,
            text: source_text,
            notes: vec![],
            migrated_to: None,
            migrated_from: Some(MigrationFrom {
                source_date,
                source_id: source_id.to_string(),
            }),
        };

        // Update source bullet
        let sb = source_log
            .bullets
            .iter_mut()
            .find(|b| b.id == source_id)
            .unwrap();
        sb.status = BulletStatus::Backlogged;
        sb.migrated_to = Some(MigrationTo {
            target_date: MigrationTarget::Backlog,
            target_id: backlog_id,
        });
        let updated_source = sb.clone();

        // Append to backlog
        backlog.bullets.push(backlog_bullet.clone());

        // Save both
        self.save_daily_log(&source_log)?;
        self.save_backlog(&backlog)?;

        Ok((updated_source, backlog_bullet))
    }

    /// List all open tasks across recent days.
    pub fn list_open_tasks(
        &self,
        lookback_days: u32,
    ) -> crate::error::Result<Vec<(NaiveDate, Bullet)>> {
        let today = Local::now().date_naive();
        let mut results = Vec::new();

        for i in 0..lookback_days {
            let date = today - chrono::Duration::days(i64::from(i));
            let log = self.load_daily_log(date)?;
            for bullet in log.bullets {
                if bullet.status == BulletStatus::Open {
                    results.push((date, bullet));
                }
            }
        }

        Ok(results)
    }

    /// Get all open tasks for a specific date (for daily review).
    pub fn daily_review(&self, date: NaiveDate) -> crate::error::Result<Vec<Bullet>> {
        let log = self.load_daily_log(date)?;
        Ok(log
            .bullets
            .into_iter()
            .filter(|b| b.status == BulletStatus::Open)
            .collect())
    }

    /// List bullets for a date with optional filters.
    pub fn list_bullets(
        &self,
        date: NaiveDate,
        type_filter: Option<BulletType>,
        status_filter: Option<BulletStatus>,
    ) -> crate::error::Result<Vec<Bullet>> {
        let log = self.load_daily_log(date)?;
        let filtered = log
            .bullets
            .into_iter()
            .filter(|b| {
                if let Some(t) = type_filter
                    && b.bullet_type() != t
                {
                    return false;
                }
                if let Some(s) = status_filter
                    && b.status != s
                {
                    return false;
                }
                true
            })
            .collect();
        Ok(filtered)
    }

    /// Trace a bullet's migration chain.
    pub fn migration_history(
        &self,
        date: NaiveDate,
        id: &str,
    ) -> crate::error::Result<Vec<(NaiveDate, String, BulletStatus)>> {
        let mut chain = Vec::new();
        let mut current_date = date;
        let mut current_id = id.to_string();

        // Walk backward to find the origin
        loop {
            let log = self.load_daily_log(current_date)?;
            let bullet = log.bullets.iter().find(|b| b.id == current_id);
            let bullet = match bullet {
                Some(b) => b,
                None => break,
            };

            if let Some(MigrationFrom {
                source_date,
                source_id,
            }) = &bullet.migrated_from
            {
                current_date = *source_date;
                current_id = source_id.clone();
            } else {
                break;
            }
        }

        // Now walk forward from the origin
        loop {
            let log = self.load_daily_log(current_date)?;
            let bullet = log.bullets.iter().find(|b| b.id == current_id);
            let bullet = match bullet {
                Some(b) => b,
                None => break,
            };

            chain.push((current_date, current_id.clone(), bullet.status));

            if let Some(MigrationTo {
                target_date: MigrationTarget::Date(d),
                target_id,
            }) = &bullet.migrated_to
            {
                current_date = *d;
                current_id = target_id.clone();
            } else {
                break;
            }
        }

        Ok(chain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        (dir, store)
    }

    #[test]
    fn add_task_to_empty_day() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(
                BulletType::Task,
                "Fix the bug".to_string(),
                vec![],
                Some(date),
            )
            .unwrap();

        assert_eq!(bullet.status, BulletStatus::Open);
        assert_eq!(bullet.text, "Fix the bug");
        assert_eq!(bullet.id.len(), 8);

        // Verify persisted
        let log = store.load_daily_log(date).unwrap();
        assert_eq!(log.bullets.len(), 1);
        assert_eq!(log.bullets[0].id, bullet.id);
    }

    #[test]
    fn add_event() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(
                BulletType::Event,
                "Sprint planning 10am".to_string(),
                vec![],
                Some(date),
            )
            .unwrap();

        assert_eq!(bullet.status, BulletStatus::Event);
    }

    #[test]
    fn add_note() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(
                BulletType::Note,
                "Important context".to_string(),
                vec!["Detail one".to_string()],
                Some(date),
            )
            .unwrap();

        assert_eq!(bullet.status, BulletStatus::Note);
        assert_eq!(bullet.notes.len(), 1);
    }

    #[test]
    fn add_to_existing_day() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let b1 = store
            .add_bullet(BulletType::Task, "First".to_string(), vec![], Some(date))
            .unwrap();
        let b2 = store
            .add_bullet(BulletType::Task, "Second".to_string(), vec![], Some(date))
            .unwrap();

        let log = store.load_daily_log(date).unwrap();
        assert_eq!(log.bullets.len(), 2);
        assert_eq!(log.bullets[0].id, b1.id);
        assert_eq!(log.bullets[1].id, b2.id);
        // IDs are different
        assert_ne!(b1.id, b2.id);
    }

    #[test]
    fn add_preserves_existing_bullets() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let b1 = store
            .add_bullet(BulletType::Task, "First".to_string(), vec![], Some(date))
            .unwrap();
        store
            .add_bullet(BulletType::Event, "Meeting".to_string(), vec![], Some(date))
            .unwrap();

        let log = store.load_daily_log(date).unwrap();
        assert_eq!(log.bullets[0].text, "First");
        assert_eq!(log.bullets[0].id, b1.id);
        assert_eq!(log.bullets[0].status, BulletStatus::Open);
    }

    #[test]
    fn update_bullet_text() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Old text".to_string(), vec![], Some(date))
            .unwrap();

        let updated = store
            .update_bullet(date, &bullet.id, Some("New text".to_string()), None)
            .unwrap();

        assert_eq!(updated.text, "New text");
        assert_eq!(updated.status, BulletStatus::Open); // unchanged
    }

    #[test]
    fn update_bullet_notes() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Task".to_string(), vec![], Some(date))
            .unwrap();

        let updated = store
            .update_bullet(
                date,
                &bullet.id,
                None,
                Some(vec!["Note 1".to_string(), "Note 2".to_string()]),
            )
            .unwrap();

        assert_eq!(updated.notes.len(), 2);
        assert_eq!(updated.text, "Task"); // unchanged
    }

    #[test]
    fn update_bullet_not_found() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let result = store.update_bullet(date, "deadbeef", Some("New".to_string()), None);
        assert!(matches!(result, Err(Error::BulletNotFound { .. })));
    }

    #[test]
    fn update_immutable_event_fails() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let event = store
            .add_bullet(BulletType::Event, "Meeting".to_string(), vec![], Some(date))
            .unwrap();

        let result = store.update_bullet(date, &event.id, Some("Changed".to_string()), None);
        assert!(matches!(result, Err(Error::ImmutableBullet { .. })));
    }

    #[test]
    fn complete_task_success() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Do thing".to_string(), vec![], Some(date))
            .unwrap();

        let completed = store.complete_task(date, &bullet.id).unwrap();
        assert_eq!(completed.status, BulletStatus::Done);

        // Verify persisted
        let log = store.load_daily_log(date).unwrap();
        assert_eq!(log.bullets[0].status, BulletStatus::Done);
    }

    #[test]
    fn cancel_task_success() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Do thing".to_string(), vec![], Some(date))
            .unwrap();

        let cancelled = store.cancel_task(date, &bullet.id).unwrap();
        assert_eq!(cancelled.status, BulletStatus::Cancelled);
    }

    #[test]
    fn complete_event_fails() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let event = store
            .add_bullet(BulletType::Event, "Meeting".to_string(), vec![], Some(date))
            .unwrap();

        let result = store.complete_task(date, &event.id);
        assert!(matches!(result, Err(Error::NotATask { .. })));
    }

    #[test]
    fn complete_already_done_fails() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Do thing".to_string(), vec![], Some(date))
            .unwrap();

        store.complete_task(date, &bullet.id).unwrap();

        // Try to complete again
        let result = store.complete_task(date, &bullet.id);
        assert!(matches!(result, Err(Error::InvalidStatusTransition { .. })));
    }

    #[test]
    fn cancel_already_cancelled_fails() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Do thing".to_string(), vec![], Some(date))
            .unwrap();

        store.cancel_task(date, &bullet.id).unwrap();

        let result = store.cancel_task(date, &bullet.id);
        assert!(matches!(result, Err(Error::InvalidStatusTransition { .. })));
    }

    // -- Migrate tests --

    #[test]
    fn migrate_task_to_next_day() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let target = NaiveDate::from_ymd_opt(2026, 4, 11).unwrap();

        let bullet = store
            .add_bullet(
                BulletType::Task,
                "Migrate me".to_string(),
                vec![],
                Some(date),
            )
            .unwrap();

        let (source, target_bullet) = store.migrate_task(date, &bullet.id, Some(target)).unwrap();

        assert_eq!(source.status, BulletStatus::Migrated);
        assert!(matches!(
            &source.migrated_to,
            Some(MigrationTo { target_date: MigrationTarget::Date(d), .. }) if *d == target
        ));

        assert_eq!(target_bullet.status, BulletStatus::Open);
        assert_eq!(target_bullet.text, "Migrate me");
        assert!(matches!(
            &target_bullet.migrated_from,
            Some(MigrationFrom { source_date, .. }) if *source_date == date
        ));

        // Verify persisted
        let target_log = store.load_daily_log(target).unwrap();
        assert_eq!(target_log.bullets.len(), 1);
        assert_eq!(target_log.bullets[0].id, target_bullet.id);
    }

    #[test]
    fn migrate_task_defaults_to_tomorrow() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let tomorrow = NaiveDate::from_ymd_opt(2026, 4, 11).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Task".to_string(), vec![], Some(date))
            .unwrap();

        let (source, _) = store.migrate_task(date, &bullet.id, None).unwrap();
        assert!(matches!(
            &source.migrated_to,
            Some(MigrationTo { target_date: MigrationTarget::Date(d), .. }) if *d == tomorrow
        ));
    }

    #[test]
    fn migrate_to_existing_day_preserves_bullets() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let target = NaiveDate::from_ymd_opt(2026, 4, 11).unwrap();

        // Add a bullet to the target day first
        store
            .add_bullet(
                BulletType::Task,
                "Existing task".to_string(),
                vec![],
                Some(target),
            )
            .unwrap();

        let bullet = store
            .add_bullet(
                BulletType::Task,
                "Migrate me".to_string(),
                vec![],
                Some(date),
            )
            .unwrap();

        store.migrate_task(date, &bullet.id, Some(target)).unwrap();

        let target_log = store.load_daily_log(target).unwrap();
        assert_eq!(target_log.bullets.len(), 2);
        assert_eq!(target_log.bullets[0].text, "Existing task");
        assert_eq!(target_log.bullets[1].text, "Migrate me");
    }

    #[test]
    fn migrate_event_fails() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let event = store
            .add_bullet(BulletType::Event, "Meeting".to_string(), vec![], Some(date))
            .unwrap();

        let result = store.migrate_task(date, &event.id, None);
        assert!(matches!(result, Err(Error::NotATask { .. })));
    }

    #[test]
    fn migrate_done_task_fails() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(
                BulletType::Task,
                "Done task".to_string(),
                vec![],
                Some(date),
            )
            .unwrap();
        store.complete_task(date, &bullet.id).unwrap();

        let result = store.migrate_task(date, &bullet.id, None);
        assert!(matches!(result, Err(Error::InvalidStatusTransition { .. })));
    }

    // -- Unmigrate tests --

    #[test]
    fn unmigrate_untouched_target_deletes_it() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let target = NaiveDate::from_ymd_opt(2026, 4, 11).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Task".to_string(), vec![], Some(date))
            .unwrap();

        store.migrate_task(date, &bullet.id, Some(target)).unwrap();

        let outcome = store.unmigrate_task(date, &bullet.id).unwrap();
        assert_eq!(outcome, UnmigrateOutcome::TargetDeleted);

        // Source is back to open
        let source_log = store.load_daily_log(date).unwrap();
        assert_eq!(source_log.bullets[0].status, BulletStatus::Open);
        assert!(source_log.bullets[0].migrated_to.is_none());
        assert!(source_log.bullets[0].migrated_from.is_none());

        // Target is deleted
        let target_log = store.load_daily_log(target).unwrap();
        assert!(target_log.bullets.is_empty());
    }

    #[test]
    fn unmigrate_modified_target_cancels_it() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let target = NaiveDate::from_ymd_opt(2026, 4, 11).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Task".to_string(), vec![], Some(date))
            .unwrap();

        let (_, target_bullet) = store.migrate_task(date, &bullet.id, Some(target)).unwrap();

        // Modify the target bullet by adding notes
        store
            .update_bullet(
                target,
                &target_bullet.id,
                None,
                Some(vec!["Added context".to_string()]),
            )
            .unwrap();

        let outcome = store.unmigrate_task(date, &bullet.id).unwrap();
        assert_eq!(outcome, UnmigrateOutcome::TargetCancelled);

        // Target is cancelled, not deleted
        let target_log = store.load_daily_log(target).unwrap();
        assert_eq!(target_log.bullets.len(), 1);
        assert_eq!(target_log.bullets[0].status, BulletStatus::Cancelled);
    }

    #[test]
    fn unmigrate_blocked_by_chain() {
        let (_dir, store) = test_store();
        let d1 = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 4, 11).unwrap();
        let d3 = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();

        let bullet = store
            .add_bullet(BulletType::Task, "Task".to_string(), vec![], Some(d1))
            .unwrap();

        let (_, target1) = store.migrate_task(d1, &bullet.id, Some(d2)).unwrap();
        store.migrate_task(d2, &target1.id, Some(d3)).unwrap();

        // Can't unmigrate d1 because d2's target has been migrated onward
        let result = store.unmigrate_task(d1, &bullet.id);
        assert!(matches!(result, Err(Error::UnmigrateBlockedByChain { .. })));
    }

    // -- Backlog tests --

    #[test]
    fn backlog_task_success() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let bullet = store
            .add_bullet(
                BulletType::Task,
                "Low priority".to_string(),
                vec![],
                Some(date),
            )
            .unwrap();

        let (source, backlog_bullet) = store.backlog_task(date, &bullet.id).unwrap();

        assert_eq!(source.status, BulletStatus::Backlogged);
        assert!(matches!(
            &source.migrated_to,
            Some(MigrationTo {
                target_date: MigrationTarget::Backlog,
                ..
            })
        ));

        assert_eq!(backlog_bullet.status, BulletStatus::Open);
        assert_eq!(backlog_bullet.text, "Low priority");
        assert!(matches!(
            &backlog_bullet.migrated_from,
            Some(MigrationFrom { source_date, .. }) if *source_date == date
        ));

        // Verify backlog persisted
        let backlog = store.load_backlog().unwrap();
        assert_eq!(backlog.bullets.len(), 1);
    }

    #[test]
    fn backlog_event_fails() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        let event = store
            .add_bullet(BulletType::Event, "Meeting".to_string(), vec![], Some(date))
            .unwrap();

        let result = store.backlog_task(date, &event.id);
        assert!(matches!(result, Err(Error::NotATask { .. })));
    }

    // -- Query tests --

    #[test]
    fn daily_review_returns_open_tasks() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        store
            .add_bullet(BulletType::Task, "Open 1".to_string(), vec![], Some(date))
            .unwrap();
        let b2 = store
            .add_bullet(BulletType::Task, "Done".to_string(), vec![], Some(date))
            .unwrap();
        store.complete_task(date, &b2.id).unwrap();
        store
            .add_bullet(BulletType::Task, "Open 2".to_string(), vec![], Some(date))
            .unwrap();
        store
            .add_bullet(BulletType::Event, "Event".to_string(), vec![], Some(date))
            .unwrap();

        let open = store.daily_review(date).unwrap();
        assert_eq!(open.len(), 2);
        assert_eq!(open[0].text, "Open 1");
        assert_eq!(open[1].text, "Open 2");
    }

    #[test]
    fn list_bullets_with_type_filter() {
        let (_dir, store) = test_store();
        let date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();

        store
            .add_bullet(BulletType::Task, "Task".to_string(), vec![], Some(date))
            .unwrap();
        store
            .add_bullet(BulletType::Event, "Event".to_string(), vec![], Some(date))
            .unwrap();
        store
            .add_bullet(BulletType::Note, "Note".to_string(), vec![], Some(date))
            .unwrap();

        let events = store
            .list_bullets(date, Some(BulletType::Event), None)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text, "Event");
    }

    #[test]
    fn migration_history_three_day_chain() {
        let (_dir, store) = test_store();
        let d1 = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 4, 11).unwrap();
        let d3 = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();

        let bullet = store
            .add_bullet(
                BulletType::Task,
                "Persistent task".to_string(),
                vec![],
                Some(d1),
            )
            .unwrap();

        let (_, t1) = store.migrate_task(d1, &bullet.id, Some(d2)).unwrap();
        let (_, t2) = store.migrate_task(d2, &t1.id, Some(d3)).unwrap();
        store.complete_task(d3, &t2.id).unwrap();

        // Trace from the middle
        let history = store.migration_history(d2, &t1.id).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].0, d1);
        assert_eq!(history[0].2, BulletStatus::Migrated);
        assert_eq!(history[1].0, d2);
        assert_eq!(history[1].2, BulletStatus::Migrated);
        assert_eq!(history[2].0, d3);
        assert_eq!(history[2].2, BulletStatus::Done);
    }
}
