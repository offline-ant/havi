use super::*;
use ::image::{DynamicImage, RgbaImage};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SCREENSHOT_QUIESCENCE_MS: u64 = 250;
#[cfg(debug_assertions)]
pub(super) const SCREENSHOT_MAX_SETTLE_MS: u64 = 10000;
#[cfg(not(debug_assertions))]
pub(super) const SCREENSHOT_MAX_SETTLE_MS: u64 = 2000;
#[cfg(debug_assertions)]
pub(super) const SCREENSHOT_MAX_LOAD_WAIT_MS: u64 = 10000;
#[cfg(not(debug_assertions))]
pub(super) const SCREENSHOT_MAX_LOAD_WAIT_MS: u64 = 2000;
const SCREENSHOT_SETTLE_FRAMES: u8 = 1;
const SCREENSHOT_POLL_MS: f64 = 0.05;

#[derive(Clone, Debug)]
pub(super) enum ScreenshotMode {
    WaitingForLoad {
        output_path: PathBuf,
        deadline: Instant,
    },
    WaitingForSettle {
        output_path: PathBuf,
        deadline: Instant,
        last_visual_change: Instant,
    },
    Settling { output_path: PathBuf, frames_left: u8 },
    Capturing,
}

fn write_png(path: &PathBuf, image: &RgbaImage) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create screenshot dir {}: {}", parent.display(), err))?;
    }
    DynamicImage::ImageRgba8(image.clone())
        .save(path)
        .map_err(|err| format!("write screenshot {}: {}", path.display(), err))
}

fn durable_exit_success(path: &PathBuf) -> ! {
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    if let Ok(file) = std::fs::OpenOptions::new().read(true).open(path) {
        let _ = file.sync_all();
    }
    unsafe {
        libc::_exit(0);
    }
}

fn durable_exit_failure() -> ! {
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    unsafe {
        libc::_exit(1);
    }
}

impl App {
    fn schedule_screenshot_poll(&mut self, cx: &mut Cx) {
        self.screenshot_poll = cx.start_timeout(SCREENSHOT_POLL_MS);
        self.request_spin_redraw(cx);
    }

    pub(super) fn extend_screenshot_load_deadline(&mut self) {
        let Some(ScreenshotMode::WaitingForLoad { output_path, .. }) = self.screenshot_mode.clone() else {
            return;
        };
        self.screenshot_mode = Some(ScreenshotMode::WaitingForLoad {
            output_path,
            deadline: Instant::now() + Duration::from_millis(SCREENSHOT_MAX_LOAD_WAIT_MS),
        });
    }

    fn advance_screenshot_waiting_for_load(
        &mut self,
        cx: &mut Cx,
        output_path: PathBuf,
        deadline: Instant,
    ) {
        if !self.start_navigation_done { 
            self.screenshot_mode = Some(ScreenshotMode::WaitingForLoad {
                output_path,
                deadline,
            });
            self.schedule_screenshot_poll(cx);
            return;
        }
        let Some(webview) = self.active_webview() else {
            self.screenshot_mode = Some(ScreenshotMode::WaitingForLoad {
                output_path,
                deadline,
            });
            self.schedule_screenshot_poll(cx);
            return;
        };

        let now = Instant::now();
        let load_complete = webview.load_status() == libhavi::LoadStatus::Complete;
        let timed_out = now > deadline;

        if !load_complete && !timed_out { 
            self.screenshot_mode = Some(ScreenshotMode::WaitingForLoad {
                output_path,
                deadline,
            });
            self.schedule_screenshot_poll(cx);
            return;
        }

        self.screenshot_mode = Some(ScreenshotMode::WaitingForSettle {
            output_path,
            deadline: now + Duration::from_millis(SCREENSHOT_MAX_SETTLE_MS),
            last_visual_change: self.last_active_page_visual_change.unwrap_or(now),
        });
        self.schedule_screenshot_poll(cx);
    }

    pub(super) fn maybe_start_screenshot_capture(&mut self, cx: &mut Cx) {
        let Some(ScreenshotMode::WaitingForLoad {
            output_path,
            deadline,
        }) = self.screenshot_mode.clone() else {
            return;
        };
        self.advance_screenshot_waiting_for_load(cx, output_path, deadline);
    }

    pub(super) fn update_screenshot_mode(&mut self, cx: &mut Cx) {
        let Some(mode) = self.screenshot_mode.clone() else {
            return;
        };
        match mode {
            ScreenshotMode::WaitingForLoad {
                output_path,
                deadline,
            } => {
                self.advance_screenshot_waiting_for_load(cx, output_path, deadline);
            }
            ScreenshotMode::WaitingForSettle {
                output_path,
                deadline,
                mut last_visual_change,
            } => {
                if let Some(active_visual_change) = self.last_active_page_visual_change {
                    if active_visual_change > last_visual_change {
                        last_visual_change = active_visual_change;
                    }
                }
                let now = Instant::now();
                let quiesced = now.duration_since(last_visual_change)
                    > Duration::from_millis(SCREENSHOT_QUIESCENCE_MS);
                let timed_out = now > deadline;
                if quiesced || timed_out {
                    eprintln!("[havi][screenshot] entering settling output={}", output_path.display());
                    self.screenshot_mode = Some(ScreenshotMode::Settling {
                        output_path,
                        frames_left: SCREENSHOT_SETTLE_FRAMES,
                    });
                    self.schedule_screenshot_poll(cx);
                    return;
                }
                self.screenshot_mode = Some(ScreenshotMode::WaitingForSettle {
                    output_path,
                    deadline,
                    last_visual_change,
                });
                self.schedule_screenshot_poll(cx);
                return;
            }
            ScreenshotMode::Settling {
                output_path,
                frames_left,
            } => {
                if frames_left > 0 {
                    self.screenshot_mode = Some(ScreenshotMode::Settling {
                        output_path,
                        frames_left: frames_left - 1,
                    });
                    self.schedule_screenshot_poll(cx);
                    return;
                }

                let Some(webview) = self.active_webview() else {
                    return;
                };
                let output_path_clone = output_path.clone();
                eprintln!("[havi][screenshot] calling take_screenshot output={}", output_path_clone.display());
                webview.take_screenshot(None, move |result| match result {
                    Ok(image) => match write_png(&output_path_clone, &image) {
                        Ok(()) => {
                            println!("HAVI_SCREENSHOT={}", output_path_clone.display());
                            durable_exit_success(&output_path_clone);
                        }
                        Err(err) => {
                            eprintln!("[havi] screenshot failed: {}", err);
                            durable_exit_failure();
                        }
                    },
                    Err(err) => {
                        eprintln!("[havi] screenshot failed: {:?}", err);
                        durable_exit_failure();
                    }
                });
                self.screenshot_mode = Some(ScreenshotMode::Capturing);
                self.request_spin_redraw(cx);
            }
            ScreenshotMode::Capturing => {}
        }
    }

    pub(super) fn handle_screenshot_capture_results(&mut self, _cx: &mut Cx) {}
}
