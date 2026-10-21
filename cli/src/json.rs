//! A few hand-written JSON helpers (the CLI has no `serde`).

/// `s` as a JSON string literal, quotes included.
pub fn string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    push_string(&mut out, s);
    out
}

/// Append `s` to `out` as a JSON string literal.
pub fn push_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
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
}

/// A JSON string, or `null`.
pub fn opt_string(s: Option<&str>) -> String {
    s.map_or_else(|| "null".to_string(), string)
}

/// A JSON array of already-encoded values.
pub fn array<I: IntoIterator<Item = String>>(items: I) -> String {
    let items: Vec<String> = items.into_iter().collect();
    format!("[{}]", items.join(","))
}

/// A JSON array of strings.
pub fn string_array<'s, I: IntoIterator<Item = &'s str>>(items: I) -> String {
    array(items.into_iter().map(string))
}

/// Builds a JSON object, keys in insertion order.
#[derive(Default)]
pub struct Object {
    out: String,
}

impl Object {
    /// Add `key` with an already-encoded JSON `value`.
    pub fn field(&mut self, key: &str, value: &str) -> &mut Self {
        self.out.push(if self.out.is_empty() { '{' } else { ',' });
        push_string(&mut self.out, key);
        self.out.push(':');
        self.out.push_str(value);
        self
    }

    /// The finished object.
    pub fn finish(&self) -> String {
        if self.out.is_empty() {
            "{}".to_string()
        } else {
            format!("{}}}", self.out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes() {
        assert_eq!(string("a\"b\\c\n\u{1}é"), r#""a\"b\\c\n\u0001é""#);
    }

    #[test]
    fn objects_and_arrays() {
        assert_eq!(Object::default().finish(), "{}");
        let mut o = Object::default();
        o.field("a", "1").field("b\"", &opt_string(None));
        assert_eq!(o.finish(), r#"{"a":1,"b\"":null}"#);
        assert_eq!(string_array(["x", "y"]), r#"["x","y"]"#);
        assert_eq!(array(Vec::new()), "[]");
    }
}
