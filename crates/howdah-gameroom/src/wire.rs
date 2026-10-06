//! Reply and request encodings.
//!
//! ASIP 1.0 (and the bot API) replies are `key=value` lines: values have `%`
//! written as `%25` and newlines as `%13`, a reply may start with a
//! `&junkvar=xxx…` prefix, and it ends with a `--END--` line. Game lists are
//! numbered entries (`1`, `2`, …, plus `num`), each itself a `key=value`
//! block. ASIP 2.0 replies are JSON. Both decode to a [`Record`].

use serde_json::{Map, Value};

/// The encoding of a request or reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// `key=value` (ASIP 1.0, the bot API).
    KeyValue,
    /// JSON (ASIP 2.0, the browser client).
    Json,
}

/// One decoded reply: field names to values, with values kept as JSON so
/// both encodings fit (a `key=value` reply has only strings).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub fields: Map<String, Value>,
    /// What the reply was encoded as.
    pub format: Option<Format>,
}

impl Record {
    /// Decodes a reply in whichever format it came in.
    pub fn decode(text: &str) -> Result<Record, String> {
        if text.trim_start().starts_with('{') {
            match serde_json::from_str(text.trim()) {
                Ok(Value::Object(fields)) => Ok(Record { fields, format: Some(Format::Json) }),
                Ok(_) => Err("the JSON reply isn't an object".into()),
                Err(e) => Err(format!("bad JSON reply: {e}")),
            }
        } else {
            Ok(Record { fields: decode_key_values(text), format: Some(Format::KeyValue) })
        }
    }

    /// A field as text: strings as they are, numbers written out.
    pub fn str(&self, key: &str) -> Option<String> {
        match self.fields.get(key)? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(if *b { "1" } else { "0" }.into()),
            _ => None,
        }
    }

    /// A field as text, `None` when missing or empty.
    pub fn nonempty(&self, key: &str) -> Option<String> {
        self.str(key).filter(|s| !s.is_empty())
    }

    /// A field holding an object (JSON), as a record.
    pub fn object(&self, key: &str) -> Option<Record> {
        match self.fields.get(key)? {
            Value::Object(fields) => Some(Record { fields: fields.clone(), format: Some(Format::Json) }),
            _ => None,
        }
    }

    /// A field as a whole number.
    pub fn int(&self, key: &str) -> Option<i64> {
        self.str(key)?.trim().parse().ok()
    }

    /// A field holding `1`/`0`.
    pub fn flag(&self, key: &str) -> bool {
        self.int(key).is_some_and(|n| n != 0)
    }

    /// A list of records: a JSON array of objects, or in `key=value`
    /// replies the numbered entries of the whole reply (the list's own
    /// key isn't used there; each reply holds one list).
    pub fn list(&self, key: &str) -> Vec<Record> {
        match self.fields.get(key) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|v| match v {
                    Value::Object(fields) => {
                        Some(Record { fields: fields.clone(), format: Some(Format::Json) })
                    }
                    _ => None,
                })
                .collect(),
            _ => self.numbered(),
        }
    }

    /// The numbered `key=value` entries `1..=num`.
    fn numbered(&self) -> Vec<Record> {
        let n = self.int("num").unwrap_or(0).max(0);
        (1..=n)
            .filter_map(|i| self.str(&i.to_string()))
            .map(|block| Record { fields: decode_key_values(&block), format: Some(Format::KeyValue) })
            .collect()
    }
}

/// Decodes a `key=value` reply into string fields.
pub fn decode_key_values(text: &str) -> Map<String, Value> {
    let mut body = text;
    if let Some(rest) = body.strip_prefix("&junkvar=") {
        body = rest.trim_start_matches('x');
    }
    let mut fields = Map::new();
    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        if line == "--END--" {
            break;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.to_string(), Value::String(unescape(value)));
        }
    }
    fields
}

/// Undoes the reply escaping, in the reverse order of the server's.
fn unescape(value: &str) -> String {
    value.replace("%13", "\n").replace("%25", "%")
}

/// Encodes request parameters: form fields for `key=value`, an object of
/// strings for JSON. Empty values are left out, as 4steps does.
pub fn encode_request(format: Format, params: &[(&str, String)]) -> String {
    let params = params.iter().filter(|(_, v)| !v.is_empty());
    match format {
        Format::KeyValue => {
            params.map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v))).collect::<Vec<_>>().join("&")
        }
        Format::Json => {
            let map: Map<String, Value> =
                params.map(|(k, v)| (k.to_string(), Value::String(v.clone()))).collect();
            Value::Object(map).to_string()
        }
    }
}

/// Percent-encodes a form value.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_values_are_unescaped_and_stop_at_end() {
        let r = Record::decode("&junkvar=xxxxsid=abc\nchat=a%13b%2513\n--END--\nafter=1\n").unwrap();
        assert_eq!(r.format, Some(Format::KeyValue));
        assert_eq!(r.str("sid").as_deref(), Some("abc"));
        assert_eq!(r.str("chat").as_deref(), Some("a\nb%13"), "%2513 is a literal %13");
        assert_eq!(r.str("after"), None);
    }

    #[test]
    fn json_values_read_as_text() {
        let r = Record::decode(r#"{"sid":"abc","num":3,"ok":true}"#).unwrap();
        assert_eq!(r.format, Some(Format::Json));
        assert_eq!(r.int("num"), Some(3));
        assert!(r.flag("ok"));
        assert_eq!(r.nonempty("missing"), None);
    }

    #[test]
    fn lists_in_both_formats() {
        let json = Record::decode(r#"{"livegames":[{"id":"5","wusername":"a"},{"id":"6"}]}"#).unwrap();
        let games = json.list("livegames");
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].str("wusername").as_deref(), Some("a"));
        let kv = Record::decode("num=2\n1=gid=7%13role=w\n2=gid=8%13role=b\n").unwrap();
        let games = kv.list("opengames");
        assert_eq!(games.iter().map(|g| g.int("gid").unwrap()).collect::<Vec<_>>(), [7, 8]);
    }

    #[test]
    fn requests_are_encoded() {
        let params =
            [("action", "login".to_string()), ("password", "p&q =".into()), ("empty", String::new())];
        assert_eq!(encode_request(Format::KeyValue, &params), "action=login&password=p%26q%20%3D");
        assert_eq!(encode_request(Format::Json, &params), r#"{"action":"login","password":"p&q ="}"#);
    }
}
