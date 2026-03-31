/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Configuration options for a single run of the servo application. Created
//! from command line arguments.

use std::default::Default;
use std::path::PathBuf;
use std::process;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use servo_url::BrowserUrl;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Opts {
    pub time_profiling: Option<OutputOptions>,
    pub time_profiler_trace_path: Option<String>,
    pub nonincremental_layout: bool,
    pub user_stylesheets: Vec<(Vec<u8>, BrowserUrl)>,
    pub hard_fail: bool,
    pub debug: DiagnosticsLogging,
    pub multiprocess: bool,
    pub force_ipc: bool,
    pub background_hang_monitor: bool,
    pub sandbox: bool,
    pub random_pipeline_closure_probability: Option<f32>,
    pub random_pipeline_closure_seed: Option<usize>,
    pub shaders_path: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
    pub certificate_path: Option<String>,
    pub ignore_certificate_errors: bool,
    pub unminify_js: bool,
    pub local_script_source: Option<String>,
    pub unminify_css: bool,
    pub print_pwm: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DiagnosticsLogging {
    pub help: bool,
    pub style_tree: bool,
    pub rule_tree: bool,
    pub flow_tree: bool,
    pub stacking_context_tree: bool,
    pub scroll_tree: bool,
    pub display_list: bool,
    pub relayout_event: bool,
    pub profile_script_events: bool,
    pub style_statistics: bool,
    pub gc_profile: bool,
}

impl DiagnosticsLogging {
    pub fn new() -> Self {
        let mut config: DiagnosticsLogging = Default::default();

        #[cfg(debug_assertions)]
        {
            if let Ok(diagnostics_var) = std::env::var("SERVO_DIAGNOSTICS") {
                if let Err(error) = config.extend_from_string(&diagnostics_var) {
                    eprintln!("Could not parse debug logging option: {error}");
                }
            }
        }

        config
    }

    fn print_debug_options_usage(app: &str) {
        fn print_option(name: &str, description: &str) {
            println!("\t{:<35} {}", name, description);
        }

        println!(
            "Usage: {} debug option,[options,...]\n\twhere options include\n\nOptions:",
            app
        );
        print_option("help", "Show this help message");
        print_option("style-tree", "Log the style tree after each restyle");
        print_option("rule-tree", "Log the rule tree");
        print_option("flow-tree", "Log the fragment tree after each layout");
        print_option(
            "stacking-context-tree",
            "Log the stacking context tree after each layout",
        );
        print_option("scroll-tree", "Log the scroll tree after each layout");
        print_option("display-list", "Log the display list after each layout");
        print_option("style-stats", "Log style sharing cache statistics");
        print_option("relayout-event", "Log when relayout occurs");
        print_option("profile-script-events", "Log script event processing time");
        print_option("gc-profile", "Log garbage collection statistics");
        println!();

        process::exit(0);
    }

    pub fn extend_from_string(&mut self, option_string: &str) -> Result<(), String> {
        for option in option_string.split(',') {
            let option = option.trim();
            match option {
                "help" => Self::print_debug_options_usage("servo"),
                "display-list" => self.display_list = true,
                "stacking-context-tree" => self.stacking_context_tree = true,
                "flow-tree" => self.flow_tree = true,
                "rule-tree" => self.rule_tree = true,
                "style-tree" => self.style_tree = true,
                "style-stats" => self.style_statistics = true,
                "scroll-tree" => self.scroll_tree = true,
                "gc-profile" => self.gc_profile = true,
                "profile-script-events" => self.profile_script_events = true,
                "relayout-event" => self.relayout_event = true,
                "" => {},
                _ => return Err(format!("Unknown diagnostic option: {option}")),
            };
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum OutputOptions {
    FileName(String),
    Stdout(f64),
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            time_profiling: None,
            time_profiler_trace_path: None,
            nonincremental_layout: false,
            user_stylesheets: Vec::new(),
            hard_fail: true,
            multiprocess: false,
            force_ipc: false,
            background_hang_monitor: false,
            random_pipeline_closure_probability: None,
            random_pipeline_closure_seed: None,
            sandbox: false,
            debug: Default::default(),
            config_dir: None,
            shaders_path: None,
            certificate_path: None,
            ignore_certificate_errors: false,
            unminify_js: false,
            local_script_source: None,
            unminify_css: false,
            print_pwm: false,
        }
    }
}

static OPTIONS: OnceLock<Opts> = OnceLock::new();

pub fn initialize_options(opts: Opts) {
    OPTIONS.set(opts).expect("Already initialized");
}

#[inline]
pub fn get() -> &'static Opts {
    OPTIONS.get_or_init(Default::default)
}
