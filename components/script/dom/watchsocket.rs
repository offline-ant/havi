/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HPPR WatchSocket DOM binding.
//!
//! Provides a WebSocket-like EventTarget interface for HPPR WATCH streaming.
//! Fires onopen, onmessage, onerror, onclose events.

use std::cell::Cell;
use std::ptr::NonNull;

use dom_struct::dom_struct;
use ipc_channel::ipc::{self, IpcSender};
use ipc_channel::router::ROUTER;
use js::jsval::{ObjectValue, UndefinedValue};
use js::realm::AutoRealm;
use net_traits::{CoreResourceMsg, HpprProtocolError, WatchDomAction, WatchNetworkEvent};
use hppr_client::env_target::parse_via;
use hppr_client::Signer;
use profile_traits::ipc as ProfiledIpc;
use script_bindings::conversions::SafeToJSValConvertible;

use crate::dom::bindings::codegen::Bindings::WatchSocketBinding::WatchSocketMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::reflector::{DomGlobal, DomObject, reflect_dom_object};
use crate::dom::bindings::root::{Dom, DomRoot};
use crate::dom::bindings::str::{DOMString, USVString};
use crate::dom::bindings::trace::NoTrace;
use crate::dom::bindings::cell::DomRefCell;
use crate::dom::errorevent::ErrorEvent;
use crate::dom::event::{Event, EventBubbles, EventCancelable};
use crate::dom::eventtarget::EventTarget;
use crate::dom::globalscope::GlobalScope;
use crate::dom::hpprerror::HpprError;
use crate::dom::html::htmlxframe::HTMLXFrame;
use crate::dom::messageevent::MessageEvent;
use crate::script_runtime::CanGc;
use crate::task::TaskOnce;
use crate::task_source::SendableTaskSource;

#[derive(Clone, Copy, Debug, JSTraceable, MallocSizeOf, PartialEq)]
enum WatchSocketState {
    Connecting = 0,
    Open = 1,
    Closing = 2,
    Closed = 3,
}

/// HPPR WATCH streaming socket.
///
/// Provides WebSocket-like EventTarget interface for coordinate watching.
#[dom_struct]
pub(crate) struct WatchSocket {
    eventtarget: EventTarget,
    ready_state: Cell<WatchSocketState>,
    #[ignore_malloc_size_of = "IPC channels don't implement MallocSizeOf"]
    #[no_trace]
    sender: IpcSender<WatchDomAction>,
    urc: NoTrace<String>,
    /// Elements subscribed for watch notifications
    watch_subscribers: DomRefCell<Vec<Dom<HTMLXFrame>>>,
}

impl WatchSocket {
    fn new_inherited(sender: IpcSender<WatchDomAction>, urc: String) -> Self {
        Self {
            eventtarget: EventTarget::new_inherited(),
            ready_state: Cell::new(WatchSocketState::Connecting),
            sender,
            urc: NoTrace(urc),
            watch_subscribers: DomRefCell::new(Vec::new()),
        }
    }

    /// Create a new WatchSocket and initiate the WATCH connection.
    pub(crate) fn new(
        global: &GlobalScope,
        endpoint: &str,
        signer: Signer,
        urc: String,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        // Create IPC channels for bidirectional communication
        let (dom_action_sender, resource_action_receiver): (
            IpcSender<WatchDomAction>,
            ipc::IpcReceiver<WatchDomAction>,
        ) = ipc::channel().unwrap();
        let (resource_event_sender, dom_event_receiver): (
            IpcSender<WatchNetworkEvent>,
            ProfiledIpc::IpcReceiver<WatchNetworkEvent>,
        ) = ProfiledIpc::channel(global.time_profiler_chan().clone()).unwrap();

        // Create the DOM object
        let ws = reflect_dom_object(
            Box::new(WatchSocket::new_inherited(dom_action_sender, urc.clone())),
            global,
            can_gc,
        );

        // Set up router to handle events from network thread
        let address = Trusted::new(&*ws);
        let task_source = global.task_manager().dom_manipulation_task_source().to_sendable();
        ROUTER.add_typed_route(
            dom_event_receiver.to_ipc_receiver(),
            Box::new(move |message| {
                match message.unwrap() {
                    WatchNetworkEvent::ConnectionEstablished => {
                        task_source.queue(ConnectionEstablishedTask {
                            address: address.clone(),
                        });
                    },
                    WatchNetworkEvent::Message(data) => {
                        task_source.queue(MessageReceivedTask {
                            address: address.clone(),
                            data,
                        });
                    },
                    WatchNetworkEvent::Close => {
                        close_the_watch_connection(address.clone(), &task_source, None);
                    },
                    WatchNetworkEvent::Fail(error) => {
                        close_the_watch_connection(address.clone(), &task_source, Some(error));
                    },
                }
            }),
        );

        // Send WATCH request to network thread
        let via = match parse_via(endpoint) {
            Ok(v) => v,
            Err(_) => return ws,
        };
        let _ = global.core_resource_thread().send(CoreResourceMsg::HpprWatch {
            endpoint: via,
            signer,
            urc,
            event_sender: resource_event_sender,
            action_receiver: resource_action_receiver,
        });

        ws
    }

    /// Create a WatchSocket in pending state (not yet connected).
    /// Used for error paths where the socket is immediately failed.
    pub(crate) fn new_pending(
        global: &GlobalScope,
        urc: String,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let (dom_action_sender, _resource_action_receiver) = ipc::channel().unwrap();
        reflect_dom_object(
            Box::new(WatchSocket::new_inherited(dom_action_sender, urc)),
            global,
            can_gc,
        )
    }

    /// Fail the connection with an error (used for approval denial).
    pub(crate) fn fail_with_error(&self, error: &str, can_gc: CanGc) {
        self.ready_state.set(WatchSocketState::Closed);

        let protocol_error = HpprProtocolError {
            error_type: "FORBIDDEN".to_string(),
            detail: error.to_string(),
            fatal: true,
        };
        fire_error_event(self, &protocol_error, can_gc);

        // Fire close event
        let event = Event::new(
            &self.global(),
            atom!("close"),
            EventBubbles::DoesNotBubble,
            EventCancelable::NotCancelable,
            can_gc,
        );
        event.fire(self.upcast(), can_gc);
    }

    pub(crate) fn add_watch_subscriber(&self, el: &HTMLXFrame) {
        let mut subs = self.watch_subscribers.borrow_mut();
        if !subs.iter().any(|s| &**s as *const _ == el as *const _) {
            subs.push(Dom::from_ref(el));
        }
    }

    pub(crate) fn remove_watch_subscriber(&self, el: &HTMLXFrame) {
        self.watch_subscribers
            .borrow_mut()
            .retain(|s| &**s as *const _ != el as *const _);
    }

    pub(crate) fn notify_watch_subscribers(&self, data: &str, can_gc: CanGc) {
        for sub in self.watch_subscribers.borrow().iter() {
            sub.on_watch_message(data, can_gc);
        }
    }

    /// Close the watch connection (callable from outside the WebIDL trait).
    pub(crate) fn close(&self) {
        match self.ready_state.get() {
            WatchSocketState::Closing | WatchSocketState::Closed => {},
            WatchSocketState::Connecting | WatchSocketState::Open => {
                self.ready_state.set(WatchSocketState::Closing);
                let _ = self.sender.send(WatchDomAction::Close);
            },
        }
    }
}

fn close_the_watch_connection(
    address: Trusted<WatchSocket>,
    task_source: &SendableTaskSource,
    error: Option<HpprProtocolError>,
) {
    task_source.queue(CloseTask {
        address,
        error,
    });
}

/// Fire an ErrorEvent with an HpprError on a WatchSocket.
fn fire_error_event(ws: &WatchSocket, protocol_error: &HpprProtocolError, can_gc: CanGc) {
    let global = ws.global();
    let hppr_error = HpprError::from_protocol_error(&global, protocol_error, can_gc);
    rooted!(in(*GlobalScope::get_cx()) let error_val = ObjectValue(hppr_error.reflector().get_jsobject().get()));
    let event = ErrorEvent::new(
        &global,
        atom!("error"),
        EventBubbles::DoesNotBubble,
        EventCancelable::NotCancelable,
        DOMString::from(&*protocol_error.detail),
        DOMString::new(),
        0, 0,
        error_val.handle(),
        can_gc,
    );
    event.upcast::<Event>().fire(ws.upcast(), can_gc);
}

impl WatchSocketMethods<crate::DomTypeHolder> for WatchSocket {
    // Event handlers
    event_handler!(open, GetOnopen, SetOnopen);
    event_handler!(close, GetOnclose, SetOnclose);
    event_handler!(error, GetOnerror, SetOnerror);
    event_handler!(message, GetOnmessage, SetOnmessage);

    /// Returns the ready state.
    fn ReadyState(&self) -> u16 {
        self.ready_state.get() as u16
    }

    /// Returns the URC being watched.
    fn Urc(&self) -> USVString {
        USVString(self.urc.0.clone())
    }

    /// Close the watch connection.
    fn Close(&self) {
        match self.ready_state.get() {
            WatchSocketState::Closing | WatchSocketState::Closed => {
                // Already closing or closed, do nothing
            },
            WatchSocketState::Connecting | WatchSocketState::Open => {
                self.ready_state.set(WatchSocketState::Closing);
                let _ = self.sender.send(WatchDomAction::Close);
            },
        }
    }
}

/// Task queued when the WATCH connection is established.
struct ConnectionEstablishedTask {
    address: Trusted<WatchSocket>,
}

impl TaskOnce for ConnectionEstablishedTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let ws = self.address.root();

        // Only fire if still connecting (not already closed)
        if ws.ready_state.get() != WatchSocketState::Connecting {
            return;
        }

        ws.ready_state.set(WatchSocketState::Open);
        ws.upcast().fire_event(atom!("open"), CanGc::from_cx(cx));
    }
}

/// Task queued when a WATCH message is received.
struct MessageReceivedTask {
    address: Trusted<WatchSocket>,
    data: String,
}

impl TaskOnce for MessageReceivedTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let ws = self.address.root();

        // Only fire if connection is open
        if ws.ready_state.get() != WatchSocketState::Open {
            return;
        }

        // Fire MessageEvent with the data
        let global = ws.global();
        let mut realm = AutoRealm::new(
            cx,
            NonNull::new(ws.reflector().get_jsobject().get()).unwrap(),
        );
        let cx = &mut *realm;
        rooted!(&in(cx) let mut message = UndefinedValue());
        self.data.safe_to_jsval(cx.into(), message.handle_mut(), CanGc::from_cx(cx));
        MessageEvent::dispatch_jsval(
            ws.upcast(),
            &global,
            message.handle(),
            None,
            None,
            vec![],
            CanGc::from_cx(cx),
        );

        // Notify <x watch> subscribers
        ws.notify_watch_subscribers(&self.data, CanGc::from_cx(cx));
    }
}

/// Task queued when the WATCH connection closes.
struct CloseTask {
    address: Trusted<WatchSocket>,
    error: Option<HpprProtocolError>,
}

impl TaskOnce for CloseTask {
    fn run_once(self, cx: &mut js::context::JSContext) {
        let ws = self.address.root();
        let can_gc = CanGc::from_cx(cx);

        if ws.ready_state.get() == WatchSocketState::Closed {
            return;
        }

        ws.ready_state.set(WatchSocketState::Closed);

        // Fire ErrorEvent if connection failed
        if let Some(ref protocol_error) = self.error {
            fire_error_event(&ws, protocol_error, can_gc);
        }

        // Fire close event
        let event = Event::new(
            &ws.global(),
            atom!("close"),
            EventBubbles::DoesNotBubble,
            EventCancelable::NotCancelable,
            can_gc,
        );
        event.fire(ws.upcast(), can_gc);

        // Notify <x watch> subscribers of error
        if self.error.is_some() {
            for sub in ws.watch_subscribers.borrow().iter() {
                sub.on_watch_error(can_gc);
            }
        }
    }
}
