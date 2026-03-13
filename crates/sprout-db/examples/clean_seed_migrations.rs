//! One-shot utility to remove test seed migration records from _sqlx_migrations.
//! Run: cargo run -p sprout-db --example clean_seed_migrations

use sqlx::MySqlPool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "mysql://sprout:sprout_dev@localhost:3306/sprout".to_string());

    let pool = MySqlPool::connect(&db_url).await?;

    let result = sqlx::query(
        "DELETE FROM _sqlx_migrations WHERE version IN (20260313999997, 20260313999999)",
    )
    .execute(&pool)
    .await?;

    println!("Deleted {} seed migration records", result.rows_affected());

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM channels WHERE id = UNHEX(REPLACE('9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50', '-', ''))"
    )
    .fetch_one(&pool)
    .await?;

    println!("Seeded channel exists: {}", count.0 > 0);

    Ok(())
}
