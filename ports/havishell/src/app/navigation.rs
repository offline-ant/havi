use super::App;

pub(super) enum NavCommand {
    Back,
    Forward,
    Reload,
    Navigate(String),
}

pub(super) enum NavInput {
    Direct(libhavi::BrowserUrl),
    BarePublicGroup(String),
}

pub(super) fn parse_navigation_url(url_str: &str) -> Option<libhavi::BrowserUrl> {
    if let Ok(url) = libhavi::BrowserUrl::parse(url_str) {
        return Some(url);
    }

    // Preserve explicit scheme input exactly. Heuristic normalization only
    // applies to bare coordinates.
    if url_str.contains("://") {
        return None;
    }

    let coord = url_str.trim_start_matches('/');
    if coord.is_empty() {
        return None;
    }

    libhavi::BrowserUrl::parse(&format!("hppr://{}", coord)).ok()
}

fn parse_bare_public_group(input: &str) -> Option<String> {
    if input.is_empty()
        || input.contains("://")
        || input.contains('/')
        || input.contains('@')
        || input.starts_with('~')
        || input.chars().any(char::is_whitespace)
    {
        return None;
    }

    hppr_packet::validation_utils::validate_group_app(input, "Group").ok()?;
    Some(input.to_string())
}

pub(super) fn classify_nav_input(input: &str) -> Option<NavInput> {
    if let Some(group) = parse_bare_public_group(input) {
        return Some(NavInput::BarePublicGroup(group));
    }

    parse_navigation_url(input).map(NavInput::Direct)
}

pub(super) fn group_landing_url(group: &str, app: &str) -> String {
    format!("hppr://{}/{}/index.html", group, app)
}

impl App {
    pub(super) fn go_back(&self) {
        if let Some(webview) = self.active_webview() {
            webview.go_back(1);
        }
    }

    pub(super) fn go_forward(&self) {
        if let Some(webview) = self.active_webview() {
            webview.go_forward(1);
        }
    }

    pub(super) fn reload(&self) {
        if let Some(webview) = self.active_webview() {
            webview.reload();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_nav_input, parse_navigation_url, NavInput};

    #[test]
    fn preserves_havi_admin_url() {
        let url = parse_navigation_url("havi:///diagnostics").unwrap();
        assert_eq!(url.as_str(), "havi:///diagnostics");
    }

    #[test]
    fn rewrites_bare_coordinate_to_hppr() {
        let url = parse_navigation_url("sol/chat/").unwrap();
        assert_eq!(url.as_str(), "hppr://sol/chat/");
    }

    #[test]
    fn rewrites_slash_prefixed_coordinate_to_hppr() {
        let url = parse_navigation_url("//sol/chat/").unwrap();
        assert_eq!(url.as_str(), "hppr://sol/chat/");
    }

    #[test]
    fn rewrites_double_slash_input_to_hppr() {
        let url = parse_navigation_url("//u/example").unwrap();
        assert_eq!(url.as_str(), "hppr://u/example");
    }

    #[test]
    fn classifies_bare_public_group() {
        match classify_nav_input("lab.eu").unwrap() {
            NavInput::BarePublicGroup(group) => assert_eq!(group, "lab.eu"),
            NavInput::Direct(_) => panic!("expected bare public group"),
        }
    }

    #[test]
    fn bare_public_group_rejects_at_sign() {
        assert!(matches!(
            classify_nav_input("ops@lab.eu"),
            Some(NavInput::Direct(_)) | None
        ));
    }
}
