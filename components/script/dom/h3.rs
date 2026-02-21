/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! H3 crypto namespace — window.H3.

use dom_struct::dom_struct;
use script_bindings::codegen::GenericBindings::H3Binding::HpprKeyPair;
use script_bindings::codegen::GenericUnionTypes::ArrayBufferOrArrayBufferViewOrUSVString;

use crate::dom::bindings::codegen::Bindings::H3Binding::H3Methods;
use crate::dom::bindings::error::{Error, Fallible};
use crate::dom::bindings::reflector::Reflector;
use crate::dom::bindings::str::DOMString;
use crate::dom::window::Window;
use script_bindings::cformat;

#[dom_struct]
pub(crate) struct H3 {
    reflector_: Reflector,
}

impl H3Methods<crate::DomTypeHolder> for H3 {
    /// Derive a signing/verifying key pair from password, name, and domain key.
    fn DeriveKeyPair(
        _win: &Window,
        password: DOMString,
        name: DOMString,
        domain_key: DOMString,
        phc: Option<DOMString>,
    ) -> Fallible<HpprKeyPair> {
        let phc_ref = phc.as_ref().map(|s| s.to_string());
        let (sk, vk) = hppr_client::H3::derive_key_pair(
            &password.to_string(),
            &name.to_string(),
            &domain_key.to_string(),
            phc_ref.as_deref(),
        )
        .map_err(|e| Error::Type(cformat!("{}", e)))?;

        Ok(HpprKeyPair {
            signingKey: DOMString::from(sk),
            verifyingKey: DOMString::from(vk),
        })
    }

    /// BLAKE3-256 hash of data, returned as raw b64a (43 chars).
    fn Hash(
        _win: &Window,
        data: ArrayBufferOrArrayBufferViewOrUSVString,
    ) -> Fallible<DOMString> {
        let bytes = match &data {
            ArrayBufferOrArrayBufferViewOrUSVString::ArrayBuffer(ab) => ab.to_vec(),
            ArrayBufferOrArrayBufferViewOrUSVString::ArrayBufferView(abv) => abv.to_vec(),
            ArrayBufferOrArrayBufferViewOrUSVString::USVString(s) => s.0.as_bytes().to_vec(),
        };
        let digest = hsb3::blake3::hash(&bytes);
        Ok(DOMString::from(hsb3::b64a_encode(digest.as_bytes())))
    }

    /// Sign a raw b64a hash with a signing key (&.xxx.H3).
    /// Returns raw b64a signature (86 chars).
    fn Sign(_win: &Window, hash: DOMString, signing_key: DOMString) -> Fallible<DOMString> {
        let hash_bytes = hsb3::b64a_decode(&hash.to_string())
            .map_err(|e| Error::Type(cformat!("invalid hash: {}", e)))?;
        if hash_bytes.len() != 32 {
            return Err(Error::Type(c"hash must be 32 bytes".to_owned()));
        }
        let (tc, sk_bytes) = hsb3::typed_b64a::t_b64a_h3_decode(&signing_key.to_string())
            .map_err(|e| Error::Type(cformat!("invalid signing key: {}", e)))?;
        if tc != '&' {
            return Err(Error::Type(c"signing key must start with &".to_owned()));
        }
        let mut msg32 = [0u8; 32];
        msg32.copy_from_slice(&hash_bytes);
        let (sig, _) = hsb3::schnorr_blake3_sign(&msg32, &sk_bytes, None)
            .map_err(|e| Error::Type(cformat!("sign failed: {}", e)))?;
        Ok(DOMString::from(hsb3::b64a_encode(&sig)))
    }

    /// Verify a signature against a hash and verifying key.
    fn Verify(
        _win: &Window,
        hash: DOMString,
        signature: DOMString,
        verifying_key: DOMString,
    ) -> Fallible<bool> {
        let hash_bytes = hsb3::b64a_decode(&hash.to_string())
            .map_err(|e| Error::Type(cformat!("invalid hash: {}", e)))?;
        if hash_bytes.len() != 32 {
            return Err(Error::Type(c"hash must be 32 bytes".to_owned()));
        }
        let sig_bytes = hsb3::b64a_decode(&signature.to_string())
            .map_err(|e| Error::Type(cformat!("invalid signature: {}", e)))?;
        if sig_bytes.len() != 64 {
            return Err(Error::Type(c"signature must be 64 bytes".to_owned()));
        }
        let (tc, vk_bytes) = hsb3::typed_b64a::t_b64a_h3_decode(&verifying_key.to_string())
            .map_err(|e| Error::Type(cformat!("invalid verifying key: {}", e)))?;
        if tc != 'V' {
            return Err(Error::Type(c"verifying key must start with V".to_owned()));
        }
        let mut msg32 = [0u8; 32];
        msg32.copy_from_slice(&hash_bytes);
        let mut sig64 = [0u8; 64];
        sig64.copy_from_slice(&sig_bytes);
        Ok(hsb3::schnorr_blake3_verify(&sig64, &vk_bytes, &msg32))
    }

    /// Generate a new key pair.
    fn GenerateKey(_win: &Window) -> Fallible<HpprKeyPair> {
        let sk = hsb3::generate_key();
        let vk = hsb3::get_verification_key(&sk)
            .map_err(|e| Error::Type(cformat!("key generation failed: {}", e)))?;
        let sk_str = hsb3::typed_b64a::t_b64a_h3_encode('&', &sk);
        let vk_str = hsb3::typed_b64a::t_b64a_h3_encode('V', &vk);
        Ok(HpprKeyPair {
            signingKey: DOMString::from(sk_str),
            verifyingKey: DOMString::from(vk_str),
        })
    }
}
