//! Parity proof for the embedded HTML GUI's named-groups surface.
//!
//! Complements `parity_cli` (CLI surface) and `api_manifest` (downstream
//! clients). The GUI is a single file — the test scans it for concrete
//! `api(...)` call sites rather than running a browser, so regressions
//! surface at `cargo nextest` time instead of waiting for a manual
//! smoke test.

use x0x::api::Method;

const GUI_HTML: &str = include_str!("../src/gui/x0x-gui.html");

/// Fragments the GUI must contain for each named-groups endpoint. These
/// are matched as plain substrings against the committed HTML — they do
/// not need to be complete expressions, just distinctive enough that a
/// rename or removal of the call site breaks the test.
///
/// Keep in sync with `src/gui/x0x-gui.html`. When a daemon route is
/// renamed, update both files.
const REQUIRED: &[(Method, &str, &str)] = &[
    // Core
    (
        Method::Post,
        "'/groups',{method:'POST'",
        "create space with preset",
    ),
    (
        Method::Get,
        "api('/groups/'+data.groupId)",
        "space detail fetch",
    ),
    (
        Method::Get,
        "api('/groups/'+gid)",
        "admin loader fetches group",
    ),
    (
        Method::Delete,
        "'/groups/'+gid,{method:'DELETE'}",
        "leave space",
    ),
    // Members / roles / bans
    (Method::Get, "'/groups/'+gid+'/members'", "roster load"),
    (
        Method::Patch,
        "/groups/${gid}/members/${aid}/role",
        "set role",
    ),
    (
        Method::Post,
        "/groups/${gid}/ban/${aid}`,{method:'POST'}",
        "ban member",
    ),
    (
        Method::Delete,
        "/groups/${gid}/ban/${aid}`,{method:'DELETE'}",
        "unban member",
    ),
    // Join requests
    (
        Method::Get,
        "'/groups/'+gid+'/requests'",
        "list requests + request access both hit this prefix",
    ),
    (
        Method::Post,
        "'/groups/'+gid+'/requests',{method:'POST'",
        "submit access request",
    ),
    (
        Method::Post,
        "/groups/${gid}/requests/${rid}/approve",
        "approve request",
    ),
    (
        Method::Post,
        "/groups/${gid}/requests/${rid}/reject",
        "reject request",
    ),
    // Discovery
    (Method::Get, "/groups/discover?q=", "discover search"),
    (Method::Get, "/groups/discover/nearby", "shard-only nearby"),
    // Policy / state chain
    (Method::Patch, "'/groups/'+gid+'/policy'", "policy patch"),
    (Method::Get, "'/groups/'+gid+'/state'", "state readout"),
    (Method::Post, "'/groups/'+gid+'/state/seal'", "seal state"),
    (
        Method::Post,
        "'/groups/'+gid+'/state/withdraw'",
        "withdraw state",
    ),
    // Invite (regression guard — still works)
    (Method::Post, "'/groups/'+gid+'/invite'", "invite"),
];

#[test]
fn gui_named_group_endpoints_are_wired() {
    let mut missing = Vec::new();
    for (method, fragment, why) in REQUIRED {
        if !GUI_HTML.contains(fragment) {
            missing.push(format!(
                "  {method} — expected fragment missing: {fragment}\n      reason: {why}"
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "\n\nThe embedded GUI is missing named-group API call sites ({} \
         regressions). These are required to keep the browser UI at \
         parity with the CLI and the Communitas apps:\n{}\n\n\
         If an endpoint was renamed, update both src/gui/x0x-gui.html \
         and tests/gui_named_group_parity.rs::REQUIRED.\n",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn gui_exposes_all_four_presets() {
    // Every preset name from `GroupPolicyPreset` must be reachable from
    // the create-space modal; otherwise we have a public-group gap.
    for preset in [
        "private_secure",
        "public_request_secure",
        "public_open",
        "public_announce",
    ] {
        assert!(
            GUI_HTML.contains(preset),
            "GUI must expose the '{preset}' preset in the create-space modal"
        );
    }
}

#[test]
fn gui_renders_discover_view() {
    assert!(
        GUI_HTML.contains("function renderDiscover()"),
        "GUI must define renderDiscover for the /discover navigation target"
    );
    assert!(
        GUI_HTML.contains("navigate('discover')"),
        "GUI sidebar must link to the discover view"
    );
}

#[test]
fn gui_renders_admin_controls_inline() {
    // Admin controls live inside the space-detail panel. The host div id
    // is templated; check for the prefix.
    assert!(
        GUI_HTML.contains("nag-admin-"),
        "GUI must reserve a host slot for the per-space admin panel"
    );
    assert!(
        GUI_HTML.contains("nagRenderAdmin"),
        "GUI must invoke nagRenderAdmin from renderSpaceDetail"
    );
    assert!(
        GUI_HTML.contains("data-nag-policy-apply"),
        "policy editor must expose an Apply button"
    );
    assert!(
        GUI_HTML.contains("data-nag-state-seal"),
        "state panel must expose a Seal action"
    );
    assert!(
        GUI_HTML.contains("data-nag-state-withdraw"),
        "state panel must expose a Withdraw action"
    );
}
