use super::App;

pub(super) enum NavCommand {
    Back,
    Forward,
    Reload,
    Navigate(String),
}

pub(super) fn parse_navigation_url(url_str: &str) -> Option<servo::BrowserUrl> {
    if let Ok(url) = servo::BrowserUrl::parse(url_str) {
        return Some(url);
    }

    // Bare host/path input is treated as https. Inputs that already contain
    // an explicit authority separator (://) are left unchanged.
    if url_str.contains("://") {
        return None;
    }

    servo::BrowserUrl::parse(&format!("https://{}", url_str)).ok()
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
    fn rewrites_bare_host_to_https() {
        let url = parse_navigation_url("example.com").unwrap();
        assert_eq!(url.as_str(), "https://example.com/");
    }
}
