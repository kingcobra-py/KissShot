use crate::database::sql::{self, fetch_user, User};
use crate::logging::get_logger;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::Message;
use teloxide::Bot;

/// Result type for database operations with user-friendly error messages
pub type DatabaseResult<T> = Result<T, DatabaseError>;

#[derive(Debug)]
pub enum DatabaseError {
    ConnectionFailed(String),
    UserNotFound,
    OperationFailed(String),
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::ConnectionFailed(msg) => {
                write!(f, "Database connection failed: {}", msg)
            }
            DatabaseError::UserNotFound => write!(f, "User not found"),
            DatabaseError::OperationFailed(msg) => write!(f, "Database operation failed: {}", msg),
        }
    }
}

/// Safely fetch a user with proper error handling
pub async fn safe_fetch_user(user_id: i64) -> DatabaseResult<Option<User>> {
    match sql::fetch_user(user_id).await {
        Ok(user) => Ok(user),
        Err(e) => {
            if e.to_string().contains("Too many connections") {
                println!("🔄 Database connection issue detected, attempting to reset...");
                if let Err(reset_err) = sql::reset_connection().await {
                    eprintln!("❌ Database connection reset failed: {}", reset_err);
                    return Err(DatabaseError::ConnectionFailed(reset_err.to_string()));
                }
                println!("✅ Database connection reset successful");

                // Try again after reset
                match sql::fetch_user(user_id).await {
                    Ok(user) => Ok(user),
                    Err(e) => Err(DatabaseError::OperationFailed(e.to_string())),
                }
            } else {
                Err(DatabaseError::OperationFailed(e.to_string()))
            }
        }
    }
}

/// Check if user is registered with proper error handling
pub async fn safe_check_registration(user_id: i64) -> bool {
    match sql::fetch_user(user_id).await {
        Ok(Some(_)) => true,
        _ => false,
    }
}

/// Get user status with proper error handling
pub async fn safe_get_user_status(user_id: i64) -> String {
    match sql::fetch_user(user_id).await {
        Ok(Some(user)) => user.status,
        _ => "FREE".to_string(),
    }
}

/// Check if user has admin privileges
pub async fn safe_check_admin(user_id: i64) -> bool {
    match sql::fetch_user(user_id).await {
        Ok(Some(user)) => user.status == "ADMIN",
        _ => false,
    }
}

/// Check if user has premium privileges
pub async fn safe_check_premium(user_id: i64) -> bool {
    match sql::fetch_user(user_id).await {
        Ok(Some(user)) => user.status == "PREMIUM" || user.status == "ADMIN",
        _ => false,
    }
}

/// Get user limits based on their status
pub async fn safe_get_user_limits(user_id: i64) -> (i32, i32) {
    match sql::fetch_user(user_id).await {
        Ok(Some(user)) => match user.status.as_str() {
            "ADMIN" | "PREMIUM" => (5001, 10000), // (scrape_limit, other_limit)
            _ => (1001, 1000),                    // Free user limits
        },
        _ => (1001, 1000), // Free user limits as default
    }
}

/// Send a database error message to the user
pub async fn send_database_error_message(
    bot: &Bot,
    message: &Message,
    error: &DatabaseError,
    operation: &str,
) {
    let error_message = match error {
        DatabaseError::ConnectionFailed(_) => {
            format!(
                "<b>{} Failed ❌</b>\n\n<b>Reason:</b> Database connection failed. Please try again later.",
                operation
            )
        }
        DatabaseError::UserNotFound => {
            format!(
                "<b>{} Failed ❌</b>\n\n<b>Reason:</b> User not found in database.",
                operation
            )
        }
        DatabaseError::OperationFailed(msg) => {
            format!("<b>{} Failed ❌</b>\n\n<b>Reason:</b> {}", operation, msg)
        }
    };

    let _ = bot
        .send_message(message.chat.id, error_message)
        .parse_mode(teloxide::types::ParseMode::Html)
        .await;
}

/// Macro to handle database errors consistently across plugins
#[macro_export]
macro_rules! handle_database_error {
    ($bot:expr_2021, $message:expr_2021, $result:expr_2021, $operation:expr_2021) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                let db_error = crate::plugins::helpers::database::DatabaseError::OperationFailed(
                    e.to_string(),
                );
                let _ = crate::plugins::helpers::database::send_database_error_message(
                    $bot, $message, &db_error, $operation,
                )
                .await;
                return;
            }
        }
    };
}

/// Macro to handle database errors and return a default value
#[macro_export]
macro_rules! handle_database_error_with_default {
    ($bot:expr_2021, $message:expr_2021, $result:expr_2021, $operation:expr_2021, $default:expr_2021) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                let _ = crate::plugins::helpers::database::send_database_error_message(
                    $bot, $message, &e, $operation,
                )
                .await;
                $default
            }
        }
    };
}

/// Macro for simple database operations that don't need user feedback
#[macro_export]
macro_rules! safe_database_operation {
    ($result:expr_2021, $default:expr_2021) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                // Check if it's a connection error and try to reconnect
                if e.to_string().contains("Too many connections")
                    || e.to_string().contains("Connection error")
                {
                    println!("⚠️ Database connection issue detected, attempting to reset...");
                    if let Err(reset_err) = crate::database::sql::reset_connection().await {
                        eprintln!("❌ Database connection reset failed: {}", reset_err);
                    } else {
                        println!("✅ Database connection reset successful");
                    }
                }
                eprintln!("❌ Database operation failed: {}", e);
                $default
            }
        }
    };
}

/// Macro to handle database errors with custom error message
#[macro_export]
macro_rules! handle_database_error_custom {
    ($bot:expr_2021, $message:expr_2021, $result:expr_2021, $operation:expr_2021, $custom_msg:expr_2021) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                eprintln!("❌ {} failed: {}", $operation, e);
                if let Err(send_err) = $bot
                    .send_message($message.chat.id, $custom_msg)
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await
                {
                    eprintln!("Failed to send custom error message: {}", send_err);
                }
                return;
            }
        }
    };
}
