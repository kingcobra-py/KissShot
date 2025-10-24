use crate::logging::get_logger;
use futures::FutureExt;
use std::panic;
use teloxide::payloads::SendMessageSetters;
use teloxide::prelude::Requester;
use teloxide::types::ParseMode;
use teloxide::{Bot, RequestError};
use tokio::task::JoinHandle;

/// Global error handler for the entire application
pub struct GlobalErrorHandler {
    bot: Bot,
    logger: std::sync::Arc<crate::logging::LoggerHandle>,
}

impl GlobalErrorHandler {
    pub async fn new(bot: Bot) -> Self {
        let logger = get_logger("error_handler").await;
        Self { bot, logger }
    }

    /// Initialize global panic handler
    pub fn init_panic_handler(&self) {
        let bot_clone = self.bot.clone();
        let logger_clone = self.logger.clone();

        panic::set_hook(Box::new(move |panic_info| {
            let error_msg = format!(
                "🚨 **CRITICAL ERROR DETECTED** 🚨\n\n\
                **Location:** {}\n\
                **Message:** {}\n\
                **Timestamp:** {}\n\n\
                The bot encountered an error. Please try again.",
                panic_info
                    .location()
                    .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                    .unwrap_or_else(|| "Unknown".to_string()),
                panic_info
                    .payload()
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic_info
                        .payload()
                        .downcast_ref::<String>()
                        .map(|s| s.as_str()))
                    .unwrap_or("Unknown error"),
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
            );

            // Log the panic
            let logger = logger_clone.clone();
            let error_msg_clone = error_msg.clone();
            tokio::spawn(async move {
                logger.critical(&error_msg_clone).await;
            });

            eprintln!("{}", error_msg);
        }));
    }

    /// Handle errors from spawned tasks
    pub async fn handle_task_error(&self, error: &str, task_name: &str) {
        let error_msg = format!(
            "⚠️ **Task Error** ⚠️\n\n\
            **Task:** {}\n\
            **Error:** {}\n\
            **Timestamp:** {}",
            task_name,
            error,
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );

        self.logger.error(&error_msg).await;
    }

    /// Send error message to user
    pub async fn send_error_to_user(&self, chat_id: teloxide::types::ChatId, error_msg: &str) {
        if let Err(e) = Self::send_user_error(&self.bot, chat_id, error_msg).await {
            self.logger
                .error(&format!("Failed to send error to user: {}", e))
                .await;
        }
    }

    /// Handle database errors globally
    #[allow(dead_code)]
    pub async fn handle_database_error(&self, error: &str, operation: &str) {
        let error_msg = format!(
            "🗄️ **Database Error** 🗄️\n\n\
            **Operation:** {}\n\
            **Error:** {}\n\
            **Timestamp:** {}",
            operation,
            error,
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );

        self.logger.error(&error_msg).await;
    }

    /// Handle plugin errors globally
    #[allow(dead_code)]
    pub async fn handle_plugin_error(&self, plugin_name: &str, error: &str, user_id: Option<u64>) {
        let user_info = user_id
            .map(|id| format!("User ID: {}", id))
            .unwrap_or_else(|| "Unknown user".to_string());

        let error_msg = format!(
            "🔌 **Plugin Error** 🔌\n\n\
            **Plugin:** {}\n\
            **User:** {}\n\
            **Error:** {}\n\
            **Timestamp:** {}",
            plugin_name,
            user_info,
            error,
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );

        self.logger.error(&error_msg).await;
    }

    /// Handle Telegram API errors
    pub async fn handle_telegram_error(&self, error: &RequestError, operation: &str) {
        let error_msg = match error {
            RequestError::Network(e) => format!("Network error: {}", e),
            RequestError::Api(e) => format!("Telegram API error: {}", e),
            RequestError::RetryAfter(seconds) => {
                format!("Rate limited, retry after {} seconds", seconds)
            }
            _ => format!("Unknown Telegram error: {}", error),
        };

        let full_error = format!(
            "📱 **Telegram API Error** 📱\n\n\
            **Operation:** {}\n\
            **Error:** {}\n\
            **Timestamp:** {}",
            operation,
            error_msg,
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );

        self.logger.error(&full_error).await;
    }

    /// Send error message to user's chat
    async fn send_user_error(
        bot: &Bot,
        chat_id: teloxide::types::ChatId,
        error_msg: &str,
    ) -> Result<(), RequestError> {
        bot.send_message(chat_id, error_msg)
            .parse_mode(ParseMode::Html)
            .await?;

        Ok(())
    }

    /// Wrap a task with error handling
    pub fn spawn_with_error_handling<F, T>(&self, task_name: &str, future: F) -> JoinHandle<()>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let error_handler = self.clone();
        let task_name = task_name.to_string();

        tokio::spawn(async move {
            if let Err(e) = std::panic::AssertUnwindSafe(future).catch_unwind().await {
                let error_msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic in task".to_string()
                };

                error_handler
                    .handle_task_error(&error_msg, &task_name)
                    .await;
            }
        })
    }

    /// Wrap a task with error handling and send errors to user
    pub fn spawn_with_error_handling_with_chat<F, T>(
        &self,
        task_name: &str,
        chat_id: teloxide::types::ChatId,
        future: F,
    ) -> JoinHandle<()>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let error_handler = self.clone();
        let task_name = task_name.to_string();

        tokio::spawn(async move {
            if let Err(e) = std::panic::AssertUnwindSafe(future).catch_unwind().await {
                let error_msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic in task".to_string()
                };

                error_handler
                    .handle_task_error(&error_msg, &task_name)
                    .await;

                // Send user-friendly error to the user
                let user_error = format!(
                    "❌ **Error Occurred** ❌\n\n\
                    **Task:** {}\n\
                    **Message:** {}\n\n\
                    Please try again or contact support if the issue persists.",
                    task_name,
                    if error_msg.contains("can't parse entities") {
                        "Invalid message format detected. This has been fixed."
                    } else {
                        "An unexpected error occurred."
                    }
                );

                error_handler.send_error_to_user(chat_id, &user_error).await;
            }
        })
    }
}

impl Clone for GlobalErrorHandler {
    fn clone(&self) -> Self {
        Self {
            bot: self.bot.clone(),
            logger: self.logger.clone(),
        }
    }
}

/// Macro to handle errors in async functions
#[macro_export]
macro_rules! handle_error {
    ($error_handler:expr, $result:expr, $operation:expr) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                $error_handler.handle_telegram_error(&e, $operation).await;
                return;
            }
        }
    };
}

/// Macro to handle database errors in async functions
#[macro_export]
macro_rules! handle_db_error {
    ($error_handler:expr, $result:expr, $operation:expr) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                $error_handler
                    .handle_database_error(&e.to_string(), $operation)
                    .await;
                return;
            }
        }
    };
}

/// Macro to handle plugin errors
#[macro_export]
macro_rules! handle_plugin_error {
    ($error_handler:expr, $result:expr, $plugin_name:expr, $user_id:expr) => {
        match $result {
            Ok(value) => value,
            Err(e) => {
                $error_handler
                    .handle_plugin_error($plugin_name, &e.to_string(), $user_id)
                    .await;
                return;
            }
        }
    };
}
