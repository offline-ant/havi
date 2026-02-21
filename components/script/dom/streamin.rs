/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR StreamIn DOM binding.
//!
//! Provides an EventTarget interface for HPPR STREAM_IN publisher streaming.
//! Pushes data to the repo via write(). In publisher mode (key provided),
//! data is wrapped into signed trailer-format segments automatically.

use std::cell::Cell;
use std::rc::Rc;

use dom_struct::dom_struct;
use ipc_channel::ipc::{self, IpcSender};
use ipc_channel::router::ROUTER;
use js::typedarray::ArrayBufferU8;
use net_traits::{
    CoreResourceMsg, HpprProtocolError, StreamInDomAction, StreamInNetworkEvent,
    StreamInPublisherParams,
};
use hppr_client::env_target::parse_via;
use hppr_client::Signer;
use profile_traits::ipc as ProfiledIpc;

use script_bindings::reflector::DomObject;

use crate::dom::bindings::buffer_source::create_buffer_source;
use crate::dom::bindings::codegen::Bindings::StreamInBinding::{StreamInMethods, StreamInOptions};
use crate::dom::bindings::codegen::UnionTypes::ArrayBufferViewOrArrayBuffer;
use crate::dom::bindings::error::Error;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object};
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::bindings::trace::NoTrace;
use crate::dom::errorevent::ErrorEvent;
use crate::dom::event::{Event, EventBubbles, EventCancelable};
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprerror::HpprError;
use crate::dom::messageevent::MessageEvent;
use crate::dom::promise::Promise;
use crate::script_runtime::CanGc;
use crate::task::TaskOnce;
use crate::task_source::SendableTaskSource;

#[derive(Clone, Copy, Debug, JSTraceable, MallocSizeOf, PartialEq)]
enum StreamInState {
    Connecting = 0,
    Open = 1,
    Closing = 2,
    Closed = 3,
}

/// HPPR STREAM_IN publisher.
///
/// Pushes data to the repo via write(). In publisher mode, data is
/// automatically wrapped into signed trailer-format segments.
#[dom_struct]
pub(crate) struct StreamIn {
    eventtarget: EventTarget,
    ready_state: Cell<StreamInState>,
    #[ignore_malloc_size_of = "IPC channels don't implement MallocSizeOf"]
    #[no_trace]
    sender: IpcSender<StreamInDomAction>,
    prefix: NoTrace<String>,
}

impl StreamIn {
    fn new_inherited(sender: IpcSender<StreamInDomAction>, prefix: String) -> Self {
        Self {
            eventtarget: EventTarget::new_inherited(),
            ready_state: Cell::new(StreamInState::Connecting),
            sender,
            prefix: NoTrace(prefix),
        }
    }

    /// Create a new StreamIn and initiate the STREAM_IN connection.
    pub(crate) fn new(
        global: &GlobalScope,
        endpoint: &str,
        signer: Signer,
        prefix: String,
        publisher_params: Option<StreamInPublisherParams>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        // Create IPC channels
        let (dom_action_sender, resource_action_receiver): (
            IpcSender<StreamInDomAction>,
            ipc::IpcReceiver<StreamInDomAction>,
        ) = ipc::channel().unwrap();
        let (resource_event_sender, dom_event_receiver): (
            IpcSender<StreamInNetworkEvent>,
            ProfiledIpc::IpcReceiver<StreamInNetworkEvent>,
        ) = ProfiledIpc::channel(global.time_profiler_chan().clone()).unwrap();

        // Create the DOM object
        let si = reflect_dom_object(
            Box::new(StreamIn::new_inherited(dom_action_sender, prefix.clone())),
            global,
            can_gc,
        );

        // Set up router for network events
        let address = Trusted::new(&*si);
        let task_source = global
            .task_manager()
            .dom_manipulation_task_source()
            .to_sendable();
        ROUTER.add_typed_route(
            dom_event_receiver.to_ipc_receiver(),
            Box::new(move |message| match message.unwrap() {
                StreamInNetworkEvent::Ready => {
                    task_source.queue(StreamInConnectionTask {
                        address: address.clone(),
                    });
                },
                StreamInNetworkEvent::Packet(data) => {
                    task_source.queue(StreamInPacketTask {
                        address: address.clone(),
                        data,
                    });
                },
                StreamInNetworkEvent::Close => {
                    close_stream_in(address.clone(), &task_source, None);
                },
                StreamInNetworkEvent::Fail(error) => {
                    close_stream_in(address.clone(), &task_source, Some(error));
                },
            }),
        );

        // Send STREAM_IN request to network thread
        let via = match parse_via(endpoint) {
            Ok(v) => v,
            Err(_) => return si,
        };
        let _ = global
            .core_resource_thread()
            .send(CoreResourceMsg::HpprStreamIn {
                endpoint: via,
                signer,
                prefix,
                publisher_params,
                event_sender: resource_event_sender,
                action_receiver: resource_action_receiver,
            });

        si
    }

    /// Create a StreamIn in pending state (not yet connected).
    pub(crate) fn new_pending(
        global: &GlobalScope,
        prefix: String,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let (dom_action_sender, _) = ipc::channel().unwrap();
        reflect_dom_object(
            Box::new(StreamIn::new_inherited(dom_action_sender, prefix)),
            global,
            can_gc,
        )
    }

    /// Build StreamInPublisherParams from WebIDL options.
    pub(crate) fn publisher_params_from_options(
        options: &StreamInOptions,
    ) -> Option<StreamInPublisherParams> {
        let key = options.key.as_ref()?;
        let headers = match &options.headers {
            Some(map) => map.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            None => Vec::new(),
        };
        let max_segment_size = options.maxSegmentSize.map(|v| v as usize);
        let flush_seq = options.flushSeq.as_ref().map(|s| s.to_vec());
        Some(StreamInPublisherParams {
            key: key.to_string(),
            headers,
            max_segment_size,
            flush_seq,
        })
    }

    /// Close the stream connection.
    pub(crate) fn close(&self) {
        match self.ready_state.get() {
            StreamInState::Closing | StreamInState::Closed => {},
            StreamInState::Connecting | StreamInState::Open => {
                self.ready_state.set(StreamInState::Closing);
                let _ = self.sender.send(StreamInDomAction::Close);
            },
        }
    }

    /// Fail the connection with an error.
    pub(crate) fn fail_with_error(&self, error: &str, can_gc: CanGc) {
        self.ready_state.set(StreamInState::Closed);

        let protocol_error = HpprProtocolError {
            error_type: "FORBIDDEN".to_string(),
            detail: error.to_string(),
            fatal: true,
        };
        fire_stream_in_error(self, &protocol_error, can_gc);

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

fn close_stream_in(
    address: Trusted<StreamIn>,
    task_source: &SendableTaskSource,
    error: Option<HpprProtocolError>,
) {
    task_source.queue(StreamInCloseTask { address, error });
}

/// Fire an ErrorEvent on a StreamIn.
fn fire_stream_in_error(si: &StreamIn, protocol_error: &HpprProtocolError, can_gc: CanGc) {
    let global = si.global();
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
    event.upcast::<Event>().fire(si.upcast(), can_gc);
}

impl StreamInMethods<crate::DomTypeHolder> for StreamIn {
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

    /// Write data to the stream.
    fn Write(&self, data: ArrayBufferViewOrArrayBuffer) -> Rc<Promise> {
        let can_gc = CanGc::note();
        let global = self.global();
        let promise = Promise::new(&global, can_gc);

        if self.ready_state.get() != StreamInState::Open {
            promise.reject_error(Error::InvalidState(None), can_gc);
            return promise;
        }

        let bytes = match data {
            ArrayBufferViewOrArrayBuffer::ArrayBufferView(view) => view.to_vec(),
            ArrayBufferViewOrArrayBuffer::ArrayBuffer(buffer) => buffer.to_vec(),
        };

        let _ = self.sender.send(StreamInDomAction::Write(bytes));
        promise.resolve_native(&(), can_gc);
        promise
    }

    /// Manually close the current segment (publisher mode only).
    fn FinishSegment(&self) {
        if self.ready_state.get() == StreamInState::Open {
            let _ = self.sender.send(StreamInDomAction::FinishSegment);
        }
    }

    fn Close(&self) {
        self.close();
    }
}

/// Task: STREAM_IN connection established (repo accepted, OK received).
struct StreamInConnectionTask {
    address: Trusted<StreamIn>,
}

impl TaskOnce for StreamInConnectionTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let si = self.address.root();
        if si.ready_state.get() != StreamInState::Connecting {
            return;
        }
        si.ready_state.set(StreamInState::Open);
        si.upcast().fire_event(atom!("open"), CanGc::from_cx(cx));
    }
}

/// Task: complete packet from publisher mode segment.
struct StreamInPacketTask {
    address: Trusted<StreamIn>,
    data: Vec<u8>,
}

impl TaskOnce for StreamInPacketTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let si = self.address.root();
        if si.ready_state.get() != StreamInState::Open {
            return;
        }
        let global = si.global();
        let can_gc = CanGc::from_cx(cx);

        rooted!(&in(cx) let mut array_buffer_ptr = std::ptr::null_mut::<js::jsapi::JSObject>());
        create_buffer_source::<ArrayBufferU8>(cx.into(), &self.data, array_buffer_ptr.handle_mut(), can_gc)
            .expect("Failed to create ArrayBuffer for packet data");
        rooted!(&in(cx) let js_val = js::jsval::ObjectValue(*array_buffer_ptr));

        MessageEvent::dispatch_jsval(
            si.upcast(),
            &global,
            js_val.handle(),
            None,
            None,
            vec![],
            can_gc,
        );
    }
}

/// Task: STREAM_IN connection closed.
struct StreamInCloseTask {
    address: Trusted<StreamIn>,
    error: Option<HpprProtocolError>,
}

impl TaskOnce for StreamInCloseTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let si = self.address.root();
        let can_gc = CanGc::from_cx(cx);

        if si.ready_state.get() == StreamInState::Closed {
            return;
        }

        si.ready_state.set(StreamInState::Closed);

        if let Some(ref protocol_error) = self.error {
            fire_stream_in_error(&si, protocol_error, can_gc);
        }

        let event = Event::new(
            &si.global(),
            atom!("close"),
            EventBubbles::DoesNotBubble,
            EventCancelable::NotCancelable,
            can_gc,
        );
        event.fire(si.upcast(), can_gc);
    }
}
