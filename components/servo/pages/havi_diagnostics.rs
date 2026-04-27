/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use crate::hppr::client::HpprdClientAsync;
use crate::hppr::credentials::CredentialStoreHandle;
use crate::hppr::util::{append_location, signing_to_verifying_key};

async fn inspect_route_content_pointer_auth(
    group: &str,
    app: &str,
    location: &str,
    client: &Arc<HpprdClientAsync>,
    credential_store: &CredentialStoreHandle,
) -> serde_json::Value {
    if group.trim().is_empty() || app.trim().is_empty() {
        return serde_json::json!({"error": "missing group/app"});
    }

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
        match client.get_admin_identity().await {
            Ok(repo_vkey) => {
                local_repo_vkey = Some(repo_vkey.clone());
                match client.get_local_route_app(group, app, &repo_vkey).await {
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
                    }
                    Err(e) => {
                        route_json = serde_json::json!({
                            "configured": false,
                            "endpoint": route_endpoint.to_string(),
                            "upstreamVerificationKey": serde_json::Value::Null,
                            "error": e,
                        });
                    }
                }
            }
            Err(e) => {
                route_json["error"] = serde_json::json!(e);
            }
        }
    } else {
        route_json["error"] = serde_json::json!("admin credentials unavailable");
    }

    let public_network_json = match hppr_client::lookup_route_if_public_async(group, app).await {
        Ok(Some(lookup)) => serde_json::json!({
            "available": true,
            "endpoint": lookup.endpoint.to_string(),
            "upstreamVerificationKey": lookup.upstream_verification_key,
            "contentAuthority": lookup.content_authority,
            "rootSigner": lookup.root_signer,
            "groupRouteAuthorityKey": lookup.group_record.as_ref().map(|r| r.route_authority_key.clone()),
            "groupChain": lookup.group_chain.iter().map(|r| serde_json::json!({
                "parentGroup": r.parent_group.clone(),
                "childLabel": r.child_label.clone(),
                "resolvedGroup": r.resolved_group.clone(),
                "routeAuthorityKey": r.route_authority_key.clone(),
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
            "groupRouteAuthorityKey": serde_json::Value::Null,
            "groupChain": serde_json::Value::Null,
            "error": "not public name",
        }),
        Err(e) => serde_json::json!({
            "available": false,
            "endpoint": serde_json::Value::Null,
            "upstreamVerificationKey": serde_json::Value::Null,
            "contentAuthority": serde_json::Value::Null,
            "rootSigner": serde_json::Value::Null,
            "groupRouteAuthorityKey": serde_json::Value::Null,
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
            }
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
            }
            Err(e) => {
                content_pointer_json["repoVerificationKey"] = serde_json::json!(repo_vkey);
                content_pointer_json["error"] = serde_json::json!(e);
            }
        }
    }

    let mut route_key_present = false;
    let mut route_signing_key: Option<String> = None;
    let mut requester_vkey: Option<String> = None;
    let mut route_key_error: Option<String> = None;

    if let (Some(_), Some(repo_vkey)) = (credential_store.get_admin(), local_repo_vkey.clone()) {
        match client.get_route_auth(group, Some(app), &repo_vkey).await {
            Ok(route_key) => {
                route_key_present = true;
                if let Ok(hppr_client::Signer::Ring2 { signing_key, .. }) =
                    hppr_client::Signer::parse(&route_key.auth)
                {
                    requester_vkey = signing_to_verifying_key(&signing_key).ok();
                    route_signing_key = Some(signing_key);
                }
            }
            Err(e) => {
                route_key_error = Some(e);
            }
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
            }
        }
    }

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
        "inspect" => {
            let Some(group) = params.get("group") else {
                return serde_json::json!({"ok": false, "error": "missing group"}).to_string();
            };
            let Some(app) = params.get("app") else {
                return serde_json::json!({"ok": false, "error": "missing app"}).to_string();
            };
            let location = params.get("location").map(String::as_str).unwrap_or("");
            let data = inspect_route_content_pointer_auth(group, app, location, client, credential_store).await;
            serde_json::json!({"ok": true, "data": data}).to_string()
        }
        _ => serde_json::json!({"ok": false, "error": format!("unknown command: {}", cmd)})
            .to_string(),
    }
}
