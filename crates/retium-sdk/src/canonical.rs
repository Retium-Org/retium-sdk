//! What a Retium transaction actually signs.
//!
//! A signature proves intent, including position in your own sequence (the
//! nonce) and which accounts the transaction may write to (`declared`). The
//! node rebuilds this payload from the fields you send and refuses the
//! transaction if the rebuilt payload does not match your signature, so it can
//! validate, but never rewrite, what you signed.
//!
//! Signed message = `blake3(bincode(CanonicalPayload))`.
//!
//! ```
//! use retium_sdk::{canonical::CanonicalPayload, key::SecretKey};
//! let key = SecretKey::from_seed(&[1u8; 32]);
//! let me = key.address();
//! let payload = CanonicalPayload::transfer(&me, "rete...recipient...", 1_000_000, 0, 5_000_000);
//! let signature = key.sign_hex(&payload.signing_hash());
//! # let _ = signature;
//! ```

use serde::{Deserialize, Serialize};

/// Payload version. Version 6 (October 2026) adds the three key hashes at
/// the end: one key per transaction, see [`crate::key`].
pub const VERSION: u8 = 6;

/// Gas paid in RTM (the default).
pub const FEE_RTM: u8 = 0;
/// Gas paid in RTD, Retium Dollar, at 1.5x the RTM fee.
pub const FEE_RTD: u8 = 1;

/// Chain identifiers. The chain id is signed, so a transaction made for one
/// network can never be replayed on another.
///
/// The public testnet at `rpc.retium.org` uses [`ChainId::Devnet`], the
/// default everywhere in this SDK.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ChainId {
    /// The live network.
    Mainnet = 1,
    /// Reserved.
    Testnet = 2,
    /// The public testnet.
    Devnet = 3,
}

impl ChainId {
    /// The number that goes into the signed payload.
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Parse `mainnet` / `testnet` / `devnet`, or a bare number.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "1" | "mainnet" => Some(Self::Mainnet),
            "2" | "testnet" => Some(Self::Testnet),
            "3" | "devnet" => Some(Self::Devnet),
            _ => None,
        }
    }
}

/// The ledger account that collects gas.
pub const TREASURY: &str = "rete_treasury";
/// Recipient written into an API key payload.
pub const API_KEY_REGISTRY: &str = "API_KEY_REGISTRY";

/// The exact struct the node serialises with bincode before hashing. Field
/// order is part of the format, so do not reorder.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CanonicalPayload {
    /// Payload version: [`VERSION`].
    pub version: u8,
    /// Which network this is for.
    pub chain_id: u32,
    /// `transfer`, `deploy`, `call`, `api_key` and so on.
    pub action: String,
    /// Your address.
    pub sender: String,
    /// The other side, which is a wallet, a contract id or a reserved name.
    pub recipient: String,
    /// Atomic units moved by this transaction.
    pub amount: u128,
    /// Your next sequence number.
    pub nonce: u64,
    /// Your own confirmed balance when you signed, in atomic units. Keepers
    /// check their view is at least this, which makes their result independent
    /// of when other transactions arrive.
    pub sender_balance_claim: u128,
    /// Contract method for `call`, empty otherwise.
    pub method: String,
    /// blake3 of the call arguments, of the endpoint, or zeros.
    pub args_hash: [u8; 32],
    /// Accounts this transaction may write to, sorted and de-duplicated.
    pub declared: Vec<String>,
    /// The coin the gas is paid in: [`FEE_RTM`] or [`FEE_RTD`]. Signed, so a
    /// relay cannot change it.
    pub fee_currency: u8,
    /// blake3 of the public key you will sign your NEXT transaction with
    /// (key `nonce + 1` of your seed, [`crate::key::SecretKey::key_at`]). The
    /// chain keeps this hash and accepts only that key next time. Zeros only
    /// for a multisig vault, whose members rotate instead.
    pub next_key_hash: [u8; 32],
    /// blake3 of your identity key ([`crate::key::SecretKey::identity_key`]),
    /// published once; zeros to leave it unchanged.
    pub login_key_hash: [u8; 32],
    /// blake3 of the first key of your co-signing chain
    /// ([`crate::key::SecretKey::cosign_key`] at counter 0), published once;
    /// zeros to leave it unchanged.
    pub cosign_key_hash: [u8; 32],
}

impl CanonicalPayload {
    // One parameter per signed field. Grouping them would hide what the
    // signature covers, which is the whole point of this type.
    #[allow(clippy::too_many_arguments)]
    fn new(
        action: &str,
        sender: &str,
        recipient: &str,
        amount: u128,
        nonce: u64,
        balance_claim: u128,
        method: String,
        args_hash: [u8; 32],
        declared: Vec<String>,
    ) -> Self {
        Self {
            version: VERSION,
            chain_id: ChainId::Devnet.as_u32(),
            action: action.to_string(),
            sender: sender.to_string(),
            recipient: recipient.to_string(),
            amount,
            nonce,
            sender_balance_claim: balance_claim,
            method,
            args_hash,
            declared: normalize(declared),
            fee_currency: FEE_RTM,
            next_key_hash: [0u8; 32],
            login_key_hash: [0u8; 32],
            cosign_key_hash: [0u8; 32],
        }
    }

    /// The key hashes a transaction announces: the hash of the key due at
    /// `nonce + 1`, and (once) the identity and co-signing key hashes.
    pub fn with_key_hashes(
        mut self,
        next_key_hash: [u8; 32],
        login_key_hash: Option<[u8; 32]>,
        cosign_key_hash: Option<[u8; 32]>,
    ) -> Self {
        self.next_key_hash = next_key_hash;
        self.login_key_hash = login_key_hash.unwrap_or([0u8; 32]);
        self.cosign_key_hash = cosign_key_hash.unwrap_or([0u8; 32]);
        self
    }

    /// Pay the gas in RTD instead of RTM ([`FEE_RTD`]), or back ([`FEE_RTM`]).
    pub fn with_fee_currency(mut self, fee_currency: u8) -> Self {
        self.fee_currency = fee_currency;
        self
    }

    /// The fee coin as the request body names it, "RTM" or "RTD".
    pub fn fee_currency_name(&self) -> &'static str {
        if self.fee_currency == FEE_RTD {
            "RTD"
        } else {
            "RTM"
        }
    }

    /// Send RTM to another wallet.
    pub fn transfer(
        sender: &str,
        recipient: &str,
        amount: u128,
        nonce: u64,
        balance_claim: u128,
    ) -> Self {
        Self::new(
            "transfer",
            sender,
            recipient,
            amount,
            nonce,
            balance_claim,
            String::new(),
            [0u8; 32],
            vec![sender.into(), recipient.into(), TREASURY.into()],
        )
    }

    /// Deploy a contract. `contract_id` is the id the code will have. See
    /// [`crate::contract::contract_id`].
    pub fn deploy(sender: &str, contract_id: &str, nonce: u64, balance_claim: u128) -> Self {
        Self::new(
            "deploy",
            sender,
            contract_id,
            0,
            nonce,
            balance_claim,
            String::new(),
            [0u8; 32],
            vec![sender.into(), contract_id.into(), TREASURY.into()],
        )
    }

    /// Call a contract method. `args` is the exact argument string you send.
    /// `amount` is RTM attached to the call, 0 for none.
    pub fn call(
        sender: &str,
        contract_id: &str,
        method: &str,
        args: &str,
        amount: u128,
        nonce: u64,
        balance_claim: u128,
    ) -> Self {
        // A call's write set is not known in advance, so it is signed empty and
        // the node derives it.
        Self::new(
            "call",
            sender,
            contract_id,
            amount,
            nonce,
            balance_claim,
            method.to_string(),
            *blake3::hash(args.as_bytes()).as_bytes(),
            Vec::new(),
        )
    }

    /// Register, rename, revoke or upgrade a developer API key. `payload` is
    /// the exact JSON text sent with the request, and `amount` is the tier
    /// price for an upgrade, 0 otherwise.
    pub fn api_key(
        sender: &str,
        payload: &str,
        amount: u128,
        nonce: u64,
        balance_claim: u128,
    ) -> Self {
        Self::new(
            "api_key",
            sender,
            API_KEY_REGISTRY,
            amount,
            nonce,
            balance_claim,
            String::new(),
            *blake3::hash(payload.as_bytes()).as_bytes(),
            vec![sender.into(), TREASURY.into()],
        )
    }

    /// Build for a network other than the default. The chain id is signed, so
    /// this must match the network you send to.
    pub fn on_chain(mut self, chain: ChainId) -> Self {
        self.chain_id = chain.as_u32();
        self
    }

    /// The 32 bytes to sign.
    pub fn signing_hash(&self) -> [u8; 32] {
        let bytes = bincode::serialize(self).expect("CanonicalPayload always serialises");
        *blake3::hash(&bytes).as_bytes()
    }
}

/// Sorted and de-duplicated, the form the node compares against.
pub fn normalize(mut list: Vec<String>) -> Vec<String> {
    list.sort();
    list.dedup();
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "rete0000000000000000000000000000000000000000000000000000000000000000dead";
    const YOU: &str = "rete1111111111111111111111111111111111111111111111111111111111111111beef";

    #[test]
    fn declared_is_sorted_and_deduplicated() {
        let p = CanonicalPayload::transfer(ME, YOU, 1, 0, 1);
        let mut expected = vec![ME.to_string(), YOU.to_string(), TREASURY.to_string()];
        expected.sort();
        assert_eq!(p.declared, expected);

        let self_send = CanonicalPayload::transfer(ME, ME, 1, 0, 1);
        assert_eq!(
            self_send.declared.len(),
            2,
            "sender and recipient collapse to one entry"
        );
    }

    #[test]
    fn a_call_declares_nothing_and_hashes_its_arguments() {
        let p = CanonicalPayload::call(ME, "contract", "mint", "[\"10\"]", 0, 1, 0);
        assert!(p.declared.is_empty(), "the node derives a call's write set");
        assert_eq!(p.args_hash, *blake3::hash(b"[\"10\"]").as_bytes());
        assert_eq!(p.method, "mint");
    }

    #[test]
    fn every_signed_field_changes_the_hash() {
        let base = CanonicalPayload::transfer(ME, YOU, 10, 1, 100);
        let h = base.signing_hash();

        let mut amount = base.clone();
        amount.amount = 11;
        let mut nonce = base.clone();
        nonce.nonce = 2;
        let mut claim = base.clone();
        claim.sender_balance_claim = 101;
        let chain = base.clone().on_chain(ChainId::Mainnet);
        let mut recipient = base.clone();
        recipient.recipient = ME.into();

        for (name, other) in [
            ("amount", amount),
            ("nonce", nonce),
            ("balance claim", claim),
            ("chain", chain),
            ("recipient", recipient),
        ] {
            assert_ne!(
                h,
                other.signing_hash(),
                "{name} must be covered by the signature"
            );
        }
    }

    #[test]
    fn the_same_payload_always_hashes_the_same() {
        let a = CanonicalPayload::transfer(ME, YOU, 10, 1, 100);
        let b = CanonicalPayload::transfer(ME, YOU, 10, 1, 100);
        assert_eq!(a.signing_hash(), b.signing_hash());
    }

    #[test]
    fn chain_ids_parse() {
        assert_eq!(ChainId::parse("testnet"), Some(ChainId::Testnet));
        assert_eq!(ChainId::parse("3"), Some(ChainId::Devnet));
        assert_eq!(ChainId::parse("nope"), None);
    }
}
