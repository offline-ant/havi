/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Browser-owned local packet-store runtime.
//!
//! This is HAVI's default repo-backed runtime when `HAVI_HOME` is unset.
//! It uses the shared packet-store implementation directly in-process.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use embedder_traits::HpprProtocolError;
use hppr_client::{Greeting, HpprMessageRequest as HpprRequest, HpprResponse};
use hppr_packet::Packet;
use hppr_packet::chunk::{ChunkKind, ChunkManifest, is_chunk_manifest, parse_chunk_manifest};
use hppr_packet::packet::PacketType;
use hppr_packet::tai::Tai;
use hppr_packet::urc::{URC, urc};
use hppr_packet::{
    create_blob, create_plex, create_seal, read_packet, try_headers_data, wrap_plex_in_seal,
};
use hpprd::acl::Identity;
use hpprd::repository::{PacketStoreCore, Repository, SqliteRepository};

use super::config;
use super::state_db::{StateDb, global_state_db};

const LOCAL_REPO_NAME_KEY: &str = "local_runtime_repo_name";
const LOCAL_SIGNING_KEY_KEY: &str = "local_runtime_signing_key";
const LOCAL_VERIFYING_KEY_KEY: &str = "local_runtime_verifying_key";
const DEFAULT_LOCAL_REPO_NAME: &str = "havi-local";
const DEFAULT_LOCAL_SESSION_ID: &str = "local";
const DEFAULT_LOCAL_STATUS: &str = "browser-local";
const DEFAULT_LOCAL_BACKEND: &str = "inline-packet-store";

pub type BrowserLocalRuntimeHandle = Arc<BrowserLocalRuntime>;

pub fn default_repo_backed_runtime_is_local() -> bool {
    std::env::var("HAVI_HOME")
        .ok()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
}

pub fn global_local_runtime() -> BrowserLocalRuntimeHandle {
    static GLOBAL_LOCAL_RUNTIME: OnceLock<BrowserLocalRuntimeHandle> = OnceLock::new();
    GLOBAL_LOCAL_RUNTIME
        .get_or_init(|| {
            Arc::new(
                BrowserLocalRuntime::open_default().unwrap_or_else(|error| {
                    panic!("failed to initialize HAVI browser-local packet runtime: {}", error)
                }),
            )
        })
        .clone()
}

pub struct BrowserLocalRuntime {
    _tokio_runtime: tokio::runtime::Runtime,
    store: Arc<SqliteRepository>,
    repo_name: String,
    verifying_key: String,
    signing_key: String,
    packet_store_path: PathBuf,
}

impl BrowserLocalRuntime {
    pub fn open_default() -> Result<Self, String> {
        Self::open(config::packet_store_path(), global_state_db().as_ref())
    }

    pub fn open(packet_store_path: PathBuf, settings: &StateDb) -> Result<Self, String> {
        if let Some(parent) = packet_store_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create packet-store parent '{}': {}",
                    parent.display(),
                    error
                )
            })?;
        }

        let repo_name = settings
            .get_setting(LOCAL_REPO_NAME_KEY)?
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_LOCAL_REPO_NAME.to_string());
        let (signing_key, verifying_key) = load_or_create_local_keypair(settings)?;

        let tokio_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("failed to create browser-local tokio runtime: {}", error))?;
        let store = Arc::new(
            SqliteRepository::new_core_only(&packet_store_path, tokio_runtime.handle())
                .map_err(|error| format!("failed to open browser-local packet store: {}", error))?,
        );

        Ok(Self {
            _tokio_runtime: tokio_runtime,
            store,
            repo_name,
            verifying_key,
            signing_key,
            packet_store_path,
        })
    }

    pub fn packet_store_path(&self) -> &Path {
        &self.packet_store_path
    }

    pub fn repo_name(&self) -> &str {
        &self.repo_name
    }

    pub fn verifying_key(&self) -> &str {
        &self.verifying_key
    }

    pub fn signing_key(&self) -> &str {
        &self.signing_key
    }

    pub fn status_label(&self) -> &'static str {
        DEFAULT_LOCAL_STATUS
    }

    pub fn backend_label(&self) -> &'static str {
        DEFAULT_LOCAL_BACKEND
    }

    pub fn store_packet(&self, packet: &hppr_packet::PacketRef) -> Result<Vec<String>, String> {
        self.store
            .local_service()
            .commit_store(self.store.as_ref(), packet)
            .map_err(|error| error.to_string())?;
        Ok(packet.hashes().lines().map(str::to_string).collect())
    }

    pub fn get_packet(&self, urc_text: &str) -> Result<Packet, String> {
        let coord = parse_urc(urc_text)?;
        PacketStoreCore::get_packet(self.store.as_ref(), &coord).map_err(|error| error.to_string())
    }

    pub fn get_packet_by_hash(&self, hash: &str) -> Result<Packet, String> {
        PacketStoreCore::get_packet_by_hash(self.store.as_ref(), hash)
            .map_err(|error| error.to_string())
    }

    pub fn list_entries(&self, urc_text: &str) -> Result<Vec<String>, String> {
        let coord = parse_urc(urc_text)?;
        PacketStoreCore::list_entries(self.store.as_ref(), &coord).map_err(|error| error.to_string())
    }

    pub fn tip_lines(&self, urc_text: &str) -> Result<Vec<String>, String> {
        let coord = parse_urc(urc_text)?;
        Repository::tips(self.store.as_ref(), &coord, &Identity::internal())
            .map_err(|error| error.to_string())
    }

    pub fn detach_hash(&self, hash: &str) -> Result<(), String> {
        self.store
            .local_service()
            .commit_unindex_hash(self.store.as_ref(), hash)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn headers_text(&self, urc_text: &str) -> Result<String, String> {
        let packet = self.get_packet(urc_text)?;
        packet_headers_text(&packet)
    }

    pub fn hello_response(&self) -> Result<HpprResponse, String> {
        Ok(HpprResponse::greeting(build_local_greeting(
            self.repo_name(),
            self.verifying_key(),
            self.backend_label(),
        )?))
    }

    pub fn process_request(
        &self,
        request: HpprRequest,
        page_url: Option<&str>,
    ) -> Result<HpprResponse, HpprProtocolError> {
        match request {
            HpprRequest::Hello => self.hello_response().map_err(protocol_error),
            HpprRequest::Get { urc } => self
                .get_packet(&urc)
                .map(HpprResponse::packet)
                .map_err(protocol_error),
            HpprRequest::Headers { urc } => self
                .headers_text(&urc)
                .map(HpprResponse::lines)
                .map_err(protocol_error),
            HpprRequest::List { urc } => self
                .list_entries(&urc)
                .map(lines_response)
                .map_err(protocol_error),
            HpprRequest::Tips { urc } => self
                .tip_lines(&urc)
                .map(lines_response)
                .map_err(protocol_error),
            HpprRequest::Store { packet } => {
                let packet = read_packet(packet)
                    .map_err(|error| protocol_error(format!("invalid STORE packet: {}", error)))?;
                self.store_packet(packet.as_pkt_ref())
                    .map(lines_response)
                    .map_err(protocol_error)
            }
            HpprRequest::Ingest { packet } => {
                let packet = read_packet(packet)
                    .map_err(|error| protocol_error(format!("invalid INGEST packet: {}", error)))?;
                self.store_packet(packet.as_pkt_ref())
                    .map(lines_response)
                    .map_err(protocol_error)
            }
            HpprRequest::Add { headers, data, .. } => {
                let packet = self
                    .packet_from_add_args(&headers, data.as_deref().unwrap_or(&[]), page_url)
                    .map_err(protocol_error)?;
                self.store_packet(packet.as_pkt_ref())
                    .map(lines_response)
                    .map_err(protocol_error)
            }
            HpprRequest::Detach { hash } => {
                self.detach_hash(&hash).map_err(protocol_error)?;
                Ok(HpprResponse::empty())
            }
            HpprRequest::Members { .. } => Err(protocol_error(
                "browser-local runtime does not expose MEMBERS",
            )),
            HpprRequest::Generic { cmd, .. } => Err(protocol_error(format!(
                "browser-local runtime does not expose generic command {}",
                cmd
            ))),
        }
    }

    pub fn store_text_page(
        &self,
        group: &str,
        api: &str,
        key: &str,
        content_type: &str,
        body: &[u8],
    ) -> Result<Vec<String>, String> {
        let packet = create_seal(
            self.signing_key(),
            group,
            api,
            key,
            &Tai::now(),
            &[("Content-Type", content_type)],
            body,
        )
        .map_err(|error| error.to_string())?;
        self.store_packet(packet.as_pkt_ref())
    }

    pub fn get_local_route_api(
        &self,
        group: &str,
        api: &str,
        repo_vkey: &str,
    ) -> Result<super::client::LocalRouteApiInfo, String> {
        let packet = self.get_packet(&format!("//repo/route/api//{}/{}/|/seal/{}", group, api, repo_vkey))?;
        parse_local_route_api_packet(&packet)
    }

    pub fn get_local_route_group(
        &self,
        group: &str,
        repo_vkey: &str,
    ) -> Result<super::client::LocalRouteGroupInfo, String> {
        let packet = self.get_packet(&format!("//repo/route/group//{}/|/seal/{}", group, repo_vkey))?;
        parse_local_route_group_packet(&packet)
    }

    pub fn get_route_auth(
        &self,
        group: &str,
        api: Option<&str>,
        repo_vkey: &str,
    ) -> Result<super::client::RouteAuthInfo, String> {
        let mut urcs = Vec::with_capacity(2);
        if let Some(api) = api {
            urcs.push(format!("//repo/route/auth//{}/{}/|/seal/{}", group, api, repo_vkey));
        }
        urcs.push(format!("//repo/route/auth//{}/|/seal/{}", group, repo_vkey));

        for urc_text in urcs {
            let packet = match self.get_packet(&urc_text) {
                Ok(packet) => packet,
                Err(_) => continue,
            };
            if let Some(auth) = packet.header("Auth") {
                return Ok(super::client::RouteAuthInfo {
                    auth: auth.to_string(),
                });
            }
        }

        Err("Route auth packet not found".to_string())
    }

    fn packet_from_add_args(
        &self,
        headers: &[u8],
        body: &[u8],
        page_url: Option<&str>,
    ) -> Result<Packet, String> {
        let mut seal_by = None;
        let mut group = None;
        let mut api = None;
        let mut key = None;
        let mut tai: Option<Tai> = None;
        let mut pre_plex_ref_headers: Vec<(&str, &str)> = Vec::new();
        let mut post_plex_ref_headers: Vec<(&str, &str)> = Vec::new();
        let mut blob_reference = None;
        let mut plex_reference = None;
        let mut data_length: Option<usize> = None;
        let mut saw_plex_markline = false;

        let (header_bytes, inline_data) =
            try_headers_data(headers).unwrap_or((headers, body));
        for line in std::str::from_utf8(header_bytes)
            .map_err(|error| error.to_string())?
            .split('\n')
        {
            if line.is_empty() {
                continue;
            }
            let (header_name, value) = line
                .split_once(": ")
                .ok_or_else(|| "INVALID headers: missing ': '".to_string())?;

            if header_name == "Data-Length" {
                if data_length.is_some() {
                    return Err("INVALID headers: duplicate Data-Length".to_string());
                }
                if value.is_empty()
                    || !value.chars().all(|c| c.is_ascii_digit())
                    || (value.len() > 1 && value.starts_with('0'))
                {
                    return Err("INVALID headers: invalid Data-Length".to_string());
                }
                data_length = Some(value.parse::<usize>().map_err(|error| error.to_string())?);
                continue;
            }

            match header_name {
                "Seal-By" => {
                    if seal_by.is_some() {
                        return Err("INVALID headers: duplicate Seal-By".to_string());
                    }
                    seal_by = Some(value);
                }
                "Group" => {
                    if saw_plex_markline {
                        return Err(
                            "INVALID headers: Group must be listed before 🖧: P.".to_string(),
                        );
                    }
                    group = Some(value);
                }
                "API" => {
                    if saw_plex_markline {
                        return Err(
                            "INVALID headers: API must be listed before 🖧: P.".to_string(),
                        );
                    }
                    api = Some(value);
                }
                "Key" => {
                    if saw_plex_markline {
                        return Err(
                            "INVALID headers: Key must be listed before 🖧: P.".to_string(),
                        );
                    }
                    key = Some(value);
                }
                "TAI" => {
                    if saw_plex_markline {
                        return Err(
                            "INVALID headers: TAI must be listed before 🖧: P.".to_string(),
                        );
                    }
                    tai = Some(Tai::parse(value.to_string()).map_err(|error| error.to_string())?);
                }
                "🖧" => match value.as_bytes().first() {
                    Some(b'B') => {
                        if blob_reference.is_some() {
                            return Err("INVALID headers: duplicate blob ref".to_string());
                        }
                        blob_reference = Some(value);
                    }
                    Some(b'P') => {
                        if plex_reference.is_some() {
                            return Err("INVALID headers: duplicate plex ref".to_string());
                        }
                        saw_plex_markline = true;
                        plex_reference = Some(value);
                    }
                    Some(b'S') => {
                        return Err(
                            "INVALID headers: Seal markline not supported in ADD create mode"
                                .to_string(),
                        )
                    }
                    _ => {
                        return Err(format!(
                            "INVALID headers: unrecognized markline value ({})",
                            value
                        ))
                    }
                },
                "Seal-Sig" => {
                    return Err("INVALID headers: Seal-Sig not allowed".to_string())
                }
                _ => {
                    if plex_reference.is_none() {
                        pre_plex_ref_headers.push((header_name, value));
                    } else {
                        post_plex_ref_headers.push((header_name, value));
                    }
                }
            }
        }

        let has_custom_headers = !pre_plex_ref_headers.is_empty() || !post_plex_ref_headers.is_empty();
        let has_plex_headers = group.is_some()
            || api.is_some()
            || key.is_some()
            || tai.is_some()
            || blob_reference.is_some()
            || has_custom_headers;

        enum Variant {
            Blob,
            Plex,
            Seal,
        }

        let variant = if seal_by.is_some() {
            Variant::Seal
        } else if has_plex_headers {
            Variant::Plex
        } else {
            Variant::Blob
        };

        if let Some(expected) = data_length
            && inline_data.len() != expected
        {
            return Err("INVALID headers: Data-Length mismatch".to_string());
        }

        if blob_reference.is_some() && !inline_data.is_empty() {
            return Err(
                "INVALID headers: 🖧: B... cannot be combined with inline data".to_string(),
            );
        }

        if matches!(variant, Variant::Blob) {
            if blob_reference.is_some() {
                return Err(
                    "INVALID headers: Creating a blob from a blob reference is meaningless"
                        .to_string(),
                );
            }
            return create_blob(inline_data).map_err(|error| error.to_string());
        }

        let referenced_plex = match plex_reference {
            Some(hash) => Some(self.get_packet(&format!("////{}", hash))?),
            None => None,
        };

        let blob_bytes: Cow<'_, [u8]> = if !inline_data.is_empty() {
            Cow::Borrowed(inline_data)
        } else if let Some(hash) = blob_reference {
            Cow::Owned(self.get_packet(&format!("////{}", hash))?.data().to_vec())
        } else if let Some(packet) = referenced_plex.as_ref() {
            Cow::Borrowed(packet.data())
        } else {
            Cow::Borrowed(&[])
        };

        let referenced_headers = referenced_plex.as_ref().map(|packet| packet.unpack());
        let (default_group, default_api, default_key) =
            default_page_context(page_url).unwrap_or_else(|| {
                (
                    "u".to_string(),
                    "index".to_string(),
                    "root".to_string(),
                )
            });
        let group_value = group
            .or_else(|| referenced_headers.as_ref().and_then(|packet| packet.group))
            .unwrap_or(default_group.as_str());
        let api_value = api
            .or_else(|| referenced_headers.as_ref().and_then(|packet| packet.api))
            .unwrap_or(default_api.as_str());
        let key_value = key
            .or_else(|| referenced_headers.as_ref().and_then(|packet| packet.key))
            .unwrap_or(default_key.as_str());
        let tai = tai
            .or_else(|| referenced_headers.as_ref().and_then(|packet| packet.tai.map(|value| value.owned())))
            .unwrap_or_else(Tai::now);

        let merged_headers: Vec<(&str, &str)> = pre_plex_ref_headers
            .into_iter()
            .chain(
                referenced_headers
                    .as_ref()
                    .map(|packet| packet.custom_headers())
                    .into_iter()
                    .flatten(),
            )
            .chain(post_plex_ref_headers)
            .collect();

        let signing_key = match seal_by {
            Some(value) => Some(self.resolve_signing_key(value)?),
            None => None,
        };

        if !has_plex_headers
            && let Some(signing_key) = signing_key.as_ref()
            && let Some(plex) = referenced_plex
        {
            return wrap_plex_in_seal(signing_key, &plex).map_err(|error| error.to_string());
        }

        match signing_key {
            Some(signing_key) => create_seal(
                &signing_key,
                group_value,
                api_value,
                key_value,
                &tai,
                &merged_headers,
                blob_bytes.as_ref(),
            )
            .map_err(|error| error.to_string()),
            None => create_plex(
                group_value,
                api_value,
                key_value,
                &tai,
                &merged_headers,
                blob_bytes.as_ref(),
            )
            .map_err(|error| error.to_string()),
        }
    }

    fn resolve_signing_key(&self, seal_by: &str) -> Result<String, String> {
        if let Some((verifying_key, signing_key)) = seal_by.split_once(' ') {
            hppr_packet::crypto::t_b64a_h3_decode(signing_key)
                .map_err(|error| format!("INVALID Seal-By secret key: {}", error))?;
            let (_, signing_key_bytes) = hppr_packet::crypto::t_b64a_h3_decode(signing_key)
                .map_err(|error| format!("INVALID Seal-By secret key: {}", error))?;
            let derived = hppr_packet::crypto::get_verification_key(&signing_key_bytes)
                .map_err(|error| error.to_string())?;
            if derived != verifying_key {
                return Err(format!(
                    "INVALID Seal-By: signing key did not match provided {}",
                    verifying_key
                ));
            }
            return Ok(signing_key.to_string());
        }

        if seal_by == "ring0" {
            return Ok(self.signing_key().to_string());
        }

        Err(
            "INVALID Seal-By: expected ring0 or \"<vkey> <skey>\"".to_string(),
        )
    }

    pub fn read_packet_bytes(
        &self,
        packet_hash: &str,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, String> {
        if length == 0 {
            return Ok(Vec::new());
        }

        let packet = self.get_packet_by_hash(packet_hash)?;
        let headers = packet
            .headers()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect::<Vec<_>>();

        if is_chunk_manifest(&headers) {
            let manifest =
                parse_chunk_manifest(&headers).map_err(|error| format!("invalid chunk manifest: {}", error))?;
            return self.read_manifest_bytes(&manifest, offset, length);
        }

        Ok(slice_bytes(packet.data(), offset, length).to_vec())
    }

    fn read_manifest_bytes(
        &self,
        manifest: &ChunkManifest,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, String> {
        let Some((start, end)) = clamp_range(manifest.total_length, offset, length) else {
            return Ok(Vec::new());
        };

        let mut out = Vec::with_capacity((end - start) as usize);
        for chunk in &manifest.chunks {
            if chunk.end <= start || chunk.start >= end {
                continue;
            }

            let read_start = start.max(chunk.start);
            let read_end = end.min(chunk.end);
            let local_offset = read_start - chunk.start;
            let local_len = (read_end - read_start) as usize;
            let bytes = match chunk.kind {
                ChunkKind::Blob => {
                    let packet = self.get_packet_by_hash(&chunk.hash)?;
                    slice_bytes(packet.data(), local_offset, local_len).to_vec()
                }
                ChunkKind::Manifest => self.read_packet_bytes(&chunk.hash, local_offset, local_len)?,
            };
            if bytes.len() != local_len {
                return Err(format!(
                    "chunk {} returned {} bytes, expected {}",
                    chunk.hash,
                    bytes.len(),
                    local_len
                ));
            }
            out.extend_from_slice(&bytes);
        }

        Ok(out)
    }
}

fn build_local_greeting(
    repo_name: &str,
    verifying_key: &str,
    backend: &str,
) -> Result<Greeting, String> {
    let packet = format!(
        "🖧: 0.H3\nCommand-Flow: session\nSession-ID: {}\nRepo-Name: {}\nSeal-By: {}\nFormat: H3\nSession-Commands: 🖧HELLO 1 | 🖧GET 1 | 🖧HEADERS 1 | 🖧LIST 1 | 🖧STORE 1 | 🖧TIPS 1\nAllow-Null-Command: 0\nStatus: {}\nHpprd-Backend: {}\nData-Length: 0\n\n",
        DEFAULT_LOCAL_SESSION_ID,
        repo_name,
        verifying_key,
        DEFAULT_LOCAL_STATUS,
        backend,
    );
    let packet = read_packet(packet.into_bytes()).map_err(|error| error.to_string())?;
    Greeting::from_packet(packet.as_pkt_ref()).map_err(|error| error.to_string())
}

fn load_or_create_local_keypair(settings: &StateDb) -> Result<(String, String), String> {
    let signing_key = settings.get_setting(LOCAL_SIGNING_KEY_KEY)?;
    let verifying_key = settings.get_setting(LOCAL_VERIFYING_KEY_KEY)?;
    match (signing_key, verifying_key) {
        (Some(signing_key), Some(verifying_key)) => Ok((signing_key, verifying_key)),
        _ => {
            let (signing_key, verifying_key) =
                hppr_packet::crypto::generate_signing_verifying_pair();
            settings.set_setting(LOCAL_SIGNING_KEY_KEY, &signing_key)?;
            settings.set_setting(LOCAL_VERIFYING_KEY_KEY, &verifying_key)?;
            Ok((signing_key, verifying_key))
        }
    }
}

fn protocol_error(detail: impl Into<String>) -> HpprProtocolError {
    HpprProtocolError {
        error_type: "INTERNAL".to_string(),
        detail: detail.into(),
        fatal: false,
    }
}

fn parse_urc(urc_text: &str) -> Result<URC, String> {
    urc(urc_text).map_err(|error| error.to_string())
}

fn lines_response(lines: Vec<String>) -> HpprResponse {
    let mut text = String::new();
    for line in lines {
        text.push_str(&line);
        text.push('\n');
    }
    HpprResponse::lines(text)
}

fn packet_headers_text(packet: &Packet) -> Result<String, String> {
    let bytes = packet.as_bytes();
    let marker = b"\n\n";
    let Some(index) = bytes.windows(marker.len()).position(|window| window == marker) else {
        return Err("packet headers missing blank line".to_string());
    };
    String::from_utf8(bytes[..index + marker.len()].to_vec()).map_err(|error| error.to_string())
}

fn default_page_context(page_url: Option<&str>) -> Option<(String, String, String)> {
    let page_url = page_url?;
    let address = super::url::HAVIAddress::parse(page_url).ok()?;
    if address.is_listing() {
        return None;
    }
    let parts = address.parts();
    if parts.group.is_empty() || parts.api.is_empty() {
        return None;
    }
    let requested_key = address.key_with_slash();
    let key = if requested_key.is_empty() || requested_key == "/" {
        "index.html".to_string()
    } else {
        requested_key.trim_matches('/').to_string()
    };
    Some((parts.group, parts.api, key))
}

fn parse_local_route_api_packet(packet: &Packet) -> Result<super::client::LocalRouteApiInfo, String> {
    let packet_type = hppr_packet::Packet::parse(packet.as_bytes().to_vec().into_boxed_slice())
        .map(|packet| packet.packet_type())
        .unwrap_or(PacketType::Null);
    if packet_type != PacketType::Seal {
        return Err(format!(
            "Local route API packet is not sealed (got type: {:?})",
            packet_type
        ));
    }
    let upstream = match packet.header("Upstream") {
        Some(value) => Some(
            hppr_client::parse_via(value)
                .map_err(|error| format!("invalid Upstream header '{}': {}", value, error))?,
        ),
        None => None,
    };
    Ok(super::client::LocalRouteApiInfo {
        upstream,
        upstream_verifier: packet
            .header("Upstream-Verifier")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        content_authority: packet
            .header("Content-Authority")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    })
}

fn parse_local_route_group_packet(packet: &Packet) -> Result<super::client::LocalRouteGroupInfo, String> {
    let upstream_raw = packet
        .header("Upstream")
        .ok_or_else(|| "Local route group packet missing Upstream header".to_string())?;
    let upstream = hppr_client::parse_via(upstream_raw)
        .map_err(|error| format!("invalid Upstream header '{}': {}", upstream_raw, error))?;
    let route_authority_key = packet
        .header("Route-Authority-Key")
        .ok_or_else(|| "Local route group packet missing Route-Authority-Key header".to_string())?
        .to_string();
    Ok(super::client::LocalRouteGroupInfo {
        upstream,
        route_authority_key,
        upstream_verifier: packet
            .header("Upstream-Verifier")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        home_api: packet
            .header("Home-API")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    })
}

fn clamp_range(total: u64, offset: u64, length: usize) -> Option<(u64, u64)> {
    if length == 0 || offset >= total {
        return None;
    }
    let end = offset.saturating_add(length as u64).min(total);
    Some((offset, end))
}

fn slice_bytes(bytes: &[u8], offset: u64, length: usize) -> &[u8] {
    let Some((start, end)) = clamp_range(bytes.len() as u64, offset, length) else {
        return &[];
    };
    &bytes[start as usize..end as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use hppr_client::ResponseKind;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_paths(name: &str) -> (PathBuf, PathBuf) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("havi-local-runtime-{}-{}", name, stamp));
        (root.join("packets.sqlite"), root.join("state.sqlite"))
    }

    #[test]
    fn local_greeting_uses_session_command_capabilities() {
        let greeting =
            build_local_greeting("havi-local", "V.test.H3", "inline-packet-store").unwrap();
        let raw = std::str::from_utf8(greeting.raw_bytes()).unwrap();

        assert!(raw.contains("\nCommand-Flow: session\n"));
        assert!(raw.contains("\nSession-Commands: 🖧HELLO 1 | 🖧GET 1"));
        assert!(!raw.contains("\nCommands:"));
    }

    #[test]
    fn local_runtime_store_get_and_list_roundtrip() {
        let (packet_path, state_path) = temp_paths("roundtrip");
        let state = StateDb::open(state_path.clone()).unwrap();
        let runtime = BrowserLocalRuntime::open(packet_path.clone(), &state).unwrap();

        runtime
            .store_text_page(
                "~localruntime",
                "app",
                "index.html",
                "text/html; charset=utf-8",
                b"<h1>hello</h1>",
            )
            .unwrap();

        let packet = runtime.get_packet("//~localruntime/app//index.html").unwrap();
        assert_eq!(packet.header("Content-Type"), Some("text/html; charset=utf-8"));
        assert_eq!(packet.data(), b"<h1>hello</h1>");

        let list = runtime.list_entries("//~localruntime/app//index.html/").unwrap();
        assert!(list.contains(&"|/".to_string()));

        let _ = std::fs::remove_file(packet_path);
        let _ = std::fs::remove_file(state_path);
    }

    #[test]
    fn local_runtime_add_uses_page_context_defaults() {
        let (packet_path, state_path) = temp_paths("add");
        let state = StateDb::open(state_path.clone()).unwrap();
        let runtime = BrowserLocalRuntime::open(packet_path.clone(), &state).unwrap();

        let response = runtime
            .process_request(
                HpprRequest::Add {
                    headers: b"Key: user/test.txt\nContent-Type: text/plain\n".to_vec(),
                    data: Some(b"hello local add".to_vec()),
                    seal_with: None,
                },
                Some("hppr://~localruntime/app//index.html"),
            )
            .unwrap();
        assert!(matches!(response.kind, ResponseKind::Lines(_)));

        let packet = runtime.get_packet("//~localruntime/app//user/test.txt").unwrap();
        assert_eq!(packet.data(), b"hello local add");

        let _ = std::fs::remove_file(packet_path);
        let _ = std::fs::remove_file(state_path);
    }
}
