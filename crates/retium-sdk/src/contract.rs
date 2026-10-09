//! Contract identity, RCP-1 metadata, and the checks the chain runs on a deploy
//! and a call.
//!
//! A contract's id is not assigned by the chain. It is derived from the code
//! itself, who deploys it, and the metadata sent with the deploy:
//!
//! ```text
//! contract_id = hex(blake3( wasm || deployer || [weight] || name? || symbol? || rcp_type? || traits... ))
//! ```
//!
//! Two consequences worth understanding before you deploy:
//!
//! * You can compute the id **before** you send anything, and the payload you
//!   sign names that id. Deploying the same bytes twice from the same account
//!   with the same metadata is refused, because the id already exists.
//! * Anyone can deploy code with the same name or symbol as yours. Only the
//!   code hash is computed by the chain. `name`, `symbol`, `traits` and
//!   `capabilities` are declared by whoever deploys, so a wallet or explorer
//!   that trusts a symbol is trusting the deployer, not the chain. Identify
//!   contracts by id or code hash.
//!
//! [`preflight`] runs every check the chain runs on a deploy that can be run
//! without the chain, so a deploy that would be refused fails here, before it
//! costs gas.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The largest contract the chain accepts, in bytes (1 MiB). A deploy above
/// this is refused with HTTP 413, so check locally before you upload.
pub const MAX_WASM_BYTES: usize = 1024 * 1024;

/// Gas weight the RPC deploy path always uses.
pub const DEPLOY_WEIGHT: u8 = 5;

/// The largest `metadata` object the public endpoint forwards with a deploy, in
/// bytes of compact JSON.
pub const MAX_DEPLOY_METADATA_BYTES: usize = 8 * 1024;

/// The highest `decimals` a token may declare.
pub const MAX_DECIMALS: u32 = 38;

/// The marker the node looks for inside the wasm, followed by JSON and a NUL byte.
pub const METADATA_MARKER: &[u8] = b"RCP_METADATA:";

/// The function every contract must export. The chain calls it for every
/// action and the contract crate's `entrypoint!` defines it for you.
pub const ENTRY_EXPORT: &str = "on_use";

/// RCP-1 metadata, as the node reads it out of the wasm.
///
/// All seven fields are required, exactly as the node parses them. A field
/// left out makes the whole deploy fail. Write empty values where you have
/// nothing to say (`"lockable_fields":[]`, `"evolves_from":""`).
///
/// `weight` is checked against the chain's own analysis of your code (see
/// [`simulated_weight`]). It must be that number or one above it. For a Rust
/// contract that is nearly always 5.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// What kind of contract this is, e.g. `"token"`, `"vault"`.
    pub rcp_type: String,
    /// Free-form markers other tools can read, e.g. `"fungible"`.
    pub traits: Vec<String>,
    /// What the contract can do, e.g. `"execute"`, `"transfer"`.
    pub capabilities: Vec<String>,
    /// Fields the contract promises never to change once set.
    pub lockable_fields: Vec<String>,
    /// The id of the contract this one replaces, or `""`.
    pub evolves_from: String,
    /// Gas weight 1 to 5.
    pub weight: u8,
    /// Your version string, e.g. `"v1.0.0"`.
    pub version: String,
}

impl Metadata {
    /// The rules the node applies to every metadata record.
    pub fn validate(&self) -> Result<()> {
        if !(1..=5).contains(&self.weight) {
            return Err(Error::Invalid(format!(
                "RCP-1 weight must be 1-5, got {}",
                self.weight
            )));
        }
        if self.traits.is_empty() && self.capabilities.is_empty() {
            return Err(Error::Invalid(
                "RCP-1 metadata needs at least one trait or capability".into(),
            ));
        }
        if self.rcp_type.is_empty() {
            return Err(Error::Invalid(
                "RCP-1 metadata needs a non-empty rcp_type".into(),
            ));
        }
        if self.version.is_empty() {
            return Err(Error::Invalid(
                "RCP-1 metadata needs a non-empty version".into(),
            ));
        }
        Ok(())
    }

    /// What the node records for a contract that embeds no metadata.
    fn fallback() -> Self {
        Self {
            rcp_type: "Unknown".into(),
            traits: Vec::new(),
            capabilities: vec!["execute".into()],
            lockable_fields: Vec::new(),
            evolves_from: String::new(),
            weight: DEPLOY_WEIGHT,
            version: "v0.0.0".into(),
        }
    }
}

/// Read the RCP-1 metadata embedded in a wasm module.
///
/// `Ok(None)` when the module has no marker (or no NUL after it). The node then
/// records a minimal `"Unknown"` contract. `Err` when there is a marker but the
/// JSON after it is not what the node accepts, and the node refuses that deploy.
pub fn read_metadata(wasm: &[u8]) -> Result<Option<Metadata>> {
    let Some(pos) = wasm
        .windows(METADATA_MARKER.len())
        .position(|w| w == METADATA_MARKER)
    else {
        return Ok(None);
    };
    let rest = &wasm[pos + METADATA_MARKER.len()..];
    let Some(end) = rest.iter().position(|&b| b == 0) else {
        return Ok(None);
    };
    let text = std::str::from_utf8(&rest[..end])
        .map_err(|_| Error::Invalid("embedded RCP-1 metadata is not UTF-8".into()))?;
    let meta: Metadata = serde_json::from_str(text)
        .map_err(|e| Error::Invalid(format!("embedded RCP-1 metadata is not valid ({e})")))?;
    meta.validate()?;
    Ok(Some(meta))
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

/// The chain's identity for a piece of code, `blake3(wasm)` in hex.
///
/// This is computed by the node, not declared by you. It is how anyone checks
/// that a deployed contract is the code you published.
pub fn code_hash(wasm: &[u8]) -> String {
    blake3::hash(wasm).to_hex().to_string()
}

/// The weight the chain computes for this code, 1 to 5.
///
/// The same opcode analysis the node runs. Instructions, loops, calls, memory
/// operations and branches are scored and mapped onto 1 to 5. A module that does not
/// parse scores 5.
pub fn simulated_weight(wasm: &[u8]) -> u8 {
    use wasmparser::{Operator, Parser, Payload};

    let (mut total, mut loops, mut calls, mut memory, mut branches) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    for payload in Parser::new(0).parse_all(wasm) {
        match payload {
            Ok(Payload::CodeSectionEntry(body)) => {
                // The node reads the whole body, local declarations included,
                // as operators. Read it the same way so the score is the same.
                let mut reader = body.get_binary_reader();
                while !reader.eof() {
                    let before = reader.current_position();
                    let Ok(op) = reader.read_operator() else {
                        if reader.current_position() == before {
                            break;
                        }
                        continue;
                    };
                    total += 1;
                    match op {
                        Operator::Loop { .. } | Operator::Block { .. } | Operator::If { .. } => {
                            loops += 1
                        }
                        Operator::Br { .. } | Operator::BrIf { .. } | Operator::BrTable { .. } => {
                            branches += 1
                        }
                        Operator::Call { .. } | Operator::CallIndirect { .. } => calls += 1,
                        Operator::I32Load { .. }
                        | Operator::I64Load { .. }
                        | Operator::F32Load { .. }
                        | Operator::F64Load { .. }
                        | Operator::I32Load8S { .. }
                        | Operator::I32Load8U { .. }
                        | Operator::I32Load16S { .. }
                        | Operator::I32Load16U { .. }
                        | Operator::I64Load8S { .. }
                        | Operator::I64Load8U { .. }
                        | Operator::I64Load16S { .. }
                        | Operator::I64Load16U { .. }
                        | Operator::I64Load32S { .. }
                        | Operator::I64Load32U { .. }
                        | Operator::I32Store { .. }
                        | Operator::I64Store { .. }
                        | Operator::F32Store { .. }
                        | Operator::F64Store { .. }
                        | Operator::I32Store8 { .. }
                        | Operator::I32Store16 { .. }
                        | Operator::I64Store8 { .. }
                        | Operator::I64Store16 { .. }
                        | Operator::I64Store32 { .. }
                        | Operator::MemorySize { .. }
                        | Operator::MemoryGrow { .. } => memory += 1,
                        _ => {}
                    }
                }
            }
            Ok(_) => {}
            Err(_) => return 5,
        }
    }
    let score = total / 100 + loops * 5 + calls * 3 + memory * 2 + branches;
    match score {
        0..=20 => 1,
        21..=100 => 2,
        101..=300 => 3,
        301..=600 => 4,
        _ => 5,
    }
}

/// Does the module parse, and does it export the function the chain calls?
fn check_module(wasm: &[u8]) -> Result<()> {
    use wasmparser::{ExternalKind, Parser, Payload};

    let mut has_entry = false;
    for payload in Parser::new(0).parse_all(wasm) {
        match payload {
            Ok(Payload::ExportSection(exports)) => {
                for export in exports {
                    let export = export
                        .map_err(|e| Error::Invalid(format!("the wasm does not parse ({e})")))?;
                    if export.name == ENTRY_EXPORT && export.kind == ExternalKind::Func {
                        has_entry = true;
                    }
                }
            }
            Ok(_) => {}
            Err(e) => return Err(Error::Invalid(format!("the wasm does not parse ({e})"))),
        }
    }
    if !has_entry {
        return Err(Error::Invalid(format!(
            "the wasm has no '{ENTRY_EXPORT}' export. Contracts built on retium-contract get it from entrypoint!"
        )));
    }
    Ok(())
}

/// What the chain will make of a deploy, worked out before you send it.
#[derive(Debug, Clone)]
pub struct Preflight {
    /// `blake3` of the code.
    pub code_hash: String,
    /// The weight the chain computes for the code.
    pub simulated_weight: u8,
    /// True when the wasm carries its own RCP-1 metadata.
    pub embedded: bool,
    /// The record the chain will store, meaning the embedded metadata (or the
    /// minimal fallback) with the deploy request's fields applied over it.
    pub metadata: Metadata,
}

/// Run every check the chain runs on a deploy that does not need the chain.
///
/// `deploy_metadata` is the `metadata` object you send with the deploy, if
/// any. Its fields override the embedded ones the way the node applies them.
/// `name` replaces `rcp_type`. `traits`, `capabilities`, `weight` and
/// `version` replace theirs. `symbol` and `decimals` are added as
/// `symbol:` and `decimals:` traits.
///
/// Two things need the chain and are not checked here. An `evolves_from`
/// parent must exist and be deployed by you, and the id must not be taken.
pub fn preflight(wasm: &[u8], deploy_metadata: Option<&serde_json::Value>) -> Result<Preflight> {
    if wasm.len() > MAX_WASM_BYTES {
        return Err(Error::Invalid(format!(
            "contract is {} bytes, the limit is {} ({} KiB). Build with --release and strip debug info",
            wasm.len(),
            MAX_WASM_BYTES,
            MAX_WASM_BYTES / 1024
        )));
    }
    if wasm.len() < 8 || &wasm[..4] != b"\0asm" {
        return Err(Error::Invalid("that file is not a wasm module".into()));
    }
    if let Some(meta) = deploy_metadata {
        if !meta.is_object() {
            return Err(Error::Invalid(
                "deploy metadata must be a JSON object".into(),
            ));
        }
        let len = meta.to_string().len();
        if len > MAX_DEPLOY_METADATA_BYTES {
            return Err(Error::Invalid(format!(
                "deploy metadata is {len} bytes, the limit is {MAX_DEPLOY_METADATA_BYTES}"
            )));
        }
    }
    check_module(wasm)?;

    let simulated = simulated_weight(wasm);
    let embedded = read_metadata(wasm)?;
    let is_embedded = embedded.is_some();
    let mut metadata = embedded.unwrap_or_else(Metadata::fallback);

    // The deploy transaction itself is weight 5. Without deploy metadata its
    // payload is the bare code, which the chain scores, so the code must score
    // 4 or 5.
    if deploy_metadata.is_none() && simulated + 1 < DEPLOY_WEIGHT {
        return Err(Error::Invalid(format!(
            "the chain scores this code at weight {simulated}. A deploy without metadata is weight {DEPLOY_WEIGHT}, \
             which only code scoring 4 or 5 may use. Send deploy metadata with weight {simulated}"
        )));
    }

    let mut raw_texts = Vec::new();
    if let Some(user) = deploy_metadata {
        apply_deploy_metadata(&mut metadata, user)?;
        for key in ["name", "symbol"] {
            if let Some(s) = user.get(key).and_then(|v| v.as_str()) {
                raw_texts.push(s.to_string());
            }
        }
    }

    if let Some(bad) = std::iter::once(&metadata.rcp_type)
        .chain(&metadata.traits)
        .chain(&metadata.capabilities)
        .chain(&metadata.lockable_fields)
        .chain(std::iter::once(&metadata.version))
        .chain(std::iter::once(&metadata.evolves_from))
        .chain(&raw_texts)
        .find(|s| has_hidden_chars(s))
    {
        return Err(Error::Invalid(format!(
            "{bad:?} contains invisible or text-direction characters"
        )));
    }
    if let Some(bad) = std::iter::once(metadata.rcp_type.as_str())
        .chain(
            metadata
                .traits
                .iter()
                .filter_map(|t| t.strip_prefix("symbol:")),
        )
        .chain(raw_texts.iter().map(String::as_str))
        .find(|s| is_reserved_name(s))
    {
        return Err(Error::Invalid(format!(
            "the name {bad:?} is reserved for the chain's own coins"
        )));
    }

    metadata.validate()?;
    if metadata.weight < simulated || metadata.weight > simulated + 1 {
        return Err(Error::Invalid(format!(
            "declared weight {} does not fit this code. The chain scores it at {simulated}, \
             so the weight must be {simulated}{}",
            metadata.weight,
            if simulated < 5 {
                format!(" or {}", simulated + 1)
            } else {
                String::new()
            }
        )));
    }

    Ok(Preflight {
        code_hash: code_hash(wasm),
        simulated_weight: simulated,
        embedded: is_embedded,
        metadata,
    })
}

/// Refuse a module the chain would refuse anyway. [`preflight`] with no
/// deploy metadata.
pub fn check_deployable(wasm: &[u8]) -> Result<()> {
    preflight(wasm, None).map(|_| ())
}

/// The deploy request's metadata applied over the embedded record, in the
/// node's order.
fn apply_deploy_metadata(meta: &mut Metadata, user: &serde_json::Value) -> Result<()> {
    if let Some(name) = user.get("name").and_then(|v| v.as_str()) {
        if !name.is_empty() {
            meta.rcp_type = name.to_string();
        }
    }
    if let Some(traits) = user.get("traits").and_then(|v| v.as_array()) {
        let mut merged: Vec<String> = traits
            .iter()
            .filter_map(|t| t.as_str())
            .filter(|t| !t.starts_with("symbol:") && !t.starts_with("decimals:"))
            .map(str::to_string)
            .collect();
        if !merged.is_empty() {
            if meta.traits.iter().any(|t| t == "fungible")
                && !merged.iter().any(|t| t == "fungible")
            {
                merged.push("fungible".into());
            }
            meta.traits = merged;
        }
    }
    if let Some(symbol) = user.get("symbol").and_then(|v| v.as_str()) {
        if !symbol.is_empty() {
            meta.traits.retain(|t| !t.starts_with("symbol:"));
            meta.traits.push(format!("symbol:{symbol}"));
        }
    }
    if let Some(caps) = user.get("capabilities").and_then(|v| v.as_array()) {
        let caps: Vec<String> = caps
            .iter()
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();
        if !caps.is_empty() {
            meta.capabilities = caps;
        }
    }
    if let Some(d) = user.get("decimals").and_then(|v| {
        v.as_str()
            .and_then(|s| s.parse::<u32>().ok())
            .or_else(|| v.as_u64().and_then(|n| u32::try_from(n).ok()))
    }) {
        if d > MAX_DECIMALS {
            return Err(Error::Invalid(format!(
                "decimals {d} out of range (0-{MAX_DECIMALS})"
            )));
        }
        meta.traits.retain(|t| !t.starts_with("decimals:"));
        meta.traits.push(format!("decimals:{d}"));
    }
    if let Some(weight) = user.get("weight").and_then(|v| v.as_u64()) {
        // The node stores it as one byte. A larger number wraps there and is
        // refused here.
        meta.weight = u8::try_from(weight).unwrap_or(u8::MAX);
    }
    if let Some(version) = user.get("version").and_then(|v| v.as_str()) {
        meta.version = version.to_string();
    }
    Ok(())
}

/// Characters that are invisible or reorder text. The chain refuses them in
/// every name, trait and version it shows to users.
fn has_hidden_chars(s: &str) -> bool {
    s.chars().any(|c| {
        c.is_control()
            || matches!(c,
                '\u{00AD}' | '\u{0600}'..='\u{0605}' | '\u{061C}' | '\u{06DD}' | '\u{070F}'
                | '\u{0890}'..='\u{0891}' | '\u{08E2}' | '\u{180E}'
                | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}'
                | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}'
                | '\u{110BD}' | '\u{110CD}' | '\u{13430}'..='\u{1343F}'
                | '\u{1BCA0}'..='\u{1BCA3}' | '\u{1D173}'..='\u{1D17A}'
                | '\u{2028}' | '\u{2029}'
                | '\u{034F}' | '\u{115F}'..='\u{1160}' | '\u{17B4}'..='\u{17B5}'
                | '\u{180B}'..='\u{180D}' | '\u{180F}' | '\u{3164}' | '\u{FE00}'..='\u{FE0F}'
                | '\u{FFA0}' | '\u{FFF0}'..='\u{FFF8}' | '\u{E0000}'..='\u{E0FFF}')
    })
}

/// RTM, RTD and Retium Dollar belong to the chain. Compared on letters and
/// digits only, lower case, with full-width forms and the Cyrillic and Greek
/// letters that print like Latin ones folded.
fn is_reserved_name(s: &str) -> bool {
    let folded: String = s
        .chars()
        .map(|c| match c {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            'А' | 'а' | 'Α' | 'α' => 'a',
            'В' | 'в' | 'Β' | 'β' => 'b',
            'Е' | 'е' | 'Ε' | 'ε' => 'e',
            'І' | 'і' | 'Ι' | 'ι' | 'Ӏ' => 'i',
            'М' | 'м' | 'Μ' | 'μ' => 'm',
            'О' | 'о' | 'Ο' | 'ο' => 'o',
            'Р' | 'р' | 'Ρ' | 'ρ' => 'p',
            'Т' | 'т' | 'Τ' | 'τ' => 't',
            'У' | 'у' | 'Υ' | 'υ' => 'u',
            other => other,
        })
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    matches!(folded.as_str(), "rtd" | "rtm" | "retiumdollar")
}

/// Check a method name and its arguments before they are signed.
///
/// An argument may not hold `,` `(` `)` or `::`, and a method name is letters,
/// digits and `_`. A call with exactly one empty argument is refused too.
///
/// Need to pass structured data? Use several plain arguments, or encode it
/// (hex, for example) and decode it in the contract.
pub fn check_call(method: &str, args: &[String]) -> Result<()> {
    if method.is_empty()
        || !method
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::Invalid(format!(
            "method {method:?}: use letters, digits and _ only"
        )));
    }
    for (i, arg) in args.iter().enumerate() {
        if arg.contains(',') || arg.contains('(') || arg.contains(')') || arg.contains("::") {
            return Err(Error::Invalid(format!(
                "argument {} ({arg:?}) contains , ( ) or ::, which are not allowed in an argument; \
                 split it into several arguments or encode it",
                i + 1
            )));
        }
    }
    if args.len() == 1 && args[0].is_empty() {
        return Err(Error::Invalid(
            "a single empty argument is not allowed, pass no arguments instead".into(),
        ));
    }
    Ok(())
}

/// Check a free read's method and arguments: [`check_call`], and every
/// character must travel as-is in a URL, because the read endpoint does not
/// decode one. Letters, digits and `- _ . ~ :` are allowed.
///
/// For anything else use [`crate::rpc::Client::view`], which carries
/// arguments as JSON.
pub fn check_query(method: &str, args: &[String]) -> Result<()> {
    check_call(method, args)?;
    for arg in args {
        if !arg.bytes().all(url_raw_safe) {
            return Err(Error::Invalid(format!(
                "argument {arg:?} cannot be sent in a free read. Use letters, digits and - _ . ~ : only, \
                 or Client::view"
            )));
        }
    }
    Ok(())
}

fn url_raw_safe(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b':')
}

/// A contract id or other path segment that can go into a URL as-is.
#[cfg(feature = "rpc")]
pub(crate) fn check_path_segment(what: &str, s: &str) -> Result<()> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(Error::Invalid(format!("{what} {s:?} is not a valid id")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{"rcp_type":"token","traits":["fungible"],"capabilities":["execute"],"lockable_fields":[],"evolves_from":"","weight":5,"version":"v1.0.0"}"#;

    /// A minimal valid module with one exported `on_use` function, and `meta` in a
    /// custom section so the marker sits inside the module.
    fn module(meta: Option<&str>) -> Vec<u8> {
        let mut m = b"\0asm\x01\0\0\0".to_vec();
        // type section, one func type () -> ()
        m.extend_from_slice(&[0x01, 0x04, 0x01, 0x60, 0x00, 0x00]);
        // function section, one function of type 0
        m.extend_from_slice(&[0x03, 0x02, 0x01, 0x00]);
        // export section, "on_use" func 0
        m.extend_from_slice(&[0x07, 0x0a, 0x01, 0x06]);
        m.extend_from_slice(b"on_use");
        m.extend_from_slice(&[0x00, 0x00]);
        // code section, one body, no locals, `end`
        m.extend_from_slice(&[0x0a, 0x04, 0x01, 0x02, 0x00, 0x0b]);
        if let Some(meta) = meta {
            let mut payload = vec![4u8];
            payload.extend_from_slice(b"rcp1");
            payload.extend_from_slice(METADATA_MARKER);
            payload.extend_from_slice(meta.as_bytes());
            payload.push(0);
            m.push(0x00);
            leb(&mut m, payload.len());
            m.extend_from_slice(&payload);
        }
        m
    }

    fn leb(out: &mut Vec<u8>, mut n: usize) {
        loop {
            let byte = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
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
    fn metadata_needs_all_seven_fields_like_the_node() {
        let full = module(Some(FULL));
        assert_eq!(read_metadata(&full).unwrap().unwrap().rcp_type, "token");

        let short = module(Some(
            r#"{"rcp_type":"token","traits":["fungible"],"capabilities":["execute"],"weight":5,"version":"v1"}"#,
        ));
        let err = read_metadata(&short).unwrap_err().to_string();
        assert!(err.contains("lockable_fields"), "{err}");

        assert!(read_metadata(&module(None)).unwrap().is_none());
    }

    #[test]
    fn a_tiny_module_scores_weight_one() {
        assert_eq!(simulated_weight(&module(None)), 1);
        assert_eq!(
            simulated_weight(b"\0asm\x01\0\0\0garbage"),
            5,
            "unparsable scores 5"
        );
    }

    #[test]
    fn a_light_module_without_metadata_is_refused_like_the_chain_does() {
        let err = check_deployable(&module(None)).unwrap_err().to_string();
        assert!(err.contains("weight 1"), "{err}");
        // With deploy metadata naming the right weight it passes.
        let meta = serde_json::json!({"weight": 1});
        let p = preflight(&module(None), Some(&meta)).unwrap();
        assert_eq!(p.metadata.weight, 1);
        assert!(!p.embedded);
    }

    #[test]
    fn the_declared_weight_must_fit_the_code() {
        let light = module(Some(FULL));
        let meta = serde_json::json!({"name": "Counter"});
        let err = preflight(&light, Some(&meta)).unwrap_err().to_string();
        assert!(err.contains("must be 1 or 2"), "{err}");
        let fixed = serde_json::json!({"name": "Counter", "weight": 2});
        let p = preflight(&light, Some(&fixed)).unwrap();
        assert_eq!(p.metadata.rcp_type, "Counter", "name replaces rcp_type");
        assert!(p.embedded);
    }

    #[test]
    fn deploy_metadata_is_merged_the_way_the_node_merges_it() {
        let wasm = module(Some(FULL));
        let meta = serde_json::json!({
            "weight": 1, "symbol": "CNT", "decimals": 6,
            "traits": ["counter", "symbol:IGNORED"], "version": "v2"
        });
        let p = preflight(&wasm, Some(&meta)).unwrap();
        assert_eq!(
            p.metadata.traits,
            vec!["counter", "fungible", "symbol:CNT", "decimals:6"],
            "fungible kept, symbol and decimals added once"
        );
        assert_eq!(p.metadata.version, "v2");
    }

    #[test]
    fn what_the_chain_refuses_is_refused_here() {
        let wasm = module(Some(FULL));
        let w = |v: serde_json::Value| preflight(&wasm, Some(&v)).unwrap_err().to_string();
        assert!(w(serde_json::json!({"weight": 1, "symbol": "RTD"})).contains("reserved"));
        assert!(
            w(serde_json::json!({"weight": 1, "name": "R\u{0435}tium Dollar"}))
                .contains("reserved"),
            "Cyrillic e"
        );
        assert!(w(serde_json::json!({"weight": 1, "name": "Coin\u{202E}"})).contains("invisible"));
        assert!(w(serde_json::json!({"weight": 1, "decimals": 39})).contains("out of range"));
        assert!(w(serde_json::json!({"weight": 1, "version": ""})).contains("version"));
        assert!(w(serde_json::json!({"weight": 1, "x": "y".repeat(9000)})).contains("limit"));
    }

    #[test]
    fn a_module_without_the_entry_point_is_refused() {
        let mut m = module(None);
        let pos = m.windows(6).position(|w| w == b"on_use").unwrap();
        m[pos..pos + 6].copy_from_slice(b"no_use");
        let err = check_deployable(&m).unwrap_err().to_string();
        assert!(err.contains("on_use"), "{err}");
    }

    #[test]
    fn oversized_and_non_wasm_files_are_refused() {
        assert!(check_deployable(&vec![0u8; MAX_WASM_BYTES + 1]).is_err());
        assert!(check_deployable(b"not wasm at all").is_err());
    }

    #[test]
    fn call_arguments_that_the_chain_would_split_are_refused() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(check_call("bump", &a(&["5"])).is_ok());
        assert!(check_call("bump", &a(&[])).is_ok());
        assert!(
            check_call("transfer", &a(&["rete00", "", "3"])).is_ok(),
            "empty among others is fine"
        );
        assert!(check_call("bump", &a(&["1,2"])).is_err());
        assert!(check_call("bump", &a(&["f(x)"])).is_err());
        assert!(check_call("bump", &a(&["5)"])).is_err());
        assert!(check_call("bump", &a(&["a::b"])).is_err());
        assert!(check_call("bump", &a(&[""])).is_err());
        assert!(check_call("bump(", &a(&[])).is_err());
        assert!(check_call("", &a(&[])).is_err());
        assert!(
            check_call("get", &a(&["symbol:X"])).is_ok(),
            "a single colon is fine"
        );
    }

    #[test]
    fn free_reads_take_only_url_safe_arguments() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(check_query("balance_of", &a(&["rete00ab", "12.5"])).is_ok());
        assert!(check_query("find", &a(&["a b"])).is_err());
        assert!(check_query("find", &a(&["a&b=c"])).is_err());
        assert!(check_query("find", &a(&["100%"])).is_err());
    }
}
