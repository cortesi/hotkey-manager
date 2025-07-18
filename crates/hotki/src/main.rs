mod config;
mod hud;
mod platform_specific;
mod ringbuffer;

use crate::config::Config;
use crate::hud::HudWindow;
use crate::ringbuffer::init_tracing;
use clap::Parser;
use dioxus::{
    desktop::{
        trayicon::{
            init_tray_icon,
            menu::{Menu, MenuItem, PredefinedMenuItem},
            Icon,
        },
        use_muda_event_handler, Config as DioxusConfig, WindowBuilder,
    },
    prelude::*,
    LaunchBuilder,
};
use hotkey_manager::Server;
use std::{env, fs, process};
use tracing::{debug, error, info, Level};

const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");


#[derive(Parser, Debug)]
#[command(name = "hotki")]
#[command(about = "Hotkey Manager GUI", long_about = None)]
#[command(after_help = r#"ENVIRONMENT VARIABLES:
  HOTKI_CONFIG    Path to RON configuration file (required for GUI mode)

EXAMPLES:
  Run GUI:
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
        // Load config from environment variable (required)
        let config_path = match env::var("HOTKI_CONFIG") {
            Ok(path) => path,
            Err(_) => {
                error!("Error: HOTKI_CONFIG environment variable not set");
                error!("Please set HOTKI_CONFIG to the path of your RON configuration file");
                error!("Example: HOTKI_CONFIG=/path/to/config.ron hotki");
                process::exit(1);
            }
        };

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

        // Parse the config
        let config = match ron::from_str::<Config>(&config_content) {
            Ok(config) => config,
            Err(e) => {
                error!("Failed to parse config file '{config_path}': {e}");
                process::exit(1);
            }
        };

        // Configure the app as a background agent before anything else
        platform_specific::configure_as_agent_app();
        LaunchBuilder::desktop()
            .with_cfg(
                DioxusConfig::new()
                    .with_window(
                        WindowBuilder::new()
                            .with_transparent(true)
                            .with_visible(false)
                            .with_resizable(false),
                    )
                    .with_custom_head(
                        r#"<style>
                        #app { background: transparent; }
                    </style>"#
                            .to_string(),
                    ),
            )
            .with_context(config)
            .launch(App);
    }
}

#[component]
fn App() -> Element {
    // Initialize system tray icon
    use_effect(move || {
        // Create a simple tray menu
        let tray_menu = Menu::new();

        // Add menu items with IDs to handle click events
        let config_path =
            env::var("HOTKI_CONFIG").unwrap_or_else(|_| "Config not found".to_string());
        let config_item = MenuItem::with_id("config", &config_path, false, None);
        let reveal_item = MenuItem::with_id("reveal", "Reveal Config in Finder", true, None);
        let separator = PredefinedMenuItem::separator();
        let quit_item = MenuItem::with_id("quit", "Quit", true, None);

        let _ = tray_menu.append(&config_item);
        let _ = tray_menu.append(&reveal_item);
        let _ = tray_menu.append(&separator);
        let _ = tray_menu.append(&quit_item);

        // Initialize tray icon with custom logo
        let icon_bytes = include_bytes!("../logo/tray-icon.png");
        let img = image::load_from_memory(icon_bytes).unwrap().to_rgba8();
        let (width, height) = img.dimensions();
        let rgba_data = img.into_raw();

        let tray_icon = init_tray_icon(
            tray_menu.clone(),
            Some(Icon::from_rgba(rgba_data, width, height).unwrap()),
        );

        // Set the menu to be shown on both left and right click
        tray_icon.set_menu(Some(Box::new(tray_menu)));

        // Set tooltip
        let _ = tray_icon.set_tooltip(Some("Hotkey Manager"));

        debug!("Tray icon initialized");
    });

    // Handle tray menu click events
    use_muda_event_handler(move |event| {
        match event.id().as_ref() {
            "reveal" => {
                debug!("Reveal config in Finder clicked");
                if let Ok(config_path) = env::var("HOTKI_CONFIG") {
                    // Use the 'open' command to reveal the file in Finder
                    let _ = std::process::Command::new("open")
                        .arg("-R") // -R flag reveals the file in Finder
                        .arg(&config_path)
                        .spawn();
                }
            }
            "quit" => {
                // Quit the application
                process::exit(0);
            }
            _ => {}
        }
    });

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        HudWindow {}
    }
}
