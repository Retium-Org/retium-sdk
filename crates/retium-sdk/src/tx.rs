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
//! treat a transfer as done, and expect [`Error::Busy`] now and then — it means
//! "ask again in a moment", not "failed".

use serde_json::json;

use crate::amount;
use crate::canonical::{CanonicalPayload, ChainId};
use crate::contract;
use crate::error::{Error, Result};
use crate::key::SecretKey;
use crate::rpc::{Client, Submitted};

/// Which network a transaction is signed for. The chain id is part of the
/// signature, so this must match the endpoint you send to.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Chain id to sign for.
    pub chain: ChainId,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            chain: ChainId::Devnet,
        }
    }
}

struct Context {
    nonce: u64,
    balance_claim: u128,
}

async fn context(client: &Client, address: &str) -> Result<Context> {
    match client.wallet(address).await? {
        Some(w) => Ok(Context {
            nonce: w.nonce,
            balance_claim: w.balance_atomic()?,
        }),
        None => Err(Error::Refused(format!(
            "the chain has no record of {address} — it needs funds before it can send anything"
        ))),
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

/// [`transfer`] on a chosen network.
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
    let payload = CanonicalPayload::transfer(&from, to, atomic, ctx.nonce, ctx.balance_claim)
        .on_chain(opts.chain);
    let body = json!({
        "from": from,
        "to": to,
        "amount": atomic.to_string(),
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "signature": key.sign_hex(&payload.signing_hash()),
        "public_key": key.public_key().to_hex(),
    });
    client.submit("/api/wallet/send", &body).await
}

/// Deploy a contract.
///
/// `metadata` is the optional JSON object the node stores with the contract
/// (`name`, `symbol`, …). It is part of the contract id, so the same code with
/// different metadata is a different contract.
pub async fn deploy(
    client: &Client,
    key: &SecretKey,
    wasm: &[u8],
    metadata: Option<&serde_json::Value>,
) -> Result<Submitted> {
    deploy_with(client, key, wasm, metadata, Options::default()).await
}

/// [`deploy`] on a chosen network.
pub async fn deploy_with(
    client: &Client,
    key: &SecretKey,
    wasm: &[u8],
    metadata: Option<&serde_json::Value>,
    opts: Options,
) -> Result<Submitted> {
    contract::check_deployable(wasm)?;
    let from = key.address();
    let id = contract::contract_id(wasm, &from, contract::DEPLOY_WEIGHT, metadata);
    let ctx = context(client, &from).await?;
    let payload =
        CanonicalPayload::deploy(&from, &id, ctx.nonce, ctx.balance_claim).on_chain(opts.chain);

    use base64_lite::encode as b64;
    let mut body = json!({
        "from": from,
        "wasm_base64": b64(wasm),
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "signature": key.sign_hex(&payload.signing_hash()),
        "public_key": key.public_key().to_hex(),
    });
    if let Some(meta) = metadata {
        body["metadata"] = meta.clone();
    }
    client.submit("/api/contract/deploy", &body).await
}

/// Call a contract method that changes state. `value_rtm` attaches RTM to the
/// call; pass `None` for a call that sends nothing.
///
/// A contract can never take RTM out of your wallet by itself — the only money
/// it receives from you is what you attach here, and you sign that amount.
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

/// [`call`] on a chosen network.
// One parameter per thing the chain signs; grouping them would hide what is
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
    let from = key.address();
    let value = match value_rtm {
        Some(v) => amount::parse_rtm(v)?,
        None => 0,
    };
    // The node re-parses "id::method(args)" and hashes the comma-joined args,
    // so the string signed here must be exactly the one sent.
    let args_joined = args.join(",");
    let ctx = context(client, &from).await?;
    let payload = CanonicalPayload::call(
        &from,
        contract_id,
        method,
        &args_joined,
        value,
        ctx.nonce,
        ctx.balance_claim,
    )
    .on_chain(opts.chain);

    let mut body = json!({
        "from": from,
        "contract_id": contract_id,
        "method": method,
        "args": args,
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "signature": key.sign_hex(&payload.signing_hash()),
        "public_key": key.public_key().to_hex(),
    });
    if value > 0 {
        body["amount"] = json!(value.to_string());
    }
    client.submit("/api/contract/call", &body).await
}

/// Lock RTM as stake. Staking turns RTM into PIE.
pub async fn stake(client: &Client, key: &SecretKey, amount_rtm: &str) -> Result<Submitted> {
    stake_inner(client, key, amount_rtm, true, Options::default()).await
}

/// Release stake back to RTM.
pub async fn unstake(client: &Client, key: &SecretKey, amount_rtm: &str) -> Result<Submitted> {
    stake_inner(client, key, amount_rtm, false, Options::default()).await
}

async fn stake_inner(
    client: &Client,
    key: &SecretKey,
    amount_rtm: &str,
    staking: bool,
    opts: Options,
) -> Result<Submitted> {
    let from = key.address();
    let atomic = amount::parse_rtm(amount_rtm)?;
    if atomic % amount::ATOMIC_PER_RTM != 0 {
        return Err(Error::InvalidAmount(format!(
            "{amount_rtm}: stake and unstake take whole RTM"
        )));
    }
    let ctx = context(client, &from).await?;
    let payload = if staking {
        CanonicalPayload::stake(&from, atomic, ctx.nonce, ctx.balance_claim)
    } else {
        CanonicalPayload::unstake(&from, atomic, ctx.nonce, ctx.balance_claim)
    }
    .on_chain(opts.chain);
    let body = json!({
        "amount": atomic.to_string(),
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "signature": key.sign_hex(&payload.signing_hash()),
        "public_key": key.public_key().to_hex(),
    });
    let path = if staking {
        format!("/api/wallet/{from}/stake")
    } else {
        format!("/api/wallet/{from}/unstake")
    };
    client.submit(&path, &body).await
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
