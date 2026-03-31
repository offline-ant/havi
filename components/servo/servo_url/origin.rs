/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::RefCell;
use std::net::IpAddr;
use std::rc::Rc;

use malloc_size_of::malloc_size_of_is_0;
use malloc_size_of_derive::MallocSizeOf;
use serde::{Deserialize, Serialize};
use url::{Host, Origin};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, Hash, MallocSizeOf, PartialEq, Serialize)]
pub enum ImmutableOrigin {
    Opaque(OpaqueOrigin),
    Tuple(String, Host, u16),
}

pub trait DomainComparable {
    fn has_domain(&self) -> bool;
    fn immutable(&self) -> &ImmutableOrigin;
}

impl DomainComparable for OriginSnapshot {
    fn has_domain(&self) -> bool {
        self.1.is_some()
    }
    fn immutable(&self) -> &ImmutableOrigin {
        &self.0
    }
}

impl DomainComparable for MutableOrigin {
    fn has_domain(&self) -> bool {
        (self.0).1.borrow().is_some()
    }
    fn immutable(&self) -> &ImmutableOrigin {
        &(self.0).0
    }
}

impl ImmutableOrigin {
    pub fn new(origin: Origin) -> ImmutableOrigin {
        match origin {
            Origin::Opaque(_) => ImmutableOrigin::new_opaque(),
            Origin::Tuple(scheme, host, port) => ImmutableOrigin::Tuple(scheme, host, port),
        }
    }

    pub fn same_origin(&self, other: &impl DomainComparable) -> bool {
        self == other.immutable()
    }

    pub fn same_origin_domain(&self, other: &impl DomainComparable) -> bool {
        !other.has_domain() && self == other.immutable()
    }

    pub fn new_opaque() -> ImmutableOrigin {
        ImmutableOrigin::Opaque(OpaqueOrigin::Opaque(Uuid::new_v4()))
    }

    pub fn new_opaque_data_url_worker() -> ImmutableOrigin {
        ImmutableOrigin::Opaque(OpaqueOrigin::SecureWorkerFromDataUrl(Uuid::new_v4()))
    }

    pub fn scheme(&self) -> Option<&str> {
        match *self {
            ImmutableOrigin::Opaque(_) => None,
            ImmutableOrigin::Tuple(ref scheme, _, _) => Some(&**scheme),
        }
    }

    pub fn host(&self) -> Option<&Host> {
        match *self {
            ImmutableOrigin::Opaque(_) => None,
            ImmutableOrigin::Tuple(_, ref host, _) => Some(host),
        }
    }

    pub fn port(&self) -> Option<u16> {
        match *self {
            ImmutableOrigin::Opaque(_) => None,
            ImmutableOrigin::Tuple(_, _, port) => Some(port),
        }
    }

    pub fn into_url_origin(self) -> Origin {
        match self {
            ImmutableOrigin::Opaque(_) => Origin::new_opaque(),
            ImmutableOrigin::Tuple(scheme, host, port) => Origin::Tuple(scheme, host, port),
        }
    }

    pub fn is_tuple(&self) -> bool {
        match *self {
            ImmutableOrigin::Opaque(..) => false,
            ImmutableOrigin::Tuple(..) => true,
        }
    }

    pub fn is_potentially_trustworthy(&self) -> bool {
        if matches!(self, ImmutableOrigin::Opaque(_)) {
            return false;
        }

        if let ImmutableOrigin::Tuple(scheme, host, _) = self {
            if scheme == "https" || scheme == "wss" {
                return true;
            }
            if scheme == "file" {
                return true;
            }

            if let Ok(ip_addr) = host.to_string().parse::<IpAddr>() {
                return ip_addr.is_loopback();
            }
            if let Host::Domain(domain) = host {
                if domain == "localhost" || domain.ends_with(".localhost") {
                    return true;
                }
            }
        }
        if let ImmutableOrigin::Tuple(scheme, _, _) = self {
            if scheme.starts_with("hppr") {
                return true;
            }
        }
        false
    }

    pub fn ascii_serialization(&self) -> String {
        self.clone().into_url_origin().ascii_serialization()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum OpaqueOrigin {
    Opaque(Uuid),
    SecureWorkerFromDataUrl(Uuid),
}
malloc_size_of_is_0!(OpaqueOrigin);

#[derive(Clone, Debug, Deserialize, Eq, Hash, MallocSizeOf, PartialEq, Serialize)]
pub struct OriginSnapshot(ImmutableOrigin, Option<Host>);

impl OriginSnapshot {
    pub fn immutable(&self) -> &ImmutableOrigin {
        &self.0
    }

    pub fn has_domain(&self) -> bool {
        self.1.is_some()
    }

    pub fn same_origin(&self, other: &impl DomainComparable) -> bool {
        self.immutable() == other.immutable()
    }

    pub fn same_origin_domain(&self, other: &OriginSnapshot) -> bool {
        if let Some(ref self_domain) = self.1 {
            if let Some(ref other_domain) = other.1 {
                self_domain == other_domain && self.0.scheme() == other.0.scheme()
            } else {
                false
            }
        } else {
            self.0.same_origin_domain(other)
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MutableOrigin(Rc<(ImmutableOrigin, RefCell<Option<Host>>)>);

malloc_size_of_is_0!(MutableOrigin);

impl MutableOrigin {
    pub fn snapshot(&self) -> OriginSnapshot {
        OriginSnapshot(self.0.0.clone(), self.0.1.borrow().clone())
    }

    pub fn new(origin: ImmutableOrigin) -> MutableOrigin {
        MutableOrigin(Rc::new((origin, RefCell::new(None))))
    }

    pub fn immutable(&self) -> &ImmutableOrigin {
        &(self.0).0
    }

    pub fn is_tuple(&self) -> bool {
        self.immutable().is_tuple()
    }

    pub fn scheme(&self) -> Option<&str> {
        self.immutable().scheme()
    }

    pub fn host(&self) -> Option<&Host> {
        self.immutable().host()
    }

    pub fn port(&self) -> Option<u16> {
        self.immutable().port()
    }

    pub fn same_origin(&self, other: &MutableOrigin) -> bool {
        self.immutable() == other.immutable()
    }

    pub fn same_origin_domain(&self, other: &MutableOrigin) -> bool {
        if let Some(ref self_domain) = *(self.0).1.borrow() {
            if let Some(ref other_domain) = *(other.0).1.borrow() {
                self_domain == other_domain &&
                    self.immutable().scheme() == other.immutable().scheme()
            } else {
                false
            }
        } else {
            self.immutable().same_origin_domain(other)
        }
    }

    pub fn domain(&self) -> Option<Host> {
        (self.0).1.borrow().clone()
    }

    pub fn set_domain(&self, domain: Host) {
        *(self.0).1.borrow_mut() = Some(domain);
    }

    pub fn has_domain(&self) -> bool {
        (self.0).1.borrow().is_some()
    }

    pub fn effective_domain(&self) -> Option<Host> {
        if !self.is_tuple() {
            return None;
        }
        self.immutable()
            .host()
            .map(|host| self.domain().unwrap_or_else(|| host.clone()))
    }
}
