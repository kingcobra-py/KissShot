use crate::config::*;
use crate::database::*;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::result::Result;
use std::time::SystemTime;
use tokio::sync::Mutex;
static ANTISPAM_STATE: Lazy<Mutex<AntispamState>> =
    Lazy::new(|| Mutex::new(AntispamState::default()));
#[derive(Default)]
struct AntispamState {
    last_request: HashMap<i64, SystemTime>,
    antispam: HashMap<i64, u64>,
}
pub async fn get_user_cooldown(user_id: i64) -> u64 {
    if let Ok(sql) = sql::get_sql().await {
        if let Ok(Some(user)) = sql.fetch_user(user_id).await {
            return user.antispam as u64;
        }
    }
    match get_config() {
        Ok(cfg) => cfg.config.default_user_value.antispam as u64,
        Err(_) => 10,
    }
}
pub async fn get_antispam(user_id: i64) -> i64 {
    let mut state = ANTISPAM_STATE.lock().await;
    let now = SystemTime::now();
    let default_cooldown = get_user_cooldown(user_id).await;
    let cooldown = *state.antispam.get(&user_id).unwrap_or(&default_cooldown);
    match state.last_request.get(&user_id) {
        Some(&last_ts) => {
            let elapsed = now.duration_since(last_ts).unwrap_or_default().as_secs();
            if elapsed >= cooldown {
                state.last_request.insert(user_id, now);
                0
            } else {
                (cooldown - elapsed) as i64
            }
        }
        None => {
            state.last_request.insert(user_id, now);
            0
        }
    }
}
pub async fn set_antispam(user_id: i64, cooldown_secs: u64) {
    let mut state = ANTISPAM_STATE.lock().await;
    state.antispam.insert(user_id, cooldown_secs);
}
pub async fn clear_antispam(user_id: i64) {
    let mut state = ANTISPAM_STATE.lock().await;
    state.last_request.remove(&user_id);
    state.antispam.remove(&user_id);
}
pub async fn reset_user_antispam(user_id: i64) {
    let mut state = ANTISPAM_STATE.lock().await;
    state.last_request.remove(&user_id);
}
pub async fn get_antispam_status(user_id: i64) -> (i64, u64) {
    let state = ANTISPAM_STATE.lock().await;
    let now = SystemTime::now();

    let default_cooldown = get_user_cooldown(user_id).await;
    let cooldown = *state.antispam.get(&user_id).unwrap_or(&default_cooldown);

    let remaining = match state.last_request.get(&user_id) {
        Some(&last_ts) => {
            let elapsed = now.duration_since(last_ts).unwrap_or_default().as_secs();
            if elapsed >= cooldown {
                0
            } else {
                (cooldown - elapsed) as i64
            }
        }
        None => 0,
    };

    (remaining, cooldown)
}
pub async fn set_user_antispam(user_id: i64, cooldown_secs: u64) -> Result<(), String> {
    if cooldown_secs > 3600 {
        return Err("Cooldown cannot exceed 3600 seconds (1 hour)".to_string());
    }

    set_antispam(user_id, cooldown_secs).await;
    Ok(())
}
pub async fn clear_user_antispam(user_id: i64) {
    clear_antispam(user_id).await;
}
pub async fn get_user_antispam_info(user_id: i64) -> (i64, u64) {
    get_antispam_status(user_id).await
}
