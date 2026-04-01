/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use crate::hppr::client::HpprdClientAsync;
use crate::hppr::credentials::CredentialStoreHandle;
use crate::hppr::join_fixture::{JoinFixtureState, get_join_fixture_state, set_join_fixture_state};
use crate::hppr::util::{append_location, signing_to_verifying_key};

async fn inspect_route_content_pointer_auth_join(
    group: &str,
    app: &str,
    location: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> serde_json::Value {
    if group.trim().is_empty() || app.trim().is_empty() {
        return serde_json::json!({"error": "missing group/app"});
    }

    let fixture = get_join_fixture_state();

    let mut route_endpoint = client.target();
    let mut route_upstream_key: Option<String> = None;
    let mut route_json = serde_json::json!({
        "configured": false,
        "endpoint": route_endpoint.to_string(),
        "upstreamVerificationKey": serde_json::Value::Null,
        "error": serde_json::Value::Null,
    });

    let mut local_repo_vkey: Option<String> = None;

    if credential_store.get_admin().is_some() {
        match client
            .get_admin_identity()
            .await
        {
            Ok(repo_vkey) => {
                local_repo_vkey = Some(repo_vkey.clone());
                match client
                    .get_route(group, app, &repo_vkey)
                    .await
                {
                    Ok(route) => {
                        if let Some(upstream) = route.upstream.clone() {
                            route_endpoint = upstream;
                        }
                        route_upstream_key = route.upstream_verification_key.clone();
                        route_json = serde_json::json!({
                            "configured": true,
                            "endpoint": route_endpoint.to_string(),
                            "upstreamVerificationKey": route_upstream_key,
                            "error": serde_json::Value::Null,
                        });
                    },
                    Err(e) => {
                        route_json = serde_json::json!({
                            "configured": false,
                            "endpoint": route_endpoint.to_string(),
                            "upstreamVerificationKey": serde_json::Value::Null,
                            "error": e,
                        });
                    },
                }
            },
            Err(e) => {
                route_json["error"] = serde_json::json!(e);
            },
        }
    } else {
        route_json["error"] = serde_json::json!("admin credentials unavailable");
    }

    let public_network_json = match hppr_client::lookup_network_if_public_async(group, app).await {
        Ok(Some(lookup)) => serde_json::json!({
            "available": true,
            "endpoint": lookup.endpoint.to_string(),
            "upstreamVerificationKey": lookup.upstream_verification_key,
            "contentAuthority": lookup.content_authority,
            "rootSigner": lookup.root_signer,
            "groupNetworkKey": lookup.group_record.as_ref().map(|r| r.network_key.clone()),
            "groupChain": lookup.group_chain.iter().map(|r| serde_json::json!({
                "parentGroup": r.parent_group.clone(),
                "childLabel": r.child_label.clone(),
                "resolvedGroup": r.resolved_group.clone(),
                "networkKey": r.network_key.clone(),
                "upstream": r.upstream.to_string(),
                "upstreamVerificationKey": r.upstream_verification_key.clone(),
                "homeApp": r.home_app.clone(),
            })).collect::<Vec<_>>(),
            "error": serde_json::Value::Null,
        }),
        Ok(None) => serde_json::json!({
            "available": false,
            "endpoint": serde_json::Value::Null,
            "upstreamVerificationKey": serde_json::Value::Null,
            "contentAuthority": serde_json::Value::Null,
            "rootSigner": serde_json::Value::Null,
            "groupNetworkKey": serde_json::Value::Null,
            "groupChain": serde_json::Value::Null,
            "error": "not public name",
        }),
        Err(e) => serde_json::json!({
            "available": false,
            "endpoint": serde_json::Value::Null,
            "upstreamVerificationKey": serde_json::Value::Null,
            "contentAuthority": serde_json::Value::Null,
            "rootSigner": serde_json::Value::Null,
            "groupNetworkKey": serde_json::Value::Null,
            "groupChain": serde_json::Value::Null,
            "error": e.to_string(),
        }),
    };

    let route_anyone = Arc::new(HpprdClientAsync::new_with_signer(
        route_endpoint.clone(),
        hppr_client::Signer::anyone(),
    ));

    let mut content_pointer_json = serde_json::json!({
        "available": false,
        "endpoint": route_endpoint.to_string(),
        "repoVerificationKey": serde_json::Value::Null,
        "root": serde_json::Value::Null,
        "signer": serde_json::Value::Null,
        "targetGet": serde_json::Value::Null,
        "error": serde_json::Value::Null,
    });

    let remote_repo_vkey = match &route_upstream_key {
        Some(v) => Some(v.clone()),
        None => match route_anyone.get_admin_identity().await {
            Ok(v) => Some(v),
            Err(e) => {
                content_pointer_json["error"] = serde_json::json!(e);
                None
            },
        },
    };

    let mut content_root: Option<String> = None;
    let mut content_authority: Option<String> = None;
    let mut target_get: Option<String> = None;

    if let Some(repo_vkey) = remote_repo_vkey.clone() {
        match route_anyone.get_content_pointer(group, app, &repo_vkey).await {
            Ok(content_pointer) => {
                let target = append_location(&content_pointer.root, location);
                let target_urc = format!("{}/|/seal/{}", target, content_pointer.authority);
                content_root = Some(content_pointer.root.clone());
                content_authority = Some(content_pointer.authority.clone());
                target_get = Some(target_urc.clone());
                content_pointer_json = serde_json::json!({
                    "available": true,
                    "endpoint": route_endpoint.to_string(),
                    "repoVerificationKey": repo_vkey,
                    "root": content_pointer.root,
                    "authority": content_pointer.authority,
                    "targetGet": target_urc,
                    "error": serde_json::Value::Null,
                });
            },
            Err(e) => {
                content_pointer_json["repoVerificationKey"] = serde_json::json!(repo_vkey);
                content_pointer_json["error"] = serde_json::json!(e);
            },
        }
    }

    let mut route_key_present = false;
    let mut route_signing_key: Option<String> = None;
    let mut requester_vkey: Option<String> = None;
    let mut route_key_error: Option<String> = None;

    if let (Some(_), Some(repo_vkey)) = (credential_store.get_admin(), local_repo_vkey.clone()) {
        match client
            .get_route_key(group, &repo_vkey)
            .await
        {
            Ok(route_key) => {
                route_key_present = true;
                route_signing_key = Some(route_key.signing_key.clone());
                requester_vkey = signing_to_verifying_key(&route_key.signing_key).ok();
            },
            Err(e) => {
                route_key_error = Some(e);
            },
        }
    } else {
        route_key_error = Some("admin credentials unavailable".to_string());
    }

    let mut auth_probe = "not_checked".to_string();
    let mut auth_error: Option<String> = None;

    if let (Some(root), Some(content_authority_value), Some(signing_key)) = (
        content_root.as_ref(),
        content_authority.as_ref(),
        route_signing_key.as_ref(),
    ) {
        let ring2_signer = hppr_client::Signer::ring2(group, signing_key);
        let route_auth = Arc::new(HpprdClientAsync::new_with_signer(
            route_endpoint.clone(),
            ring2_signer,
        ));
        let target = append_location(root, location);
        let target_urc = format!("{}/|/seal/{}", target, content_authority_value);
        match route_auth.get_packet_authenticated(&target_urc).await {
            Ok(_) => auth_probe = "authorized".to_string(),
            Err(e) => {
                if e.contains("UNAUTHORIZED") {
                    auth_probe = "unauthorized".to_string();
                } else {
                    auth_probe = "error".to_string();
                    auth_error = Some(e);
                }
            },
        }
    }

    let mut join_remote = "not_checked".to_string();
    let mut join_error: Option<String> = None;
    let mut join_reply_path: Option<String> = None;
    let mut join_request_path: Option<String> = None;

    if let Some(vkey) = requester_vkey.as_ref() {
        let reply_path = format!("//{}/admin/request/join/{}/reply/|", group, vkey);
        let request_path = format!("//{}/admin/request/join/|/seal/{}", group, vkey);
        join_reply_path = Some(reply_path.clone());
        join_request_path = Some(request_path.clone());

        match route_anyone.get_packet_authenticated(&reply_path).await {
            Ok(packet) => {
                let status = packet
                    .header("Request-Status")
                    .unwrap_or("unknown")
                    .trim()
                    .to_ascii_lowercase();
                join_remote = match status.as_str() {
                    "approved" | "denied" | "pending" => status,
                    _ => "unknown".to_string(),
                };
            },
            Err(reply_err) => match route_anyone.get_packet_authenticated(&request_path).await {
                Ok(_) => {
                    join_remote = "pending".to_string();
                },
                Err(request_err) => {
                    if reply_err.contains("NOT_FOUND") && request_err.contains("NOT_FOUND") {
                        join_remote = "none".to_string();
                    } else {
                        join_remote = "error".to_string();
                        join_error = Some(format!("reply: {}; request: {}", reply_err, request_err));
                    }
                },
            },
        }
    }

    let fixture_str = fixture.as_str().to_string();
    let effective_join = match fixture {
        JoinFixtureState::None => join_remote.clone(),
        JoinFixtureState::Pending => "pending".to_string(),
        JoinFixtureState::Approved => "approved".to_string(),
    };

    serde_json::json!({
        "route": route_json,
        "publicNetwork": public_network_json,
        "deploy": content_pointer_json,
        "auth": {
            "routeKeyPresent": route_key_present,
            "routeKeyError": route_key_error,
            "requesterVerificationKey": requester_vkey,
            "probe": auth_probe,
            "error": auth_error,
            "targetGet": target_get,
        },
        "join": {
            "remote": join_remote,
            "fixture": fixture_str,
            "effective": effective_join,
            "replyPath": join_reply_path,
            "requestPath": join_request_path,
            "error": join_error,
        },
    })
}

pub async fn handle_diagnostics_api(
    path: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> String {
    let query = path.split('?').nth(1).unwrap_or("");
    let params: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();

    let cmd = params.get("cmd").map(|s| s.as_str()).unwrap_or("inspect");

    match cmd {
        "join_fixture_get" => serde_json::json!({
            "ok": true,
            "data": {"state": get_join_fixture_state().as_str()}
        })
        .to_string(),
        "join_fixture_set" => {
            let Some(raw_state) = params.get("state") else {
                return serde_json::json!({"ok": false, "error": "missing state"}).to_string();
            };
            let Some(state) = JoinFixtureState::parse(raw_state) else {
                return serde_json::json!({"ok": false, "error": "invalid state (expected none|pending|approved)"}).to_string();
            };
            set_join_fixture_state(state);
            serde_json::json!({
                "ok": true,
                "data": {"state": state.as_str()}
            })
            .to_string()
        },
        "inspect" => {
            let Some(group) = params.get("group") else {
                return serde_json::json!({"ok": false, "error": "missing group"}).to_string();
            };
            let Some(app) = params.get("app") else {
                return serde_json::json!({"ok": false, "error": "missing app"}).to_string();
            };
            let location = params.get("location").map(String::as_str).unwrap_or("");
            let data = inspect_route_content_pointer_auth_join(group, app, location, client, credential_store).await;
            serde_json::json!({"ok": true, "data": data}).to_string()
        },
        _ => serde_json::json!({"ok": false, "error": format!("unknown command: {}", cmd)})
            .to_string(),
    }
}
