use chrono::{DateTime, Utc};
use colored::*;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;
#[derive(Debug, Error)]
pub enum LoggerError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Logger already initialized")]
    AlreadyInitialized,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}
impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warning => write!(f, "WARNING"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct LogRecord {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub logger_name: String,
    pub module: String,
    pub function: String,
    pub line: u32,
    pub message: String,
    pub thread_id: u64,
    pub process_id: u32,
    pub extra: HashMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggerConfig {
    pub level: LogLevel,
    pub console_output: bool,
    pub file_output: bool,
    pub json_output: bool,
    pub max_file_size: u64,
    pub backup_count: u32,
    pub colored_console: bool,
    pub include_traceback: bool,
    pub log_sql: bool,
    pub log_telegram: bool,
    pub log_errors: bool,
    pub log_performance: bool,
    pub buffer_size: usize,
}
impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            console_output: true,
            file_output: true,
            json_output: false,
            max_file_size: 10 * 1024 * 1024,
            backup_count: 5,
            colored_console: true,
            include_traceback: true,
            log_sql: false,
            log_telegram: true,
            log_errors: true,
            log_performance: false,
            buffer_size: 1000,
        }
    }
}
#[derive(Debug, Clone)]
pub enum LogMessage {
    Record(LogRecord),
    Flush,
    Shutdown,
}
static LOGGER: OnceLock<Arc<KissShotLogger>> = OnceLock::new();
pub struct KissShotLogger {
    config: RwLock<LoggerConfig>,
    log_dir: PathBuf,
    loggers: RwLock<HashMap<String, Arc<LoggerHandle>>>,
    tx: mpsc::Sender<LogMessage>,
    handle: RwLock<Option<JoinHandle<()>>>,
    is_running: AtomicBool,
    message_count: AtomicU64,
}
#[derive(Clone)]
pub struct LoggerHandle {
    name: String,
    module: String,
    tx: mpsc::Sender<LogMessage>,
}
impl KissShotLogger {
    pub async fn new() -> Self {
        let log_dir = PathBuf::from("workdir/logs");
        fs::create_dir_all(&log_dir).expect("Failed to create log directory");
        let config = LoggerConfig::default();
        let (tx, rx) = mpsc::channel(config.buffer_size);
        let logger = Self {
            config: RwLock::new(config),
            log_dir,
            loggers: RwLock::new(HashMap::new()),
            tx,
            handle: RwLock::new(None),
            is_running: AtomicBool::new(false),
            message_count: AtomicU64::new(0),
        };
        logger.start_processor(rx).await;
        logger
    }
    async fn start_processor(&self, mut rx: mpsc::Receiver<LogMessage>) {
        let log_dir = self.log_dir.clone();
        let config = self.config.read().clone();
        let handle = tokio::spawn(async move {
            let mut file_handles = HashMap::new();
            let mut json_handles = HashMap::new();
            let mut error_handles = HashMap::new();
            while let Some(message) = rx.recv().await {
                match message {
                    LogMessage::Record(record) => {
                        Self::process_record(
                            &record,
                            &config,
                            &log_dir,
                            &mut file_handles,
                            &mut json_handles,
                            &mut error_handles,
                        )
                        .await;
                    }
                    LogMessage::Flush => {
                        Self::flush_handles(
                            &mut file_handles,
                            &mut json_handles,
                            &mut error_handles,
                        )
                        .await;
                    }
                    LogMessage::Shutdown => break,
                }
            }
            Self::flush_handles(&mut file_handles, &mut json_handles, &mut error_handles).await;
        });
        *self.handle.write() = Some(handle);
        self.is_running.store(true, Ordering::SeqCst);
    }
    async fn process_record(
        record: &LogRecord,
        config: &LoggerConfig,
        log_dir: &Path,
        file_handles: &mut HashMap<String, File>,
        json_handles: &mut HashMap<String, File>,
        error_handles: &mut HashMap<String, File>,
    ) {
        if config.console_output {
            let formatted = Self::format_console(record, config.colored_console);
            println!("{}", formatted);
        }
        if config.file_output {
            Self::write_to_file(record, log_dir, "kissshot.log", file_handles, |r| {
                format!(
                    "{} | {} | {} | {}:{}:{} | {}\n",
                    r.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    r.level,
                    r.logger_name,
                    r.module,
                    r.function,
                    r.line,
                    r.message
                )
            })
            .await;
        }
        if config.json_output {
            Self::write_to_file(record, log_dir, "kissshot.json", json_handles, |r| {
                serde_json::to_string(&r).unwrap() + "\n"
            })
            .await;
        }
        if config.log_errors && matches!(record.level, LogLevel::Error | LogLevel::Critical) {
            Self::write_to_file(record, log_dir, "errors.log", error_handles, |r| {
                format!(
                    "{} | {} | {} | {}:{}:{} | {}\n",
                    r.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    r.level,
                    r.logger_name,
                    r.module,
                    r.function,
                    r.line,
                    r.message
                )
            })
            .await;
        }
    }
    async fn write_to_file<F>(
        record: &LogRecord,
        log_dir: &Path,
        filename: &str,
        handles: &mut HashMap<String, File>,
        formatter: F,
    ) where
        F: Fn(&LogRecord) -> String,
    {
        let path = log_dir.join(filename);
        let handle = handles.entry(filename.to_string()).or_insert_with(|| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("Failed to open log file")
        });
        if let Err(e) = writeln!(handle, "{}", formatter(record)) {
            eprintln!("Failed to write to log file: {}", e);
        }
    }
    async fn flush_handles(
        file_handles: &mut HashMap<String, File>,
        json_handles: &mut HashMap<String, File>,
        error_handles: &mut HashMap<String, File>,
    ) {
        for handle in file_handles.values_mut() {
            let _ = handle.flush();
        }
        for handle in json_handles.values_mut() {
            let _ = handle.flush();
        }
        for handle in error_handles.values_mut() {
            let _ = handle.flush();
        }
    }
    fn format_console(record: &LogRecord, colored: bool) -> String {
        if !colored {
            return format!(
                "{} | {:8} | {:20} | {}",
                record.timestamp.format("%Y-%m-%d %H:%M:%S"),
                record.level.to_string(),
                record.logger_name,
                record.message
            );
        }
        let timestamp = record
            .timestamp
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
            .dimmed();
        let level = match record.level {
            LogLevel::Debug => "🔍 DEBUG".cyan().bold(),
            LogLevel::Info => "ℹ️ INFO".green().bold(),
            LogLevel::Warning => "⚠️ WARNING".yellow().bold(),
            LogLevel::Error => "❌ ERROR".red().bold(),
            LogLevel::Critical => "🚨 CRITICAL".magenta().blink(),
        };
        let logger_name = Self::format_logger_name(&record.logger_name);
        let message = Self::format_message(&record.message, record.level);
        format!("{} | {} | {} | {}", timestamp, level, logger_name, message)
    }
    fn format_logger_name(name: &str) -> ColoredString {
        match name {
            "main" => "🎯 main".green().bold(),
            "telegram" => "📱 telegram".blue(),
            "database" => "🗄️ database".cyan(),
            "security" => "🔒 security".yellow(),
            "bot_commands" => "🤖 bot_commands".magenta(),
            "user_actions" => "👤 user_actions".green(),
            "errors" => "💥 errors".red(),
            _ => format!("📝 {}", name).dimmed(),
        }
    }
    fn format_message(message: &str, level: LogLevel) -> ColoredString {
        if message.starts_with("Starting") {
            format!("🚀 {}", message).green().bold()
        } else if message.starts_with("Bot started") {
            format!("✅ {}", message).green().bold()
        } else if message.starts_with("Bot stopped") {
            format!("🛑 {}", message).yellow().bold()
        } else if message.starts_with("Bot failed") {
            format!("💥 {}", message).red().bold()
        } else if message.contains("Connected!") {
            format!("🔗 {}", message).green()
        } else if message.contains("Disconnected") {
            format!("🔌 {}", message).yellow()
        } else {
            match level {
                LogLevel::Debug => message.cyan(),
                LogLevel::Info => message.green(),
                LogLevel::Warning => message.yellow(),
                LogLevel::Error => message.red(),
                LogLevel::Critical => message.magenta().blink(),
            }
        }
    }
    pub async fn configure(&self, config: LoggerConfig) {
        *self.config.write() = config;
    }
    pub fn get_logger(&self, name: &str) -> Arc<LoggerHandle> {
        let mut loggers = self.loggers.write();
        loggers
            .entry(name.to_string())
            .or_insert_with(|| {
                Arc::new(LoggerHandle {
                    name: name.to_string(),
                    module: name.to_string(),
                    tx: self.tx.clone(),
                })
            })
            .clone()
    }
    pub async fn shutdown(&self) {
        self.is_running.store(false, Ordering::SeqCst);
        let _ = self.tx.send(LogMessage::Shutdown).await;
        if let Some(handle) = self.handle.write().take() {
            let _ = handle.await;
        }
    }
    pub async fn flush(&self) {
        let _ = self.tx.send(LogMessage::Flush).await;
    }
    pub async fn show_startup_banner(&self) {
        let banner = r#"
╔══════════════════════════════════════════════════════════════════════════════╗
║                                                                              ║
║  ██╗  ██╗██╗███████╗███████╗███████╗██╗  ██╗ ██████╗ ████████╗               ║
║  ██║ ██╔╝██║██╔════╝██╔════╝██╔════╝██║  ██║██╔═══██╗╚══██╔══╝               ║
║  █████╔╝ ██║███████╗███████╗███████╗███████║██║   ██║   ██║                  ║
║  ██╔═██╗ ██║╚════██║╚════██║╚════██║██╔══██║██║   ██║   ██║                  ║
║  ██║  ██╗██║███████║███████║███████║██║  ██║╚██████╔╝   ██║                  ║
║  ╚═╝  ╚═╝╚═╝╚══════╝╚══════╝╚══════╝╚═╝  ╚═╝ ╚═════╝    ╚═╝                  ║
║                                                                              ║
║  🤖 Starting KissShot...                                                     ║
║  📊 Enhanced Logging System                                                  ║
║                                                                              ║
╚══════════════════════════════════════════════════════════════════════════════╝
        "#;
        println!("{}", banner.green().bold());
    }
    pub async fn handle_flood_wait(&self, wait_time: u64) {
        let logger = self.get_logger("main");
        logger
            .warning(&format!("🚫 Telegram Flood Wait: {} seconds", wait_time))
            .await;
        if wait_time <= 600 {
            logger.info("🔄 Starting automatic countdown...").await;
            for remaining in (1..=wait_time).rev() {
                let (minutes, seconds) = (remaining / 60, remaining % 60);
                let progress = ((wait_time - remaining) * 30) / wait_time;
                let bar = "█".repeat(progress as usize) + &"░".repeat((30 - progress) as usize);
                print!(
                    "\r⏳ Flood Wait: [{}] {:02}:{:02} remaining",
                    bar, minutes, seconds
                );
                io::stdout().flush().unwrap();
                sleep(Duration::from_secs(1)).await;
            }
            println!("\n✅ Flood wait completed! Ready to retry...");
            logger.info("🔄 Flood wait period completed").await;
        } else {
            logger
                .warning("⏰ Wait time too long for automatic countdown")
                .await;
        }
    }
}
impl LoggerHandle {
    pub async fn log(
        &self,
        level: LogLevel,
        message: &str,
        extra: HashMap<String, serde_json::Value>,
    ) {
        let record = LogRecord {
            timestamp: Utc::now(),
            level,
            logger_name: self.name.clone(),
            module: self.module.clone(),
            function: "unknown".to_string(),
            line: 0,
            message: message.to_string(),
            thread_id: 0,
            process_id: std::process::id(),
            extra,
        };
        let _ = self.tx.send(LogMessage::Record(record)).await;
    }
    pub async fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message, HashMap::new()).await;
    }
    pub async fn info(&self, message: &str) {
        self.log(LogLevel::Info, message, HashMap::new()).await;
    }
    pub async fn warning(&self, message: &str) {
        self.log(LogLevel::Warning, message, HashMap::new()).await;
    }
    pub async fn error(&self, message: &str) {
        self.log(LogLevel::Error, message, HashMap::new()).await;
    }
    pub async fn critical(&self, message: &str) {
        self.log(LogLevel::Critical, message, HashMap::new()).await;
    }
    pub async fn log_telegram_event(
        &self,
        event_type: &str,
        extra: HashMap<String, serde_json::Value>,
    ) {
        let mut full_extra = extra;
        full_extra.insert("event_type".to_string(), json!(event_type));
        self.info(&format!("Telegram Event: {}", event_type)).await;
    }
    pub async fn log_sql_query(
        &self,
        query: &str,
        params: Option<Vec<serde_json::Value>>,
        duration: Option<f64>,
    ) {
        let mut extra = HashMap::new();
        extra.insert("query".to_string(), json!(query));
        if let Some(p) = params {
            extra.insert("params".to_string(), json!(p));
        }
        if let Some(d) = duration {
            extra.insert("duration".to_string(), json!(format!("{:.4}s", d)));
        }
        self.debug("SQL Query").await;
    }
    pub async fn log_user_action(
        &self,
        user_id: i64,
        action: &str,
        extra: HashMap<String, serde_json::Value>,
    ) {
        let mut full_extra = extra;
        full_extra.insert("user_id".to_string(), json!(user_id));
        full_extra.insert("action".to_string(), json!(action));
        self.info(&format!("User {} performed action: {}", user_id, action))
            .await;
    }
}
pub async fn get_logger(name: &str) -> Arc<LoggerHandle> {
    let logger = LOGGER.get_or_init(|| {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { Arc::new(KissShotLogger::new().await) })
        })
    });
    logger.get_logger(name)
}
pub async fn configure_logger(config: LoggerConfig) {
    let logger = LOGGER.get_or_init(|| {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { Arc::new(KissShotLogger::new().await) })
        })
    });
    logger.configure(config).await;
}
pub async fn shutdown_logger() {
    if let Some(logger) = LOGGER.get() {
        logger.shutdown().await;
    }
}
pub async fn flush_logger() {
    if let Some(logger) = LOGGER.get() {
        logger.flush().await;
    }
}
pub async fn telegram_logger() -> Arc<LoggerHandle> {
    get_logger("telegram").await
}
pub async fn database_logger() -> Arc<LoggerHandle> {
    get_logger("database").await
}
pub async fn security_logger() -> Arc<LoggerHandle> {
    get_logger("security").await
}
pub async fn bot_commands_logger() -> Arc<LoggerHandle> {
    get_logger("bot_commands").await
}
pub async fn user_actions_logger() -> Arc<LoggerHandle> {
    get_logger("user_actions").await
}
pub async fn errors_logger() -> Arc<LoggerHandle> {
    get_logger("errors").await
}
pub async fn log_performance<F, R>(logger: Arc<LoggerHandle>, func: F) -> R
where
    F: FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = R> + Send>>,
    R: Send + 'static,
{
    let start = Instant::now();
    let result = func().await;
    let duration = start.elapsed();
    logger
        .info(&format!("Function executed in {:.4?}", duration))
        .await;
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    #[tokio::test]
    async fn test_logger_creation() {
        let logger = KissShotLogger::new().await;
        assert!(logger.is_running.load(Ordering::SeqCst));
        let handle = logger.get_logger("test");
        handle.info("Test message").await;
        logger.flush().await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_logger_handle_methods() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("test_logger");
        handle.debug("Debug message").await;
        handle.info("Info message").await;
        handle.warning("Warning message").await;
        handle.error("Error message").await;
        handle.critical("Critical message").await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_logger_with_extra_data() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("test_logger");
        let mut extra = HashMap::new();
        extra.insert("user_id".to_string(), serde_json::Value::Number(123.into()));
        extra.insert(
            "action".to_string(),
            serde_json::Value::String("test_action".to_string()),
        );
        handle
            .log(LogLevel::Info, "Test message with extra data", extra)
            .await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_telegram_event_logging() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("telegram");
        let mut extra = HashMap::new();
        extra.insert(
            "chat_id".to_string(),
            serde_json::Value::Number(123456.into()),
        );
        extra.insert(
            "message_id".to_string(),
            serde_json::Value::Number(789.into()),
        );
        handle.log_telegram_event("message_received", extra).await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_sql_query_logging() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("database");
        let params = Some(vec![
            serde_json::Value::String("test_user".to_string()),
            serde_json::Value::Number(123.into()),
        ]);
        handle
            .log_sql_query("SELECT * FROM users WHERE id = ?", params, Some(0.05))
            .await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_user_action_logging() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("user_actions");
        let mut extra = HashMap::new();
        extra.insert(
            "ip_address".to_string(),
            serde_json::Value::String("192.168.1.1".to_string()),
        );
        extra.insert(
            "user_agent".to_string(),
            serde_json::Value::String("Mozilla/5.0".to_string()),
        );
        handle.log_user_action(12345, "login", extra).await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_logger_configuration() {
        let logger = KissShotLogger::new().await;
        let config = LoggerConfig {
            level: LogLevel::Debug,
            console_output: true,
            file_output: true,
            json_output: false,
            max_file_size: 1024 * 1024,
            backup_count: 5,
            colored_console: true,
            include_traceback: true,
            log_sql: true,
            log_telegram: true,
            log_errors: true,
            log_performance: true,
            buffer_size: 1000,
        };
        logger.configure(config).await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_logger_flush() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("test");
        handle.info("Message before flush").await;
        logger.flush().await;
        handle.info("Message after flush").await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_startup_banner() {
        let logger = KissShotLogger::new().await;
        logger.show_startup_banner().await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_flood_wait_handling() {
        let logger = KissShotLogger::new().await;
        logger.handle_flood_wait(30).await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_log_performance() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("performance");
        let result = log_performance(handle.clone(), || {
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                42
            })
        })
        .await;
        assert_eq!(result, 42);
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_multiple_loggers() {
        let logger = KissShotLogger::new().await;
        let telegram_logger = logger.get_logger("telegram");
        let database_logger = logger.get_logger("database");
        let security_logger = logger.get_logger("security");
        telegram_logger.info("Telegram event").await;
        database_logger.info("Database query").await;
        security_logger.info("Security event").await;
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_logger_shutdown() {
        let logger = KissShotLogger::new().await;
        let handle = logger.get_logger("test");
        handle.info("Message before shutdown").await;
        logger.shutdown().await;
        assert!(!logger.is_running.load(Ordering::SeqCst));
    }
    #[test]
    fn test_log_level_display() {
        assert_eq!(format!("{}", LogLevel::Debug), "DEBUG");
        assert_eq!(format!("{}", LogLevel::Info), "INFO");
        assert_eq!(format!("{}", LogLevel::Warning), "WARNING");
        assert_eq!(format!("{}", LogLevel::Error), "ERROR");
        assert_eq!(format!("{}", LogLevel::Critical), "CRITICAL");
    }
    #[test]
    fn test_log_level_equality() {
        assert_eq!(LogLevel::Debug, LogLevel::Debug);
        assert_ne!(LogLevel::Debug, LogLevel::Info);
        assert_eq!(LogLevel::Critical, LogLevel::Critical);
    }
    #[test]
    fn test_logger_error_display() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let logger_error = LoggerError::Io(io_error);
        assert!(format!("{}", logger_error).contains("File not found"));
    }
    #[tokio::test]
    async fn test_concurrent_logging() {
        let logger = KissShotLogger::new().await;
        let handle = Arc::new(logger.get_logger("concurrent"));
        let mut handles = vec![];
        for i in 0..10 {
            let handle_clone = handle.clone();
            let task = tokio::spawn(async move {
                handle_clone
                    .info(&format!("Concurrent message {}", i))
                    .await;
            });
            handles.push(task);
        }
        for handle in handles {
            handle.await.unwrap();
        }
        logger.shutdown().await;
    }
    #[tokio::test]
    async fn test_logger_with_different_modules() {
        let logger = KissShotLogger::new().await;
        let main_logger = logger.get_logger("main");
        let telegram_logger = logger.get_logger("telegram");
        let database_logger = logger.get_logger("database");
        main_logger.info("Main module message").await;
        telegram_logger.info("Telegram module message").await;
        database_logger.info("Database module message").await;
        logger.shutdown().await;
    }
}
