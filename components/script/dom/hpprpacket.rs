/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR Packet DOM binding.
//!
//! Represents an HPPR packet (Blob, Plex, or Seal) in the DOM.
//! Uses hppr_packet crate for proper validation and parsing.

use std::ptr::{self, NonNull};
use std::time::UNIX_EPOCH;

use dom_struct::dom_struct;
use encoding_rs::UTF_8;
use hppr_packet;
use js::jsapi::{ClippedTime, JS_ClearPendingException, JS_GetPendingException, JS_ParseJSON, JSObject, NewDateObject};
use crate::script_runtime::JSContext as SafeJSContext;
use js::jsval::UndefinedValue;
use js::typedarray::ArrayBuffer;
use malloc_size_of_derive::MallocSizeOf;

use crate::body::decode_to_utf16_with_bom_removal;
use crate::dom::bindings::codegen::Bindings::HpprPacketBinding::HpprPacketMethods;
use crate::dom::bindings::error::Error;
use crate::dom::bindings::reflector::{DomGlobal, Reflector, reflect_dom_object};
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::DOMString;
use crate::dom::bindings::trace::RootedTraceableBox;
use crate::dom::blob::Blob;
use crate::dom::globalscope::GlobalScope;
use crate::script_runtime::CanGc;
use script_bindings::cformat;

/// HPPR packet type for WebIDL compatibility.
#[derive(Clone, Copy, Debug, MallocSizeOf, PartialEq)]
pub(crate) enum HpprPacketType {
    Blob,
    Plex,
    Seal,
    Null,
}

impl From<hppr_packet::PacketType> for HpprPacketType {
    fn from(pt: hppr_packet::PacketType) -> Self {
        match pt {
            hppr_packet::PacketType::Blob => HpprPacketType::Blob,
            hppr_packet::PacketType::Plex => HpprPacketType::Plex,
            hppr_packet::PacketType::Seal => HpprPacketType::Seal,
            hppr_packet::PacketType::Null => HpprPacketType::Null,
        }
    }
}

impl HpprPacketType {
    fn as_str(&self) -> &'static str {
        match self {
            HpprPacketType::Blob => "Blob",
            HpprPacketType::Plex => "Plex",
            HpprPacketType::Seal => "Seal",
            HpprPacketType::Null => "Null",
        }
    }
}

/// DOM representation of an HPPR packet.
///
/// Stores validated packet. Use `packet()` to get a packet reference.
#[dom_struct]
pub(crate) struct HpprPacket {
    reflector_: Reflector,
    #[ignore_malloc_size_of = "hppr_packet::Packet"]
    #[no_trace]
    raw: hppr_packet::Packet,
}

impl HpprPacket {
    fn new_inherited(raw: hppr_packet::Packet) -> Result<Self, String> {
        // Packet is already validated by hppr_packet::read_packet during construction
        Ok(Self {
            reflector_: Reflector::new(),
            raw,
        })
    }

    /// Get a packet reference for accessing parsed data.
    fn packet(&self) -> &hppr_packet::PacketRef {
        self.raw.as_pkt_ref()
    }

    /// Create HpprPacket from raw bytes (validates packet format, hash, and signatures).
    pub(crate) fn new(
        global: &GlobalScope,
        packet: hppr_packet::Packet,
        can_gc: CanGc,
    ) -> Result<DomRoot<Self>, String> {
        let packet = Self::new_inherited(packet)?;
        Ok(reflect_dom_object(Box::new(packet), global, can_gc))
    }

    /// Get raw packet bytes.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.raw.as_bytes()
    }

    /// Get packet data (body bytes).
    pub(crate) fn data(&self) -> &[u8] {
        self.packet().data()
    }

    /// Get a specific header value by name (first occurrence).
    fn get_header_value(&self, name: &str) -> Option<&str> {
        self.packet().header(name)
    }

    /// Get all values for a header name.
    fn get_header_values(&self, name: &str) -> Vec<&str> {
        self.packet().headers()
            .filter(|(n, _)| *n == name)
            .map(|(_, v)| v)
            .collect()
    }

    /// Get packet type.
    fn packet_type(&self) -> HpprPacketType {
        HpprPacketType::from(self.packet().packet_type())
    }

    /// Access a field from `unpack()` for Plex/Seal packets. Returns None for Blob/Null.
    fn plex_field<'a, T>(&'a self, f: impl FnOnce(&hppr_packet::Unpacked<'a>) -> Option<T>) -> Option<T> {
        match self.packet().packet_type() {
            hppr_packet::PacketType::Plex | hppr_packet::PacketType::Seal => {
                f(&self.packet().unpack())
            },
            _ => None,
        }
    }

    /// Extract Group (for Plex/Seal packets).
    fn group(&self) -> Option<&str> {
        self.plex_field(|u| u.group)
    }

    /// Extract App (for Plex/Seal packets).
    fn app(&self) -> Option<&str> {
        self.plex_field(|u| u.app)
    }

    /// Extract Location (for Plex/Seal packets).
    fn location(&self) -> Option<&str> {
        self.plex_field(|u| u.location)
    }

    /// Extract Seal-By (for Seal packets only).
    fn seal_by(&self) -> Option<&str> {
        match self.packet().packet_type() {
            hppr_packet::PacketType::Seal => self.packet().unpack().seal_by,
            _ => None,
        }
    }

    /// Extract TAI (for Plex/Seal packets).
    fn tai(&self) -> Option<&str> {
        self.plex_field(|u| u.tai.map(|v| &**v))
    }
}

/// Create a HeapArrayBuffer from a byte slice.
///
/// # Safety
/// `raw_cx` must be a valid JSContext pointer with an active compartment.
#[expect(unsafe_code)]
unsafe fn create_heap_arraybuffer(
    raw_cx: *mut js::jsapi::JSContext,
    data: &[u8],
) -> Result<RootedTraceableBox<js::typedarray::HeapArrayBuffer>, Error> {
    use js::typedarray::HeapArrayBuffer;

    rooted!(in(raw_cx) let mut array_buffer = ptr::null_mut::<JSObject>());
    unsafe {
        ArrayBuffer::create(
            raw_cx,
            js::typedarray::CreateWith::Slice(data),
            array_buffer.handle_mut(),
        )
        .map_err(|_| Error::Operation(Some("Failed to create ArrayBuffer".to_string())))?;
    }

    HeapArrayBuffer::from(array_buffer.get())
        .map(RootedTraceableBox::new)
        .map_err(|()| Error::Operation(Some("Failed to create HeapArrayBuffer".to_string())))
}

impl HpprPacketMethods<crate::DomTypeHolder> for HpprPacket {
    /// Returns the packet hash (T.B64A.H3 format, 48 chars).
    fn Hash(&self) -> DOMString {
        DOMString::from(self.packet().pkt_hash())
    }

    /// Returns the packet type as string ("Blob", "Plex", "Seal", "Null").
    fn Type(&self) -> DOMString {
        DOMString::from(self.packet_type().as_str())
    }

    /// Get a single header value by name.
    fn GetHeader(&self, name: DOMString) -> Option<DOMString> {
        self.get_header_value(&name.str()).map(DOMString::from)
    }

    /// Get all values for a header name.
    fn GetHeaders(&self, name: DOMString) -> Vec<DOMString> {
        self.get_header_values(&name.str())
            .into_iter()
            .map(DOMString::from)
            .collect()
    }

    /// Get all headers as "Name: value" strings.
    fn Headers(&self) -> Vec<DOMString> {
        self.packet()
            .headers()
            .map(|(n, v)| DOMString::from(format!("{}: {}", n, v)))
            .collect()
    }

    /// Get custom plex headers only (excludes Group, App, Location, Tai, Blob markline, Data-Length).
    fn CustomHeaders(&self) -> Vec<DOMString> {
        self.packet()
            .unpack()
            .custom_headers()
            .map(|(n, v)| DOMString::from(format!("{}: {}", n, v)))
            .collect()
    }

    /// Returns the Group header (Plex/Seal only).
    fn GetGroup(&self) -> Option<DOMString> {
        self.group().map(DOMString::from)
    }

    /// Returns the App header (Plex/Seal only).
    fn GetApp(&self) -> Option<DOMString> {
        self.app().map(DOMString::from)
    }

    /// Returns the Location header (Plex/Seal only).
    fn GetLocation(&self) -> Option<DOMString> {
        self.location().map(DOMString::from)
    }

    /// Returns the TAI timestamp (Plex/Seal only).
    fn GetTai(&self) -> Option<DOMString> {
        self.tai().map(DOMString::from)
    }

    /// Returns the TAI as a JavaScript Date object (Plex/Seal only).
    #[expect(unsafe_code)]
    fn TaiDate(&self, cx: SafeJSContext) -> Option<NonNull<JSObject>> {
        let tai = self.plex_field(|u| u.tai)?;

        // Convert TAI to milliseconds since Unix epoch
        let millis = tai
            .system_time()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0);

        unsafe {
            let time = ClippedTime { t: millis };
            NonNull::new(NewDateObject(*cx, time))
        }
    }

    /// Returns the full coordinate (//<group>/<app>/<location>).
    fn GetCoordinate(&self) -> Option<DOMString> {
        match (self.group(), self.app(), self.location()) {
            (Some(g), Some(a), Some(l)) => {
                Some(DOMString::from(format!("//{}/{}/{}", g, a, l)))
            },
            _ => None,
        }
    }

    /// Returns the Seal-By header (Seal only).
    fn GetSealBy(&self) -> Option<DOMString> {
        self.seal_by().map(DOMString::from)
    }

    /// Returns the data length in bytes.
    fn DataLength(&self) -> u64 {
        self.data().len() as u64
    }

    /// Returns the packet data as an ArrayBuffer.
    #[expect(unsafe_code)]
    fn ArrayBuffer(&self, cx: SafeJSContext) -> Result<RootedTraceableBox<js::typedarray::HeapArrayBuffer>, Error> {
        unsafe { create_heap_arraybuffer(*cx, self.data()) }
    }

    /// Returns the packet data as a Blob (sync - data already in memory).
    fn Blob(&self) -> DomRoot<Blob> {
        let global = self.global();
        let data = self.data().to_vec();
        let content_type = self.get_header_value("Content-Type")
            .unwrap_or("application/octet-stream")
            .to_string();

        let blob_impl = constellation_traits::BlobImpl::new_from_bytes(data, content_type);
        Blob::new(&global, blob_impl, CanGc::note())
    }

    /// Returns the packet data as text (UTF-8 decoded, sync - data already in memory).
    fn Text(&self) -> Result<crate::dom::bindings::str::USVString, Error> {
        match std::str::from_utf8(self.data()) {
            Ok(text) => Ok(crate::dom::bindings::str::USVString(text.to_string())),
            Err(e) => Err(Error::Type(cformat!("Invalid UTF-8: {}", e))),
        }
    }

    /// Returns the packet data parsed as JSON (sync - data already in memory).
    #[expect(unsafe_code)]
    fn Json(&self, cx: SafeJSContext, mut retval: js::rust::MutableHandle<'_, js::jsapi::Value>) -> Result<(), Error> {
        // Decode to UTF-16 with BOM removal per RFC 8259
        let json_text = decode_to_utf16_with_bom_removal(self.data(), UTF_8);

        unsafe {
            if !JS_ParseJSON(
                *cx,
                json_text.as_ptr(),
                json_text.len() as u32,
                retval.reborrow().into(),
            ) {
                // JSON parsing failed - capture exception
                rooted!(in(*cx) let mut exception = UndefinedValue());
                if JS_GetPendingException(*cx, exception.handle_mut().into()) {
                    JS_ClearPendingException(*cx);
                }
                return Err(Error::Syntax(Some("Invalid JSON".to_string())));
            }

            Ok(())
        }
    }

    /// Returns the raw packet bytes as an ArrayBuffer.
    #[expect(unsafe_code)]
    fn Raw(&self, cx: SafeJSContext) -> Result<RootedTraceableBox<js::typedarray::HeapArrayBuffer>, Error> {
        unsafe { create_heap_arraybuffer(*cx, self.as_bytes()) }
    }
}
