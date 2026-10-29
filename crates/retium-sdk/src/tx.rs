//! Build, sign and submit transactions.
//!
//! Each call here does the same four things, so you can read one and know them
//! all:
//!
//! 1. ask the node for your next nonce and confirmed balance,
//! 2. build the canonical payload and sign its hash locally,
//! 3. post the signed request,
//! 4. hand back the transaction id.
//!
//! Submitting is not the same as settled. Wait for
//! [`Client::wait_for_final`](crate::rpc::Client::wait_for_final) before you
//! treat a transfer as done, and expect [`Error::Busy`] now and then. It means
//! "ask again in a moment", not "failed".

use serde_json::json;

use crate::amount;
use crate::canonical::{CanonicalPayload, ChainId, FEE_RTD, FEE_RTM};
use crate::contract;
use crate::error::{Error, Result};
use crate::key::SecretKey;
use crate::rpc::{Client, Submitted};

/// How a transaction is signed. Both fields are part of the signature.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Chain id to sign for. The public testnet is [`ChainId::Devnet`].
    pub chain: ChainId,
    /// The coin the gas is paid in: [`FEE_RTM`] (the default) or [`FEE_RTD`],
    /// Retium Dollar, at 1.5x the RTM fee. Paying in RTD needs RTD in the
    /// sending wallet.
    pub fee_currency: u8,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            chain: ChainId::Devnet,
            fee_currency: FEE_RTM,
        }
    }
}

impl Options {
    /// Pay the gas in RTD instead of RTM.
    pub fn gas_in_rtd(mut self) -> Self {
        self.fee_currency = FEE_RTD;
        self
    }

    fn apply(&self, payload: CanonicalPayload) -> CanonicalPayload {
        payload
            .on_chain(self.chain)
            .with_fee_currency(self.fee_currency)
    }
}

struct Context {
    nonce: u64,
    balance_claim: u128,
    /// The key hash the chain expects next (None for a wallet never seen).
    expected_key_hash: Option<[u8; 32]>,
    published_login: bool,
    published_cosign: bool,
}

async fn context(client: &Client, address: &str) -> Result<Context> {
    match client.wallet(address).await? {
        Some(w) => Ok(Context {
            nonce: w.nonce,
            balance_claim: w.balance_atomic()?,
            expected_key_hash: w
                .next_key_hash
                .as_deref()
                .and_then(|h| hex::decode(h).ok())
                .and_then(|b| b.try_into().ok()),
            published_login: w.login_key_hash.is_some(),
            published_cosign: w.cosign.as_ref().map(|c| !c.is_null()).unwrap_or(false),
        }),
        None => Err(Error::Invalid(format!(
            "the chain has no record of {address}, it needs funds before it can send anything"
        ))),
    }
}

/// One key per transaction. Signs `payload` with key `ctx.nonce` of `key`'s
/// seed, after checking that is the key the chain expects, and returns the
/// signature with the key fields every body carries: `public_key`,
/// `next_key_hash`, and on the first transaction `login_key_hash` and
/// `cosign_key_hash`.
fn signed_fields(
    key: &SecretKey,
    payload: CanonicalPayload,
    ctx: &Context,
) -> Result<(CanonicalPayload, serde_json::Value)> {
    use crate::key::key_hash;
    let signing = key.key_at(ctx.nonce);
    if let Some(expected) = ctx.expected_key_hash {
        if key_hash(&signing.public_key()) != expected {
            return Err(Error::Invalid(format!(
                "the chain expects a different key at nonce {}. This is not the seed of {}, or the wallet moved on",
                ctx.nonce,
                key.address()
            )));
        }
    }
    let next = key_hash(&key.key_at(ctx.nonce + 1).public_key());
    let login = (!ctx.published_login).then(|| key_hash(&key.identity_key().public_key()));
    let cosign = (!ctx.published_cosign).then(|| key_hash(&key.cosign_key(0).public_key()));
    let payload = payload.with_key_hashes(next, login, cosign);
    let mut fields = json!({
        "signature": signing.sign_hex(&payload.signing_hash()),
        "public_key": signing.public_key().to_hex(),
        "next_key_hash": hex::encode(next),
    });
    if let Some(l) = login {
        fields["login_key_hash"] = json!(hex::encode(l));
    }
    if let Some(c) = cosign {
        fields["cosign_key_hash"] = json!(hex::encode(c));
    }
    Ok((payload, fields))
}

fn merge(body: &mut serde_json::Value, fields: serde_json::Value) {
    if let (Some(b), Some(f)) = (body.as_object_mut(), fields.as_object()) {
        for (k, v) in f {
            b.insert(k.clone(), v.clone());
        }
    }
}

/// Send RTM to another wallet. `amount_rtm` is human input, e.g. `"1.5"`.
pub async fn transfer(
    client: &Client,
    key: &SecretKey,
    to: &str,
    amount_rtm: &str,
) -> Result<Submitted> {
    transfer_with(client, key, to, amount_rtm, Options::default()).await
}

/// [`transfer`] with chosen options (network, gas coin).
pub async fn transfer_with(
    client: &Client,
    key: &SecretKey,
    to: &str,
    amount_rtm: &str,
    opts: Options,
) -> Result<Submitted> {
    if !crate::address::is_valid(to) {
        return Err(Error::InvalidAddress(to.to_string()));
    }
    let from = key.address();
    let atomic = amount::parse_rtm(amount_rtm)?;
    let ctx = context(client, &from).await?;
    let payload = opts.apply(CanonicalPayload::transfer(
        &from,
        to,
        atomic,
        ctx.nonce,
        ctx.balance_claim,
    ));
    let (payload, fields) = signed_fields(key, payload, &ctx)?;
    let mut body = json!({
        "from": from,
        "to": to,
        "amount": atomic.to_string(),
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "fee_currency": payload.fee_currency_name(),
    });
    merge(&mut body, fields);
    client.submit("/api/wallet/send", &body).await
}

/// Deploy a contract.
///
/// `metadata` is the optional JSON object the node stores with the contract
/// (`name`, `symbol` and so on). It is part of the contract id, so the same code with
/// different metadata is a different contract.
///
/// Runs [`contract::preflight`] first, so a deploy the chain would refuse
/// fails here without costing gas.
pub async fn deploy(
    client: &Client,
    key: &SecretKey,
    wasm: &[u8],
    metadata: Option<&serde_json::Value>,
) -> Result<Submitted> {
    deploy_with(client, key, wasm, metadata, Options::default()).await
}

/// [`deploy`] with chosen options (network, gas coin).
pub async fn deploy_with(
    client: &Client,
    key: &SecretKey,
    wasm: &[u8],
    metadata: Option<&serde_json::Value>,
    opts: Options,
) -> Result<Submitted> {
    contract::preflight(wasm, metadata)?;
    let from = key.address();
    let id = contract::contract_id(wasm, &from, contract::DEPLOY_WEIGHT, metadata);
    let ctx = context(client, &from).await?;
    let payload = opts.apply(CanonicalPayload::deploy(
        &from,
        &id,
        ctx.nonce,
        ctx.balance_claim,
    ));
    let (payload, fields) = signed_fields(key, payload, &ctx)?;

    use base64_lite::encode as b64;
    let mut body = json!({
        "from": from,
        "wasm_base64": b64(wasm),
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "fee_currency": payload.fee_currency_name(),
    });
    merge(&mut body, fields);
    if let Some(meta) = metadata {
        body["metadata"] = meta.clone();
    }
    client.submit("/api/contract/deploy", &body).await
}

/// Call a contract method that changes state. `value_rtm` attaches RTM to the
/// call. Pass `None` for a call that sends nothing.
///
/// A contract can never take RTM out of your wallet by itself. The only money
/// it receives from you is what you attach here, and you sign that amount.
///
/// The method and arguments are checked with [`contract::check_call`] before
/// anything is signed. An argument holding `,` `(` `)` or `::` is refused.
pub async fn call(
    client: &Client,
    key: &SecretKey,
    contract_id: &str,
    method: &str,
    args: &[String],
    value_rtm: Option<&str>,
) -> Result<Submitted> {
    call_with(
        client,
        key,
        contract_id,
        method,
        args,
        value_rtm,
        Options::default(),
    )
    .await
}

/// [`call`] with chosen options (network, gas coin).
// One parameter per thing the chain signs. Grouping them would hide what is
// covered by the signature.
#[allow(clippy::too_many_arguments)]
pub async fn call_with(
    client: &Client,
    key: &SecretKey,
    contract_id: &str,
    method: &str,
    args: &[String],
    value_rtm: Option<&str>,
    opts: Options,
) -> Result<Submitted> {
    contract::check_call(method, args)?;
    let from = key.address();
    let value = match value_rtm {
        Some(v) => amount::parse_rtm(v)?,
        None => 0,
    };
    // The signed string must be exactly the one sent.
    let args_joined = args.join(",");
    let ctx = context(client, &from).await?;
    let payload = opts.apply(CanonicalPayload::call(
        &from,
        contract_id,
        method,
        &args_joined,
        value,
        ctx.nonce,
        ctx.balance_claim,
    ));
    let (payload, fields) = signed_fields(key, payload, &ctx)?;

    let mut body = json!({
        "from": from,
        "contract_id": contract_id,
        "method": method,
        "args": args,
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "fee_currency": payload.fee_currency_name(),
    });
    merge(&mut body, fields);
    if value > 0 {
        body["amount"] = json!(value.to_string());
    }
    client.submit("/api/contract/call", &body).await
}

/// A developer API key made by [`register_api_key`].
#[derive(Debug, Clone)]
pub struct NewApiKey {
    /// The secret to send as `X-API-Key`. Only its hash goes on chain, so this
    /// is the one time you see it. Keep it like a password.
    pub secret: String,
    /// `blake3(secret)` in hex, the id the chain knows the key by.
    pub key_hash: String,
    /// The registering transaction.
    pub submitted: Submitted,
}

/// The hash the chain stores for an API key secret.
pub fn api_key_hash(secret: &str) -> String {
    blake3::hash(secret.as_bytes()).to_hex().to_string()
}

/// Make a new developer API key on the free tier and register its hash.
///
/// The key works once the transaction is final. `label` is 1 to 32
/// characters and is stored on chain in plain text. The request itself goes
/// to an endpoint that wants an API key, so the client needs one already.
pub async fn register_api_key(client: &Client, key: &SecretKey, label: &str) -> Result<NewApiKey> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 32 || label.chars().any(char::is_control) {
        return Err(Error::Invalid(
            "an API key label is 1 to 32 characters with no control characters".into(),
        ));
    }
    let mut raw = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut raw);
    let secret = format!("rk_{}", hex::encode(raw));
    let key_hash = api_key_hash(&secret);
    let payload = json!({"op": "register", "key_hash": key_hash, "label": label}).to_string();
    let submitted = api_key_op(client, key, &payload, 0).await?;
    Ok(NewApiKey {
        secret,
        key_hash,
        submitted,
    })
}

/// Revoke an API key by its hash. A revoked key never works again.
pub async fn revoke_api_key(client: &Client, key: &SecretKey, key_hash: &str) -> Result<Submitted> {
    check_key_hash(key_hash)?;
    let payload = json!({"op": "revoke", "key_hash": key_hash}).to_string();
    api_key_op(client, key, &payload, 0).await
}

/// Buy a paid tier for 30 days, or extend it. The price is read from the
/// node's tier table and sent to the treasury.
pub async fn upgrade_api_key(
    client: &Client,
    key: &SecretKey,
    key_hash: &str,
    tier: &str,
) -> Result<Submitted> {
    check_key_hash(key_hash)?;
    let tiers = client.api_tiers().await?;
    let price = tiers
        .tiers
        .iter()
        .find(|t| t.name == tier)
        .map(|t| amount::parse_atomic(&t.price_atomic))
        .transpose()?
        .filter(|p| *p > 0)
        .ok_or_else(|| Error::Invalid(format!("'{tier}' is not a paid tier")))?;
    let payload = json!({"op": "tier", "key_hash": key_hash, "tier": tier}).to_string();
    api_key_op(client, key, &payload, price).await
}

fn check_key_hash(key_hash: &str) -> Result<()> {
    if key_hash.len() != 64
        || !key_hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Invalid(format!(
            "{key_hash} is not a key hash (64 lowercase hex characters)"
        )));
    }
    Ok(())
}

async fn api_key_op(
    client: &Client,
    key: &SecretKey,
    payload: &str,
    amount: u128,
) -> Result<Submitted> {
    let from = key.address();
    let ctx = context(client, &from).await?;
    // The hash covers the exact payload text, so the same string is signed
    // and sent.
    let signed = CanonicalPayload::api_key(&from, payload, amount, ctx.nonce, ctx.balance_claim);
    let (signed, fields) = signed_fields(key, signed, &ctx)?;
    let mut body = json!({
        "address": from,
        "payload": payload,
        "amount": amount.to_string(),
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": signed.declared,
        "fee_currency": signed.fee_currency_name(),
    });
    merge(&mut body, fields);
    client.submit("/api/developer/key", &body).await
}

/// Minimal base64 (RFC 4648) so the SDK carries one less dependency.
mod base64_lite {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(input: &[u8]) -> String {
        let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
        for chunk in input.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            out.push(ALPHABET[(n >> 18) as usize & 63] as char);
            out.push(ALPHABET[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                ALPHABET[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHABET[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::encode;

        #[test]
        fn matches_the_rfc_examples() {
            assert_eq!(encode(b""), "");
            assert_eq!(encode(b"f"), "Zg==");
            assert_eq!(encode(b"fo"), "Zm8=");
            assert_eq!(encode(b"foo"), "Zm9v");
            assert_eq!(encode(b"foob"), "Zm9vYg==");
            assert_eq!(encode(b"fooba"), "Zm9vYmE=");
            assert_eq!(encode(b"foobar"), "Zm9vYmFy");
            assert_eq!(encode(b"\0asm\x01\0\0\0"), "AGFzbQEAAAA=");
        }
    }
}
