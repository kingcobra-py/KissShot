#[macro_export]
macro_rules! log {
    ($level:ident, $logger:expr_2021, $($arg:tt)*) => {
        {
            let message = format!($($arg)*);
            $logger.$level(&message).await;
        }
    };
}
#[macro_export]
macro_rules! debug {
    ($logger:expr_2021, $($arg:tt)*) => {
        log!(debug, $logger, $($arg)*)
    };
}
#[macro_export]
macro_rules! info {
    ($logger:expr_2021, $($arg:tt)*) => {
        log!(info, $logger, $($arg)*)
    };
}
#[macro_export]
macro_rules! warning {
    ($logger:expr_2021, $($arg:tt)*) => {
        log!(warning, $logger, $($arg)*)
    };
}
#[macro_export]
macro_rules! error {
    ($logger:expr_2021, $($arg:tt)*) => {
        log!(error, $logger, $($arg)*)
    };
}
#[macro_export]
macro_rules! critical {
    ($logger:expr_2021, $($arg:tt)*) => {
        log!(critical, $logger, $($arg)*)
    };
}
#[cfg(test)]
mod tests {
    use crate::logging::log::KissShotLogger;
    use std::sync::Arc;
    #[tokio::test]
    async fn test_log_macro() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("test_macro");
        log!(info, handle, "Test message with macro");
        log!(debug, handle, "Debug message: {}", "test");
        log!(warning, handle, "Warning with number: {}", 42);
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_debug_macro() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("debug_macro");
        debug!(handle, "Debug message");
        debug!(handle, "Debug with variable: {}", "test_var");
        debug!(handle, "Debug with multiple args: {} {}", "arg1", "arg2");
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_info_macro() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("info_macro");
        info!(handle, "Info message");
        info!(handle, "Info with number: {}", 123);
        info!(
            handle,
            "Info with multiple variables: {} {} {}", "a", "b", "c"
        );
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_warning_macro() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("warning_macro");
        warning!(handle, "Warning message");
        warning!(handle, "Warning with error code: {}", 500);
        warning!(
            handle,
            "Warning: {} failed after {} attempts",
            "operation",
            3
        );
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_error_macro() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("error_macro");
        error!(handle, "Error message");
        error!(handle, "Error in function: {}", "test_function");
        error!(handle, "Error: {} at line {}", "syntax error", 42);
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_critical_macro() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("critical_macro");
        critical!(handle, "Critical message");
        critical!(handle, "Critical error: {}", "system failure");
        critical!(handle, "Critical: {} service is down", "database");
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_macro_with_complex_formatting() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("complex_macro");
        let user_id = 12345;
        let action = "login";
        let ip = "192.168.1.1";
        info!(
            handle,
            "User {} performed {} from IP {}", user_id, action, ip
        );
        let query = "SELECT * FROM users";
        let duration = 0.05;
        debug!(handle, "Query '{}' executed in {:.3}s", query, duration);
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_macro_with_json_data() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("json_macro");
        let data = serde_json::json!({
            "user_id": 123,
            "action": "test",
            "timestamp": "2023-01-01T00:00:00Z"
        });
        info!(handle, "Processing data: {}", data);
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_macro_with_arc_logger() {
        let logger = KissShotLogger::new().await;
        let handle = Arc::new(logger.get_logger("arc_macro"));
        info!(handle, "Message with Arc logger");
        debug!(handle, "Debug with Arc: {}", "test");
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_macro_concurrent_usage() {
        let logger = KissShotLogger::new().await;
        let handle = Arc::new(logger.get_logger("concurrent_macro"));
        let mut tasks = vec![];
        for i in 0..5 {
            let handle_clone = handle.clone();
            let task = tokio::spawn(async move {
                info!(handle_clone, "Concurrent message {}", i);
                debug!(handle_clone, "Debug message {}", i);
                warning!(handle_clone, "Warning message {}", i);
            });
            tasks.push(task);
        }
        for task in tasks {
            task.await.unwrap();
        }
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_macro_with_different_log_levels() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("levels_macro");
        debug!(handle, "This is a debug message");
        info!(handle, "This is an info message");
        warning!(handle, "This is a warning message");
        error!(handle, "This is an error message");
        critical!(handle, "This is a critical message");
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_macro_with_empty_message() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("empty_macro");
        info!(handle, "");
        debug!(handle, "{}", "");
        warning!(handle, "   ");
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_macro_with_special_characters() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("special_macro");
        info!(
            handle,
            "Message with special chars: {} {} {}", "ñ", "é", "ü"
        );
        error!(handle, "Error with symbols: @#$%^&*()");
        critical!(handle, "Critical with unicode: {} {}", "🚨", "⚠️");
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_macro_performance() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("performance_macro");
        let start = std::time::Instant::now();
        for i in 0..100 {
            info!(handle, "Performance test message {}", i);
        }
        let duration = start.elapsed();
        info!(handle, "Logged 100 messages in {:?}", duration);
        logger.shutdown().await;
    }
}
