//! Talking to a Retium node.
//!
//! Everything goes through a public RPC endpoint. You never address keepers
//! directly, and you never need one of your own to build on Retium.
//!
//! Reads are open. Writes need an API key header on the public endpoint. A key
//! only raises your rate limit and grants no authority. Authority comes from
//! your signature.

use std::time::Duration;

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::amount;
use crate::error::{Error, Result};

/// The public endpoint. Nothing else needs to be reachable from your machine.
pub const PUBLIC_RPC: &str = "https://rpc.retium.org";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A wallet as the chain sees it.
#[derive(Deserialize, Debug, Clone)]
pub struct Wallet {
    /// The wallet's address.
    pub address: String,
    /// Confirmed RTM, a decimal string.
    pub balance_rtm: String,
    /// Staked PIE, whole units.
    #[serde(default)]
    pub balance_pie: String,
    /// The next nonce to sign with.
    pub nonce: u64,
}

impl Wallet {
    /// Confirmed balance in atomic units, which is what a transaction's balance claim
    /// needs.
    pub fn balance_atomic(&self) -> Result<u128> {
        amount::parse_rtm(&self.balance_rtm)
    }
}

/// What the node says about a submitted transaction.
#[derive(Deserialize, Debug, Clone)]
pub struct Submitted {
    /// The transaction's id. Look it up to follow its progress.
    pub tx_id: String,
    /// The node's own word for what it did with it.
    #[serde(default)]
    pub status: Option<String>,
    /// The block it went into, when the node says so.
    #[serde(default, deserialize_with = "number_or_text")]
    pub block_id: Option<u64>,
    /// Present on a deploy.
    #[serde(default)]
    pub contract_id: Option<String>,
}

/// Some endpoints send a block id as a number and some as text.
fn number_or_text<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<u64>, D::Error> {
    Ok(match Option::<serde_json::Value>::deserialize(d)? {
        Some(serde_json::Value::Number(n)) => n.as_u64(),
        Some(serde_json::Value::String(s)) => s.parse().ok(),
        _ => None,
    })
}

/// A token an account holds.
#[derive(Deserialize, Debug, Clone)]
pub struct TokenBalance {
    /// The token contract.
    pub contract_id: String,
    /// The token's symbol, for example `RTD`. Declared by whoever deployed
    /// it, so identify tokens by `contract_id`.
    #[serde(default)]
    pub symbol: String,
    /// Decimal places.
    #[serde(default)]
    pub decimals: u32,
    /// Balance in the token's smallest unit.
    pub balance: String,
}

impl TokenBalance {
    /// The balance as a decimal string, trailing zeros trimmed.
    pub fn display(&self) -> String {
        let raw = self.balance.trim_start_matches('0');
        let d = self.decimals as usize;
        if d == 0 {
            return if raw.is_empty() {
                "0".into()
            } else {
                raw.into()
            };
        }
        let padded = format!("{raw:0>width$}", width = d + 1);
        let (whole, frac) = padded.split_at(padded.len() - d);
        let frac = frac.trim_end_matches('0');
        if frac.is_empty() {
            whole.to_string()
        } else {
            format!("{whole}.{frac}")
        }
    }
}

/// One row of the API key price list.
#[derive(Deserialize, Debug, Clone)]
pub struct ApiTier {
    /// `free`, `pro` or `enterprise`.
    pub name: String,
    /// Requests allowed per day.
    pub per_day: u64,
    /// Requests allowed per second.
    pub per_sec: u64,
    /// Price for one period in RTM.
    pub price_rtm: String,
    /// The same price in atomic units.
    pub price_atomic: String,
}

/// The API key price list.
#[derive(Deserialize, Debug, Clone)]
pub struct ApiTiers {
    /// How long a paid tier lasts.
    pub period_days: u64,
    /// The tiers, cheapest first.
    pub tiers: Vec<ApiTier>,
}

/// A developer API key as the chain records it. The secret is never stored.
#[derive(Deserialize, Debug, Clone)]
pub struct ApiKeyInfo {
    /// `blake3(secret)` in hex.
    pub key_hash: String,
    /// Your label for it.
    pub label: String,
    /// The tier in force now.
    pub tier: String,
    /// Unix time a paid tier runs out, 0 if none was bought.
    #[serde(default)]
    pub tier_until: u64,
    /// True once revoked.
    pub revoked: bool,
    /// Requests counted today on the node that answered.
    #[serde(default)]
    pub used_today: u64,
}

/// How far a transaction has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finality {
    /// The node has it, and it is not final yet.
    Pending,
    /// Executed, and can still be reorganised in principle.
    SoftFinal,
    /// Settled. This is the one to wait for before you act on it.
    HardFinal,
    /// It will never execute.
    Rejected,
}

impl Finality {
    fn parse(s: &str) -> Self {
        match s {
            "HardFinal" => Self::HardFinal,
            "SoftFinal" => Self::SoftFinal,
            "Rejected" | "Voided" => Self::Rejected,
            _ => Self::Pending,
        }
    }

    /// Nothing more will happen to this transaction.
    pub fn is_final(self) -> bool {
        matches!(self, Self::HardFinal | Self::Rejected)
    }
}

/// What became of a deploy or a call once it was final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The contract ran and accepted it.
    Ok,
    /// The contract or the chain refused it, with the reason. The fee is
    /// still paid.
    Failed(String),
    /// Final, but the contract has not run yet.
    Pending,
    /// Not a deploy or a call, or not final yet.
    NotApplicable,
}

/// A transaction as the node reports it.
#[derive(Debug, Clone)]
pub struct TxStatus {
    /// The transaction's id.
    pub tx_id: String,
    /// `transfer`, `deploy`, `call` and so on.
    pub action: String,
    /// How far it has got.
    pub finality: Finality,
    /// What the contract made of it.
    pub outcome: Outcome,
}

/// A deployed contract's public record.
#[derive(Deserialize, Debug, Clone)]
pub struct ContractInfo {
    /// The contract's id.
    pub contract_id: String,
    /// blake3 of the deployed wasm. Compare this with your own build.
    #[serde(default)]
    pub code_hash: Option<String>,
    /// The RCP-1 metadata the deployer declared.
    #[serde(default)]
    pub rcp_metadata: Option<serde_json::Value>,
}

/// The network's health, as the public endpoint reports it.
#[derive(Deserialize, Debug, Clone)]
pub struct Health {
    /// `"ok"` when the node is serving.
    pub status: String,
    /// The tick the network is on.
    pub tick: u64,
    /// The node's software version.
    #[serde(default)]
    pub version: String,
}

/// An HTTP client for one Retium endpoint.
///
/// Cheap to clone. Reuse one per process rather than building it per call.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    api_key: Option<String>,
}

impl Client {
    /// A client for `base` (for example [`PUBLIC_RPC`]).
    pub fn new(base: impl Into<String>) -> Result<Self> {
        Self::builder(base).build()
    }

    /// A client with options.
    pub fn builder(base: impl Into<String>) -> ClientBuilder {
        ClientBuilder {
            base: base.into(),
            api_key: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// The endpoint this client talks to.
    pub fn endpoint(&self) -> &str {
        &self.base
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        let res = self
            .http
            .get(format!("{}{}", self.base, path))
            .send()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        let status = res.status();
        if status.as_u16() == 404 {
            return Ok(None);
        }
        let body = res
            .text()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(error_for(status.as_u16(), &body));
        }
        serde_json::from_str(&body)
            .map(Some)
            .map_err(|e| Error::Unexpected(format!("{e} in {}", first_line(&body))))
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        let mut req = self.http.post(format!("{}{}", self.base, path)).json(body);
        if let Some(key) = &self.api_key {
            req = req.header("X-API-Key", key);
        }
        let res = req
            .send()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        let status = res.status();
        let text = res
            .text()
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        if status.is_success() {
            return serde_json::from_str(&text)
                .map_err(|e| Error::Unexpected(format!("{e} in {}", first_line(&text))));
        }
        Err(error_for(status.as_u16(), &text))
    }

    /// A wallet, or `None` if the chain has never seen it.
    pub async fn wallet(&self, address: &str) -> Result<Option<Wallet>> {
        if !crate::address::is_valid(address) {
            return Err(Error::InvalidAddress(address.to_string()));
        }
        self.get(&format!("/api/wallet/{address}")).await
    }

    /// The tokens an account holds, RTD included.
    pub async fn tokens(&self, address: &str) -> Result<Vec<TokenBalance>> {
        if !crate::address::is_valid(address) {
            return Err(Error::InvalidAddress(address.to_string()));
        }
        #[derive(Deserialize)]
        struct Tokens {
            #[serde(default)]
            tokens: Vec<TokenBalance>,
        }
        let t: Option<Tokens> = self.get(&format!("/api/wallet/{address}/tokens")).await?;
        Ok(t.map(|t| t.tokens).unwrap_or_default())
    }

    /// The next nonce for an account (0 for one the chain has not seen).
    pub async fn next_nonce(&self, address: &str) -> Result<u64> {
        Ok(self.wallet(address).await?.map(|w| w.nonce).unwrap_or(0))
    }

    /// How far a transaction has got.
    pub async fn finality(&self, tx_id: &str) -> Result<Finality> {
        Ok(self
            .transaction(tx_id)
            .await?
            .map(|t| t.finality)
            .unwrap_or(Finality::Pending))
    }

    /// A transaction's progress and, for a deploy or a call, its result.
    /// `None` while the node has not indexed it yet.
    pub async fn transaction(&self, tx_id: &str) -> Result<Option<TxStatus>> {
        #[derive(Deserialize)]
        struct Tx {
            #[serde(default)]
            finality: Option<String>,
            #[serde(default)]
            exec_status: Option<String>,
            #[serde(default)]
            exec_error: Option<String>,
            #[serde(default)]
            rejection_reason: Option<String>,
            #[serde(default)]
            action: Option<String>,
        }
        crate::contract::check_path_segment("transaction", tx_id)?;
        let tx: Option<Tx> = self.get(&format!("/api/transaction/{tx_id}")).await?;
        Ok(tx.map(|t| {
            let finality = t
                .finality
                .as_deref()
                .map(Finality::parse)
                .unwrap_or(Finality::Pending);
            let outcome = match t.exec_status.as_deref() {
                Some("ok") => Outcome::Ok,
                Some("failed") => Outcome::Failed(t.exec_error.clone().unwrap_or_default()),
                Some("pending") => Outcome::Pending,
                _ if finality == Finality::Rejected => {
                    Outcome::Failed(t.rejection_reason.clone().unwrap_or_default())
                }
                _ => Outcome::NotApplicable,
            };
            TxStatus {
                tx_id: tx_id.to_string(),
                action: t.action.unwrap_or_default(),
                finality,
                outcome,
            }
        }))
    }

    /// Wait until a transaction is settled or refused.
    ///
    /// Polls gently. Give it a timeout you are happy to block for. For a
    /// deploy or a call, use [`Client::wait_for_result`] to learn whether
    /// the contract accepted it.
    pub async fn wait_for_final(&self, tx_id: &str, timeout: Duration) -> Result<Finality> {
        self.wait(tx_id, timeout, false).await.map(|t| t.finality)
    }

    /// Wait until a transaction is settled and, for a deploy or a call, until
    /// its result is known. A call can be final and still have failed, so
    /// check [`TxStatus::outcome`].
    pub async fn wait_for_result(&self, tx_id: &str, timeout: Duration) -> Result<TxStatus> {
        self.wait(tx_id, timeout, true).await
    }

    async fn wait(&self, tx_id: &str, timeout: Duration, result: bool) -> Result<TxStatus> {
        let start = std::time::Instant::now();
        loop {
            if let Some(t) = self.transaction(tx_id).await? {
                if t.finality.is_final() && !(result && t.outcome == Outcome::Pending) {
                    return Ok(t);
                }
            }
            if start.elapsed() >= timeout {
                return Err(Error::Unexpected(format!(
                    "{tx_id} was not settled after {}s",
                    timeout.as_secs()
                )));
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    /// A deployed contract's record, or `None` if there is no such contract.
    pub async fn contract(&self, contract_id: &str) -> Result<Option<ContractInfo>> {
        crate::contract::check_path_segment("contract", contract_id)?;
        self.get(&format!("/api/contract/{contract_id}")).await
    }

    /// Read a contract without sending a transaction. No gas, no state change,
    /// no API key.
    ///
    /// The read endpoint takes the method and arguments in the URL and does
    /// not decode them, so they are limited to letters, digits and
    /// `- _ . ~ :` (see [`crate::contract::check_query`]). Addresses, ids and
    /// numbers all fit. For anything else use [`Client::view`].
    pub async fn query(
        &self,
        contract_id: &str,
        method: &str,
        args: &[String],
    ) -> Result<serde_json::Value> {
        crate::contract::check_path_segment("contract", contract_id)?;
        crate::contract::check_query(method, args)?;
        let path = format!(
            "/api/contract/{contract_id}/query?method={method}&args={}",
            args.join(",")
        );
        self.get(&path)
            .await?
            .ok_or_else(|| Error::Invalid(format!("no contract {contract_id}")))
    }

    /// The same free read as [`Client::query`], sent as JSON, so arguments may
    /// hold any text that [`crate::contract::check_call`] allows.
    ///
    /// It is a POST, and the public endpoint asks for an API key on every POST:
    /// build the client with [`ClientBuilder::api_key`].
    pub async fn view(
        &self,
        contract_id: &str,
        method: &str,
        args: &[String],
    ) -> Result<serde_json::Value> {
        crate::contract::check_path_segment("contract", contract_id)?;
        crate::contract::check_call(method, args)?;
        let body = serde_json::json!({
            "contract_id": contract_id,
            "method": method,
            "args": args,
            "view": true,
            // A view signs nothing and uses no nonce.
            "nonce": 0,
        });
        self.post("/api/contract/call", &body).await
    }

    /// The API key price list. No API key needed.
    pub async fn api_tiers(&self) -> Result<ApiTiers> {
        self.get("/api/developer/tiers")
            .await?
            .ok_or_else(|| Error::Unexpected("no answer from /api/developer/tiers".into()))
    }

    /// The API keys a wallet has registered. No API key needed.
    pub async fn api_keys(&self, address: &str) -> Result<Vec<ApiKeyInfo>> {
        if !crate::address::is_valid(address) {
            return Err(Error::InvalidAddress(address.to_string()));
        }
        #[derive(Deserialize)]
        struct Keys {
            #[serde(default)]
            keys: Vec<ApiKeyInfo>,
        }
        let keys: Option<Keys> = self.get(&format!("/api/developer/keys/{address}")).await?;
        Ok(keys.map(|k| k.keys).unwrap_or_default())
    }

    /// The network's health.
    pub async fn health(&self) -> Result<Health> {
        self.get("/health")
            .await?
            .ok_or_else(|| Error::Unexpected("no answer from /health".into()))
    }

    /// Submit an already-signed request body to one of the write endpoints.
    /// [`crate::tx`] builds these for you.
    pub async fn submit(&self, path: &str, body: &serde_json::Value) -> Result<Submitted> {
        self.post(path, body).await
    }
}

/// Options for a [`Client`].
pub struct ClientBuilder {
    base: String,
    api_key: Option<String>,
    timeout: Duration,
}

impl ClientBuilder {
    /// Send this key with writes. It raises your rate limit and grants no
    /// authority over any account.
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        let key = key.into();
        self.api_key = if key.trim().is_empty() {
            None
        } else {
            Some(key)
        };
        self
    }

    /// How long to wait for the node before giving up. Default 30s.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<Client> {
        let http = reqwest::Client::builder()
            .timeout(self.timeout)
            .user_agent(concat!("retium-sdk/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::Transport(e.to_string()))?;
        Ok(Client {
            http,
            base: self.base.trim_end_matches('/').to_string(),
            api_key: self.api_key,
        })
    }
}

/// One rule for every refusal the node sends back.
fn error_for(status: u16, body: &str) -> Error {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or_else(|| first_line(body));
    match status {
        // Mid-handover, no open block right now, or over the rate limit.
        429 | 503 => Error::Busy(message),
        400 | 401 | 403 | 409 | 413 | 422 => Error::Refused(message),
        code => Error::Node {
            status: code,
            body: message,
        },
    }
}

fn first_line(s: &str) -> String {
    let line = s.lines().next().unwrap_or("").trim();
    match line.char_indices().nth(300) {
        Some((cut, _)) => format!("{}...", &line[..cut]),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finality_maps_the_states_that_matter() {
        assert_eq!(Finality::parse("HardFinal"), Finality::HardFinal);
        assert_eq!(Finality::parse("Rejected"), Finality::Rejected);
        assert_eq!(Finality::parse("SoftFinal"), Finality::SoftFinal);
        assert_eq!(Finality::parse("anything else"), Finality::Pending);
        assert!(Finality::HardFinal.is_final() && Finality::Rejected.is_final());
        assert!(!Finality::SoftFinal.is_final() && !Finality::Pending.is_final());
    }

    #[test]
    fn busy_and_rate_limited_are_worth_retrying_and_refusals_are_not() {
        assert!(error_for(503, r#"{"error":"tx_not_delivered"}"#).is_retryable());
        assert!(error_for(429, "slow down").is_retryable());
        let refused = error_for(400, r#"{"error":"Nonce mismatch: got 1 but expected 2"}"#);
        assert!(!refused.is_retryable());
        assert!(refused.to_string().contains("Nonce mismatch"), "{refused}");
    }

    #[test]
    fn the_endpoint_never_keeps_a_trailing_slash() {
        let c = Client::new("https://rpc.retium.org/").unwrap();
        assert_eq!(c.endpoint(), "https://rpc.retium.org");
    }

    #[test]
    fn token_balances_print_as_decimals() {
        let t = |b: &str, d: u32| {
            TokenBalance {
                contract_id: String::new(),
                symbol: "RTD".into(),
                decimals: d,
                balance: b.into(),
            }
            .display()
        };
        assert_eq!(t("8485000000000000000", 18), "8.485");
        assert_eq!(t("10000000000000000000", 18), "10");
        assert_eq!(t("5", 18), "0.000000000000000005");
        assert_eq!(t("0", 18), "0");
        assert_eq!(t("42", 0), "42");
    }

    #[test]
    fn an_empty_api_key_is_no_key() {
        let c = Client::builder("http://x").api_key("   ").build().unwrap();
        assert!(c.api_key.is_none());
    }

    #[test]
    fn long_node_errors_are_trimmed_to_one_line() {
        let msg = first_line(&format!("first line\nsecond\n{}", "x".repeat(500)));
        assert_eq!(msg, "first line");
        let long = "é".repeat(400);
        let cut = first_line(&long);
        assert_eq!(cut, format!("{}...", "é".repeat(300)));
    }
}
