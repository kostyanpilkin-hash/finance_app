//! Calendar periods. Weeks start on Monday; months are calendar months.

use chrono::{Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src/types/bindings/")]
pub enum PeriodKind {
    Day,
    Week,
    Month,
    Custom,
}

/// Inclusive local-date range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateRange {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl DateRange {
    pub fn contains(&self, date: NaiveDate) -> bool {
        date >= self.from && date <= self.to
    }
}

fn week_bounds(anchor: NaiveDate) -> DateRange {
    let monday = anchor - Duration::days(anchor.weekday().num_days_from_monday() as i64);
    DateRange {
        from: monday,
        to: monday + Duration::days(6),
    }
}

fn month_bounds(anchor: NaiveDate) -> DateRange {
    let first = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1).unwrap();
    let next_month = if anchor.month() == 12 {
        NaiveDate::from_ymd_opt(anchor.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(anchor.year(), anchor.month() + 1, 1).unwrap()
    };
    DateRange {
        from: first,
        to: next_month - Duration::days(1),
    }
}

use chrono::Datelike;

/// Range for day/week/month anchored at `anchor`.
pub fn range(kind: PeriodKind, anchor: NaiveDate) -> DateRange {
    match kind {
        PeriodKind::Day => DateRange {
            from: anchor,
            to: anchor,
        },
        PeriodKind::Week => week_bounds(anchor),
        PeriodKind::Month => month_bounds(anchor),
        PeriodKind::Custom => DateRange {
            from: anchor,
            to: anchor,
        },
    }
}

/// Anchor for the same period one step backward/forward; for a day one day etc.
pub fn shift_anchor(kind: PeriodKind, anchor: NaiveDate, back: bool) -> NaiveDate {
    let dir = if back { -1 } else { 1 };
    match kind {
        PeriodKind::Day => anchor + Duration::days(dir),
        PeriodKind::Week => anchor + Duration::weeks(dir),
        PeriodKind::Month => {
            let m = anchor.month() as i64 + dir;
            let (y, m) = if m < 1 {
                (anchor.year() - 1, m + 12)
            } else if m > 12 {
                (anchor.year() + 1, m - 12)
            } else {
                (anchor.year(), m)
            };
            let last = month_bounds(NaiveDate::from_ymd_opt(y, m as u32, 1).unwrap())
                .to
                .day();
            NaiveDate::from_ymd_opt(y, m as u32, anchor.day().min(last)).unwrap()
        }
        PeriodKind::Custom => anchor,
    }
}

/// Previous adjacent period (for "compare with previous period").
pub fn previous(kind: PeriodKind, r: &DateRange) -> DateRange {
    match kind {
        PeriodKind::Day => DateRange {
            from: r.from - Duration::days(1),
            to: r.from - Duration::days(1),
        },
        PeriodKind::Week => week_bounds(r.from - Duration::days(7)),
        PeriodKind::Month => month_bounds(r.from - Duration::days(1)),
        PeriodKind::Custom => {
            let len = r.to - r.from;
            DateRange {
                from: r.from - len - Duration::days(1),
                to: r.from - Duration::days(1),
            }
        }
    }
}

/// Local today (device timezone).
pub fn today_local() -> NaiveDate {
    Local::now().date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn week_starts_on_monday() {
        let r = range(PeriodKind::Week, d(2026, 11, 1)); // Sunday
        assert_eq!(r.from, d(2026, 10, 26));
        assert_eq!(r.to, d(2026, 11, 1));
        assert_eq!(r.from.weekday(), Weekday::Mon);
    }

    #[test]
    fn month_bounds_cross_year() {
        let r = range(PeriodKind::Month, d(2026, 12, 15));
        assert_eq!((r.from, r.to), (d(2026, 12, 1), d(2026, 12, 31)));
    }

    #[test]
    fn shift_month_handles_short_months() {
        let jan31 = d(2026, 1, 31);
        let feb = shift_anchor(PeriodKind::Month, jan31, false);
        assert_eq!(feb, d(2026, 2, 28));
        assert_eq!(
            shift_anchor(PeriodKind::Month, d(2026, 3, 31), false),
            d(2026, 4, 30)
        );
    }

    #[test]
    fn previous_custom_same_length() {
        let r = DateRange {
            from: d(2026, 11, 10),
            to: d(2026, 11, 19),
        };
        let p = previous(PeriodKind::Custom, &r);
        assert_eq!((p.from, p.to), (d(2026, 10, 31), d(2026, 11, 9)));
    }
}
