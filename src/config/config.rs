use config as config_crate;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum ConfigurationError {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}
type Result<T> = std::result::Result<T, ConfigurationError>;
#[derive(Debug, Deserialize, Clone)]
pub struct TelegramBot {
    #[serde(rename = "api_id")]
    pub api_id: i64,
    #[serde(rename = "api_hash")]
    pub api_hash: String,
    #[serde(rename = "bot_token")]
    pub bot_token: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct TelegramUser {
    #[serde(rename = "api_id")]
    pub api_id: i64,
    #[serde(rename = "api_hash")]
    pub api_hash: String,
    #[serde(rename = "phone_number")]
    pub phone_number: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct Telegram {
    #[serde(rename = "bot")]
    pub bot: TelegramBot,
    #[serde(rename = "user")]
    pub user: TelegramUser,
}
#[derive(Debug, Deserialize, Clone)]
pub struct SqlDatabase {
    #[serde(rename = "uri")]
    pub uri: String,
    #[serde(rename = "host")]
    pub host: String,
    #[serde(rename = "port")]
    pub port: u16,
    #[serde(rename = "user")]
    pub user: String,
    #[serde(rename = "password")]
    pub password: String,
    #[serde(rename = "database")]
    pub database: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct RedisDatabase {
    #[serde(rename = "uri")]
    pub uri: String,
    #[serde(rename = "host")]
    pub host: String,
    #[serde(rename = "port")]
    pub port: u16,
    #[serde(rename = "user")]
    pub user: String,
    #[serde(rename = "password")]
    pub password: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct Database {
    #[serde(rename = "sql")]
    pub sql: SqlDatabase,
    #[serde(rename = "redis")]
    pub redis: RedisDatabase,
}
#[derive(Debug, Deserialize, Clone)]
pub struct TeloxideConfig {
    #[serde(rename = "workdir")]
    pub workdir: String,
    #[serde(rename = "session_name")]
    pub session_name: String,
    #[serde(rename = "workers")]
    pub workers: u32,
    #[serde(rename = "plugins_dir")]
    pub plugins_dir: String,
    #[serde(rename = "parse_mode")]
    pub parse_mode: String,
    #[serde(rename = "timeout")]
    pub timeout: u32,
    #[serde(rename = "request_concurrency")]
    pub request_concurrency: u32,
    #[serde(rename = "max_network_retries")]
    pub max_network_retries: u32,
    #[serde(rename = "api_url")]
    pub api_url: Option<String>,
    #[serde(rename = "logging")]
    pub logging: bool,
}
#[derive(Debug, Deserialize, Clone)]
pub struct BasicConfig {
    #[serde(rename = "admin")]
    pub admin: Vec<i64>,
    #[serde(rename = "channel")]
    pub channel: String,
    #[serde(rename = "group")]
    pub group: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct LimitsConfig {
    #[serde(rename = "max_cc_scr")]
    pub max_cc_scr: u32,
    #[serde(rename = "max_sk_scr")]
    pub max_sk_scr: u32,
    #[serde(rename = "max_cc_chk")]
    pub max_cc_chk: u32,
    #[serde(rename = "max_sk_chk")]
    pub max_sk_chk: u32,
}
#[derive(Debug, Deserialize, Clone)]
pub struct RegexConfig {
    #[serde(rename = "cc_regex")]
    pub cc_regex: String,
    #[serde(rename = "sk_regex")]
    pub sk_regex: String,
    #[serde(rename = "bin_regex")]
    pub bin_regex: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct GiftCodeConfig {
    #[serde(rename = "prefix")]
    pub prefix: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct ProxyConfig {
    #[serde(rename = "proxy")]
    pub proxy: Option<String>,
}
#[derive(Debug, Deserialize, Clone)]
pub struct DefaultUserValueConfig {
    #[serde(rename = "antispam")]
    pub antispam: u32,
    #[serde(rename = "balance")]
    pub balance: i64,
    #[serde(rename = "status")]
    pub status: String,
}
#[derive(Debug, Deserialize, Clone)]
pub struct ConfigSection {
    #[serde(rename = "basic")]
    pub basic: BasicConfig,
    #[serde(rename = "limits")]
    pub limits: LimitsConfig,
    #[serde(rename = "regex")]
    pub regex: RegexConfig,
    #[serde(rename = "gift_code")]
    pub gift_code: GiftCodeConfig,
    #[serde(rename = "proxy")]
    pub proxy: ProxyConfig,
    #[serde(rename = "default_user_value")]
    pub default_user_value: DefaultUserValueConfig,
}
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(rename = "telegram")]
    pub telegram: Telegram,
    #[serde(rename = "database")]
    pub database: Database,
    #[serde(rename = "teloxide")]
    pub teloxide: TeloxideConfig,
    #[serde(rename = "config")]
    pub config: ConfigSection,
}
static CONFIG: OnceLock<Result<Arc<AppConfig>>> = OnceLock::new();
pub fn get_config() -> Result<&'static AppConfig> {
    match CONFIG.get_or_init(|| load_config()) {
        Ok(config) => Ok(config),
        Err(e) => Err(match e {
            ConfigurationError::Config(err) => ConfigurationError::Config(err.clone()),
            ConfigurationError::Validation(msg) => ConfigurationError::Validation(msg.clone()),
            ConfigurationError::MissingField(field) => ConfigurationError::MissingField(field),
        }),
    }
}
fn load_config() -> Result<Arc<AppConfig>> {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.toml");
    println!("Loading config from: {:?}", config_path);
    if !config_path.exists() {
        return Err(ConfigurationError::Config(format!(
            "Configuration file not found at: {:?}",
            config_path
        )));
    }
    let config = config_crate::Config::builder()
        .add_source(config_crate::File::from(config_path))
        .build()
        .map_err(|e| {
            println!("Config build error: {}", e);
            ConfigurationError::Config(e.to_string())
        })?;
    let app_config: AppConfig = config.try_deserialize().map_err(|e| {
        println!("Deserialize error: {}", e);
        ConfigurationError::Config(e.to_string())
    })?;
    validate_config(&app_config)?;
    Ok(Arc::new(app_config))
}
fn validate_config(config: &AppConfig) -> Result<()> {
    if config.telegram.bot.api_id <= 0 {
        return Err(ConfigurationError::Validation(
            "Telegram Bot API ID must be positive".into(),
        ));
    }
    if config.telegram.user.api_id <= 0 {
        return Err(ConfigurationError::Validation(
            "Telegram User API ID must be positive".into(),
        ));
    }
    if config.telegram.bot.bot_token.is_empty() {
        return Err(ConfigurationError::Validation(
            "Telegram Bot token cannot be empty".into(),
        ));
    }
    if config.database.sql.port == 0 {
        return Err(ConfigurationError::Validation(
            "SQL database port cannot be 0".into(),
        ));
    }
    if config.database.redis.port == 0 {
        return Err(ConfigurationError::Validation(
            "Redis port cannot be 0".into(),
        ));
    }
    if config.teloxide.workers == 0 {
        return Err(ConfigurationError::Validation(
            "Teloxide workers cannot be 0".into(),
        ));
    }
    if config.teloxide.timeout == 0 {
        return Err(ConfigurationError::Validation("Timeout cannot be 0".into()));
    }
    if config.config.basic.admin.is_empty() {
        return Err(ConfigurationError::Validation(
            "Admin list cannot be empty".into(),
        ));
    }
    if config.config.limits.max_cc_scr == 0
        || config.config.limits.max_sk_scr == 0
        || config.config.limits.max_cc_chk == 0
        || config.config.limits.max_sk_chk == 0
    {
        return Err(ConfigurationError::Validation(
            "All limits must be greater than 0".into(),
        ));
    }
    if config.config.regex.cc_regex.is_empty()
        || config.config.regex.sk_regex.is_empty()
        || config.config.regex.bin_regex.is_empty()
    {
        return Err(ConfigurationError::Validation(
            "Regex patterns cannot be empty".into(),
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_validation_telegram_bot_api_id() {
        let mut config = create_test_config();
        config.telegram.bot.api_id = 0;
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigurationError::Validation(_)
        ));
    }
    #[test]
    fn test_validation_empty_bot_token() {
        let mut config = create_test_config();
        config.telegram.bot.bot_token = String::new();
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigurationError::Validation(_)
        ));
    }
    #[test]
    fn test_validation_empty_admin_list() {
        let mut config = create_test_config();
        config.config.basic.admin.clear();
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigurationError::Validation(_)
        ));
    }
    #[test]
    fn test_validation_zero_limits() {
        let mut config = create_test_config();
        config.config.limits.max_cc_scr = 0;
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigurationError::Validation(_)
        ));
    }
    #[test]
    fn test_validation_empty_regex() {
        let mut config = create_test_config();
        config.config.regex.cc_regex = String::new();
        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigurationError::Validation(_)
        ));
    }
    fn create_test_config() -> AppConfig {
        AppConfig {
            telegram: Telegram {
                bot: TelegramBot {
                    api_id: 12345,
                    api_hash: "test_hash".to_string(),
                    bot_token: "test_token".to_string(),
                },
                user: TelegramUser {
                    api_id: 12345,
                    api_hash: "test_hash".to_string(),
                    phone_number: "+1234567890".to_string(),
                },
            },
            database: Database {
                sql: SqlDatabase {
                    uri: "postgresql://localhost:5432/test".to_string(),
                    host: "localhost".to_string(),
                    port: 5432,
                    user: "test_user".to_string(),
                    password: "test_pass".to_string(),
                    database: "test_db".to_string(),
                },
                redis: RedisDatabase {
                    uri: "redis://localhost:6379".to_string(),
                    host: "localhost".to_string(),
                    port: 6379,
                    user: "test_user".to_string(),
                    password: "test_pass".to_string(),
                },
            },
            teloxide: TeloxideConfig {
                workdir: "/tmp".to_string(),
                session_name: "test_session".to_string(),
                workers: 4,
                plugins_dir: "/plugins".to_string(),
                parse_mode: "HTML".to_string(),
                timeout: 30,
                request_concurrency: 10,
                max_network_retries: 3,
                api_url: Some("https://api.telegram.org".to_string()),
                logging: true,
            },
            config: ConfigSection {
                basic: BasicConfig {
                    admin: vec![123456789],
                    channel: "@test_channel".to_string(),
                    group: "@test_group".to_string(),
                },
                limits: LimitsConfig {
                    max_cc_scr: 100,
                    max_sk_scr: 100,
                    max_cc_chk: 50,
                    max_sk_chk: 50,
                },
                regex: RegexConfig {
                    cc_regex: "\\d{16}".to_string(),
                    sk_regex: "\\d{3}".to_string(),
                    bin_regex: "\\d{6}".to_string(),
                },
                gift_code: GiftCodeConfig {
                    prefix: "GIFT".to_string(),
                },
                proxy: ProxyConfig {
                    proxy: Some("http://proxy:8080".to_string()),
                },
                default_user_value: DefaultUserValueConfig {
                    antispam: 0,
                    balance: 1000,
                    status: "active".to_string(),
                },
            },
        }
    }
}
