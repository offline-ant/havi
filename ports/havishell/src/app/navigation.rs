use super::App;

pub(super) enum NavCommand {
    Back,
    Forward,
    Reload,
    Navigate(String),
}

impl App {
    pub(super) fn navigate(&self, url_str: &str) {
        if let Some(webview) = self.active_webview() {
            if let Ok(url) = url::Url::parse(url_str) {
                webview.load(url);
            } else if let Ok(url) = url::Url::parse(&format!("https://{}", url_str)) {
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
