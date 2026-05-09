use chrono::{Datelike, NaiveDate, Weekday};

/// First date strictly after `from` whose weekday is in `work_days`.
///
/// `work_days` must be non-empty; `MigrationConfig` validation guarantees this.
pub fn next_work_day(from: NaiveDate, work_days: &[Weekday]) -> NaiveDate {
    debug_assert!(!work_days.is_empty(), "work_days must be non-empty");
    let mut d = from;
    loop {
        d = d.succ_opt().expect("date overflow in next_work_day");
        if work_days.contains(&d.weekday()) {
            return d;
        }
    }
}

/// Next `n` upcoming work days starting strictly after `from` (always returns exactly `n`).
pub fn upcoming_work_days(from: NaiveDate, work_days: &[Weekday], n: usize) -> Vec<NaiveDate> {
    let mut out = Vec::with_capacity(n);
    let mut d = from;
    while out.len() < n {
        d = next_work_day(d, work_days);
        out.push(d);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn weekdays_mon_fri() -> Vec<Weekday> {
        vec![Mon, Tue, Wed, Thu, Fri]
    }

    #[test]
    fn next_work_day_default_from_each_weekday() {
        let wd = weekdays_mon_fri();
        // Reference week: 2026-05-04 Mon ... 2026-05-10 Sun
        assert_eq!(next_work_day(ymd(2026, 5, 4), &wd), ymd(2026, 5, 5)); // Mon -> Tue
        assert_eq!(next_work_day(ymd(2026, 5, 5), &wd), ymd(2026, 5, 6)); // Tue -> Wed
        assert_eq!(next_work_day(ymd(2026, 5, 6), &wd), ymd(2026, 5, 7)); // Wed -> Thu
        assert_eq!(next_work_day(ymd(2026, 5, 7), &wd), ymd(2026, 5, 8)); // Thu -> Fri
        assert_eq!(next_work_day(ymd(2026, 5, 8), &wd), ymd(2026, 5, 11)); // Fri -> Mon
        assert_eq!(next_work_day(ymd(2026, 5, 9), &wd), ymd(2026, 5, 11)); // Sat -> Mon
        assert_eq!(next_work_day(ymd(2026, 5, 10), &wd), ymd(2026, 5, 11)); // Sun -> Mon
    }

    #[test]
    fn next_work_day_irregular_mon_thu_fri() {
        let wd = vec![Mon, Thu, Fri];
        // Reference week: 2026-05-04 Mon ... 2026-05-10 Sun
        assert_eq!(next_work_day(ymd(2026, 5, 4), &wd), ymd(2026, 5, 7)); // Mon -> Thu
        assert_eq!(next_work_day(ymd(2026, 5, 5), &wd), ymd(2026, 5, 7)); // Tue -> Thu
        assert_eq!(next_work_day(ymd(2026, 5, 6), &wd), ymd(2026, 5, 7)); // Wed -> Thu
        assert_eq!(next_work_day(ymd(2026, 5, 7), &wd), ymd(2026, 5, 8)); // Thu -> Fri
        assert_eq!(next_work_day(ymd(2026, 5, 8), &wd), ymd(2026, 5, 11)); // Fri -> Mon
        assert_eq!(next_work_day(ymd(2026, 5, 9), &wd), ymd(2026, 5, 11)); // Sat -> Mon
        assert_eq!(next_work_day(ymd(2026, 5, 10), &wd), ymd(2026, 5, 11)); // Sun -> Mon
    }

    #[test]
    fn next_work_day_single_element_always_lands_on_that_weekday() {
        let wd = vec![Mon];
        // From any day in a week, next work day is the next Monday.
        assert_eq!(next_work_day(ymd(2026, 5, 4), &wd), ymd(2026, 5, 11)); // Mon -> next Mon
        assert_eq!(next_work_day(ymd(2026, 5, 5), &wd), ymd(2026, 5, 11)); // Tue -> Mon
        assert_eq!(next_work_day(ymd(2026, 5, 8), &wd), ymd(2026, 5, 11)); // Fri -> Mon
        assert_eq!(next_work_day(ymd(2026, 5, 10), &wd), ymd(2026, 5, 11)); // Sun -> Mon
    }

    #[test]
    fn next_work_day_across_month_boundary() {
        let wd = weekdays_mon_fri();
        // 2026-05-29 is a Friday; next work day is Mon 2026-06-01.
        assert_eq!(next_work_day(ymd(2026, 5, 29), &wd), ymd(2026, 6, 1));
    }

    #[test]
    fn next_work_day_across_year_boundary() {
        let wd = weekdays_mon_fri();
        // 2026-12-31 is a Thursday; next work day is Fri 2027-01-01.
        assert_eq!(next_work_day(ymd(2026, 12, 31), &wd), ymd(2027, 1, 1));
        // 2027-12-31 is a Friday; next work day is Mon 2028-01-03.
        assert_eq!(next_work_day(ymd(2027, 12, 31), &wd), ymd(2028, 1, 3));
    }

    #[test]
    fn upcoming_work_days_returns_exact_count() {
        let wd = weekdays_mon_fri();
        let out = upcoming_work_days(ymd(2026, 5, 4), &wd, 30);
        assert_eq!(out.len(), 30);
    }

    #[test]
    fn upcoming_work_days_all_in_work_days_and_strictly_increasing() {
        let wd = vec![Mon, Thu, Fri];
        let out = upcoming_work_days(ymd(2026, 5, 1), &wd, 50);
        assert_eq!(out.len(), 50);
        for d in &out {
            assert!(wd.contains(&d.weekday()), "{d} weekday not in work_days");
        }
        for win in out.windows(2) {
            assert!(win[0] < win[1], "dates not strictly increasing");
        }
    }

    #[test]
    fn upcoming_work_days_first_equals_next_work_day() {
        let wd = weekdays_mon_fri();
        let from = ymd(2026, 5, 8); // Friday
        let upcoming = upcoming_work_days(from, &wd, 5);
        assert_eq!(upcoming[0], next_work_day(from, &wd));
    }

    #[test]
    fn upcoming_work_days_with_single_weekday_yields_consecutive_mondays() {
        let wd = vec![Mon];
        let out = upcoming_work_days(ymd(2026, 5, 1), &wd, 5);
        // Mondays in May/June 2026: 4, 11, 18, 25, then June 1.
        assert_eq!(
            out,
            vec![
                ymd(2026, 5, 4),
                ymd(2026, 5, 11),
                ymd(2026, 5, 18),
                ymd(2026, 5, 25),
                ymd(2026, 6, 1),
            ]
        );
    }
}
