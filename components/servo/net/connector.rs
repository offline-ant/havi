/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::time::Duration;

use futures::task::{Context, Poll};
use futures::{Future, TryFutureExt};
use http::uri::{Authority, Uri as Destination};
use http_body_util::combinators::BoxBody;
use hyper::body::Bytes;
use hyper::rt::Executor;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::proxy::Tunnel;
use hyper_util::client::legacy::connect::HttpConnector as HyperHttpConnector;
use hyper_util::rt::TokioIo;
use servo_config::pref;
use tokio::net::TcpStream;
use tower::Service;

use super::async_runtime::spawn_task;
use super::hosts::replace_host;

pub const BUF_SIZE: usize = 32768;

#[derive(Clone)]
pub struct ServoHttpConnector {
    inner: HyperHttpConnector,
}

impl ServoHttpConnector {
    fn new() -> ServoHttpConnector {
        let mut inner = HyperHttpConnector::new();
        inner.enforce_http(false);
        inner.set_happy_eyeballs_timeout(None);
        inner.set_connect_timeout(Some(Duration::from_secs(pref!(network_connection_timeout))));
        ServoHttpConnector { inner }
    }
}

impl Service<Destination> for ServoHttpConnector {
    type Response = TokioIo<TcpStream>;
    type Error = ConnectionError;
    type Future =
        std::pin::Pin<Box<dyn Future<Output = Result<TokioIo<TcpStream>, ConnectionError>> + Send>>;

    fn call(&mut self, dest: Destination) -> Self::Future {
        // Perform host replacement when making the actual TCP connection.
        let mut new_dest = dest.clone();
        let mut parts = dest.into_parts();

        if let Some(auth) = parts.authority {
            let host = auth.host();
            let host = replace_host(host);

            let authority = if let Some(port) = auth.port() {
                format!("{}:{}", host, port.as_str())
            } else {
                (*host).to_string()
            };

            if let Ok(authority) = Authority::from_maybe_shared(authority) {
                parts.authority = Some(authority);
                if let Ok(dest) = Destination::from_parts(parts) {
                    new_dest = dest
                }
            }
        }

        Box::pin(
            self.inner
                .call(new_dest)
                .map_err(|e| ConnectionError::HttpError(format!("{e}"))),
        )
    }

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Ok(()).into()
    }
}

pub type BoxedBody = BoxBody<Bytes, hyper::Error>;

#[derive(Debug)]
pub enum ConnectionError {
    HttpError(String),
    ProxyError(String),
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ConnectionError {}

#[derive(Clone)]
pub struct ProxyConnector {
    client: ServoHttpConnector,
    matcher: std::sync::Arc<hyper_util::client::proxy::matcher::Matcher>,
}

impl ProxyConnector {
    fn new() -> Self {
        let matcher_builder = hyper_util::client::proxy::matcher::Matcher::builder()
            .http(servo_config::pref!(network_http_proxy_uri))
            .https(servo_config::pref!(network_https_proxy_uri))
            .no(servo_config::pref!(network_http_no_proxy));
        ProxyConnector {
            client: ServoHttpConnector::new(),
            matcher: std::sync::Arc::new(matcher_builder.build()),
        }
    }
}

impl Service<Destination> for ProxyConnector {
    type Response = TokioIo<TcpStream>;
    type Error = ConnectionError;
    type Future =
        std::pin::Pin<Box<dyn Future<Output = Result<TokioIo<TcpStream>, ConnectionError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.client
            .poll_ready(cx)
            .map_err(|e| ConnectionError::ProxyError(format!("{e}")))
    }

    fn call(&mut self, req: Destination) -> Self::Future {
        match self.matcher.intercept(&req) {
            Some(intercept) => Box::pin(
                Tunnel::new(intercept.uri().clone(), self.client.clone())
                    .call(req)
                    .map_err(|e| ConnectionError::ProxyError(format!("{e}"))),
            ),
            None => Box::pin(
                self.client
                    .call(req)
                    .map_err(|e| ConnectionError::ProxyError(format!("{e}"))),
            ),
        }
    }
}

pub type ServoClient = Client<ProxyConnector, BoxedBody>;

#[derive(Clone)]
struct TokioExecutor {}

impl<F> Executor<F> for TokioExecutor
where
    F: Future<Output = ()> + 'static + std::marker::Send,
{
    fn execute(&self, fut: F) {
        spawn_task(fut);
    }
}

pub fn create_http_client() -> ServoClient {
    Client::builder(TokioExecutor {})
        .build(ProxyConnector::new())
}
