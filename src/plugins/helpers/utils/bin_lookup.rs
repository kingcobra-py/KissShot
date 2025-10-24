use reqwest::Client;
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct BinLookup {
    pub success: bool,
    pub bin: String,
    pub number: String,
    pub country: String,
    pub flag: String,
    pub vendor: String,
    #[serde(rename = "type")]
    pub btype: String,
    pub level: String,
    pub bank: String,
}
pub async fn lookup_bin(bin: &str) -> Result<BinLookup, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "https://serverless-bin-lookup.vercel.app/api/bin?bin={}",
            bin
        ))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let bin_lookup: BinLookup = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok(bin_lookup)
}
