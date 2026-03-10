/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR StreamOut DOM binding.
//!
//! Provides an EventTarget interface for HPPR STREAM_OUT subscriber streaming.
//! The network layer parses trailer-format bytes internally.
//! The ReadableStream delivers decoded payload bytes.
//! Optional `onpacket` fires for each completed packet (diagnostics).

use std::cell::Cell;

use dom_struct::dom_struct;
use stylo_atoms::Atom;
use ipc_channel::ipc::{self, IpcSender};
use ipc_channel::router::ROUTER;
use net_traits::{
    CoreResourceMsg, HpprProtocolError, StreamOutDomAction, StreamOutNetworkEvent,
};
use hppr_client::parse_via;
use hppr_client::Signer;
use profile_traits::ipc as ProfiledIpc;

use script_bindings::reflector::DomObject;

use crate::dom::bindings::codegen::Bindings::StreamOutBinding::StreamOutMethods;
use crate::dom::bindings::error::Error;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::bindings::trace::NoTrace;
use crate::dom::errorevent::ErrorEvent;
use crate::dom::event::{Event, EventBubbles, EventCancelable};
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprerror::HpprError;
use crate::dom::hpprpacket::HpprPacket;
use crate::dom::messageevent::MessageEvent;
use crate::dom::readablestream::ReadableStream;
use crate::dom::underlyingsourcecontainer::UnderlyingSourceType;
use crate::script_runtime::CanGc;
use crate::task::TaskOnce;
use crate::task_source::SendableTaskSource;

#[derive(Clone, Copy, Debug, JSTraceable, MallocSizeOf, PartialEq)]
enum StreamOutState {
    Connecting = 0,
    Open = 1,
    Closing = 2,
    Closed = 3,
}

/// HPPR STREAM_OUT subscriber.
///
/// The network layer parses trailer-format bytes and delivers decoded
/// payload bytes through the ReadableStream. Optional `onpacket` fires
/// for each completed packet.
#[dom_struct]
pub(crate) struct StreamOut {
    eventtarget: EventTarget,
    ready_state: Cell<StreamOutState>,
    #[ignore_malloc_size_of = "IPC channels don't implement MallocSizeOf"]
    #[no_trace]
    sender: IpcSender<StreamOutDomAction>,
    prefix: NoTrace<String>,
    stream: Dom<ReadableStream>,
}

impl StreamOut {
    #[cfg_attr(crown, expect(crown::unrooted_must_root))]
    fn new_inherited(
        sender: IpcSender<StreamOutDomAction>,
        prefix: String,
        stream: &ReadableStream,
    ) -> Self {
        Self {
            eventtarget: EventTarget::new_inherited(),
            ready_state: Cell::new(StreamOutState::Connecting),
            sender,
            prefix: NoTrace(prefix),
            stream: Dom::from_ref(stream),
        }
    }

    /// Create a new StreamOut and initiate the STREAM_OUT connection.
    pub(crate) fn new(
        global: &GlobalScope,
        endpoint: &str,
        signer: Signer,
        prefix: String,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        // Create IPC channels
        let (dom_action_sender, resource_action_receiver): (
            IpcSender<StreamOutDomAction>,
            ipc::IpcReceiver<StreamOutDomAction>,
        ) = ipc::channel().unwrap();
        let (resource_event_sender, dom_event_receiver): (
            IpcSender<StreamOutNetworkEvent>,
            ProfiledIpc::IpcReceiver<StreamOutNetworkEvent>,
        ) = ProfiledIpc::channel(global.time_profiler_chan().clone()).unwrap();

        // Create the ReadableStream
        let stream = ReadableStream::new_with_external_underlying_source(
            global,
            UnderlyingSourceType::Memory(0),
            can_gc,
        )
        .unwrap();

        // Create the DOM object
        let so = reflect_dom_object(
            Box::new(StreamOut::new_inherited(
                dom_action_sender,
                prefix.clone(),
                &stream,
            )),
            global,
            can_gc,
        );

        // Set up router for network events
        let address = Trusted::new(&*so);
        let task_source = global
            .task_manager()
            .dom_manipulation_task_source()
            .to_sendable();
        ROUTER.add_typed_route(
            dom_event_receiver.to_ipc_receiver(),
            Box::new(move |message| match message.unwrap() {
                StreamOutNetworkEvent::Ready => {
                    task_source.queue(StreamOutConnectionTask {
                        address: address.clone(),
                    });
                },
                StreamOutNetworkEvent::Data(data) => {
                    task_source.queue(StreamOutDataTask {
                        address: address.clone(),
                        data,
                    });
                },
                StreamOutNetworkEvent::Packet(data) => {
                    task_source.queue(StreamOutPacketTask {
                        address: address.clone(),
                        data,
                    });
                },
                StreamOutNetworkEvent::Close => {
                    close_stream_out(address.clone(), &task_source, None);
                },
                StreamOutNetworkEvent::Fail(error) => {
                    close_stream_out(address.clone(), &task_source, Some(error));
                },
            }),
        );

        // Send STREAM_OUT request to network thread
        let via = match parse_via(endpoint) {
            Ok(v) => v,
            Err(_) => return so,
        };
        let _ = global
            .core_resource_thread()
            .send(CoreResourceMsg::HpprStreamOut {
                endpoint: via,
                signer,
                prefix,
                event_sender: resource_event_sender,
                action_receiver: resource_action_receiver,
            });

        so
    }

    /// Create a StreamOut in pending state (not yet connected).
    pub(crate) fn new_pending(
        global: &GlobalScope,
        prefix: String,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let (dom_action_sender, _) = ipc::channel().unwrap();
        let stream = ReadableStream::new_with_external_underlying_source(
            global,
            UnderlyingSourceType::Memory(0),
            can_gc,
        )
        .unwrap();
        reflect_dom_object(
            Box::new(StreamOut::new_inherited(dom_action_sender, prefix, &stream)),
            global,
            can_gc,
        )
    }

    /// Close the stream connection.
    pub(crate) fn close(&self) {
        match self.ready_state.get() {
            StreamOutState::Closing | StreamOutState::Closed => {},
            StreamOutState::Connecting | StreamOutState::Open => {
                self.ready_state.set(StreamOutState::Closing);
                let _ = self.sender.send(StreamOutDomAction::Close);
            },
        }
    }

    /// Fail the connection with an error.
    pub(crate) fn fail_with_error(&self, error: &str, can_gc: CanGc) {
        self.ready_state.set(StreamOutState::Closed);

        let protocol_error = HpprProtocolError {
            error_type: "FORBIDDEN".to_string(),
            detail: error.to_string(),
            fatal: true,
        };
        fire_stream_out_error(self, &protocol_error, can_gc);

        self.stream
            .error_native(Error::Network(None), can_gc);

        let event = Event::new(
            &self.global(),
            atom!("close"),
            EventBubbles::DoesNotBubble,
            EventCancelable::NotCancelable,
            can_gc,
        );
        event.fire(self.upcast(), can_gc);
    }
}

fn close_stream_out(
    address: Trusted<StreamOut>,
    task_source: &SendableTaskSource,
    error: Option<HpprProtocolError>,
) {
    task_source.queue(StreamOutCloseTask { address, error });
}

/// Fire an ErrorEvent on a StreamOut.
fn fire_stream_out_error(so: &StreamOut, protocol_error: &HpprProtocolError, can_gc: CanGc) {
    let global = so.global();
    let hppr_error = HpprError::from_protocol_error(&global, protocol_error, can_gc);
    rooted!(in(*GlobalScope::get_cx()) let error_val =
        js::jsval::ObjectValue(hppr_error.reflector().get_jsobject().get()));
    let event = ErrorEvent::new(
        &global,
        atom!("error"),
        EventBubbles::DoesNotBubble,
        EventCancelable::NotCancelable,
        DOMString::from(&*protocol_error.detail),
        DOMString::new(),
        0,
        0,
        error_val.handle(),
        can_gc,
    );
    event.upcast::<Event>().fire(so.upcast(), can_gc);
}

impl StreamOutMethods<crate::DomTypeHolder> for StreamOut {
    event_handler!(open, GetOnopen, SetOnopen);
    event_handler!(close, GetOnclose, SetOnclose);
    event_handler!(error, GetOnerror, SetOnerror);
    event_handler!(packet, GetOnpacket, SetOnpacket);

    fn ReadyState(&self) -> u16 {
        self.ready_state.get() as u16
    }

    fn Prefix(&self) -> USVString {
        USVString(self.prefix.0.clone())
    }

    fn Stream(&self) -> DomRoot<ReadableStream> {
        DomRoot::from_ref(&*self.stream)
    }

    fn Close(&self) {
        self.close();
    }
}

/// Task: STREAM_OUT connection established.
struct StreamOutConnectionTask {
    address: Trusted<StreamOut>,
}

impl TaskOnce for StreamOutConnectionTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let so = self.address.root();
        if so.ready_state.get() != StreamOutState::Connecting {
            return;
        }
        so.ready_state.set(StreamOutState::Open);
        so.upcast().fire_event(atom!("open"), CanGc::from_cx(cx));
    }
}

/// Task: data received from STREAM_OUT.
struct StreamOutDataTask {
    address: Trusted<StreamOut>,
    data: Vec<u8>,
}

impl TaskOnce for StreamOutDataTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let so = self.address.root();
        if so.ready_state.get() != StreamOutState::Open {
            return;
        }
        so.stream.enqueue_native(self.data, CanGc::from_cx(cx));
    }
}

/// Task: complete packet parsed from STREAM_OUT.
///
/// Fires a `packet` event carrying an `HpprPacket` object.
struct StreamOutPacketTask {
    address: Trusted<StreamOut>,
    data: Vec<u8>,
}

impl TaskOnce for StreamOutPacketTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let so = self.address.root();
        if so.ready_state.get() == StreamOutState::Connecting {
            return;
        }
        let global = so.global();
        let can_gc = CanGc::from_cx(cx);

        let packet = match hppr_packet::read_packet(self.data.into_boxed_slice()) {
            Ok(packet) => packet,
            Err(e) => {
                log::warn!("stream_out: failed to parse packet event bytes: {}", e);
                return;
            }
        };
        let packet_dom = match HpprPacket::new(&global, packet, can_gc) {
            Ok(packet_dom) => packet_dom,
            Err(e) => {
                log::warn!("stream_out: failed to wrap packet event: {}", e);
                return;
            }
        };

        rooted!(&in(cx) let js_val = js::jsval::ObjectValue(packet_dom.reflector().get_jsobject().get()));
        let event = MessageEvent::new(
            &global,
            Atom::from("packet"),
            false,
            false,
            js_val.handle(),
            DOMString::new(),
            None,
            DOMString::new(),
            vec![],
            can_gc,
        );
        event.upcast::<Event>().fire(so.upcast(), can_gc);
    }
}

/// Task: STREAM_OUT connection closed.
struct StreamOutCloseTask {
    address: Trusted<StreamOut>,
    error: Option<HpprProtocolError>,
}

impl TaskOnce for StreamOutCloseTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let so = self.address.root();
        let can_gc = CanGc::from_cx(cx);

        if so.ready_state.get() == StreamOutState::Closed {
            return;
        }

        so.ready_state.set(StreamOutState::Closed);

        if let Some(ref protocol_error) = self.error {
            fire_stream_out_error(&so, protocol_error, can_gc);
            so.stream.error_native(Error::Network(None), can_gc);
        } else {
            so.stream.controller_close_native(can_gc);
        }

        let event = Event::new(
            &so.global(),
            atom!("close"),
            EventBubbles::DoesNotBubble,
            EventCancelable::NotCancelable,
            can_gc,
        );
        event.fire(so.upcast(), can_gc);
    }
}
