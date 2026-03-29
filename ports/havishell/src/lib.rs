pub use makepad_widgets;
pub mod app;
pub mod browser_scroll;
pub mod input;
pub mod protocols;
pub mod pylon_host;
pub mod servo_web_view;
pub mod widgets;

pub fn register_script_modules(vm: &mut makepad_widgets::makepad_platform::ScriptVm) {
    crate::widgets::shell_root::script_mod(vm);
}
pub use servo_web_view::ServoWebView;
pub use servo_web_view::ServoWebViewAction;
