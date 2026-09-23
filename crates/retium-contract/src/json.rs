//! A small, strict JSON reader for the call object the node passes in.
//!
//! Deliberately a real parser rather than a substring search. A contract that
//! looked for `"caller":"` anywhere in the input could be fooled by an argument
//! containing that text — and every admin check in every contract rests on
//! `caller` being right.

use alloc::string::String;
use alloc::vec::Vec;

/// A parsed top-level JSON object, flattened to the fields a call carries.
pub struct Object {
    fields: Vec<(String, Value)>,
}

pub enum Value {
    Str(String),
    Array(Vec<String>),
    Null,
    /// Numbers, booleans and nested objects, kept as text.
    Other(String),
}

impl Object {
    /// Parse a JSON object. Only the shapes a call uses are accepted.
    pub fn parse(input: &str) -> Result<Self, String> {
        let bytes = input.as_bytes();
        let mut i = skip_ws(bytes, 0);
        if bytes.get(i) != Some(&b'{') {
            return Err(String::from("call input was not a JSON object"));
        }
        i += 1;
        let mut fields = Vec::new();
        loop {
            i = skip_ws(bytes, i);
            match bytes.get(i) {
                Some(b'}') => break,
                Some(b',') => {
                    i += 1;
                    continue;
                }
                Some(b'"') => {}
                _ => return Err(String::from("malformed call input")),
            }
            let (name, next) = parse_string(bytes, i)?;
            i = skip_ws(bytes, next);
            if bytes.get(i) != Some(&b':') {
                return Err(String::from("malformed call input: expected ':'"));
            }
            i = skip_ws(bytes, i + 1);
            let (value, next) = parse_value(bytes, i)?;
            fields.push((name, value));
            i = next;
        }
        Ok(Self { fields })
    }

    fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    /// A string field; `None` for a missing field or JSON `null`.
    pub fn string(&self, name: &str) -> Option<String> {
        match self.get(name) {
            Some(Value::Str(s)) => Some(s.clone()),
            Some(Value::Other(s)) => Some(s.clone()),
            _ => None,
        }
    }

    /// An array-of-strings field; empty when missing.
    pub fn string_array(&self, name: &str) -> Vec<String> {
        match self.get(name) {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        }
    }
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while matches!(b.get(i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        i += 1;
    }
    i
}

fn parse_string(b: &[u8], start: usize) -> Result<(String, usize), String> {
    debug_assert_eq!(b.get(start), Some(&b'"'));
    let mut out = String::new();
    let mut i = start + 1;
    while let Some(&c) = b.get(i) {
        match c {
            b'"' => {
                return Ok((out, i + 1));
            }
            b'\\' => {
                let esc = *b
                    .get(i + 1)
                    .ok_or_else(|| String::from("truncated escape"))?;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'u' => {
                        let hex = b
                            .get(i + 2..i + 6)
                            .ok_or_else(|| String::from("truncated \\u escape"))?;
                        let code = core::str::from_utf8(hex)
                            .ok()
                            .and_then(|h| u32::from_str_radix(h, 16).ok())
                            .ok_or_else(|| String::from("bad \\u escape"))?;
                        out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        i += 4;
                    }
                    other => return Err(alloc::format!("unknown escape \\{}", other as char)),
                }
                i += 2;
            }
            _ => {
                // Copy one UTF-8 character.
                let len = utf8_len(c);
                let chunk = b
                    .get(i..i + len)
                    .ok_or_else(|| String::from("truncated string"))?;
                out.push_str(core::str::from_utf8(chunk).map_err(|_| String::from("bad UTF-8"))?);
                i += len;
            }
        }
    }
    Err(String::from("unterminated string"))
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn parse_value(b: &[u8], start: usize) -> Result<(Value, usize), String> {
    match b.get(start) {
        Some(b'"') => {
            let (s, next) = parse_string(b, start)?;
            Ok((Value::Str(s), next))
        }
        Some(b'[') => {
            let mut items = Vec::new();
            let mut i = skip_ws(b, start + 1);
            loop {
                match b.get(i) {
                    Some(b']') => return Ok((Value::Array(items), i + 1)),
                    Some(b',') => {
                        i = skip_ws(b, i + 1);
                    }
                    Some(b'"') => {
                        let (s, next) = parse_string(b, i)?;
                        items.push(s);
                        i = skip_ws(b, next);
                    }
                    Some(_) => {
                        // Non-string element: keep it as text so one odd entry
                        // does not fail the whole call.
                        let (v, next) = parse_value(b, i)?;
                        if let Value::Other(s) | Value::Str(s) = v {
                            items.push(s);
                        }
                        i = skip_ws(b, next);
                    }
                    None => return Err(String::from("unterminated array")),
                }
            }
        }
        Some(b'{') => {
            // Nested object: skip it, tracking depth and strings.
            let mut depth = 0usize;
            let mut i = start;
            while let Some(&c) = b.get(i) {
                match c {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok((Value::Other(String::new()), i + 1));
                        }
                    }
                    b'"' => {
                        let (_, next) = parse_string(b, i)?;
                        i = next;
                        continue;
                    }
                    _ => {}
                }
                i += 1;
            }
            Err(String::from("unterminated object"))
        }
        Some(_) => {
            // Number, true, false or null.
            let mut i = start;
            while let Some(&c) = b.get(i) {
                if matches!(c, b',' | b'}' | b']' | b' ' | b'\n' | b'\r' | b'\t') {
                    break;
                }
                i += 1;
            }
            let text = core::str::from_utf8(&b[start..i]).map_err(|_| String::from("bad UTF-8"))?;
            if text == "null" {
                Ok((Value::Null, i))
            } else {
                Ok((Value::Other(String::from(text)), i))
            }
        }
        None => Err(String::from("unexpected end of input")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_fields_of_a_call() {
        let o = Object::parse(
            r#"{"action":"go","caller":"a","args":["1","2"],"token_context":null,"value":"5"}"#,
        )
        .unwrap();
        assert_eq!(o.string("action").unwrap(), "go");
        assert_eq!(o.string("caller").unwrap(), "a");
        assert_eq!(o.string_array("args"), ["1", "2"]);
        assert_eq!(o.string("token_context"), None);
        assert_eq!(o.string("value").unwrap(), "5");
    }

    #[test]
    fn a_field_inside_a_string_is_not_a_field() {
        let o = Object::parse(r#"{"caller":"real","args":["\"caller\":\"fake\""]}"#).unwrap();
        assert_eq!(o.string("caller").unwrap(), "real");
        assert_eq!(o.string_array("args")[0], r#""caller":"fake""#);
    }

    #[test]
    fn escapes_and_unicode_are_decoded() {
        let o = Object::parse(r#"{"a":"line\nbreak","b":"quote\"inside","c":"é"}"#).unwrap();
        assert_eq!(o.string("a").unwrap(), "line\nbreak");
        assert_eq!(o.string("b").unwrap(), "quote\"inside");
        assert_eq!(o.string("c").unwrap(), "é");
    }

    #[test]
    fn numbers_and_nested_objects_do_not_break_parsing() {
        let o = Object::parse(r#"{"n":42,"nested":{"x":"y"},"after":"kept"}"#).unwrap();
        assert_eq!(o.string("n").unwrap(), "42");
        assert_eq!(o.string("after").unwrap(), "kept");
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        for bad in ["", "[]", "{", r#"{"a"}"#, r#"{"a":"unterminated}"#] {
            assert!(Object::parse(bad).is_err(), "{bad:?} should fail to parse");
        }
    }
}
