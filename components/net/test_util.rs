/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use core::convert::Infallible;
use std::net::TcpListener as StdTcpListener;
use std::sync::{Arc, LazyLock, Mutex};

use crossbeam_channel::unbounded;
use embedder_traits::{EmbedderMsg, EmbedderProxy, EventLoopWaker, GenericEmbedderProxy};
use futures::future::ready;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse};
use hyper_util::rt::tokio::TokioIo;
use net_traits::AsyncRuntime;
use servo_url::BrowserUrl;
use tokio::net::{TcpListener, TcpStream};

use crate::async_runtime::{
    async_runtime_initialized, init_async_runtime, spawn_blocking_task, spawn_task,
};
pub use crate::hosts::replace_host_table;

static ASYNC_RUNTIME: LazyLock<Arc<Mutex<Box<dyn AsyncRuntime>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(init_async_runtime())));

pub fn create_embedder_proxy() -> EmbedderProxy {
    create_generic_embedder_proxy::<EmbedderMsg>()
}

pub fn create_generic_embedder_proxy<T>() -> GenericEmbedderProxy<T> {
    if !async_runtime_initialized() {
        let _init = ASYNC_RUNTIME.clone();
    }
    let (sender, _) = unbounded();
    let event_loop_waker = || {
        struct DummyEventLoopWaker {}
        impl DummyEventLoopWaker {
            fn new() -> DummyEventLoopWaker {
                DummyEventLoopWaker {}
            }
        }
        impl EventLoopWaker for DummyEventLoopWaker {
            fn wake(&self) {}
            fn clone_box(&self) -> Box<dyn EventLoopWaker> {
                Box::new(DummyEventLoopWaker {})
            }
        }

        Box::new(DummyEventLoopWaker::new())
    };

    GenericEmbedderProxy {
        sender: sender,
        event_loop_waker: event_loop_waker(),
    }
}

#[derive(Debug)]
pub struct Server {
    pub close_channel: tokio::sync::oneshot::Sender<()>,
}

impl Server {
    pub fn close(self) {
        self.close_channel.send(()).expect("err closing server:");
    }
}

pub fn make_server<H>(handler: H) -> (Server, BrowserUrl)
where
    H: Fn(HyperRequest<Incoming>, &mut HyperResponse<BoxBody<Bytes, hyper::Error>>)
        + Send
        + Sync
        + 'static,
{
    if !async_runtime_initialized() {
        let _ = &*ASYNC_RUNTIME;
    }
    let handler = Arc::new(handler);

    let listener = StdTcpListener::bind("0.0.0.0:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let listener =
        spawn_blocking_task::<_, TcpListener>(
            async move { TcpListener::from_std(listener).unwrap() },
        );

    let url_string = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    let url = BrowserUrl::parse(&url_string).unwrap();

    let graceful = hyper_util::server::graceful::GracefulShutdown::new();

    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
    let server = async move {
        loop {
            let stream = tokio::select! {
                stream = listener.accept() => stream.unwrap().0,
                _val = &mut rx => {
                    let _ = graceful.shutdown();
                    break;
                }
            };

            let handler = handler.clone();

            let stream = stream.into_std().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::new(5, 0)))
                .unwrap();
            let stream = TcpStream::from_std(stream).unwrap();
            let http = http1::Builder::new();
            let conn = http.serve_connection(
                TokioIo::new(stream),
                service_fn(move |req: HyperRequest<Incoming>| {
                    let mut response =
                        HyperResponse::new(Empty::new().map_err(|_| unreachable!()).boxed());
                    handler(req, &mut response);
                    ready(Ok::<_, Infallible>(response))
                }),
            );
            let conn = graceful.watch(conn);
            spawn_task(async move {
                let _ = conn.await;
            });
        }
    };

    let _ = spawn_task(server);
    (
        Server {
            close_channel: tx,
        },
        url,
    )
}

pub fn make_body(bytes: Vec<u8>) -> BoxBody<Bytes, hyper::Error> {
    Full::new(Bytes::from(bytes))
        .map_err(|_| unreachable!())
        .boxed()
}
