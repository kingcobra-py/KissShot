use crate::database::*;
use anyhow::Result;
use chrono::{DateTime, Utc};
pub async fn register_user(
    user_id: i64,
    username: String,
    balance: i64,
    status: String,
    antispam: i32,
    registered_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
) -> Result<bool> {
    let sql = sql::get_sql().await?;
    let user_opt = sql::fetch_user(user_id).await?;
    if user_opt.is_some() {
        return Ok(false);
    }
    let user = User {
        user_id,
        username,
        balance,
        status,
        antispam,
        registered_at,
        expires_at,
    };
    sql::register_user(&user).await?;
    Ok(true)
}
