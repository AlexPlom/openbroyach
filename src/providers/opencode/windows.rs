use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, Timelike, Utc};

const FIVE_HOURS_MS: f64 = 5.0 * 3600.0 * 1000.0;
const WEEK_MS: f64 = 7.0 * 24.0 * 3600.0 * 1000.0;

#[derive(Debug, Clone)]
pub struct OpenCodeGoWindows {
    pub session_spend: f64,
    pub session_resets_at: Option<i64>,
    pub weekly_spend: f64,
    pub weekly_resets_at: Option<i64>,
    pub monthly_spend: f64,
    pub monthly_resets_at: Option<i64>,
    pub monthly_period_ms: Option<i64>,
}

pub struct OpenCodeGoWindowMath;

impl OpenCodeGoWindowMath {
    /// Compute rolling windows from per-message costs
    /// costs: (timestamp_ms, cost) pairs
    /// anchor_ms: earliest-ever usage timestamp for monthly anchor
    pub fn compute(
        costs: &[(f64, f64)],
        anchor_ms: Option<f64>,
        now: DateTime<Utc>,
    ) -> OpenCodeGoWindows {
        let now_ms = now.timestamp_millis() as f64;
        let session_start = now_ms - FIVE_HOURS_MS;

        let session_spend = sum_range(costs, session_start, now_ms);
        let oldest_in_session = costs
            .iter()
            .filter(|(ms, _)| *ms >= session_start && *ms < now_ms)
            .map(|(ms, _)| *ms)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(now_ms);
        let session_resets_at = (oldest_in_session + FIVE_HOURS_MS) as i64;

        let week_start = start_of_utc_week(now_ms);
        let week_end = week_start + WEEK_MS;
        let weekly_spend = sum_range(costs, week_start, week_end);

        let month = anchored_month_bounds(now_ms, anchor_ms);
        let monthly_spend = sum_range(costs, month.0, month.1);

        OpenCodeGoWindows {
            session_spend: (session_spend * 10000.0).round() / 10000.0,
            session_resets_at: Some(session_resets_at),
            weekly_spend,
            weekly_resets_at: Some(week_end as i64),
            monthly_spend,
            monthly_resets_at: Some(month.1 as i64),
            monthly_period_ms: Some((month.1 - month.0).round() as i64),
        }
    }
}

fn sum_range(costs: &[(f64, f64)], start: f64, end: f64) -> f64 {
    let total: f64 = costs
        .iter()
        .filter(|(ms, _)| *ms >= start && *ms < end)
        .map(|(_, cost)| cost)
        .sum();
    (total * 10000.0).round() / 10000.0
}

fn start_of_utc_week(now_ms: f64) -> f64 {
    let dt = DateTime::from_timestamp_millis(now_ms as i64).unwrap_or_default();
    let naive = dt.naive_utc();
    let weekday = naive.weekday().num_days_from_monday(); // 0=Mon, 6=Sun
    let monday = naive - Duration::days(weekday as i64);
    let monday_start = NaiveDateTime::new(
        monday.date(),
        chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    );
    monday_start.and_utc().timestamp_millis() as f64
}

fn anchored_month_bounds(now_ms: f64, anchor_ms: Option<f64>) -> (f64, f64) {
    let now_dt = DateTime::from_timestamp_millis(now_ms as i64).unwrap_or_default();
    let naive = now_dt.naive_utc();

    let (start, end) = match anchor_ms {
        Some(anchor) if anchor.is_finite() => {
            let anchor_dt = DateTime::from_timestamp_millis(anchor as i64).unwrap_or_default();
            let anchor_naive = anchor_dt.naive_utc();
            let anchor_day = anchor_naive
                .day()
                .min(days_in_month(naive.year(), naive.month()));

            let mut start = NaiveDateTime::new(
                NaiveDate::from_ymd_opt(naive.year(), naive.month(), anchor_day)
                    .unwrap_or_default(),
                chrono::NaiveTime::from_hms_opt(
                    anchor_naive.hour(),
                    anchor_naive.minute(),
                    anchor_naive.second(),
                )
                .unwrap_or_default(),
            )
            .and_utc();

            if start.timestamp_millis() as f64 > now_ms {
                let (prev_year, prev_month) = if naive.month() == 1 {
                    (naive.year() - 1, 12)
                } else {
                    (naive.year(), naive.month() - 1)
                };
                start = NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(
                        prev_year,
                        prev_month,
                        anchor_day.min(days_in_month(prev_year, prev_month)),
                    )
                    .unwrap_or_default(),
                    start.time(),
                )
                .and_utc();
            }

            let (next_year, next_month) = if naive.month() == 12 {
                (naive.year() + 1, 1)
            } else {
                (naive.year(), naive.month() + 1)
            };
            let end = NaiveDateTime::new(
                NaiveDate::from_ymd_opt(
                    next_year,
                    next_month,
                    anchor_day.min(days_in_month(next_year, next_month)),
                )
                .unwrap_or_default(),
                start.time(),
            )
            .and_utc();

            (start, end)
        }
        _ => {
            let start = NaiveDateTime::new(
                NaiveDate::from_ymd_opt(naive.year(), naive.month(), 1).unwrap_or_default(),
                chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap_or_default(),
            )
            .and_utc();
            let (next_year, next_month) = if naive.month() == 12 {
                (naive.year() + 1, 1)
            } else {
                (naive.year(), naive.month() + 1)
            };
            let end = NaiveDateTime::new(
                NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap_or_default(),
                chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap_or_default(),
            )
            .and_utc();
            (start, end)
        }
    };

    (
        start.timestamp_millis() as f64,
        end.timestamp_millis() as f64,
    )
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
