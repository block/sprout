//! Project CRUD operations.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::{DbError, Result};

/// A project row as returned from the database.
#[derive(Debug, Clone)]
pub struct ProjectRecord {
    /// Unique project identifier.
    pub id: Uuid,
    /// Human-readable project name.
    pub name: String,
    /// Optional project description.
    pub description: Option<String>,
    /// Shared context/instructions injected into agents.
    pub prompt: Option<String>,
    /// Optional icon identifier.
    pub icon: Option<String>,
    /// Optional color identifier.
    pub color: Option<String>,
    /// Execution environment (`"local"` or `"blox"`).
    pub environment: String,
    /// JSONB array of repository URLs.
    pub repo_urls: serde_json::Value,
    /// Compressed public key bytes of the project creator.
    pub created_by: Vec<u8>,
    /// When the project was created.
    pub created_at: DateTime<Utc>,
    /// When the project was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the project was archived, if applicable.
    pub archived_at: Option<DateTime<Utc>>,
    /// When the project was soft-deleted, if applicable.
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Partial update for a project.
pub struct ProjectUpdate {
    /// New project name, or `None` to leave unchanged.
    pub name: Option<String>,
    /// New description, or `None` to leave unchanged.
    pub description: Option<String>,
    /// New prompt, or `None` to leave unchanged.
    pub prompt: Option<String>,
    /// New icon, or `None` to leave unchanged.
    pub icon: Option<String>,
    /// New color, or `None` to leave unchanged.
    pub color: Option<String>,
    /// New environment, or `None` to leave unchanged.
    pub environment: Option<String>,
    /// New repo_urls, or `None` to leave unchanged.
    pub repo_urls: Option<serde_json::Value>,
}

const SELECT_COLS: &str = r#"
    id, name, description, prompt, icon, color, environment, repo_urls,
    created_by, created_at, updated_at, archived_at, deleted_at
"#;

fn row_to_project_record(row: sqlx::postgres::PgRow) -> Result<ProjectRecord> {
    Ok(ProjectRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description").unwrap_or(None),
        prompt: row.try_get("prompt").unwrap_or(None),
        icon: row.try_get("icon").unwrap_or(None),
        color: row.try_get("color").unwrap_or(None),
        environment: row.try_get("environment")?,
        repo_urls: row.try_get("repo_urls")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        archived_at: row.try_get("archived_at").unwrap_or(None),
        deleted_at: row.try_get("deleted_at").unwrap_or(None),
    })
}

/// Creates a project with a client-supplied UUID (idempotent via ON CONFLICT DO NOTHING).
///
/// Returns `(record, true)` if newly created, or `(record, false)` if already exists.
#[allow(clippy::too_many_arguments)]
pub async fn create_project_with_id(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    environment: &str,
    description: Option<&str>,
    prompt: Option<&str>,
    icon: Option<&str>,
    color: Option<&str>,
    repo_urls: &serde_json::Value,
    created_by: &[u8],
) -> Result<(ProjectRecord, bool)> {
    if created_by.len() != 32 {
        return Err(DbError::InvalidData(format!(
            "pubkey must be 32 bytes, got {}",
            created_by.len()
        )));
    }

    if id.is_nil() {
        return Err(DbError::InvalidData("project id must not be nil".into()));
    }

    let rows_affected = sqlx::query(
        r#"
        INSERT INTO projects (id, name, environment, description, prompt, icon, color, repo_urls, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(environment)
    .bind(description)
    .bind(prompt)
    .bind(icon)
    .bind(color)
    .bind(repo_urls)
    .bind(created_by)
    .execute(pool)
    .await?
    .rows_affected();

    let was_created = rows_affected > 0;

    let row = sqlx::query(&format!("SELECT {SELECT_COLS} FROM projects WHERE id = $1"))
        .bind(id)
        .fetch_one(pool)
        .await?;

    Ok((row_to_project_record(row)?, was_created))
}

/// Fetches a project by ID. Returns error if not found or deleted.
pub async fn get_project(pool: &PgPool, id: Uuid) -> Result<ProjectRecord> {
    let row = sqlx::query(&format!(
        "SELECT {SELECT_COLS} FROM projects WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => row_to_project_record(r),
        None => Err(DbError::NotFound(format!("project {id} not found"))),
    }
}

/// Lists projects, optionally filtered by creator pubkey.
pub async fn list_projects(pool: &PgPool, created_by: Option<&[u8]>) -> Result<Vec<ProjectRecord>> {
    let rows = if let Some(pubkey) = created_by {
        sqlx::query(&format!(
            "SELECT {SELECT_COLS} FROM projects WHERE created_by = $1 AND deleted_at IS NULL ORDER BY created_at DESC"
        ))
        .bind(pubkey)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(&format!(
            "SELECT {SELECT_COLS} FROM projects WHERE deleted_at IS NULL ORDER BY created_at DESC"
        ))
        .fetch_all(pool)
        .await?
    };

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_project_record(row)?);
    }
    Ok(out)
}

/// Updates a project's fields dynamically.
pub async fn update_project(
    pool: &PgPool,
    id: Uuid,
    updates: ProjectUpdate,
) -> Result<ProjectRecord> {
    let has_update = updates.name.is_some()
        || updates.description.is_some()
        || updates.prompt.is_some()
        || updates.icon.is_some()
        || updates.color.is_some()
        || updates.environment.is_some()
        || updates.repo_urls.is_some();

    if !has_update {
        return Err(DbError::InvalidData(
            "at least one field must be provided for update".to_string(),
        ));
    }

    let mut set_parts: Vec<String> = Vec::new();
    let mut param_idx: usize = 1;

    if updates.name.is_some() {
        set_parts.push(format!("name = ${param_idx}"));
        param_idx += 1;
    }
    if updates.description.is_some() {
        set_parts.push(format!("description = ${param_idx}"));
        param_idx += 1;
    }
    if updates.prompt.is_some() {
        set_parts.push(format!("prompt = ${param_idx}"));
        param_idx += 1;
    }
    if updates.icon.is_some() {
        set_parts.push(format!("icon = ${param_idx}"));
        param_idx += 1;
    }
    if updates.color.is_some() {
        set_parts.push(format!("color = ${param_idx}"));
        param_idx += 1;
    }
    if updates.environment.is_some() {
        set_parts.push(format!("environment = ${param_idx}"));
        param_idx += 1;
    }
    if updates.repo_urls.is_some() {
        set_parts.push(format!("repo_urls = ${param_idx}"));
        param_idx += 1;
    }

    let sql = format!(
        "UPDATE projects SET {}, updated_at = NOW() WHERE id = ${param_idx} AND deleted_at IS NULL",
        set_parts.join(", ")
    );

    let mut q = sqlx::query(&sql);
    if let Some(ref v) = updates.name {
        q = q.bind(v);
    }
    if let Some(ref v) = updates.description {
        q = q.bind(v);
    }
    if let Some(ref v) = updates.prompt {
        q = q.bind(v);
    }
    if let Some(ref v) = updates.icon {
        q = q.bind(v);
    }
    if let Some(ref v) = updates.color {
        q = q.bind(v);
    }
    if let Some(ref v) = updates.environment {
        q = q.bind(v);
    }
    if let Some(ref v) = updates.repo_urls {
        q = q.bind(v);
    }
    q = q.bind(id);

    let result = q.execute(pool).await?;
    if result.rows_affected() == 0 {
        return Err(DbError::NotFound(format!("project {id} not found")));
    }

    get_project(pool, id).await
}

/// Soft-delete a project.
pub async fn soft_delete_project(pool: &PgPool, id: Uuid) -> Result<bool> {
    let result =
        sqlx::query("UPDATE projects SET deleted_at = NOW() WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Archive a project.
pub async fn archive_project(pool: &PgPool, id: Uuid) -> Result<()> {
    let row = sqlx::query("SELECT archived_at FROM projects WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    match row {
        None => return Err(DbError::NotFound(format!("project {id} not found"))),
        Some(r) => {
            let archived_at: Option<DateTime<Utc>> = r.try_get("archived_at")?;
            if archived_at.is_some() {
                return Err(DbError::AccessDenied(
                    "project is already archived".to_string(),
                ));
            }
        }
    }

    sqlx::query(
        "UPDATE projects SET archived_at = NOW() WHERE id = $1 AND deleted_at IS NULL AND archived_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Unarchive a project.
pub async fn unarchive_project(pool: &PgPool, id: Uuid) -> Result<()> {
    let result = sqlx::query(
        "UPDATE projects SET archived_at = NULL WHERE id = $1 AND deleted_at IS NULL AND archived_at IS NOT NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DbError::NotFound(format!(
            "project {id} not found or not archived"
        )));
    }

    Ok(())
}

/// List channels belonging to a project.
pub async fn list_project_channels(
    pool: &PgPool,
    project_id: Uuid,
) -> Result<Vec<crate::channel::ChannelRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, channel_type::text AS channel_type, visibility::text AS visibility,
               description, canvas,
               created_by, created_at, updated_at, archived_at, deleted_at,
               nip29_group_id, topic_required, max_members,
               topic, topic_set_by, topic_set_at,
               purpose, purpose_set_by, purpose_set_at,
               ttl_seconds, ttl_deadline, project_id
        FROM channels
        WHERE project_id = $1 AND deleted_at IS NULL
        ORDER BY created_at DESC
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(crate::channel::row_to_channel_record(row)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::Keys;
    use sqlx::PgPool;

    const TEST_DB_URL: &str = "postgres://sprout:sprout_dev@localhost:5432/sprout";

    async fn setup_pool() -> PgPool {
        PgPool::connect(TEST_DB_URL)
            .await
            .expect("connect to test DB")
    }

    fn random_pubkey() -> Vec<u8> {
        Keys::generate().public_key().serialize().to_vec()
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn create_and_get_project() {
        let pool = setup_pool().await;
        let pk = random_pubkey();
        let id = Uuid::new_v4();

        let (record, created) = create_project_with_id(
            &pool,
            id,
            "test-project",
            "local",
            Some("A test project"),
            Some("Be helpful"),
            Some("rocket"),
            Some("#00ff00"),
            &serde_json::json!(["https://github.com/test/repo"]),
            &pk,
        )
        .await
        .expect("create project");

        assert!(created);
        assert_eq!(record.id, id);
        assert_eq!(record.name, "test-project");
        assert_eq!(record.environment, "local");
        assert_eq!(record.description.as_deref(), Some("A test project"));
        assert_eq!(record.prompt.as_deref(), Some("Be helpful"));
        assert_eq!(record.icon.as_deref(), Some("rocket"));
        assert_eq!(record.color.as_deref(), Some("#00ff00"));

        let fetched = get_project(&pool, id).await.expect("get project");
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.name, "test-project");
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn list_projects_by_creator() {
        let pool = setup_pool().await;
        let pk1 = random_pubkey();
        let pk2 = random_pubkey();

        create_project_with_id(
            &pool,
            Uuid::new_v4(),
            "proj-a",
            "local",
            None,
            None,
            None,
            None,
            &serde_json::json!([]),
            &pk1,
        )
        .await
        .expect("create a");

        create_project_with_id(
            &pool,
            Uuid::new_v4(),
            "proj-b",
            "local",
            None,
            None,
            None,
            None,
            &serde_json::json!([]),
            &pk2,
        )
        .await
        .expect("create b");

        let all = list_projects(&pool, None).await.expect("list all");
        assert!(all.len() >= 2);

        let by_pk1 = list_projects(&pool, Some(&pk1)).await.expect("list pk1");
        assert!(by_pk1.iter().all(|p| p.created_by == pk1));

        let by_pk2 = list_projects(&pool, Some(&pk2)).await.expect("list pk2");
        assert!(by_pk2.iter().all(|p| p.created_by == pk2));
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn update_project_partial() {
        let pool = setup_pool().await;
        let pk = random_pubkey();
        let id = Uuid::new_v4();

        create_project_with_id(
            &pool,
            id,
            "original",
            "local",
            Some("desc"),
            None,
            None,
            None,
            &serde_json::json!([]),
            &pk,
        )
        .await
        .expect("create");

        let updated = update_project(
            &pool,
            id,
            ProjectUpdate {
                name: Some("renamed".into()),
                description: None,
                prompt: None,
                icon: None,
                color: None,
                environment: None,
                repo_urls: None,
            },
        )
        .await
        .expect("update");

        assert_eq!(updated.name, "renamed");
        // Description should be unchanged
        assert_eq!(updated.description.as_deref(), Some("desc"));
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn update_project_repos() {
        let pool = setup_pool().await;
        let pk = random_pubkey();
        let id = Uuid::new_v4();

        create_project_with_id(
            &pool,
            id,
            "repo-test",
            "local",
            None,
            None,
            None,
            None,
            &serde_json::json!(["https://github.com/test/a"]),
            &pk,
        )
        .await
        .expect("create");

        // Set repos
        let updated = update_project(
            &pool,
            id,
            ProjectUpdate {
                name: None,
                description: None,
                prompt: None,
                icon: None,
                color: None,
                environment: None,
                repo_urls: Some(serde_json::json!([
                    "https://github.com/test/b",
                    "https://github.com/test/c"
                ])),
            },
        )
        .await
        .expect("update repos");
        let urls: Vec<String> = serde_json::from_value(updated.repo_urls.clone()).unwrap();
        assert_eq!(urls.len(), 2);

        // Clear repos
        let cleared = update_project(
            &pool,
            id,
            ProjectUpdate {
                name: None,
                description: None,
                prompt: None,
                icon: None,
                color: None,
                environment: None,
                repo_urls: Some(serde_json::json!([])),
            },
        )
        .await
        .expect("clear repos");
        let urls: Vec<String> = serde_json::from_value(cleared.repo_urls).unwrap();
        assert!(urls.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn soft_delete_project_test() {
        let pool = setup_pool().await;
        let pk = random_pubkey();
        let id = Uuid::new_v4();

        create_project_with_id(
            &pool,
            id,
            "to-delete",
            "local",
            None,
            None,
            None,
            None,
            &serde_json::json!([]),
            &pk,
        )
        .await
        .expect("create");

        let deleted = soft_delete_project(&pool, id).await.expect("delete");
        assert!(deleted);

        let result = get_project(&pool, id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn archive_and_unarchive_project() {
        let pool = setup_pool().await;
        let pk = random_pubkey();
        let id = Uuid::new_v4();

        create_project_with_id(
            &pool,
            id,
            "to-archive",
            "local",
            None,
            None,
            None,
            None,
            &serde_json::json!([]),
            &pk,
        )
        .await
        .expect("create");

        archive_project(&pool, id).await.expect("archive");
        let archived = get_project(&pool, id).await.expect("get archived");
        assert!(archived.archived_at.is_some());

        unarchive_project(&pool, id).await.expect("unarchive");
        let unarchived = get_project(&pool, id).await.expect("get unarchived");
        assert!(unarchived.archived_at.is_none());
    }
}
