//! JSONqa parser implementation.

use crate::QaValue;
use indexmap::IndexMap;
use thiserror::Error;

/// Parse errors for JSONqa.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("expected '{{' at start")]
    ExpectedOpenBrace,
    #[error("expected '}}' at end")]
    ExpectedCloseBrace,
    #[error("expected ']' to close array")]
    ExpectedCloseBracket,
    #[error("unterminated string")]
    UnterminatedString,
    #[error("invalid escape sequence")]
    InvalidEscape,
    #[error("unexpected character: {0}")]
    UnexpectedChar(char),
}

/// Parsed JSONqa metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qa {
    entries: IndexMap<String, QaValue>,
}

impl Qa {
    /// Parse a JSONqa string. Input must be the `{...}` portion including braces.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let mut parser = Parser::new(input);
        parser.parse_qa()
    }

    /// Get a value by key.
    pub fn get(&self, key: &str) -> Option<&QaValue> {
        self.entries.get(key)
    }

    /// Get the fragment value (shortcut for entries["#"]).
    pub fn fragment(&self) -> Option<&str> {
        self.entries.get("#").and_then(|v| v.as_str())
    }

    /// Returns true if there are no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Iterate over entries.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &QaValue)> {
        self.entries.iter()
    }
}

impl Default for Qa {
    fn default() -> Self {
        Self {
            entries: IndexMap::new(),
        }
    }
}

impl std::fmt::Display for Qa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{")?;
        for (i, (key, value)) in self.entries.iter().enumerate() {
            if i > 0 {
                write!(f, ",")?;
            }
            write!(f, "{}", format_string(key))?;
            // Only omit value if it's empty string (key-only syntax)
            if let QaValue::String(s) = value {
                if s.is_empty() {
                    continue;
                }
            }
            write!(f, ":")?;
            write!(f, "{}", format_value(value))?;
        }
        write!(f, "}}")
    }
}

/// Characters that need escaping in bare strings.
const SPECIAL_CHARS: &[char] = &['{', '}', '[', ']', ',', ':', '\\', '"', '\''];

/// Check if a string needs quoting.
fn needs_quoting(s: &str) -> bool {
    s.is_empty() || s.chars().any(|c| c.is_whitespace() || SPECIAL_CHARS.contains(&c))
}

/// Format a string for output, quoting if necessary.
fn format_string(s: &str) -> String {
    if needs_quoting(s) {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
        }
        out.push('"');
        out
    } else {
        s.to_string()
    }
}

/// Format a value for output.
fn format_value(v: &QaValue) -> String {
    match v {
        QaValue::String(s) => format_string(s),
        QaValue::Array(arr) => {
            let mut out = String::from("[");
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format_value(item));
            }
            out.push(']');
            out
        }
        QaValue::Object(obj) => {
            let mut out = String::from("{");
            for (i, (key, value)) in obj.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format_string(key));
                out.push(':');
                out.push_str(&format_value(value));
            }
            out.push('}');
            out
        }
    }
}

/// Internal parser state.
struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn parse_qa(&mut self) -> Result<Qa, ParseError> {
        self.skip_ws();

        // Expect opening brace
        match self.peek() {
            Some('{') => self.advance(),
            Some(_) => return Err(ParseError::ExpectedOpenBrace),
            None => return Err(ParseError::UnexpectedEof),
        }

        let entries = self.parse_pairs()?;

        self.skip_ws();

        // Expect closing brace
        match self.peek() {
            Some('}') => self.advance(),
            Some(_) => return Err(ParseError::ExpectedCloseBrace),
            None => return Err(ParseError::ExpectedCloseBrace),
        }

        self.skip_ws();

        Ok(Qa { entries })
    }

    fn parse_pairs(&mut self) -> Result<IndexMap<String, QaValue>, ParseError> {
        let mut entries = IndexMap::new();

        self.skip_ws();

        // Empty object
        if self.peek() == Some('}') {
            return Ok(entries);
        }

        loop {
            let (key, value) = self.parse_pair()?;
            // Last-wins for duplicate keys
            entries.insert(key, value);

            self.skip_ws();

            match self.peek() {
                Some(',') => {
                    self.advance();
                    self.skip_ws();
                    // Allow trailing comma before }
                    if self.peek() == Some('}') {
                        break;
                    }
                }
                Some('}') | Some(']') | None => break,
                Some(c) => return Err(ParseError::UnexpectedChar(c)),
            }
        }

        Ok(entries)
    }

    fn parse_pair(&mut self) -> Result<(String, QaValue), ParseError> {
        self.skip_ws();
        let key = self.parse_string::<true>()?;

        self.skip_ws();

        // Check for :value
        if self.peek() == Some(':') {
            self.advance();
            self.skip_ws();
            let value = self.parse_value()?;
            Ok((key, value))
        } else {
            // Key without value gets empty string
            Ok((key, QaValue::String(String::new())))
        }
    }

    fn parse_value(&mut self) -> Result<QaValue, ParseError> {
        self.skip_ws();

        match self.peek() {
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some(_) => Ok(QaValue::String(self.parse_string::<false>()?)),
            None => Err(ParseError::UnexpectedEof),
        }
    }

    fn parse_array(&mut self) -> Result<QaValue, ParseError> {
        // Consume [
        self.advance();
        self.skip_ws();

        let mut items = Vec::new();

        // Empty array
        if self.peek() == Some(']') {
            self.advance();
            return Ok(QaValue::Array(items));
        }

        loop {
            let value = self.parse_value()?;
            items.push(value);

            self.skip_ws();

            match self.peek() {
                Some(',') => {
                    self.advance();
                    self.skip_ws();
                    // Allow trailing comma
                    if self.peek() == Some(']') {
                        break;
                    }
                }
                Some(']') => break,
                Some(c) => return Err(ParseError::UnexpectedChar(c)),
                None => return Err(ParseError::ExpectedCloseBracket),
            }
        }

        // Consume ]
        match self.peek() {
            Some(']') => {
                self.advance();
                Ok(QaValue::Array(items))
            }
            _ => Err(ParseError::ExpectedCloseBracket),
        }
    }

    fn parse_object(&mut self) -> Result<QaValue, ParseError> {
        // Consume {
        self.advance();
        self.skip_ws();

        let entries = self.parse_pairs()?;

        self.skip_ws();

        // Expect closing brace
        match self.peek() {
            Some('}') => {
                self.advance();
                Ok(QaValue::Object(entries))
            }
            _ => Err(ParseError::ExpectedCloseBrace),
        }
    }

    fn parse_string<const KEY_MODE: bool>(&mut self) -> Result<String, ParseError> {
        self.skip_ws();

        match self.peek() {
            Some('"') => self.parse_quoted('"'),
            Some('\'') => self.parse_quoted('\''),
            Some(_) => self.parse_bare::<KEY_MODE>(),
            None => Err(ParseError::UnexpectedEof),
        }
    }

    fn parse_quoted(&mut self, quote: char) -> Result<String, ParseError> {
        // Consume opening quote
        self.advance();

        let mut result = String::new();

        loop {
            match self.peek() {
                Some(c) if c == quote => {
                    self.advance();
                    return Ok(result);
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('{') => {
                            result.push('{');
                            self.advance();
                        }
                        Some('}') => {
                            result.push('}');
                            self.advance();
                        }
                        Some('[') => {
                            result.push('[');
                            self.advance();
                        }
                        Some(']') => {
                            result.push(']');
                            self.advance();
                        }
                        Some(',') => {
                            result.push(',');
                            self.advance();
                        }
                        Some(':') => {
                            result.push(':');
                            self.advance();
                        }
                        Some('\\') => {
                            result.push('\\');
                            self.advance();
                        }
                        Some('"') => {
                            result.push('"');
                            self.advance();
                        }
                        Some('\'') => {
                            result.push('\'');
                            self.advance();
                        }
                        _ => return Err(ParseError::InvalidEscape),
                    }
                }
                Some(c) => {
                    result.push(c);
                    self.advance();
                }
                None => return Err(ParseError::UnterminatedString),
            }
        }
    }

    fn parse_bare<const KEY_MODE: bool>(&mut self) -> Result<String, ParseError> {
        let mut result = String::new();

        loop {
            match self.peek() {
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('{') => {
                            result.push('{');
                            self.advance();
                        }
                        Some('}') => {
                            result.push('}');
                            self.advance();
                        }
                        Some('[') => {
                            result.push('[');
                            self.advance();
                        }
                        Some(']') => {
                            result.push(']');
                            self.advance();
                        }
                        Some(',') => {
                            result.push(',');
                            self.advance();
                        }
                        Some(':') => {
                            result.push(':');
                            self.advance();
                        }
                        Some('\\') => {
                            result.push('\\');
                            self.advance();
                        }
                        Some('"') => {
                            result.push('"');
                            self.advance();
                        }
                        Some('\'') => {
                            result.push('\'');
                            self.advance();
                        }
                        _ => return Err(ParseError::InvalidEscape),
                    }
                }
                // Stop on delimiters
                Some(':') if KEY_MODE => break,
                Some(c)
                    if c.is_whitespace()
                        || c == '{'
                        || c == '}'
                        || c == '['
                        || c == ']'
                        || c == ','
                        || c == '"'
                        || c == '\'' =>
                {
                    break;
                }
                Some(c) => {
                    result.push(c);
                    self.advance();
                }
                None => break,
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty() {
        let qa = Qa::parse("{}").unwrap();
        assert!(qa.is_empty());
    }

    #[test]
    fn test_simple_pair() {
        let qa = Qa::parse("{page:5}").unwrap();
        assert_eq!(qa.get("page"), Some(&QaValue::String("5".to_string())));
    }

    #[test]
    fn test_fragment() {
        let qa = Qa::parse("{#:section}").unwrap();
        assert_eq!(qa.fragment(), Some("section"));
        assert_eq!(qa.get("#"), Some(&QaValue::String("section".to_string())));
    }

    #[test]
    fn test_array() {
        let qa = Qa::parse("{tags:[a,b,c]}").unwrap();
        let expected = QaValue::Array(vec![
            QaValue::String("a".to_string()),
            QaValue::String("b".to_string()),
            QaValue::String("c".to_string()),
        ]);
        assert_eq!(qa.get("tags"), Some(&expected));
    }

    #[test]
    fn test_nested_object() {
        let qa = Qa::parse("{opts:{dark:1}}").unwrap();
        let mut inner = IndexMap::new();
        inner.insert("dark".to_string(), QaValue::String("1".to_string()));
        let expected = QaValue::Object(inner);
        assert_eq!(qa.get("opts"), Some(&expected));
    }

    #[test]
    fn test_key_only() {
        let qa = Qa::parse("{draft,preview}").unwrap();
        assert_eq!(qa.get("draft"), Some(&QaValue::String(String::new())));
        assert_eq!(qa.get("preview"), Some(&QaValue::String(String::new())));
    }

    #[test]
    fn test_duplicate_keys_last_wins() {
        let qa = Qa::parse("{a:1,a:2}").unwrap();
        assert_eq!(qa.get("a"), Some(&QaValue::String("2".to_string())));
    }

    #[test]
    fn test_quoted_string() {
        let qa = Qa::parse("{key:\"hello world\"}").unwrap();
        assert_eq!(
            qa.get("key"),
            Some(&QaValue::String("hello world".to_string()))
        );
    }

    #[test]
    fn test_single_quoted_string() {
        let qa = Qa::parse("{key:'hello world'}").unwrap();
        assert_eq!(
            qa.get("key"),
            Some(&QaValue::String("hello world".to_string()))
        );
    }

    #[test]
    fn test_whitespace() {
        let qa = Qa::parse("{ a : b }").unwrap();
        assert_eq!(qa.get("a"), Some(&QaValue::String("b".to_string())));
    }

    #[test]
    fn test_whitespace_in_quotes_preserved() {
        let qa = Qa::parse("{tags:\"   test\"}").unwrap();
        assert_eq!(
            qa.get("tags"),
            Some(&QaValue::String("   test".to_string()))
        );
    }

    #[test]
    fn test_escaped_chars() {
        let qa = Qa::parse("{key:a\\:b}").unwrap();
        assert_eq!(qa.get("key"), Some(&QaValue::String("a:b".to_string())));
    }

    #[test]
    fn test_escaped_in_quoted() {
        let qa = Qa::parse("{key:\"a\\\"b\"}").unwrap();
        assert_eq!(qa.get("key"), Some(&QaValue::String("a\"b".to_string())));
    }

    #[test]
    fn test_multiple_pairs() {
        let qa = Qa::parse("{page:5,sort:date}").unwrap();
        assert_eq!(qa.get("page"), Some(&QaValue::String("5".to_string())));
        assert_eq!(qa.get("sort"), Some(&QaValue::String("date".to_string())));
    }

    #[test]
    fn test_round_trip() {
        let inputs = [
            "{page:5}",
            "{tags:[a,b,c]}",
            "{opts:{dark:1}}",
            "{}",
            "{#:section}",
        ];

        for input in inputs {
            let qa = Qa::parse(input).unwrap();
            let serialized = qa.to_string();
            let reparsed = Qa::parse(&serialized).unwrap();
            assert_eq!(qa, reparsed, "Round-trip failed for: {}", input);
        }
    }

    #[test]
    fn test_round_trip_with_quotes() {
        let qa = Qa::parse("{key:\"hello world\"}").unwrap();
        let serialized = qa.to_string();
        let reparsed = Qa::parse(&serialized).unwrap();
        assert_eq!(qa, reparsed);
    }

    #[test]
    fn test_empty_array() {
        let qa = Qa::parse("{items:[]}").unwrap();
        assert_eq!(qa.get("items"), Some(&QaValue::Array(vec![])));
    }

    #[test]
    fn test_nested_array() {
        let qa = Qa::parse("{matrix:[[1,2],[3,4]]}").unwrap();
        let expected = QaValue::Array(vec![
            QaValue::Array(vec![
                QaValue::String("1".to_string()),
                QaValue::String("2".to_string()),
            ]),
            QaValue::Array(vec![
                QaValue::String("3".to_string()),
                QaValue::String("4".to_string()),
            ]),
        ]);
        assert_eq!(qa.get("matrix"), Some(&expected));
    }

    #[test]
    fn test_complex_example() {
        let qa = Qa::parse("{#:intro,lang:en}").unwrap();
        assert_eq!(qa.fragment(), Some("intro"));
        assert_eq!(qa.get("lang"), Some(&QaValue::String("en".to_string())));
    }

    #[test]
    fn test_error_unterminated_string() {
        let result = Qa::parse("{key:\"unterminated}");
        assert_eq!(result, Err(ParseError::UnterminatedString));
    }

    #[test]
    fn test_error_missing_close_brace() {
        let result = Qa::parse("{key:value");
        assert_eq!(result, Err(ParseError::ExpectedCloseBrace));
    }

    #[test]
    fn test_error_missing_open_brace() {
        let result = Qa::parse("key:value}");
        assert_eq!(result, Err(ParseError::ExpectedOpenBrace));
    }

    #[test]
    fn test_iter() {
        let qa = Qa::parse("{a:1,b:2}").unwrap();
        let pairs: Vec<_> = qa.iter().collect();
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn test_bare_colon_in_value() {
        let qa = Qa::parse("{src:hppr:127.0.0.1:8000//u/group}").unwrap();
        assert_eq!(qa.get("src"), Some(&QaValue::String("hppr:127.0.0.1:8000//u/group".to_string())));
    }

    #[test]
    fn test_bare_colon_in_value_with_comma() {
        let qa = Qa::parse("{a:x:y,b:z}").unwrap();
        assert_eq!(qa.get("a"), Some(&QaValue::String("x:y".to_string())));
        assert_eq!(qa.get("b"), Some(&QaValue::String("z".to_string())));
    }

    #[test]
    fn test_bare_colon_in_array_item() {
        let qa = Qa::parse("{urls:[hppr:localhost:4777//u/a,hppr:localhost:4778//u/b]}").unwrap();
        let expected = QaValue::Array(vec![
            QaValue::String("hppr:localhost:4777//u/a".to_string()),
            QaValue::String("hppr:localhost:4778//u/b".to_string()),
        ]);
        assert_eq!(qa.get("urls"), Some(&expected));
    }

    #[test]
    fn test_bare_colon_round_trip() {
        // Parse with bare colons, serialize (will quote), reparse
        let qa = Qa::parse("{src:hppr:127.0.0.1:8000//u/group}").unwrap();
        let serialized = qa.to_string();
        let reparsed = Qa::parse(&serialized).unwrap();
        assert_eq!(qa, reparsed);
    }

    #[test]
    fn test_colon_still_separates_key_value() {
        // First colon is still the key-value separator
        let qa = Qa::parse("{key:value}").unwrap();
        assert_eq!(qa.get("key"), Some(&QaValue::String("value".to_string())));
    }

    #[test]
    fn test_nested_object_with_colon_values() {
        let qa = Qa::parse("{opts:{endpoint:hppr:localhost:4777}}").unwrap();
        let mut inner = IndexMap::new();
        inner.insert("endpoint".to_string(), QaValue::String("hppr:localhost:4777".to_string()));
        assert_eq!(qa.get("opts"), Some(&QaValue::Object(inner)));
    }
}
