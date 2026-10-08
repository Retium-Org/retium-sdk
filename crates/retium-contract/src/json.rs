//! A small, strict JSON reader for the call object the node passes in.
//!
//! Deliberately a real parser rather than a substring search. A contract that
//! looked for `"caller":"` anywhere in the input could be fooled by an argument
//! containing that text, and every admin check in every contract rests on
//! `caller` being right.
//!
//! The node writes the input with a standard JSON writer, so strings arrive
//! escaped. They are decoded exactly, and structure (a closing quote, `]`, `}`)
//! is only ever looked for outside strings. Input that is not well formed is
//! refused as a whole, so a call never runs on a half-read input.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// A parsed top-level JSON object.
pub struct Object {
    fields: Vec<(String, Value)>,
}

pub enum Value {
    Str(String),
    /// An array whose elements are all strings.
    Strings(Vec<String>),
    Null,
    /// Any other well-formed value, such as a number, `true`, `false`, an object, or
    /// an array holding something that is not a string. Stepped over, not read,
    /// so a field this crate does not know cannot break a call.
    Other,
}

impl Object {
    /// Parse a JSON object. Anything before or after it, a missing comma or a
    /// broken string is an error.
    pub fn parse(input: &str) -> Result<Self, String> {
        let b = input.as_bytes();
        let mut i = skip_ws(b, 0);
        if b.get(i) != Some(&b'{') {
            return Err(String::from("call input was not a JSON object"));
        }
        i = skip_ws(b, i + 1);
        let mut fields = Vec::new();
        if b.get(i) == Some(&b'}') {
            i += 1;
        } else {
            loop {
                let (name, next) = parse_string(b, i)?;
                i = skip_ws(b, next);
                if b.get(i) != Some(&b':') {
                    return Err(malformed("expected ':' after a field name"));
                }
                i = skip_ws(b, i + 1);
                let (value, next) = parse_value(b, i)?;
                fields.push((name, value));
                i = skip_ws(b, next);
                match b.get(i) {
                    Some(b',') => i = skip_ws(b, i + 1),
                    Some(b'}') => {
                        i += 1;
                        break;
                    }
                    _ => return Err(malformed("expected ',' or '}' after a value")),
                }
            }
        }
        if skip_ws(b, i) != b.len() {
            return Err(malformed("text after the closing brace"));
        }
        Ok(Self { fields })
    }

    /// The first field with this name. Only fields of the outer object are
    /// here, so a name inside a string or a nested object never matches.
    fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    /// Whether the object has a field with this name.
    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// A string field. `None` when it is missing or `null`, an error when it
    /// holds anything else.
    pub fn string(&self, name: &str) -> Result<Option<String>, String> {
        match self.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Str(s)) => Ok(Some(s.clone())),
            Some(_) => Err(format!("'{name}' must be a string")),
        }
    }

    /// An array-of-strings field. Empty when it is missing or `null`, an error
    /// when it holds anything else, so one odd element can never shift the
    /// ones after it.
    pub fn strings(&self, name: &str) -> Result<Vec<String>, String> {
        match self.get(name) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Strings(items)) => Ok(items.clone()),
            Some(_) => Err(format!("'{name}' must be an array of strings")),
        }
    }
}

/// Read a JSON array of strings, such as the key list the chain returns.
pub fn string_array(input: &str) -> Result<Vec<String>, String> {
    let b = input.as_bytes();
    let start = skip_ws(b, 0);
    if b.get(start) != Some(&b'[') {
        return Err(malformed("expected an array"));
    }
    match parse_strings(b, start)? {
        Some((items, next)) if skip_ws(b, next) == b.len() => Ok(items),
        Some(_) => Err(malformed("text after the closing bracket")),
        None => Err(malformed("expected an array of strings")),
    }
}

fn malformed(what: &str) -> String {
    format!("malformed JSON: {what}")
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while matches!(b.get(i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        i += 1;
    }
    i
}

fn hex4(b: &[u8], i: usize) -> Option<u32> {
    let digits = b.get(i..i + 4)?;
    let mut value: u32 = 0;
    for &c in digits {
        let d = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => return None,
        };
        value = (value << 4) | d as u32;
    }
    Some(value)
}

/// Decode the JSON string whose opening quote is at `b[start]`. Returns the
/// text and the index just past the closing quote.
fn parse_string(b: &[u8], start: usize) -> Result<(String, usize), String> {
    if b.get(start) != Some(&b'"') {
        return Err(malformed("expected a string"));
    }
    let mut out: Vec<u8> = Vec::new();
    let mut i = start + 1;
    loop {
        let c = *b
            .get(i)
            .ok_or_else(|| malformed("a string is not closed"))?;
        match c {
            b'"' => {
                let text =
                    String::from_utf8(out).map_err(|_| malformed("a string is not UTF-8"))?;
                return Ok((text, i + 1));
            }
            b'\\' => {
                let esc = *b
                    .get(i + 1)
                    .ok_or_else(|| malformed("a string is not closed"))?;
                i += 2;
                let ch = match esc {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    b'b' => '\u{8}',
                    b'f' => '\u{c}',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'u' => {
                        let hi = hex4(b, i).ok_or_else(|| malformed("bad \\u escape"))?;
                        i += 4;
                        let code = if (0xD800..=0xDBFF).contains(&hi) {
                            // First half of a pair, so the second half must follow.
                            if b.get(i) != Some(&b'\\') || b.get(i + 1) != Some(&b'u') {
                                return Err(malformed("half of a \\u pair"));
                            }
                            let lo = hex4(b, i + 2).ok_or_else(|| malformed("bad \\u escape"))?;
                            if !(0xDC00..=0xDFFF).contains(&lo) {
                                return Err(malformed("half of a \\u pair"));
                            }
                            i += 6;
                            0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                        } else if (0xDC00..=0xDFFF).contains(&hi) {
                            return Err(malformed("half of a \\u pair"));
                        } else {
                            hi
                        };
                        char::from_u32(code).ok_or_else(|| malformed("bad \\u escape"))?
                    }
                    other => return Err(malformed(&format!("unknown escape \\{}", other as char))),
                };
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            c if c < 0x20 => return Err(malformed("a control character inside a string")),
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
}

/// Index just past the JSON string starting at `b[start]`, without decoding.
fn skip_string(b: &[u8], start: usize) -> Result<usize, String> {
    let mut i = start + 1;
    loop {
        match b.get(i) {
            None => return Err(malformed("a string is not closed")),
            Some(b'"') => return Ok(i + 1),
            Some(b'\\') => i += 2,
            Some(_) => i += 1,
        }
    }
}

/// Index just past the JSON value starting at `b[start]`. Brackets are counted
/// only outside strings.
fn skip_value(b: &[u8], start: usize) -> Result<usize, String> {
    match b.get(start) {
        None => Err(malformed("the input ends where a value should be")),
        Some(b'"') => skip_string(b, start),
        Some(b'{' | b'[') => {
            let mut depth = 0usize;
            let mut i = start;
            loop {
                match b.get(i) {
                    None => return Err(malformed("an object or array is not closed")),
                    Some(b'"') => {
                        i = skip_string(b, i)?;
                        continue;
                    }
                    Some(b'{' | b'[') => depth += 1,
                    Some(b'}' | b']') => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(i + 1);
                        }
                    }
                    Some(_) => {}
                }
                i += 1;
            }
        }
        Some(b'}' | b']' | b',' | b':') => Err(malformed("expected a value")),
        Some(_) => {
            // Number, true, false or null.
            let mut i = start;
            while let Some(&c) = b.get(i) {
                if matches!(c, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r') {
                    break;
                }
                i += 1;
            }
            Ok(i)
        }
    }
}

/// The array starting at `b[start]`, if every element is a string. `None` when
/// it holds something else, an error when it is not well formed.
fn parse_strings(b: &[u8], start: usize) -> Result<Option<(Vec<String>, usize)>, String> {
    let mut items = Vec::new();
    let mut i = skip_ws(b, start + 1);
    if b.get(i) == Some(&b']') {
        return Ok(Some((items, i + 1)));
    }
    loop {
        match b.get(i) {
            Some(b'"') => {}
            Some(b']' | b',') => return Err(malformed("expected a value")),
            Some(_) => return Ok(None),
            None => return Err(malformed("an array is not closed")),
        }
        let (item, next) = parse_string(b, i)?;
        items.push(item);
        i = skip_ws(b, next);
        match b.get(i) {
            Some(b',') => i = skip_ws(b, i + 1),
            Some(b']') => return Ok(Some((items, i + 1))),
            _ => return Err(malformed("expected ',' or ']' after an array element")),
        }
    }
}

fn parse_value(b: &[u8], start: usize) -> Result<(Value, usize), String> {
    match b.get(start) {
        Some(b'"') => {
            let (text, next) = parse_string(b, start)?;
            Ok((Value::Str(text), next))
        }
        Some(b'[') => match parse_strings(b, start)? {
            Some((items, next)) => Ok((Value::Strings(items), next)),
            None => Ok((Value::Other, skip_value(b, start)?)),
        },
        _ => {
            let end = skip_value(b, start)?;
            if b.get(start..end) == Some(b"null".as_slice()) {
                Ok((Value::Null, end))
            } else {
                Ok((Value::Other, end))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::json_string;

    /// The input exactly as the node builds it.
    fn node_input(action: &str, caller: &str, args: &[&str]) -> String {
        serde_json::json!({
            "action": action,
            "caller": caller,
            "args": args,
            "token_context": null,
            "value": "0",
        })
        .to_string()
    }

    #[test]
    fn reads_the_fields_of_a_call() {
        let o = Object::parse(
            r#"{"action":"go","caller":"a","args":["1","2"],"token_context":null,"value":"5"}"#,
        )
        .unwrap();
        assert_eq!(o.string("action").unwrap().unwrap(), "go");
        assert_eq!(o.string("caller").unwrap().unwrap(), "a");
        assert_eq!(o.strings("args").unwrap(), ["1", "2"]);
        assert_eq!(o.string("token_context").unwrap(), None);
        assert_eq!(o.string("value").unwrap().unwrap(), "5");
        assert_eq!(o.string("missing").unwrap(), None);
        assert!(o.strings("missing").unwrap().is_empty());
    }

    #[test]
    fn a_field_inside_a_string_is_not_a_field() {
        let o = Object::parse(r#"{"caller":"real","args":["\"caller\":\"fake\""]}"#).unwrap();
        assert_eq!(o.string("caller").unwrap().unwrap(), "real");
        assert_eq!(o.strings("args").unwrap()[0], r#""caller":"fake""#);
    }

    #[test]
    fn an_argument_cannot_pose_as_another_field_or_cut_the_list() {
        let input = r#"{"args":["x\",\"caller\":\"evil","\"value\":\"999","x]y","]"],"caller":"real","action":"transfer","value":"5"}"#;
        let o = Object::parse(input).unwrap();
        assert_eq!(o.string("caller").unwrap().unwrap(), "real");
        assert_eq!(o.string("value").unwrap().unwrap(), "5");
        assert_eq!(o.string("action").unwrap().unwrap(), "transfer");
        assert_eq!(
            o.strings("args").unwrap(),
            ["x\",\"caller\":\"evil", "\"value\":\"999", "x]y", "]"]
        );
    }

    #[test]
    fn a_field_inside_a_nested_object_is_not_a_field() {
        let o = Object::parse(
            r#"{"extra":{"caller":"inner","args":["no"]},"caller":"outer","args":["yes"]}"#,
        )
        .unwrap();
        assert_eq!(o.string("caller").unwrap().unwrap(), "outer");
        assert_eq!(o.strings("args").unwrap(), ["yes"]);
    }

    #[test]
    fn escapes_are_decoded() {
        let o = Object::parse(
            r#"{"a":"line\nbreak","b":"quote\"inside","c":"é","d":"a\\b\/\t\r\b\f"}"#,
        )
        .unwrap();
        assert_eq!(o.string("a").unwrap().unwrap(), "line\nbreak");
        assert_eq!(o.string("b").unwrap().unwrap(), "quote\"inside");
        assert_eq!(o.string("c").unwrap().unwrap(), "é");
        assert_eq!(o.string("d").unwrap().unwrap(), "a\\b/\t\r\u{8}\u{c}");
    }

    #[test]
    fn unicode_escapes_are_decoded_and_pairs_are_joined() {
        assert_eq!(
            string_array(r#"["é中","😀","\u0000","é"]"#).unwrap(),
            ["é中", "😀", "\u{0}", "é"]
        );
    }

    #[test]
    fn a_broken_string_is_refused_not_guessed() {
        for bad in [
            r#"["ok","\ud83d"]"#, // first half of a pair alone
            r#"["\ude00"]"#,      // second half alone
            r#"["\ud83dA"]"#,     // first half followed by a letter
            r#"["\u12G4"]"#,      // not hex
            r#"["\u+123"]"#,      // a sign is not hex
            r#"["a\q"]"#,         // unknown escape
            "[\"a\nb\"]",         // raw line break inside a string
            r#"["a","b""#,        // not closed
        ] {
            assert!(string_array(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn commas_are_required_and_nothing_may_follow() {
        for bad in [
            "",
            "[]",
            "{",
            r#"{"a"}"#,
            r#"{"a":"unterminated}"#,
            r#"{"a" "b"}"#,
            r#"{"a":"1" "b":"2"}"#,
            r#"{"a":"1",}"#,
            r#"{,"a":"1"}"#,
            r#"{"a":}"#,
            r#"{"a":"1"} extra"#,
            r#"{"a":"1"}{"caller":"second"}"#,
            r#"{"a":["x" "y"]}"#,
            r#"{"a":["x",]}"#,
        ] {
            assert!(Object::parse(bad).is_err(), "{bad:?} should fail to parse");
        }
        assert!(Object::parse(" { } ").is_ok());
        let o = Object::parse(" { \"a\" : \"1\" ,\n \"b\" : [ \"x\" , \"y\" ] } ").unwrap();
        assert_eq!(o.strings("b").unwrap(), ["x", "y"]);
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_an_error_not_a_guess() {
        let o = Object::parse(r#"{"caller":42,"action":{"x":"y"},"args":["a",1,"b"],"n":null}"#)
            .unwrap();
        assert!(o.string("caller").is_err());
        assert!(o.string("action").is_err());
        assert!(
            o.strings("args").is_err(),
            "a number among the arguments must not shift the others"
        );
        assert!(o.strings("caller").is_err());
        assert_eq!(o.string("n").unwrap(), None);
    }

    #[test]
    fn unknown_fields_of_any_shape_are_stepped_over() {
        let o = Object::parse(
            r#"{"n":42,"t":true,"nested":{"x":["]","}"],"y":{"z":null}},"list":[1,[2,"]"],{"k":"v"}],"after":"kept"}"#,
        )
        .unwrap();
        assert_eq!(o.string("after").unwrap().unwrap(), "kept");
        assert!(o.has("nested") && o.has("list") && !o.has("x"));
    }

    #[test]
    fn the_first_of_two_fields_with_one_name_is_used() {
        let o = Object::parse(r#"{"caller":"first","caller":"second"}"#).unwrap();
        assert_eq!(o.string("caller").unwrap().unwrap(), "first");
    }

    #[test]
    fn a_key_list_is_read_whole_or_not_at_all() {
        assert!(string_array("[]").unwrap().is_empty());
        assert!(string_array("  [ ]  ").unwrap().is_empty());
        assert_eq!(string_array(r#"["a:1","a:2"]"#).unwrap(), ["a:1", "a:2"]);
        assert_eq!(string_array(r#"[""]"#).unwrap(), [""]);
        // A list cut off by a short buffer is an error, never a shorter list.
        assert!(string_array(r#"["a:1","a:2","a"#).is_err());
        assert!(string_array(r#"["a:1","a:2""#).is_err());
        assert!(string_array(r#"["a:1",2]"#).is_err());
        assert!(string_array(r#"["a"] x"#).is_err());
        assert!(string_array("").is_err());
    }

    #[test]
    fn reads_what_the_node_writes() {
        let nasty = [
            "plain",
            "",
            "Pepe \"The Frog\"",
            "a\\b",
            "x]y",
            "]",
            "}",
            "\\",
            "\"caller\":\"rete-admin\"",
            "line\nnext\ttab\r",
            "\u{0}\u{1}\u{8}\u{c}\u{1f}\u{7f}",
            "é中😀",
            "a,b",
            "[\"x\"]",
        ];
        for text in nasty {
            let input = node_input(text, text, &[text, "second", text]);
            let o = Object::parse(&input).unwrap_or_else(|e| panic!("{text:?}: {e}"));
            assert_eq!(o.string("action").unwrap().unwrap(), text);
            assert_eq!(o.string("caller").unwrap().unwrap(), text);
            assert_eq!(o.strings("args").unwrap(), [text, "second", text]);
            assert_eq!(o.string("value").unwrap().unwrap(), "0");
        }
        let all: Vec<&str> = nasty.to_vec();
        let o = Object::parse(&node_input("go", "rete-alice", &all)).unwrap();
        assert_eq!(o.strings("args").unwrap(), nasty);
    }

    #[test]
    fn reads_back_what_this_crate_writes() {
        for text in [
            "plain",
            "say \"hi\"",
            "a\\b",
            "x]y",
            "line\nnext",
            "\u{1}\u{1f}",
            "é中😀",
            "",
        ] {
            let array = format!("[{}]", json_string(text));
            assert_eq!(string_array(&array).unwrap(), [text], "{text:?}");
        }
    }
}
