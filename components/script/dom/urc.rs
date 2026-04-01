/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! URC (Unified Resource Coordinate) DOM binding.
//!
//! Represents an HPPR URC in the DOM for coordinate parsing and manipulation.
//! Uses hppr_packet::urc::URC for proper validation and parsing.
//! Supports JSONqa metadata suffix: `//group/app/loc{key:value,#:fragment}`

use std::cell::RefCell;
use std::ffi::CString;
use std::ptr::NonNull;

use dom_struct::dom_struct;
use hppr_packet::urc::{CoordinateVersion, UrcMethod};
use indexmap::IndexMap;
use js::jsapi::{
    GetArrayLength, HandleValueArray, JS_DefineProperty, JS_NewPlainObject, JSObject,
    JSPROP_ENUMERATE,
};
use js::jsval::{NullValue, ObjectValue, UndefinedValue};
use js::rust::wrappers::{GetPropertyKeys, IsArrayObject, JS_GetProperty, JS_HasProperty};
use js::rust::{HandleValue, IdVector, MutableHandleValue};
use jsonqa::{Qa, QaValue};

use crate::dom::bindings::codegen::Bindings::URCBinding::{URCMethods, URCSelector};
use crate::dom::bindings::error::{Error, Fallible};
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object_with_proto};
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::globalscope::GlobalScope;
use crate::dom::window::Window;
use crate::realms::enter_realm;
use crate::script_runtime::{CanGc, JSContext as SafeJSContext};

/// DOM representation of an HPPR URC with optional JSONqa metadata.
#[dom_struct]
pub(crate) struct URC {
    reflector_: Reflector,
    #[ignore_malloc_size_of = "hppr_packet::urc::URC"]
    #[no_trace]
    inner: hppr_packet::urc::URC,
    /// JSONqa metadata parsed from `{...}` suffix.
    #[ignore_malloc_size_of = "jsonqa::Qa"]
    #[no_trace]
    qa: RefCell<Option<Qa>>,
}

impl URC {
    fn new_inherited(inner: hppr_packet::urc::URC, qa: Option<Qa>) -> Self {
        Self {
            reflector_: Reflector::new(),
            inner,
            qa: RefCell::new(qa),
        }
    }

    fn new_with_proto(
        global: &GlobalScope,
        proto: Option<js::rust::HandleObject>,
        inner: hppr_packet::urc::URC,
        qa: Option<Qa>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object_with_proto(Box::new(Self::new_inherited(inner, qa)), global, proto, can_gc)
    }

    /// Create a new URC from a parsed hppr_packet::urc::URC.
    pub(crate) fn new(
        global: &GlobalScope,
        inner: hppr_packet::urc::URC,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        Self::new_with_proto(global, None, inner, None, can_gc)
    }

    /// Create a new URC with qa metadata.
    pub(crate) fn new_with_qa(
        global: &GlobalScope,
        inner: hppr_packet::urc::URC,
        qa: Option<Qa>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        Self::new_with_proto(global, None, inner, qa, can_gc)
    }

    /// Parse input string, splitting off any `{...}` JSONqa suffix.
    fn parse_with_qa(input: &str) -> Result<(hppr_packet::urc::URC, Option<Qa>), Error> {
        // Find the opening brace for JSONqa suffix
        let (coord_part, qa_part) = if let Some(brace_idx) = input.find('{') {
            (&input[..brace_idx], Some(&input[brace_idx..]))
        } else {
            (input, None)
        };

        // Parse the coordinate
        let inner = hppr_packet::urc::URC::parse(coord_part.to_string())
            .map_err(|e| Error::Syntax(Some(e.to_string())))?;

        // Parse the qa suffix if present
        let qa = if let Some(qa_str) = qa_part {
            Some(Qa::parse(qa_str).map_err(|e| Error::Syntax(Some(e.to_string())))?)
        } else {
            None
        };

        Ok((inner, qa))
    }

    /// Get the fragment value from JSONqa (qa["#"]).
    pub(crate) fn get_fragment(&self) -> Option<DOMString> {
        let qa = self.qa.borrow();
        qa.as_ref()
            .and_then(|q| q.fragment())
            .map(|s| DOMString::from(s))
    }
}

/// Convert QaValue to JS value recursively.
#[expect(unsafe_code)]
fn qavalue_to_jsval(cx: SafeJSContext, value: &QaValue, mut rval: MutableHandleValue) {
    match value {
        QaValue::String(s) => {
            let js_str = DOMString::from(s.as_str());
            unsafe {
                use js::conversions::ToJSValConvertible;
                js_str.to_jsval(*cx, rval);
            }
        }
        QaValue::Array(arr) => {
            unsafe {
                // Build array elements first
                rooted_vec!(let mut values);
                for item in arr.iter() {
                    rooted!(in(*cx) let mut item_val = UndefinedValue());
                    qavalue_to_jsval(cx, item, item_val.handle_mut());
                    values.push(item_val.get());
                }

                let values_array = HandleValueArray::from(&values);
                rooted!(in(*cx) let arr_obj = js::jsapi::NewArrayObject(*cx, &values_array));
                if !arr_obj.get().is_null() {
                    rval.set(ObjectValue(arr_obj.get()));
                } else {
                    rval.set(NullValue());
                }
            }
        }
        QaValue::Object(obj) => {
            unsafe {
                rooted!(in(*cx) let js_obj = JS_NewPlainObject(*cx));
                if !js_obj.get().is_null() {
                    for (key, val) in obj.iter() {
                        rooted!(in(*cx) let mut prop_val = UndefinedValue());
                        qavalue_to_jsval(cx, val, prop_val.handle_mut());
                        if let Ok(ckey) = CString::new(key.as_str()) {
                            JS_DefineProperty(
                                *cx,
                                js_obj.handle().into(),
                                ckey.as_ptr(),
                                prop_val.handle().into(),
                                JSPROP_ENUMERATE as u32,
                            );
                        }
                    }
                    rval.set(ObjectValue(js_obj.get()));
                } else {
                    rval.set(NullValue());
                }
            }
        }
    }
}

/// Convert Qa to JS object.
#[expect(unsafe_code)]
fn qa_to_jsval(cx: SafeJSContext, qa: &Qa, mut rval: MutableHandleValue) {
    unsafe {
        rooted!(in(*cx) let js_obj = JS_NewPlainObject(*cx));
        if js_obj.get().is_null() {
            rval.set(NullValue());
            return;
        }

        for (key, value) in qa.iter() {
            rooted!(in(*cx) let mut prop_val = UndefinedValue());
            qavalue_to_jsval(cx, value, prop_val.handle_mut());
            if let Ok(ckey) = CString::new(key.as_str()) {
                JS_DefineProperty(
                    *cx,
                    js_obj.handle().into(),
                    ckey.as_ptr(),
                    prop_val.handle().into(),
                    JSPROP_ENUMERATE as u32,
                );
            }
        }

        rval.set(ObjectValue(js_obj.get()));
    }
}

/// Convert JS value to QaValue recursively.
#[expect(unsafe_code)]
fn jsval_to_qavalue(cx: SafeJSContext, val: HandleValue) -> Result<QaValue, Error> {
    unsafe {
        if val.is_string() {
            use js::conversions::jsstr_to_string;
            let jsstr = NonNull::new(val.to_string())
                .ok_or_else(|| Error::Type(c"Null string".to_owned()))?;
            let s = jsstr_to_string(*cx, jsstr);
            return Ok(QaValue::String(s));
        }

        if val.is_object() {
            rooted!(in(*cx) let obj = val.to_object());

            // Check if it's an array
            let mut is_array = false;
            if !IsArrayObject(*cx, val, &mut is_array) {
                return Err(Error::JSFailed);
            }

            if is_array {
                let mut len: u32 = 0;
                GetArrayLength(*cx, obj.handle().into(), &mut len);
                let mut arr = Vec::with_capacity(len as usize);
                for i in 0..len {
                    rooted!(in(*cx) let mut elem = UndefinedValue());
                    js::jsapi::JS_GetElement(*cx, obj.handle().into(), i, elem.handle_mut().into());
                    arr.push(jsval_to_qavalue(cx, elem.handle())?);
                }
                return Ok(QaValue::Array(arr));
            }

            // It's a plain object - iterate its properties
            let mut entries = IndexMap::new();
            let mut ids = IdVector::new(*cx);
            if GetPropertyKeys(
                *cx,
                obj.handle(),
                js::jsapi::JSITER_OWNONLY,
                ids.handle_mut(),
            ) {
                for id in ids.iter() {
                    rooted!(in(*cx) let id = *id);

                    // Get property name - convert id to value first
                    rooted!(in(*cx) let mut key_val = UndefinedValue());
                    let raw_id: js::jsapi::HandleId = id.handle().into();
                    if !js::jsapi::JS_IdToValue(*cx, *raw_id.ptr, key_val.handle_mut().into()) {
                        continue;
                    }
                    if !key_val.is_string() {
                        continue;
                    }
                    use js::conversions::jsstr_to_string;
                    let jsstr = match NonNull::new(key_val.to_string()) {
                        Some(s) => s,
                        None => continue,
                    };
                    let key = jsstr_to_string(*cx, jsstr);

                    // Get property value
                    rooted!(in(*cx) let mut prop_val = UndefinedValue());
                    let ckey = match CString::new(key.as_str()) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let mut found = false;
                    if !JS_HasProperty(*cx, obj.handle().into(), ckey.as_ptr(), &mut found) || !found {
                        continue;
                    }
                    JS_GetProperty(*cx, obj.handle().into(), ckey.as_ptr(), prop_val.handle_mut().into());

                    entries.insert(key, jsval_to_qavalue(cx, prop_val.handle())?);
                }
            }
            return Ok(QaValue::Object(entries));
        }

        // Primitives that aren't strings become their string representation
        if val.is_int32() {
            return Ok(QaValue::String(val.to_int32().to_string()));
        }
        if val.is_double() {
            return Ok(QaValue::String(val.to_double().to_string()));
        }
        if val.is_boolean() {
            return Ok(QaValue::String(if val.to_boolean() { "true" } else { "false" }.to_string()));
        }
        if val.is_null() || val.is_undefined() {
            return Ok(QaValue::String(String::new()));
        }

        Err(Error::Type(c"Cannot convert value to QaValue".to_owned()))
    }
}

/// Convert JS object to Qa.
#[expect(unsafe_code)]
fn jsobj_to_qa(cx: SafeJSContext, obj: *mut JSObject) -> Result<Option<Qa>, Error> {
    if obj.is_null() {
        return Ok(None);
    }

    unsafe {
        rooted!(in(*cx) let obj_handle = obj);

        let mut entries = IndexMap::new();
        let mut ids = IdVector::new(*cx);
        if GetPropertyKeys(
            *cx,
            obj_handle.handle(),
            js::jsapi::JSITER_OWNONLY,
            ids.handle_mut(),
        ) {
            for id in ids.iter() {
                rooted!(in(*cx) let id = *id);

                // Get property name
                rooted!(in(*cx) let mut key_val = UndefinedValue());
                let raw_id: js::jsapi::HandleId = id.handle().into();
                if !js::jsapi::JS_IdToValue(*cx, *raw_id.ptr, key_val.handle_mut().into()) {
                    continue;
                }
                if !key_val.is_string() {
                    continue;
                }
                use js::conversions::jsstr_to_string;
                let jsstr = match NonNull::new(key_val.to_string()) {
                    Some(s) => s,
                    None => continue,
                };
                let key = jsstr_to_string(*cx, jsstr);

                // Get property value
                rooted!(in(*cx) let mut prop_val = UndefinedValue());
                let ckey = match CString::new(key.as_str()) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let mut found = false;
                if !JS_HasProperty(*cx, obj_handle.handle().into(), ckey.as_ptr(), &mut found) || !found {
                    continue;
                }
                JS_GetProperty(*cx, obj_handle.handle().into(), ckey.as_ptr(), prop_val.handle_mut().into());

                entries.insert(key, jsval_to_qavalue(cx, prop_val.handle())?);
            }
        }

        // Build Qa from entries using parse on the serialized form
        // This ensures proper validation
        let qa_str = format_qa_entries(&entries);
        match Qa::parse(&qa_str) {
            Ok(qa) => Ok(Some(qa)),
            Err(e) => Err(Error::Syntax(Some(e.to_string()))),
        }
    }
}

/// Format entries as JSONqa string for parsing.
fn format_qa_entries(entries: &IndexMap<String, QaValue>) -> String {
    let mut out = String::from("{");
    for (i, (key, value)) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format_qa_key(key));
        out.push(':');
        out.push_str(&format_qa_value(value));
    }
    out.push('}');
    out
}

fn format_qa_key(s: &str) -> String {
    if needs_quoting(s) {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn format_qa_value(v: &QaValue) -> String {
    match v {
        QaValue::String(s) => {
            if needs_quoting(s) {
                format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                s.clone()
            }
        }
        QaValue::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_qa_value).collect();
            format!("[{}]", items.join(","))
        }
        QaValue::Object(obj) => format_qa_entries(obj),
    }
}

fn needs_quoting(s: &str) -> bool {
    s.is_empty() || s.chars().any(|c| {
        c.is_whitespace() || matches!(c, '{' | '}' | '[' | ']' | ',' | ':' | '\\' | '"' | '\'')
    })
}

impl URCMethods<crate::DomTypeHolder> for URC {
    /// Constructor: parse a URC string with optional JSONqa suffix.
    fn Constructor(
        window: &Window,
        proto: Option<js::rust::HandleObject>,
        can_gc: CanGc,
        input: USVString,
    ) -> Fallible<DomRoot<Self>> {
        let (inner, qa) = Self::parse_with_qa(&input.0)?;
        Ok(Self::new_with_proto(window.upcast::<GlobalScope>(), proto, inner, qa, can_gc))
    }

    /// Returns the full URC string including JSONqa suffix (stringifier).
    fn Href(&self) -> USVString {
        let base = self.inner.to_string();
        let qa = self.qa.borrow();
        if let Some(ref qa) = *qa {
            USVString(format!("{}{}", base, qa))
        } else {
            USVString(base)
        }
    }

    /// Returns the URC method: "hash" or "index".
    fn Method(&self) -> DOMString {
        match self.inner.method() {
            UrcMethod::Hash => DOMString::from("hash"),
            UrcMethod::Index => DOMString::from("index"),
        }
    }

    /// Returns the group component (index URCs only).
    fn GetGroup(&self) -> Option<DOMString> {
        self.inner
            .group_app_loc()
            .map(|(g, _)| DOMString::from(g))
    }

    /// Returns the app component (index URCs only).
    fn GetApp(&self) -> Option<DOMString> {
        self.inner
            .group_app_loc()
            .and_then(|(_, rest)| rest.map(|(a, _)| DOMString::from(a)))
    }

    /// Returns the location component (index URCs only).
    fn GetLocation(&self) -> Option<DOMString> {
        self.inner
            .group_app_loc()
            .and_then(|(_, rest)| rest.and_then(|(_, loc)| loc.map(DOMString::from)))
    }

    /// Returns the full coordinate (//<group>/<app>/<location>).
    fn GetCoordinate(&self) -> Option<DOMString> {
        match self.inner.group_app_loc() {
            Some((group, Some((app, Some(loc))))) => {
                Some(DOMString::from(format!("//{}/{}/{}", group, app, loc)))
            }
            Some((group, Some((app, None)))) => {
                Some(DOMString::from(format!("//{}/{}/", group, app)))
            }
            Some((group, None)) => Some(DOMString::from(format!("//{}/", group))),
            None => None,
        }
    }

    /// Returns whether this is a listing URC (ends with /).
    fn IsListing(&self) -> bool {
        self.inner.is_listing()
    }

    /// Returns the selector if present.
    fn GetSelector(&self) -> Option<URCSelector> {
        self.inner.meta().map(|sel| match sel {
            CoordinateVersion::Empty => URCSelector {
                type_: Some(DOMString::from("empty")),
                verifyingKey: None,
                tai: None,
                hash: None,
            },
            CoordinateVersion::Plex { tai, hash } => URCSelector {
                type_: Some(DOMString::from("Plex")),
                verifyingKey: None,
                tai: tai.map(|t| Some(DOMString::from(t.to_string()))),
                hash: hash.map(|h| Some(DOMString::from(h.to_string()))),
            },
            CoordinateVersion::Seal {
                verifying_key,
                tai,
                hash,
            } => URCSelector {
                type_: Some(DOMString::from("Seal")),
                verifyingKey: verifying_key.map(|vk| Some(DOMString::from(vk.to_string()))),
                tai: tai.map(|t| Some(DOMString::from(t.to_string()))),
                hash: hash.map(|h| Some(DOMString::from(h.to_string()))),
            },
        })
    }

    /// Returns the JSONqa metadata as a JS object, or null if none.
    fn GetQa(&self, cx: SafeJSContext) -> Fallible<Option<NonNull<JSObject>>> {
        let qa = self.qa.borrow();
        let qa_ref = match qa.as_ref() {
            Some(q) => q,
            None => return Ok(None),
        };

        let global = self.global();
        let _ac = enter_realm(&*global);

        rooted!(in(*cx) let mut rval = UndefinedValue());
        qa_to_jsval(cx, qa_ref, rval.handle_mut());

        if rval.is_object() {
            Ok(NonNull::new(rval.to_object()))
        } else {
            Ok(None)
        }
    }

    /// Sets the JSONqa metadata from a JS object.
    fn SetQa(&self, cx: SafeJSContext, qa: *mut JSObject) -> Fallible<()> {
        let new_qa = jsobj_to_qa(cx, qa)?;
        *self.qa.borrow_mut() = new_qa;
        Ok(())
    }

    /// Returns the fragment value from JSONqa (qa["#"]).
    fn GetFragment(&self) -> Option<DOMString> {
        self.get_fragment()
    }

    /// Join a relative coordinate to this URC, returning a new URC.
    fn Join(&self, coordinate: USVString) -> Fallible<DomRoot<Self>> {
        let joined = self
            .inner
            .join(&coordinate.0)
            .map_err(|e| Error::Syntax(Some(e.to_string())))?;
        // Joining clears the qa - it applies to the new coordinate
        Ok(Self::new(&self.global(), joined, CanGc::note()))
    }

    /// Returns a new URC with the listing flag set/unset.
    fn SetListing(&self, is_listing: bool) -> DomRoot<Self> {
        let new_inner = self.inner.clone().set_listing(is_listing);
        // Preserve qa when changing listing
        let qa = self.qa.borrow().clone();
        Self::new_with_qa(&self.global(), new_inner, qa, CanGc::note())
    }
}
