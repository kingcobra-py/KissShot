use crate::config::{get_config, Database as DbConfig};
use crate::logging::{get_logger, LoggerHandle};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use lazy_static::lazy_static;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{mysql::MySqlRow, FromRow, MySql, MySqlPool, Row};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Mutex, OnceCell};
#[derive(Debug, Error)]
pub enum SQLError {
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Not found: {0}")]
    NotFound(String),
}
pub type Result<T> = std::result::Result<T, SQLError>;
#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub user_id: i64,
    pub username: String,
    pub balance: i64,
    pub status: String,
    pub antispam: i32,
    pub registered_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}
#[async_trait]
pub trait SQLDatabase: Send + Sync {
    async fn execute(&self, query: &str, params: Option<Vec<&str>>) -> Result<u64>;
    async fn fetch_user(&self, user_id: i64) -> Result<Option<User>>;
    async fn register_user(&self, user: &User) -> Result<bool>;
    async fn update_user(&self, user_id: i64, updates: &UserUpdate) -> Result<bool>;
    async fn delete_user(&self, user_id: i64) -> Result<bool>;
    async fn user_exists(&self, user_id: i64) -> Result<bool>;
    async fn get_all_users(&self, limit: i64, offset: i64) -> Result<Vec<User>>;
    async fn get_user_stats(&self) -> Result<UserStats>;
}
#[derive(Debug, Clone, Default)]
pub struct UserUpdate {
    pub username: Option<String>,
    pub balance: Option<i64>,
    pub status: Option<String>,
    pub antispam: Option<i32>,
    pub registered_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone)]
pub struct UserStats {
    pub total_users: i64,
    pub free_users: i64,
    pub banned_users: i64,
    pub total_balance: i64,
    pub avg_balance: f64,
}
pub struct SQL {
    pool: MySqlPool,
    logger: Arc<LoggerHandle>,
    connection_timeout: Duration,
    query_timeout: Duration,
}
impl SQL {
    pub async fn new() -> Result<Self> {
        let config = get_config().map_err(|e| SQLError::Connection(e.to_string()))?;
        let logger = get_logger("Database_SQL").await;
        let database_url = format!(
            "mysql://{}:{}@{}:{}/{}",
            config.database.sql.user,
            config.database.sql.password,
            config.database.sql.host,
            config.database.sql.port,
            config.database.sql.database
        );
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .idle_timeout(Duration::from_secs(60))
            .max_lifetime(Duration::from_secs(300))
            .connect(&database_url)
            .await
            .map_err(|e| SQLError::Connection(e.to_string()))?;
        // Test connection with a simple query
        sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .map_err(|e| SQLError::Connection(format!("Database connection test failed: {}", e)))?;

        logger
            .info("SQL connection pool initialized successfully")
            .await;
        Ok(Self {
            pool,
            logger,
            connection_timeout: Duration::from_secs(10),
            query_timeout: Duration::from_secs(30),
        })
    }
    fn validate_user_id(&self, user_id: i64) -> Result<()> {
        if user_id <= 0 {
            return Err(SQLError::InvalidParameter(
                "user_id must be positive".to_string(),
            ));
        }
        Ok(())
    }
    fn validate_username(&self, username: &str) -> Result<()> {
        if username.trim().is_empty() {
            return Err(SQLError::InvalidParameter(
                "username must be non-empty".to_string(),
            ));
        }
        if username.len() > 255 {
            return Err(SQLError::InvalidParameter("username too long".to_string()));
        }
        Ok(())
    }
    pub async fn execute(&self, query: &str, params: Option<Vec<&str>>) -> Result<u64> {
        if query.trim().is_empty() {
            return Err(SQLError::InvalidParameter(
                "Query must be non-empty".to_string(),
            ));
        }
        let start = tokio::time::Instant::now();
        let mut query_builder = sqlx::query(query);
        if let Some(params_vec) = params {
            for param in params_vec {
                query_builder = query_builder.bind(param);
            }
        }
        let result = tokio::time::timeout(self.query_timeout, query_builder.execute(&self.pool))
            .await
            .map_err(|_| SQLError::Timeout("Query timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(500) {
            self.logger
                .warning(&format!("Slow SQL query: {:?}", duration))
                .await;
        }
        self.logger
            .info(&format!(
                "SQL executed successfully: {}...",
                &query[..query.len().min(100)]
            ))
            .await;
        Ok(result.rows_affected())
    }
    pub async fn fetch_user(&self, user_id: i64) -> Result<Option<User>> {
        self.validate_user_id(user_id)?;
        let start = tokio::time::Instant::now();
        let user: Option<User> = tokio::time::timeout(
            self.query_timeout,
            sqlx::query_as("SELECT * FROM users WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool),
        )
        .await
        .map_err(|_| SQLError::Timeout("Query timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(500) {
            self.logger
                .warning(&format!("Slow SQL query: {:?}", duration))
                .await;
        }
        self.logger
            .info(&format!("SQL fetched successfully: user_id={}", user_id))
            .await;
        Ok(user)
    }
    pub async fn register_user(&self, user: &User) -> Result<bool> {
        self.validate_user_id(user.user_id)?;
        self.validate_username(&user.username)?;
        let start = tokio::time::Instant::now();
        let result = tokio::time::timeout(
            self.query_timeout,
            sqlx::query(
                r#"
                INSERT INTO users (user_id, username, balance, status, antispam, registered_at, expires_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#
            )
            .bind(user.user_id)
            .bind(&user.username)
            .bind(user.balance)
            .bind(&user.status)
            .bind(user.antispam)
            .bind(user.registered_at)
            .bind(user.expires_at)
            .execute(&self.pool)
        )
        .await
        .map_err(|_| SQLError::Timeout("Query timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(500) {
            self.logger
                .warning(&format!("Slow SQL query: {:?}", duration))
                .await;
        }
        self.logger
            .info(&format!(
                "SQL registered successfully: user_id={}",
                user.user_id
            ))
            .await;
        Ok(result.rows_affected() > 0)
    }
    pub async fn update_user(&self, user_id: i64, updates: &UserUpdate) -> Result<bool> {
        self.validate_user_id(user_id)?;
        if updates
            .username
            .as_ref()
            .is_some_and(|u| u.trim().is_empty())
        {
            return Err(SQLError::InvalidParameter(
                "username must be non-empty".to_string(),
            ));
        }
        let start = tokio::time::Instant::now();
        let mut query = "UPDATE users SET ".to_string();
        let mut params: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();
        if let Some(username) = &updates.username {
            params.push("username = ?".to_string());
            binds.push(username.clone());
        }
        if let Some(balance) = updates.balance {
            params.push("balance = ?".to_string());
            binds.push(balance.to_string());
        }
        if let Some(status) = &updates.status {
            params.push("status = ?".to_string());
            binds.push(status.clone());
        }
        if let Some(antispam) = updates.antispam {
            params.push("antispam = ?".to_string());
            binds.push(antispam.to_string());
        }
        if let Some(registered_at) = updates.registered_at {
            params.push("registered_at = ?".to_string());
            binds.push(registered_at.to_rfc3339());
        }
        if let Some(expires_at) = updates.expires_at {
            params.push("expires_at = ?".to_string());
            binds.push(expires_at.to_rfc3339());
        }
        if params.is_empty() {
            return Err(SQLError::InvalidParameter(
                "No fields to update".to_string(),
            ));
        }
        query.push_str(&params.join(", "));
        query.push_str(" WHERE user_id = ?");
        binds.push(user_id.to_string());
        let mut query_builder = sqlx::query(&query);
        for bind in binds {
            query_builder = query_builder.bind(bind);
        }
        let result = tokio::time::timeout(self.query_timeout, query_builder.execute(&self.pool))
            .await
            .map_err(|_| SQLError::Timeout("Query timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(500) {
            self.logger
                .warning(&format!("Slow SQL query: {:?}", duration))
                .await;
        }
        self.logger
            .info(&format!(
                "SQL updated successfully: user_id={}, affected_rows={}",
                user_id,
                result.rows_affected()
            ))
            .await;
        Ok(result.rows_affected() > 0)
    }
    pub async fn delete_user(&self, user_id: i64) -> Result<bool> {
        self.validate_user_id(user_id)?;
        let start = tokio::time::Instant::now();
        let result = tokio::time::timeout(
            self.query_timeout,
            sqlx::query("DELETE FROM users WHERE user_id = ?")
                .bind(user_id)
                .execute(&self.pool),
        )
        .await
        .map_err(|_| SQLError::Timeout("Query timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(500) {
            self.logger
                .warning(&format!("Slow SQL query: {:?}", duration))
                .await;
        }
        self.logger
            .info(&format!(
                "SQL deleted successfully: user_id={}, affected_rows={}",
                user_id,
                result.rows_affected()
            ))
            .await;
        Ok(result.rows_affected() > 0)
    }
    pub async fn user_exists(&self, user_id: i64) -> Result<bool> {
        self.validate_user_id(user_id)?;
        let start = tokio::time::Instant::now();
        let exists: Option<i32> = tokio::time::timeout(
            self.query_timeout,
            sqlx::query_scalar("SELECT 1 FROM users WHERE user_id = ? LIMIT 1")
                .bind(user_id)
                .fetch_optional(&self.pool),
        )
        .await
        .map_err(|_| SQLError::Timeout("Query timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(500) {
            self.logger
                .warning(&format!("Slow SQL query: {:?}", duration))
                .await;
        }
        self.logger
            .info(&format!(
                "SQL exists check: user_id={}, exists={}",
                user_id,
                exists.is_some()
            ))
            .await;
        Ok(exists.is_some())
    }
    pub async fn get_all_users(&self, limit: i64, offset: i64) -> Result<Vec<User>> {
        if limit <= 0 {
            return Err(SQLError::InvalidParameter(
                "limit must be positive".to_string(),
            ));
        }
        if offset < 0 {
            return Err(SQLError::InvalidParameter(
                "offset must be non-negative".to_string(),
            ));
        }
        let start = tokio::time::Instant::now();
        let users: Vec<User> = tokio::time::timeout(
            self.query_timeout,
            sqlx::query_as("SELECT * FROM users ORDER BY registered_at DESC LIMIT ? OFFSET ?")
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool),
        )
        .await
        .map_err(|_| SQLError::Timeout("Query timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(500) {
            self.logger
                .warning(&format!("Slow SQL query: {:?}", duration))
                .await;
        }
        self.logger
            .info(&format!("SQL fetched {} users", users.len()))
            .await;
        Ok(users)
    }
    pub async fn get_user_stats(&self) -> Result<UserStats> {
        let start = tokio::time::Instant::now();
        let stats: (i64, i64, i64, i64, f64) = tokio::time::timeout(
            self.query_timeout,
            sqlx::query_as(
                r#"
                SELECT 
                    COUNT(*) as total_users,
                    COUNT(CASE WHEN status = 'FREE' THEN 1 END) as free_users,
                    COUNT(CASE WHEN status = 'BANNED' THEN 1 END) as banned_users,
                    CAST(SUM(balance) AS SIGNED) as total_balance,
                    CAST(AVG(balance) AS DOUBLE) as avg_balance
                FROM users
                "#,
            )
            .fetch_one(&self.pool),
        )
        .await
        .map_err(|_| SQLError::Timeout("Query timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(500) {
            self.logger
                .warning(&format!("Slow SQL query: {:?}", duration))
                .await;
        }
        let user_stats = UserStats {
            total_users: stats.0,
            free_users: stats.1,
            banned_users: stats.2,
            total_balance: stats.3,
            avg_balance: stats.4,
        };
        self.logger
            .info("SQL user stats retrieved successfully")
            .await;
        Ok(user_stats)
    }
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| SQLError::Connection(format!("Health check failed: {}", e)))?;
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        self.logger
            .info("SQL connection pool closed successfully")
            .await;
        Ok(())
    }
}
static SQL_INSTANCE: std::sync::Mutex<Option<Arc<SQL>>> = std::sync::Mutex::new(None);

pub async fn get_sql() -> Result<Arc<SQL>> {
    // Check if we already have a connection
    {
        let instance_guard = SQL_INSTANCE.lock().unwrap();
        if let Some(sql) = instance_guard.as_ref() {
            return Ok(sql.clone());
        }
    }

    // Create new connection
    let sql = SQL::new()
        .await
        .map(Arc::new)
        .map_err(|e| SQLError::Connection(format!("Failed to create SQL connection: {}", e)))?;

    {
        let mut instance_guard = SQL_INSTANCE.lock().unwrap();
        *instance_guard = Some(sql.clone());
    }
    Ok(sql)
}
pub async fn execute(query: &str, params: Option<Vec<&str>>) -> Result<u64> {
    let sql = get_sql().await?;
    sql.execute(query, params).await
}
pub async fn fetch_user(user_id: i64) -> Result<Option<User>> {
    let sql = get_sql().await?;
    sql.fetch_user(user_id).await
}
pub async fn register_user(user: &User) -> Result<bool> {
    let sql = get_sql().await?;
    sql.register_user(user).await
}
pub async fn update_user(user_id: i64, updates: &UserUpdate) -> Result<bool> {
    let sql = get_sql().await?;
    sql.update_user(user_id, updates).await
}
pub async fn delete_user(user_id: i64) -> Result<bool> {
    let sql = get_sql().await?;
    sql.delete_user(user_id).await
}
pub async fn user_exists(user_id: i64) -> Result<bool> {
    let sql = get_sql().await?;
    sql.user_exists(user_id).await
}

pub async fn health_check() -> Result<()> {
    let sql = get_sql().await?;
    sql.health_check().await
}

pub async fn reset_connection() -> Result<()> {
    println!("🔄 Resetting database connection pool...");

    {
        let mut instance_guard = SQL_INSTANCE.lock().unwrap();
        // Clear the existing connection to force reconnection
        *instance_guard = None;
    }

    // Force a new connection
    let _sql = get_sql().await?;
    println!("✅ Database connection pool reset successfully");

    Ok(())
}
pub async fn get_all_users(limit: i64, offset: i64) -> Result<Vec<User>> {
    let sql = get_sql().await?;
    sql.get_all_users(limit, offset).await
}
pub async fn get_user_stats() -> Result<UserStats> {
    let sql = get_sql().await?;
    sql.get_user_stats().await
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    #[tokio::test]
    async fn test_ksql_initialization() {
        let result = SQL::new().await;
        assert!(result.is_ok(), "SQL should initialize successfully");
    }
    #[tokio::test]
    async fn test_user_operations() {
        let ksql = SQL::new().await.expect("Failed to initialize SQL");
        let test_user = User {
            user_id: 999999,
            username: "test_user".to_string(),
            balance: 100,
            status: "FREE".to_string(),
            antispam: 0,
            registered_at: Utc::now(),
            expires_at: None,
        };
        let _ = ksql.delete_user(test_user.user_id).await;
        let registered = ksql.register_user(&test_user).await;
        assert!(registered.is_ok(), "User registration should succeed");
        let fetched = ksql.fetch_user(test_user.user_id).await;
        assert!(fetched.is_ok(), "User fetch should succeed");
        assert!(fetched.unwrap().is_some(), "User should exist");
        let updates = UserUpdate {
            balance: Some(200),
            ..Default::default()
        };
        let updated = ksql.update_user(test_user.user_id, &updates).await;
        assert!(updated.is_ok(), "User update should succeed");
        let exists = ksql.user_exists(test_user.user_id).await;
        assert!(exists.is_ok(), "User exists check should succeed");
        assert!(exists.unwrap(), "User should exist");
        let deleted = ksql.delete_user(test_user.user_id).await;
        assert!(deleted.is_ok(), "User deletion should succeed");
        assert!(deleted.unwrap(), "User should be deleted");
    }
}
