use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn connect_pg_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
}

pub async fn assert_scan_schema_ready(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT id, media_type, base_path, expires_at, summary_json, result_json \
         FROM scan_sessions \
         LIMIT 1",
    )
    .execute(pool)
    .await
    .map(|_| ())
}
