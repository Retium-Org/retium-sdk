//! Contract identity and RCP-1 metadata.
//!
//! A contract's id is not assigned by the chain — it is derived from the code
//! itself, who deploys it, and the metadata declared inside the wasm:
//!
//! ```text
//! contract_id = hex(blake3( wasm ‖ deployer ‖ [weight] ‖ name? ‖ symbol? ‖ rcp_type? ‖ traits… ))
//! ```
//!
//! Two consequences worth understanding before you deploy:
//!
//! * You can compute the id **before** you send anything, and the payload you
//!   sign names that id. Deploying the same bytes twice from the same account
//!   with the same metadata is refused — the id already exists.
//! * Anyone can deploy code with the same name or symbol as yours. Only the
//!   code hash is computed by the chain; `name`, `symbol`, `traits` and
//!   `capabilities` are declared by whoever deploys, so a wallet or explorer
//!   that trusts a symbol is trusting the deployer, not the chain. Identify
//!   contracts by id or code hash.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The largest contract the chain accepts, in bytes (1 MiB). A deploy above
/// this is refused with HTTP 413; check locally before you upload.
pub const MAX_WASM_BYTES: usize = 1024 * 1024;

/// Gas weight the RPC deploy path always uses.
pub const DEPLOY_WEIGHT: u8 = 5;

/// The marker the node looks for inside the wasm, followed by JSON and a NUL byte.
pub const METADATA_MARKER: &[u8] = b"RCP_METADATA:";

/// RCP-1 metadata, as the node reads it out of the wasm.
///
/// `weight` must match what the node's own analysis of your code expects
/// (within one step) or the deploy fails, so it is not a free dial: it is your
/// honest declaration of how heavy a call is, and it sets the gas price users
/// pay.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// What kind of contract this is, e.g. `"token"`, `"vault"`.
    pub rcp_type: String,
    /// Free-form markers other tools can read, e.g. `"fungible"`.
    #[serde(default)]
    pub traits: Vec<String>,
    /// What the contract can do, e.g. `"execute"`, `"transfer"`.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Gas weight 1–5.
    pub weight: u8,
    /// Your version string, e.g. `"v1.0.0"`.
    pub version: String,
}

impl Metadata {
    /// Check the rules the node checks at deploy time, so you find out here
    /// rather than after paying gas.
    pub fn validate(&self) -> Result<()> {
        if self.rcp_type.trim().is_empty() {
            return Err(Error::Refused(
                "RCP-1 metadata needs a non-empty rcp_type".into(),
            ));
        }
        if self.version.trim().is_empty() {
            return Err(Error::Refused(
                "RCP-1 metadata needs a non-empty version".into(),
            ));
        }
        if !(1..=5).contains(&self.weight) {
            return Err(Error::Refused(format!(
                "RCP-1 weight must be 1–5, got {}",
                self.weight
            )));
        }
        if self.traits.is_empty() && self.capabilities.is_empty() {
            return Err(Error::Refused(
                "RCP-1 metadata needs at least one trait or capability".into(),
            ));
        }
        Ok(())
    }
}

/// Read the RCP-1 metadata embedded in a wasm module, if it has any.
///
/// A contract without metadata still deploys — the node falls back to a
/// minimal record — but tools cannot tell what it is, so declare it.
pub fn read_metadata(wasm: &[u8]) -> Option<Metadata> {
    let pos = wasm
        .windows(METADATA_MARKER.len())
        .position(|w| w == METADATA_MARKER)?;
    let rest = &wasm[pos + METADATA_MARKER.len()..];
    let end = rest.iter().position(|&b| b == 0)?;
    serde_json::from_slice(&rest[..end]).ok()
}

/// The id the chain will give this deploy.
///
/// `user_metadata` is the JSON object you send in the deploy request (the
/// optional `name`, `symbol`, `rcp_type` and `traits` are part of the id).
/// Pass `None` if you send no metadata.
pub fn contract_id(
    wasm: &[u8],
    deployer: &str,
    weight: u8,
    user_metadata: Option<&serde_json::Value>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(wasm);
    hasher.update(deployer.as_bytes());
    hasher.update(&[weight]);
    if let Some(meta) = user_metadata {
        for field in ["name", "symbol", "rcp_type"] {
            if let Some(value) = meta.get(field).and_then(|v| v.as_str()) {
                hasher.update(value.as_bytes());
            }
        }
        if let Some(traits) = meta.get("traits").and_then(|v| v.as_array()) {
            for t in traits {
                if let Some(t) = t.as_str() {
                    hasher.update(t.as_bytes());
                }
            }
        }
    }
    hasher.finalize().to_hex().to_string()
}

/// The chain's identity for a piece of code: `blake3(wasm)`, hex.
///
/// This is computed by the node, not declared by you. It is how anyone checks
/// that a deployed contract is the code you published.
pub fn code_hash(wasm: &[u8]) -> String {
    blake3::hash(wasm).to_hex().to_string()
}

/// Refuse a module the chain would refuse anyway.
pub fn check_deployable(wasm: &[u8]) -> Result<()> {
    if wasm.len() > MAX_WASM_BYTES {
        return Err(Error::Refused(format!(
            "contract is {} bytes; the limit is {} ({} KiB). Build with --release and strip debug info",
            wasm.len(),
            MAX_WASM_BYTES,
            MAX_WASM_BYTES / 1024
        )));
    }
    if wasm.len() < 8 || &wasm[..4] != b"\0asm" {
        return Err(Error::Refused("that file is not a wasm module".into()));
    }
    if let Some(meta) = read_metadata(wasm) {
        meta.validate()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wasm_with(meta: &str) -> Vec<u8> {
        let mut v = b"\0asm\x01\0\0\0".to_vec();
        v.extend_from_slice(METADATA_MARKER);
        v.extend_from_slice(meta.as_bytes());
        v.push(0);
        v
    }

    #[test]
    fn the_id_depends_on_code_deployer_weight_and_metadata() {
        let wasm = b"\0asm\x01\0\0\0".to_vec();
        let a = contract_id(&wasm, "rete-alice", 5, None);
        assert_eq!(a.len(), 64);
        assert_ne!(a, contract_id(&wasm, "rete-bob", 5, None), "deployer");
        assert_ne!(a, contract_id(&wasm, "rete-alice", 4, None), "weight");
        assert_ne!(
            a,
            contract_id(b"\0asm\x01\0\0\x01", "rete-alice", 5, None),
            "code"
        );
        let meta = serde_json::json!({"name": "Coin", "symbol": "CNC"});
        assert_ne!(
            a,
            contract_id(&wasm, "rete-alice", 5, Some(&meta)),
            "metadata"
        );
    }

    #[test]
    fn the_same_inputs_always_give_the_same_id() {
        let wasm = wasm_with(
            r#"{"rcp_type":"token","traits":["fungible"],"capabilities":["execute"],"weight":5,"version":"v1"}"#,
        );
        let meta = serde_json::json!({"name": "Coin"});
        assert_eq!(
            contract_id(&wasm, "rete-alice", 5, Some(&meta)),
            contract_id(&wasm, "rete-alice", 5, Some(&meta))
        );
    }

    #[test]
    fn metadata_is_read_back_out_of_the_module() {
        let wasm = wasm_with(
            r#"{"rcp_type":"token","traits":["fungible"],"capabilities":["execute"],"weight":3,"version":"v1.0.0"}"#,
        );
        let meta = read_metadata(&wasm).expect("metadata");
        assert_eq!(meta.rcp_type, "token");
        assert_eq!(meta.weight, 3);
        assert!(meta.validate().is_ok());
    }

    #[test]
    fn bad_metadata_is_caught_before_paying_gas() {
        let bad = wasm_with(
            r#"{"rcp_type":"token","traits":[],"capabilities":[],"weight":9,"version":""}"#,
        );
        assert!(check_deployable(&bad).is_err());
    }

    #[test]
    fn oversized_and_non_wasm_files_are_refused() {
        assert!(check_deployable(&vec![0u8; MAX_WASM_BYTES + 1]).is_err());
        assert!(check_deployable(b"not wasm at all").is_err());
        assert!(check_deployable(b"\0asm\x01\0\0\0").is_ok());
    }
}
