//! JSONqa - JSON-like query and fragment syntax for URCs.
//!
//! Parses `{key:value,other:thing}` syntax used in URC metadata.
//!
//! # Example
//!
//! ```
//! use jsonqa::{Qa, QaValue};
//!
//! let qa = Qa::parse("{page:5,tags:[a,b,c]}").unwrap();
//! assert_eq!(qa.get("page"), Some(&QaValue::String("5".to_string())));
//! assert_eq!(qa.fragment(), None);
//!
//! let qa = Qa::parse("{#:section}").unwrap();
//! assert_eq!(qa.fragment(), Some("section"));
//! ```

mod parse;
mod value;

pub use parse::{ParseError, Qa};
pub use value::QaValue;
