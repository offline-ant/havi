//! QaValue - the value type for JSONqa metadata.

use indexmap::IndexMap;
use std::fmt;

/// A value in JSONqa metadata. All leaves are strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QaValue {
    /// Leaf value (always a string)
    String(String),
    /// Array of values
    Array(Vec<QaValue>),
    /// Nested object with ordered keys
    Object(IndexMap<String, QaValue>),
}

impl QaValue {
    /// Returns the string value if this is a String variant.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            QaValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the array if this is an Array variant.
    pub fn as_array(&self) -> Option<&Vec<QaValue>> {
        match self {
            QaValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Returns the object if this is an Object variant.
    pub fn as_object(&self) -> Option<&IndexMap<String, QaValue>> {
        match self {
            QaValue::Object(obj) => Some(obj),
            _ => None,
        }
    }
}

impl fmt::Display for QaValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_value(self, f)
    }
}

/// Characters that need escaping in bare strings.
const SPECIAL_CHARS: &[char] = &['{', '}', '[', ']', ',', ':', '\\', '"', '\''];

/// Check if a string needs quoting (contains whitespace or special chars).
fn needs_quoting(s: &str) -> bool {
    s.is_empty() || s.chars().any(|c| c.is_whitespace() || SPECIAL_CHARS.contains(&c))
}

/// Write a string, quoting if necessary.
fn write_string(s: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if needs_quoting(s) {
        // Use double quotes
        write!(f, "\"")?;
        for c in s.chars() {
            match c {
                '"' => write!(f, "\\\"")?,
                '\\' => write!(f, "\\\\")?,
                _ => write!(f, "{}", c)?,
            }
        }
        write!(f, "\"")
    } else {
        write!(f, "{}", s)
    }
}

/// Write a QaValue.
fn write_value(v: &QaValue, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match v {
        QaValue::String(s) => write_string(s, f),
        QaValue::Array(arr) => {
            write!(f, "[")?;
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write_value(item, f)?;
            }
            write!(f, "]")
        }
        QaValue::Object(obj) => {
            write!(f, "{{")?;
            for (i, (key, value)) in obj.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write_string(key, f)?;
                write!(f, ":")?;
                write_value(value, f)?;
            }
            write!(f, "}}")
        }
    }
}
