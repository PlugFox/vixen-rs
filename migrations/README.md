# Как выполнять миграции

- **Создать новую миграцию**:
  `cargo sqlx migrate add create_users_table`
- **Применить миграции**:
  `cargo sqlx migrate run`
- **Откатить последнюю миграцию**:
  `cargo sqlx migrate revert`

---

## Миграции в коде

Макрос `sqlx::migrate!()` автоматически встраивает миграции в бинарный файл во время компиляции, что удобно для развертывания.
```rs
/// Initialize the SQLite connection pool
pub async fn init_db_pool(database_url: &str) -> Result<SqlitePool> {
    // Create database if it doesn't exist
    if !sqlx::Sqlite::database_exists(database_url)
        .await
        .unwrap_or(false)
    {
        info!("Creating database at: {}", database_url);
        sqlx::Sqlite::create_database(database_url).await?;
    }

    // Create the connection pool with a maximum of 5 connections
    debug!("Connecting to database at: {}", database_url);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Run migrations
    info!("Running database migrations");
    sqlx::migrate!("./migrations").run(&pool).await?;

    // VACUUM the database to optimize it
    debug!("Running VACUUM on the database to optimize it");
    sqlx::query("VACUUM").execute(&pool).await.map_err(|e| {
        error!("Failed to VACUUM the database: {}", e);
        e
    })?;

    Ok(pool)
}
```

---

## Структура каталога миграций

Каталог миграций должен содержать все SQL-файлы миграций, а также вспомогательные файлы, такие как шаблоны и документация. Это позволяет легко управлять версиями схемы базы данных и обеспечивает удобство при разработке и развертывании приложения.

Пример структуры каталога миграций `migrations/`:
```
migrations/
├── README.md                       # Документация миграций
├── migration.template              # Шаблон для новых миграций
├── schema.sql                      # Полная схема БД (для справки)
├── seed_data.sql                   # Данные для заполнения (не миграция)
├── 20250622120000_create_users.sql # Реальная миграция
├── 20250622120001_create_posts.sql # Реальная миграция
└── 20250622120002_add_indexes.sql  # Реальная миграция
```

Пример миграции:
```sql
-- Create users table
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username VARCHAR(255) NOT NULL UNIQUE,
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
```