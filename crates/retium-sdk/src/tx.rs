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
    /// Chain id to sign for. Every network signs as [`ChainId::Devnet`] until
    /// mainnet, the public testnet included.
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
}

async fn context(client: &Client, address: &str) -> Result<Context> {
    match client.wallet(address).await? {
        Some(w) => Ok(Context {
            nonce: w.nonce,
            balance_claim: w.balance_atomic()?,
        }),
        None => Err(Error::Invalid(format!(
            "the chain has no record of {address}, it needs funds before it can send anything"
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
    let body = json!({
        "from": from,
        "to": to,
        "amount": atomic.to_string(),
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "fee_currency": payload.fee_currency_name(),
        "signature": key.sign_hex(&payload.signing_hash()),
        "public_key": key.public_key().to_hex(),
    });
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

    use base64_lite::encode as b64;
    let mut body = json!({
        "from": from,
        "wasm_base64": b64(wasm),
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "fee_currency": payload.fee_currency_name(),
        "signature": key.sign_hex(&payload.signing_hash()),
        "public_key": key.public_key().to_hex(),
    });
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
/// anything is signed. The chain joins arguments with commas, so an argument
/// holding `,` `(` `)` or `::` is refused rather than split.
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
    // The node re-parses "id::method(args)" and hashes the comma-joined args,
    // so the string signed here must be exactly the one sent.
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

    let mut body = json!({
        "from": from,
        "contract_id": contract_id,
        "method": method,
        "args": args,
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "fee_currency": payload.fee_currency_name(),
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

/// [`stake`] with chosen options.
pub async fn stake_with(
    client: &Client,
    key: &SecretKey,
    amount_rtm: &str,
    opts: Options,
) -> Result<Submitted> {
    stake_inner(client, key, amount_rtm, true, opts).await
}

/// Release stake back to RTM.
pub async fn unstake(client: &Client, key: &SecretKey, amount_rtm: &str) -> Result<Submitted> {
    stake_inner(client, key, amount_rtm, false, Options::default()).await
}

/// [`unstake`] with chosen options.
pub async fn unstake_with(
    client: &Client,
    key: &SecretKey,
    amount_rtm: &str,
    opts: Options,
) -> Result<Submitted> {
    stake_inner(client, key, amount_rtm, false, opts).await
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
    let payload = opts.apply(if staking {
        CanonicalPayload::stake(&from, atomic, ctx.nonce, ctx.balance_claim)
    } else {
        CanonicalPayload::unstake(&from, atomic, ctx.nonce, ctx.balance_claim)
    });
    let body = json!({
        "amount": atomic.to_string(),
        "nonce": ctx.nonce,
        "sender_balance_claim": ctx.balance_claim.to_string(),
        "declared": payload.declared,
        "fee_currency": payload.fee_currency_name(),
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
