use super::App;

pub(super) enum NavCommand {
    Back,
    Forward,
    Reload,
    Navigate(String),
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

impl App {
    pub(super) fn navigate(&self, url_str: &str) {
        if let Some(webview) = self.active_webview() {
            if let Some(url) = parse_navigation_url(url_str) {
                webview.load(url);
            }
        }
    }

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
    use super::parse_navigation_url;

    #[test]
    fn preserves_hppr_join_url() {
        let url = parse_navigation_url("hppr-join://sol/chat/").unwrap();
        assert_eq!(url.as_str(), "hppr-join://sol/chat/");
    }

    #[test]
    fn preserves_havi_admin_url() {
        let url = parse_navigation_url("havi:///services").unwrap();
        assert_eq!(url.as_str(), "havi:///services");
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
}
