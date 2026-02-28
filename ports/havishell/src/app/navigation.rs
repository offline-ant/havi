use super::App;
use makepad_widgets::log;

pub(super) enum NavCommand {
    Back,
    Forward,
    Reload,
    Navigate(String),
}

impl App {
    pub(super) fn navigate(&self, url_str: &str) {
        log!("[havishell] navigate: url={}", url_str);
        if let Some(webview) = self.active_webview() {
            if let Ok(url) = servo::BrowserUrl::parse(url_str) {
                log!("[havishell] navigate: loading parsed url");
                webview.load(url);
            } else if let Ok(url) = servo::BrowserUrl::parse(&format!("https://{}", url_str)) {
                log!("[havishell] navigate: loading as https url");
                webview.load(url);
            } else {
                log!("[havishell] navigate: failed to parse url");
            }
        } else {
            log!("[havishell] navigate: no active webview");
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
