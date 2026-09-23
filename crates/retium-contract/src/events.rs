//! Events: how a contract tells the outside world what happened.
//!
//! An event is a type name and a JSON payload. Indexers, explorers and wallets
//! read them; your contract cannot read them back, so never use an event as
//! storage.
//!
//! Always build payloads with [`json_string`] (or [`json_object`]). A quote or
//! a backslash in a user-supplied name will otherwise produce broken JSON that
//! no indexer can read.

use alloc::format;
use alloc::string::String;

use crate::host;
use crate::{Error, MAX_EVENT_DATA, MAX_EVENT_TYPE};

/// Emit an event. `data` must be valid JSON.
pub fn emit(event_type: &str, data: &str) -> Result<(), Error> {
    if event_type.is_empty() || event_type.len() > MAX_EVENT_TYPE {
        return Err(format!(
            "event type must be 1–{MAX_EVENT_TYPE} bytes, got {}",
            event_type.len()
        ));
    }
    if data.len() > MAX_EVENT_DATA {
        return Err(format!(
            "event data is {} bytes; the limit is {MAX_EVENT_DATA}",
            data.len()
        ));
    }
    let rc = unsafe {
        host::emit_event(
            event_type.as_ptr(),
            event_type.len() as i32,
            data.as_ptr(),
            data.len() as i32,
        )
    };
    if rc < 0 {
        return Err(format!("could not emit event '{event_type}'"));
    }
    Ok(())
}

/// Quote and escape a string so it is safe inside JSON.
pub fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Build a JSON object from name/value pairs, escaping every value.
///
/// ```
/// let payload = retium_contract::events::json_object(&[("to", "rete-bob"), ("amount", "10")]);
/// assert_eq!(payload, r#"{"to":"rete-bob","amount":"10"}"#);
/// ```
pub fn json_object(fields: &[(&str, &str)]) -> String {
    let mut out = String::from("{");
    for (i, (name, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_string(name));
        out.push(':');
        out.push_str(&json_string(value));
    }
    out.push('}');
    out
}

/// Emit an event from name/value pairs.
pub fn emit_fields(event_type: &str, fields: &[(&str, &str)]) -> Result<(), Error> {
    emit(event_type, &json_object(fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_backslashes_cannot_break_out() {
        assert_eq!(json_string("say \"hi\""), "\"say \\\"hi\\\"\"");
        assert_eq!(json_string("back\\slash"), "\"back\\\\slash\"");
        assert_eq!(json_string("line\nbreak"), "\"line\\nbreak\"");
    }

    #[test]
    fn control_characters_are_escaped() {
        assert_eq!(json_string("\u{1}"), "\"\\u0001\"");
    }

    #[test]
    fn objects_escape_every_value() {
        let payload = json_object(&[("name", "Evil\",\"admin\":\"yes")]);
        assert_eq!(payload, "{\"name\":\"Evil\\\",\\\"admin\\\":\\\"yes\"}");
    }

    #[test]
    fn event_limits_are_enforced_locally() {
        assert!(emit("", "{}").is_err());
        assert!(emit(&"t".repeat(MAX_EVENT_TYPE + 1), "{}").is_err());
        assert!(emit("t", &"x".repeat(MAX_EVENT_DATA + 1)).is_err());
    }
}
