mod config;
mod hud;
mod logs;
mod ringbuffer;

use crate::config::Config;
use crate::hud::create_hud_window;
use crate::logs::LogsWindow;
use crate::ringbuffer::init_tracing;
use clap::Parser;
use dioxus::{
    desktop::{
        trayicon::{
            init_tray_icon,
            menu::{Menu, MenuItem, PredefinedMenuItem},
            Icon,
        },
        use_muda_event_handler, window, Config as DioxusConfig, WindowCloseBehaviour,
    },
    prelude::*,
    LaunchBuilder,
};
use dioxus_desktop::tao::platform::macos::{ActivationPolicy, EventLoopWindowTargetExtMacOS};

use hotkey_manager::Server;
use std::{env, fs, process};
use tracing::{debug, error, info, Level};

fn get_config_path() -> String {
    match env::var("HOTKI_CONFIG") {
        Ok(path) => path,
        Err(_) => {
            // Default to ~/.hotki.ron
            match env::var("HOME") {
                Ok(home) => format!("{home}/.hotki.ron"),
                Err(_) => {
                    error!("Error: Neither HOTKI_CONFIG nor HOME environment variables are set");
                    error!("Please set HOTKI_CONFIG to the path of your RON configuration file");
                    error!("Example: HOTKI_CONFIG=/path/to/config.ron hotki");
                    process::exit(1);
                }
            }
        }
    }
}

fn get_config_path_safe() -> Option<String> {
    match env::var("HOTKI_CONFIG") {
        Ok(path) => Some(path),
        Err(_) => {
            // Default to ~/.hotki.ron
            match env::var("HOME") {
                Ok(home) => Some(format!("{home}/.hotki.ron")),
                Err(_) => None,
            }
        }
    }
}

const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[derive(Parser, Debug)]
#[command(name = "hotki")]
#[command(about = "Hotkey Manager GUI", long_about = None)]
#[command(after_help = r#"ENVIRONMENT VARIABLES:
  HOTKI_CONFIG    Path to RON configuration file (defaults to ~/.hotki.ron)

EXAMPLES:
  Run GUI (with default config):
    hotki
    
  Run GUI (with custom config):
    HOTKI_CONFIG=/path/to/config.ron hotki
    
  Run server:
    hotki --server"#)]
struct Args {
    /// Run as hotkey server (no GUI)
    #[arg(long)]
    server: bool,
}

fn main() {
    // Initialize tracing with info level and 2048 entry ring buffer
    init_tracing(Level::INFO, 2048);

    // Filter out empty arguments that dx might pass
    let args_vec: Vec<String> = env::args().filter(|arg| !arg.is_empty()).collect();

    let args = Args::parse_from(args_vec);

    if args.server {
        // Run in server mode
        info!("Starting hotkey server...");
        if let Err(e) = Server::new().run() {
            error!("Failed to run server: {e}");
            process::exit(1);
        }
    } else {
        // Run GUI mode
        // Load config from environment variable or default to ~/.hotki.ron
        let config_path = get_config_path();

        let config_content = match fs::read_to_string(&config_path) {
            Ok(content) => {
                info!("Loaded config from: {config_path}");
                content
            }
            Err(e) => {
                error!("Failed to read config file '{config_path}': {e}");
                process::exit(1);
            }
        };

        // Parse the confikj jjjg
        let config = match ron::from_str::<Config>(&config_content) {
            Ok(config) => config,
            Err(e) => {
                error!("Failed to parse config file '{config_path}': {e}");
                process::exit(1);
            }
        };

        use dioxus::desktop::WindowBuilder;

        let window_builder = WindowBuilder::new()
            .with_title("Hotki - Logs")
            .with_inner_size(dioxus::desktop::LogicalSize::new(800.0, 600.0))
            .with_minimizable(true)
            .with_maximizable(true)
            .with_resizable(true)
            .with_visible(false) // Hide on startup
            .with_decorations(true)
            .with_closable(true);

        let dioxus_config = DioxusConfig::new()
            .with_window(window_builder)
            .with_disable_context_menu(true)
            .with_exits_when_last_window_closes(false)
            .with_custom_event_handler(|_event, event_loop_target| {
                // Set activation policy to Accessory on macOS to prevent dock icon
                #[cfg(target_os = "macos")]
                {
                    static POLICY_SET: std::sync::Once = std::sync::Once::new();
                    POLICY_SET.call_once(|| {
                        event_loop_target
                            .set_activation_policy_at_runtime(ActivationPolicy::Accessory);
                    });
                }
            });

        LaunchBuilder::desktop()
            .with_cfg(dioxus_config)
            .with_context(config)
            .launch(LogsApp);
    }
}

#[component]
fn LogsApp() -> Element {
    use_hook(|| {
        // Set the close behavior for the main window
        // This will hide the window instead of closing it when the user clicks the close button
        window().set_close_behavior(WindowCloseBehaviour::WindowHides);

        // Create a simple tray menu
        let tray_menu = Menu::new();

        // Add menu items with IDs to handle click events
        let config_path =
            env::var("HOTKI_CONFIG").unwrap_or_else(|_| "Config not found".to_string());
        let config_item = MenuItem::with_id("config", &config_path, false, None);
        let reveal_item = MenuItem::with_id("reveal", "Reveal Config in Finder", true, None);
        let logs_item = MenuItem::with_id("logs", "Logs", true, None);
        let separator = PredefinedMenuItem::separator();
        let quit_item = MenuItem::with_id("quit", "Quit", true, None);

        let _ = tray_menu.append(&config_item);
        let _ = tray_menu.append(&reveal_item);
        let _ = tray_menu.append(&logs_item);
        let _ = tray_menu.append(&separator);
        let _ = tray_menu.append(&quit_item);

        // Initialize tray icon with custom logo
        let icon_bytes = include_bytes!("../logo/tray-icon.png");
        let img = image::load_from_memory(icon_bytes).unwrap().to_rgba8();
        let (width, height) = img.dimensions();
        let rgba_data = img.into_raw();

        let ticon = init_tray_icon(
            tray_menu.clone(),
            Some(Icon::from_rgba(rgba_data, width, height).unwrap()),
        );

        ticon.set_menu(Some(Box::new(tray_menu.clone())));
        ticon.set_show_menu_on_left_click(false); // Disable default menu on left-click
        let _ = ticon.set_tooltip(Some("Hotki"));

        debug!("Tray icon initialized");
    });

    // Handle tray menu click events
    use_muda_event_handler({
        move |event| {
            match event.id().as_ref() {
                "reveal" => {
                    debug!("Reveal config in Finder clicked");
                    if let Some(config_path) = get_config_path_safe() {
                        // Use the 'open' command to reveal the file in Finder
                        let _ = std::process::Command::new("open")
                            .arg("-R") // -R flag reveals the file in Finder
                            .arg(&config_path)
                            .spawn();
                    } else {
                        error!("Cannot determine config path: HOME environment variable not set");
                    }
                }
                "logs" => {
                    debug!("Logs menu item clicked");
                    window().set_visible(true);
                    window().set_focus();
                }
                "quit" => {
                    // Quit the application
                    process::exit(0);
                }
                _ => {}
            }
        }
    });

    // Create HUD window as a popup
    let config = use_context::<Config>();
    use_effect(move || {
        create_hud_window(config.clone());
    });

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        // Main app is now the logs window
        LogsWindow {}
    }
}
