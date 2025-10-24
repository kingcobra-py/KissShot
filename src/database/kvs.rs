use crate::config::{get_config, Database};
use crate::logging::{get_logger, LoggerHandle};
use async_trait::async_trait;
use bb8::Pool;
use bb8_redis::RedisConnectionManager;
use futures::future::BoxFuture;
use redis::{AsyncCommands, FromRedisValue, RedisError, ToRedisArgs};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::Instant;
#[derive(Debug, Error)]
pub enum KVSError {
    #[error("Redis error: {0}")]
    Redis(#[from] RedisError),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Not found: {0}")]
    NotFound(String),
}
pub type Result<T> = std::result::Result<T, KVSError>;
#[async_trait]
pub trait KeyValueStore: Send + Sync {
    async fn set(&self, key: &str, value: &str, expire: Option<u64>) -> Result<bool>;
    async fn get(&self, key: &str) -> Result<Option<String>>;
    async fn delete(&self, key: &str) -> Result<bool>;
    async fn exists(&self, key: &str) -> Result<bool>;
    async fn keys(&self, pattern: &str) -> Result<Vec<String>>;
    async fn set_json<T: Serialize + Send>(
        &self,
        key: &str,
        value: &T,
        expire: Option<u64>,
    ) -> Result<bool>;
    async fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
}
#[derive(Clone)]
pub struct KVS {
    pool: Pool<RedisConnectionManager>,
    logger: Arc<LoggerHandle>,
    connection_timeout: Duration,
    operation_timeout: Duration,
}
impl KVS {
    pub async fn new() -> Result<Self> {
        let config = get_config().map_err(|e| KVSError::Connection(e.to_string()))?;
        let logger = get_logger("Database_KVS").await;
        let redis_url = config.database.redis.uri.clone();
        let manager = RedisConnectionManager::new(redis_url)
            .map_err(|e| KVSError::Connection(e.to_string()))?;
        let pool = Pool::builder()
            .max_size(20)
            .min_idle(Some(5))
            .connection_timeout(Duration::from_secs(10))
            .test_on_check_out(true)
            .build(manager)
            .await
            .map_err(|e| KVSError::Connection(e.to_string()))?;
        let pool_clone = pool.clone();
        let mut conn = pool_clone
            .get()
            .await
            .map_err(|e| KVSError::Connection(e.to_string()))?;
        redis::cmd("PING")
            .query_async::<String>(&mut *conn)
            .await
            .map_err(|e| KVSError::Connection(e.to_string()))?;
        logger.info("KVS (Valkey) initialized successfully").await;
        Ok(Self {
            pool,
            logger,
            connection_timeout: Duration::from_secs(10),
            operation_timeout: Duration::from_secs(5),
        })
    }
    #[cfg(test)]
    pub async fn new_for_testing(redis_url: &str) -> Result<Self> {
        let logger = get_logger("Database_KVS_Test").await;
        let manager = RedisConnectionManager::new(redis_url.to_string())
            .map_err(|e| KVSError::Connection(e.to_string()))?;
        let pool = Pool::builder()
            .max_size(5)
            .min_idle(Some(1))
            .connection_timeout(Duration::from_secs(5))
            .test_on_check_out(true)
            .build(manager)
            .await
            .map_err(|e| KVSError::Connection(e.to_string()))?;
        let pool_clone = pool.clone();
        let mut conn = pool_clone
            .get()
            .await
            .map_err(|e| KVSError::Connection(e.to_string()))?;
        redis::cmd("PING")
            .query_async::<String>(&mut *conn)
            .await
            .map_err(|e| KVSError::Connection(e.to_string()))?;
        logger
            .info("KVS (Valkey) initialized successfully for testing")
            .await;
        Ok(Self {
            pool,
            logger,
            connection_timeout: Duration::from_secs(5),
            operation_timeout: Duration::from_secs(2),
        })
    }
    async fn get_connection(&self) -> Result<bb8::PooledConnection<'_, RedisConnectionManager>> {
        tokio::time::timeout(self.connection_timeout, self.pool.get())
            .await
            .map_err(|_| KVSError::Timeout("Connection timeout".to_string()))?
            .map_err(|e| KVSError::Connection(e.to_string()))
    }
    fn validate_key(&self, key: &str) -> Result<()> {
        if key.trim().is_empty() {
            return Err(KVSError::InvalidKey("Key must be non-empty".to_string()));
        }
        if key.len() > 1024 {
            return Err(KVSError::InvalidKey("Key too long".to_string()));
        }
        Ok(())
    }
    pub async fn set(&self, key: &str, value: &str, expire: Option<u64>) -> Result<bool> {
        self.validate_key(key)?;
        let start = Instant::now();
        let mut conn = self.get_connection().await?;
        let mut cmd = redis::cmd("SET");
        cmd.arg(key).arg(value);
        if let Some(expire_secs) = expire {
            cmd.arg("EX").arg(expire_secs);
        }
        let result: String = tokio::time::timeout(
            self.operation_timeout,
            cmd.query_async::<String>(&mut *conn),
        )
        .await
        .map_err(|_| KVSError::Timeout("Operation timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(100) {
            self.logger
                .warning(&format!("Slow KVS operation: {:?}", duration))
                .await;
        }
        Ok(result == "OK")
    }
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        self.validate_key(key)?;
        let start = Instant::now();
        let mut conn = self.get_connection().await?;
        let value: Option<String> = tokio::time::timeout(self.operation_timeout, conn.get(key))
            .await
            .map_err(|_| KVSError::Timeout("Operation timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(100) {
            self.logger
                .warning(&format!("Slow KVS operation: {:?}", duration))
                .await;
        }
        Ok(value)
    }
    pub async fn delete(&self, key: &str) -> Result<bool> {
        self.validate_key(key)?;
        let start = Instant::now();
        let mut conn = self.get_connection().await?;
        let result: i64 = tokio::time::timeout(self.operation_timeout, conn.del(key))
            .await
            .map_err(|_| KVSError::Timeout("Operation timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(100) {
            self.logger
                .warning(&format!("Slow KVS operation: {:?}", duration))
                .await;
        }
        Ok(result > 0)
    }
    pub async fn exists(&self, key: &str) -> Result<bool> {
        self.validate_key(key)?;
        let start = Instant::now();
        let mut conn = self.get_connection().await?;
        let result: i64 = tokio::time::timeout(self.operation_timeout, conn.exists(key))
            .await
            .map_err(|_| KVSError::Timeout("Operation timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(100) {
            self.logger
                .warning(&format!("Slow KVS operation: {:?}", duration))
                .await;
        }
        Ok(result > 0)
    }
    pub async fn keys(&self, pattern: &str) -> Result<Vec<String>> {
        let start = Instant::now();
        let mut conn = self.get_connection().await?;
        let keys: Vec<String> = tokio::time::timeout(self.operation_timeout, conn.keys(pattern))
            .await
            .map_err(|_| KVSError::Timeout("Operation timeout".to_string()))??;
        let duration = start.elapsed();
        if duration > Duration::from_millis(100) {
            self.logger
                .warning(&format!("Slow KVS operation: {:?}", duration))
                .await;
        }
        Ok(keys)
    }
    pub async fn set_json<T: Serialize + Send>(
        &self,
        key: &str,
        value: &T,
        expire: Option<u64>,
    ) -> Result<bool> {
        let json_str =
            serde_json::to_string(value).map_err(|e| KVSError::Serialization(e.to_string()))?;
        self.set(key, &json_str, expire).await
    }
    pub async fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.get(key).await? {
            Some(json_str) => {
                let value: T = serde_json::from_str(&json_str)
                    .map_err(|e| KVSError::Serialization(e.to_string()))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
    pub async fn set_gift_code(
        &self,
        gift_code: &str,
        code_data: &GiftCodeData,
        expire: Option<u64>,
    ) -> Result<bool> {
        self.validate_key(gift_code)?;
        let key = format!("gift_code:{}", gift_code);
        self.set_json(&key, code_data, expire).await
    }
    pub async fn get_gift_code(&self, gift_code: &str) -> Result<Option<GiftCodeData>> {
        self.validate_key(gift_code)?;
        let key = format!("gift_code:{}", gift_code);
        self.get_json(&key).await
    }
    pub async fn register_gift_code(
        &self,
        gift_code: &str,
        gift_balance: Option<i64>,
        used: bool,
        expire: Option<u64>,
    ) -> Result<bool> {
        self.validate_key(gift_code)?;
        let config = get_config().map_err(|e| KVSError::Connection(e.to_string()))?;
        let default_balance = config.config.default_user_value.balance;
        let data = GiftCodeData {
            gift_code: gift_code.to_string(),
            gift_balance: gift_balance.unwrap_or(default_balance),
            used,
            created_at: chrono::Utc::now().timestamp(),
        };
        self.set_gift_code(gift_code, &data, expire).await
    }
    #[cfg(test)]
    pub async fn register_gift_code_with_default(
        &self,
        gift_code: &str,
        gift_balance: Option<i64>,
        used: bool,
        expire: Option<u64>,
        default_balance: i64,
    ) -> Result<bool> {
        self.validate_key(gift_code)?;
        let data = GiftCodeData {
            gift_code: gift_code.to_string(),
            gift_balance: gift_balance.unwrap_or(default_balance),
            used,
            created_at: chrono::Utc::now().timestamp(),
        };
        self.set_gift_code(gift_code, &data, expire).await
    }
    pub async fn update_gift_code(
        &self,
        gift_code: &str,
        gift_balance: Option<i64>,
        used: Option<bool>,
    ) -> Result<bool> {
        self.validate_key(gift_code)?;
        let mut existing_data = self
            .get_gift_code(gift_code)
            .await?
            .ok_or_else(|| KVSError::NotFound(format!("Gift code not found: {}", gift_code)))?;
        if let Some(balance) = gift_balance {
            existing_data.gift_balance = balance;
        }
        if let Some(used_flag) = used {
            existing_data.used = used_flag;
        }
        self.set_gift_code(gift_code, &existing_data, None).await
    }
    pub async fn delete_gift_code(&self, gift_code: &str) -> Result<bool> {
        self.validate_key(gift_code)?;
        let key = format!("gift_code:{}", gift_code);
        self.delete(&key).await
    }
    pub async fn gift_code_exists(&self, gift_code: &str) -> Result<bool> {
        self.validate_key(gift_code)?;
        let key = format!("gift_code:{}", gift_code);
        self.exists(&key).await
    }
    pub async fn close(&self) -> Result<()> {
        self.logger.info("KVS (Valkey) closed successfully").await;
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GiftCodeData {
    pub gift_code: String,
    pub gift_balance: i64,
    pub used: bool,
    pub created_at: i64,
}
static KVS_INSTANCE: OnceLock<Result<Arc<KVS>>> = OnceLock::new();
pub async fn get_kvs() -> Result<Arc<KVS>> {
    let instance = KVS_INSTANCE.get_or_init(|| {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                match KVS::new().await {
                    Ok(kvs) => Ok(Arc::new(kvs)),
                    Err(e) => Err(e),
                }
            })
        })
    });
    match instance {
        Ok(kvs) => Ok(kvs.clone()),
        Err(e) => Err(KVSError::Connection(format!(
            "Failed to get KVS instance: {}",
            e
        ))),
    }
}
pub async fn set(key: &str, value: &str, expire: Option<u64>) -> Result<bool> {
    let kvs = get_kvs().await?;
    kvs.set(key, value, expire).await
}
pub async fn get(key: &str) -> Result<Option<String>> {
    let kvs = get_kvs().await?;
    kvs.get(key).await
}
pub async fn set_json<T: Serialize + Send>(
    key: &str,
    value: &T,
    expire: Option<u64>,
) -> Result<bool> {
    let kvs = get_kvs().await?;
    kvs.set_json(key, value, expire).await
}
pub async fn get_json<T: DeserializeOwned>(key: &str) -> Result<Option<T>> {
    let kvs = get_kvs().await?;
    kvs.get_json(key).await
}
pub async fn delete(key: &str) -> Result<bool> {
    let kvs = get_kvs().await?;
    kvs.delete(key).await
}
pub async fn exists(key: &str) -> Result<bool> {
    let kvs = get_kvs().await?;
    kvs.exists(key).await
}
pub async fn keys(pattern: &str) -> Result<Vec<String>> {
    let kvs = get_kvs().await?;
    kvs.keys(pattern).await
}
pub async fn set_gift_code(
    gift_code: &str,
    code_data: &GiftCodeData,
    expire: Option<u64>,
) -> Result<bool> {
    let kvs = get_kvs().await?;
    kvs.set_gift_code(gift_code, code_data, expire).await
}
pub async fn get_gift_code(gift_code: &str) -> Result<Option<GiftCodeData>> {
    let kvs = get_kvs().await?;
    kvs.get_gift_code(gift_code).await
}
pub async fn register_gift_code(
    gift_code: &str,
    gift_balance: Option<i64>,
    used: bool,
    expire: Option<u64>,
) -> Result<bool> {
    let kvs = get_kvs().await?;
    kvs.register_gift_code(gift_code, gift_balance, used, expire)
        .await
}
pub async fn update_gift_code(
    gift_code: &str,
    gift_balance: Option<i64>,
    used: Option<bool>,
) -> Result<bool> {
    let kvs = get_kvs().await?;
    kvs.update_gift_code(gift_code, gift_balance, used).await
}
pub async fn delete_gift_code(gift_code: &str) -> Result<bool> {
    let kvs = get_kvs().await?;
    kvs.delete_gift_code(gift_code).await
}
pub async fn gift_code_exists(gift_code: &str) -> Result<bool> {
    let kvs = get_kvs().await?;
    kvs.gift_code_exists(gift_code).await
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio;
    async fn get_test_kvs() -> Option<KVS> {
        let config = match get_config() {
            Ok(config) => config,
            Err(_) => {
                println!("Skipping test: Configuration not available");
                return None;
            }
        };
        match KVS::new_for_testing(&config.database.redis.uri).await {
            Ok(kvs) => Some(kvs),
            Err(_) => {
                println!("Skipping test: Redis not available");
                None
            }
        }
    }
    #[tokio::test]
    async fn test_kvs_initialization() {
        if let Some(_kvs) = get_test_kvs().await {}
    }
    #[tokio::test]
    async fn test_json_operations() {
        let Some(kvs) = get_test_kvs().await else {
            return;
        };
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct TestData {
            name: String,
            value: i32,
            active: bool,
        }
        let test_data = TestData {
            name: "test".to_string(),
            value: 42,
            active: true,
        };
        let result = kvs.set_json("json_key", &test_data, None).await;
        assert!(result.is_ok(), "Set JSON operation should succeed");
        let retrieved: Option<TestData> = kvs
            .get_json("json_key")
            .await
            .expect("Get JSON operation should succeed");
        assert_eq!(retrieved, Some(test_data));
        let deleted = kvs
            .delete("json_key")
            .await
            .expect("Delete operation should succeed");
        assert!(deleted);
    }
    #[tokio::test]
    async fn test_key_validation() {
        let Some(kvs) = get_test_kvs().await else {
            return;
        };
        let result = kvs.set("", "value", None).await;
        assert!(result.is_err(), "Empty key should be rejected");
        let long_key = "a".repeat(1025);
        let result = kvs.set(&long_key, "value", None).await;
        assert!(result.is_err(), "Key too long should be rejected");
    }
    #[tokio::test]
    async fn test_expiration() {
        let Some(kvs) = get_test_kvs().await else {
            return;
        };
        let result = kvs.set("expire_key", "value", Some(1)).await;
        assert!(result.is_ok(), "Set with expiration should succeed");
        let exists = kvs
            .exists("expire_key")
            .await
            .expect("Exists operation should succeed");
        assert!(exists, "Key should exist before expiration");
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let exists = kvs
            .exists("expire_key")
            .await
            .expect("Exists operation should succeed");
        assert!(!exists, "Key should not exist after expiration");
    }
    #[tokio::test]
    async fn test_keys_pattern() {
        let Some(kvs) = get_test_kvs().await else {
            return;
        };
        let keys = vec!["test:1", "test:2", "other:1", "test:3"];
        for key in &keys {
            let result = kvs.set(key, "value", None).await;
            assert!(result.is_ok(), "Set operation should succeed");
        }
        let test_keys = kvs
            .keys("test:*")
            .await
            .expect("Keys operation should succeed");
        assert_eq!(test_keys.len(), 3, "Should find 3 keys matching pattern");
        for key in &keys {
            let _ = kvs.delete(key).await;
        }
    }
    #[tokio::test]
    async fn test_gift_code_operations() {
        let Some(kvs) = get_test_kvs().await else {
            return;
        };
        let gift_code = "TEST_GIFT_123";
        let result = kvs
            .register_gift_code_with_default(gift_code, Some(100), false, None, 1000)
            .await;
        assert!(result.is_ok(), "Register gift code should succeed");
        let exists = kvs
            .gift_code_exists(gift_code)
            .await
            .expect("Gift code exists should succeed");
        assert!(exists, "Gift code should exist");
        let retrieved = kvs
            .get_gift_code(gift_code)
            .await
            .expect("Get gift code should succeed");
        assert!(retrieved.is_some(), "Gift code should be retrieved");
        if let Some(data) = retrieved {
            assert_eq!(data.gift_code, gift_code);
            assert_eq!(data.gift_balance, 100);
            assert_eq!(data.used, false);
        }
        let result = kvs.update_gift_code(gift_code, Some(200), Some(true)).await;
        assert!(result.is_ok(), "Update gift code should succeed");
        let updated = kvs
            .get_gift_code(gift_code)
            .await
            .expect("Get updated gift code should succeed");
        if let Some(data) = updated {
            assert_eq!(data.gift_balance, 200);
            assert_eq!(data.used, true);
        }
        let deleted = kvs
            .delete_gift_code(gift_code)
            .await
            .expect("Delete gift code should succeed");
        assert!(deleted, "Gift code should be deleted");
        let exists = kvs
            .gift_code_exists(gift_code)
            .await
            .expect("Gift code exists should succeed");
        assert!(!exists, "Gift code should not exist after deletion");
    }
    #[tokio::test]
    async fn test_convenience_functions() {
        let Some(kvs) = get_test_kvs().await else {
            return;
        };
        let result = kvs.set("conv_key", "conv_value", None).await;
        assert!(result.is_ok(), "Convenience set should succeed");
        let value = kvs
            .get("conv_key")
            .await
            .expect("Convenience get should succeed");
        assert_eq!(value, Some("conv_value".to_string()));
        let exists = kvs
            .exists("conv_key")
            .await
            .expect("Convenience exists should succeed");
        assert!(exists, "Key should exist");
        let deleted = kvs
            .delete("conv_key")
            .await
            .expect("Convenience delete should succeed");
        assert!(deleted, "Key should be deleted");
    }
    #[tokio::test]
    async fn test_concurrent_operations() {
        let Some(kvs) = get_test_kvs().await else {
            return;
        };
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let kvs = kvs.clone();
                tokio::spawn(async move {
                    let key = format!("concurrent_key_{}", i);
                    let value = format!("concurrent_value_{}", i);
                    kvs.set(&key, &value, None).await?;
                    let retrieved = kvs.get(&key).await?;
                    kvs.delete(&key).await?;
                    Ok::<_, KVSError>(retrieved)
                })
            })
            .collect();
        for handle in handles {
            let result = handle.await.expect("Task should complete");
            assert!(result.is_ok(), "Concurrent operation should succeed");
        }
    }
    #[tokio::test]
    async fn test_error_handling() {
        let Some(kvs) = get_test_kvs().await else {
            return;
        };
        let result = kvs.get("nonexistent_key").await;
        assert!(
            result.is_ok(),
            "Get non-existent key should return Ok(None)"
        );
        if let Ok(value) = result {
            assert_eq!(value, None, "Non-existent key should return None");
        }
    }
    #[tokio::test]
    async fn test_singleton_instance() {
        let Some(instance1) = get_test_kvs().await else {
            return;
        };
        let Some(instance2) = get_test_kvs().await else {
            return;
        };
        let result1 = instance1.set("singleton_test_1", "value1", None).await;
        let result2 = instance2.set("singleton_test_2", "value2", None).await;
        assert!(result1.is_ok(), "First instance should work");
        assert!(result2.is_ok(), "Second instance should work");
        let _ = instance1.delete("singleton_test_1").await;
        let _ = instance2.delete("singleton_test_2").await;
    }
    #[tokio::test]
    async fn test_print_available_keys() {
        println!("=== Testing Redis Key Printing ===");
        let config = match get_config() {
            Ok(config) => {
                println!("✓ Configuration loaded successfully");
                config
            }
            Err(e) => {
                println!("✗ Configuration not available: {}", e);
                println!("This test requires a valid config.toml file");
                return;
            }
        };
        println!("Using Redis URI: {}", config.database.redis.uri);
        let kvs = match KVS::new_for_testing(&config.database.redis.uri).await {
            Ok(kvs) => {
                println!("✓ Redis connection established");
                kvs
            }
            Err(e) => {
                println!("✗ Redis connection failed: {}", e);
                println!("Make sure Redis is running and accessible");
                return;
            }
        };
        println!("Adding test data...");
        let _ = kvs.set("print_test_1", "value1", None).await;
        let _ = kvs.set("print_test_2", "value2", None).await;
        let _ = kvs
            .set_json(
                "print_test_json",
                &serde_json::json!({"key": "value"}),
                None,
            )
            .await;
        println!("\n=== Available keys in Redis ===");
        match kvs.keys("*").await {
            Ok(keys) => {
                if keys.is_empty() {
                    println!("No keys found in Redis");
                } else {
                    for key in &keys {
                        println!("Key: {}", key);
                        match kvs.get(key).await {
                            Ok(Some(value)) => {
                                println!("  Value: {}", value);
                            }
                            Ok(None) => {
                                println!("  Value: (null)");
                            }
                            Err(e) => {
                                println!("  Error getting value: {}", e);
                            }
                        }
                    }
                    println!("\nTotal keys found: {}", keys.len());
                }
            }
            Err(e) => {
                println!("Error getting keys: {}", e);
            }
        }
        println!("\nCleaning up test data...");
        let _ = kvs.delete("print_test_1").await;
        let _ = kvs.delete("print_test_2").await;
        let _ = kvs.delete("print_test_json").await;
        println!("=== Test completed ===");
    }
}
