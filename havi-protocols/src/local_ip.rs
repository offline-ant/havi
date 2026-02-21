/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Local IP detection for Address sharing.

use std::net::UdpSocket;

/// Get the local IP address using UDP socket trick.
///
/// This creates a UDP socket, "connects" to an external IP (no actual traffic),
/// and reads back which local interface would be used.
pub fn get_local_ip() -> Option<String> {
    // Allow override via environment variable
    if let Ok(ip) = std::env::var("HPPR_SHARE_IP") {
        return Some(ip);
    }

    // UDP socket trick - no actual network traffic
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}
