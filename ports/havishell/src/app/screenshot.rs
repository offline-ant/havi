use super::*;
use ::image::{DynamicImage, RgbaImage};
use std::io::Write;
use std::path::PathBuf;

const SCREENSHOT_SETTLE_FRAMES: u8 = 1;

#[derive(Clone, Debug)]
pub(super) enum ScreenshotMode {
    WaitingForLoad { output_path: PathBuf },
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
    pub(super) fn maybe_start_screenshot_capture(&mut self, cx: &mut Cx) {
        let Some(ScreenshotMode::WaitingForLoad { output_path }) = self.screenshot_mode.clone() else {
            return;
        };
        if !self.start_navigation_done {
            return;
        }
        let Some(webview) = self.active_webview() else {
            return;
        };
        if webview.load_status() != servo::LoadStatus::Complete {
            return;
        }

        self.screenshot_mode = Some(ScreenshotMode::Settling {
            output_path,
            frames_left: SCREENSHOT_SETTLE_FRAMES,
        });
        self.needs_paint = true;
        self.idle_frames = 0;
        self.next_frame = cx.new_next_frame();
        cx.redraw_all();
    }

    pub(super) fn update_screenshot_mode(&mut self, cx: &mut Cx) {
        let Some(mode) = self.screenshot_mode.clone() else {
            return;
        };
        match mode {
            ScreenshotMode::WaitingForLoad { .. } => {}
            ScreenshotMode::Settling {
                output_path,
                frames_left,
            } => {
                if frames_left > 0 {
                    self.screenshot_mode = Some(ScreenshotMode::Settling {
                        output_path,
                        frames_left: frames_left - 1,
                    });
                    self.next_frame = cx.new_next_frame();
                    cx.redraw_all();
                    return;
                }

                let Some(webview) = self.active_webview() else {
                    return;
                };
                let output_path_clone = output_path.clone();
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
                self.next_frame = cx.new_next_frame();
                cx.redraw_all();
            }
            ScreenshotMode::Capturing => {}
        }
    }

    pub(super) fn handle_screenshot_capture_results(&mut self, _cx: &mut Cx) {}
}
