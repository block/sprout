//! Monthly partition manager for `events` and `delivery_log`.
//!
//! Call `ensure_future_partitions` on startup and monthly via cron.

use chrono::{Datelike, TimeZone, Utc};
use sqlx::{MySqlPool, Row};
use tracing::info;

use crate::error::{DbError, Result};

/// Tables that may be partition-managed. Allowlist prevents DDL injection.
const PARTITIONED_TABLES: &[&str] = &["events", "delivery_log"];

/// Ensures monthly partition tables exist for the next `months_ahead` months.
pub async fn ensure_future_partitions(pool: &MySqlPool, months_ahead: u32) -> Result<()> {
    let now = Utc::now();

    for i in 0..=(months_ahead as i32) {
        let year = now.year();
        let month = now.month() as i32 + i;
        let (target_year, target_month) = if month > 12 {
            (year + (month - 1) / 12, ((month - 1) % 12 + 1) as u32)
        } else {
            (year, month as u32)
        };

        let (end_year, end_month) = if target_month == 12 {
            (target_year + 1, 1u32)
        } else {
            (target_year, target_month + 1)
        };
        let end = Utc
            .with_ymd_and_hms(end_year, end_month, 1, 0, 0, 0)
            .single()
            .ok_or_else(|| {
                DbError::InvalidData(format!("invalid date: {end_year}-{end_month:02}-01"))
            })?;

        let suffix = format!("{:04}_{:02}", target_year, target_month);
        let end_str = end.format("%Y-%m-%d").to_string();
        let partition_name = format!("p{}", suffix);

        for table in PARTITIONED_TABLES {
            ensure_partition(pool, table, &partition_name, &end_str, &suffix).await?;
        }
    }

    Ok(())
}

/// Validate that a partition suffix is digits and underscores only.
fn validate_partition_suffix(suffix: &str) -> bool {
    !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit() || c == '_')
}

/// Validate that a date string matches YYYY-MM-DD format.
fn validate_date_str(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(|b| b.is_ascii_digit())
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        && bytes[8..].iter().all(|b| b.is_ascii_digit())
}

async fn ensure_partition(
    pool: &MySqlPool,
    table_name: &str,
    partition_name: &str,
    end_date_str: &str,
    suffix: &str,
) -> Result<()> {
    // Allowlist check — parameterized queries cannot be used for DDL identifiers.
    if !PARTITIONED_TABLES.contains(&table_name) {
        return Err(DbError::InvalidData(format!(
            "table not in partition allowlist: {table_name:?}"
        )));
    }
    if !validate_partition_suffix(suffix) {
        return Err(DbError::InvalidData(format!(
            "partition suffix contains invalid characters: {suffix:?}"
        )));
    }
    if !validate_date_str(end_date_str) {
        return Err(DbError::InvalidData(format!(
            "end_date_str is not YYYY-MM-DD: {end_date_str:?}"
        )));
    }

    let row = sqlx::query(
        r#"
        SELECT COUNT(*) as cnt
        FROM information_schema.PARTITIONS
        WHERE TABLE_SCHEMA = DATABASE()
          AND TABLE_NAME = ?
          AND PARTITION_NAME = ?
        "#,
    )
    .bind(table_name)
    .bind(partition_name)
    .fetch_one(pool)
    .await?;

    let cnt: i64 = row.try_get("cnt")?;
    if cnt > 0 {
        return Ok(());
    }

    // DDL identifiers cannot be parameterized in MySQL — all inputs are validated above.
    let sql = format!(
        "ALTER TABLE {table_name} ADD PARTITION \
         (PARTITION {partition_name} VALUES LESS THAN (TO_DAYS('{end_date_str}')))"
    );

    sqlx::query(&sql).execute(pool).await?;
    info!("added partition {table_name}_{suffix}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_validation() {
        assert!(validate_partition_suffix("2026_03"));
        assert!(validate_partition_suffix("9999_12"));
        assert!(!validate_partition_suffix(""));
        assert!(!validate_partition_suffix("2026-03"));
        assert!(!validate_partition_suffix("2026_03; DROP TABLE events--"));
    }

    #[test]
    fn date_str_validation() {
        assert!(validate_date_str("2026-03-01"));
        assert!(validate_date_str("9999-12-31"));
        assert!(!validate_date_str("2026-3-01"));
        assert!(!validate_date_str("2026/03/01"));
        assert!(!validate_date_str("20260301"));
        assert!(!validate_date_str("2026-03-01; DROP TABLE events--"));
    }

    #[test]
    fn table_allowlist() {
        assert!(PARTITIONED_TABLES.contains(&"events"));
        assert!(PARTITIONED_TABLES.contains(&"delivery_log"));
        assert!(!PARTITIONED_TABLES.contains(&"api_tokens"));
        assert!(!PARTITIONED_TABLES.contains(&"users"));
    }
}
