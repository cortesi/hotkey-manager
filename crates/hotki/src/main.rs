mod config;
mod hud;
mod platform_specific;
mod settings;

use crate::config::Config;
use crate::hud::HudWindow;
use clap::Parser;
use dioxus::{
    desktop::{
        trayicon::{
            init_tray_icon,
            menu::{Menu, MenuItem, PredefinedMenuItem},
        },
        use_muda_event_handler, window, Config as DioxusConfig, LogicalSize, WindowBuilder,
    },
    prelude::*,
    LaunchBuilder,
};
use hotkey_manager::Server;
use std::{env, fs, process};

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
    // Filter out empty arguments that dx might pass
    let args_vec: Vec<String> = env::args().filter(|arg| !arg.is_empty()).collect();

    let args = Args::parse_from(args_vec);

    if args.server {
        // Run in server mode
        println!("Starting hotkey server...");
        if let Err(e) = Server::new().run() {
            eprintln!("Failed to run server: {e}");
            process::exit(1);
        }
    } else {
        // Run GUI mode
        // Load config from environment variable (required)
        let config_path = match env::var("HOTKI_CONFIG") {
            Ok(path) => path,
            Err(_) => {
                eprintln!("Error: HOTKI_CONFIG environment variable not set");
                eprintln!("Please set HOTKI_CONFIG to the path of your RON configuration file");
                eprintln!("Example: HOTKI_CONFIG=/path/to/config.ron hotki");
                process::exit(1);
            }
        };

        let config_content = match fs::read_to_string(&config_path) {
            Ok(content) => {
                println!("Loaded config from: {config_path}");
                content
            }
            Err(e) => {
                eprintln!("Failed to read config file '{config_path}': {e}");
                process::exit(1);
            }
        };

        // Parse the config
        let config = match ron::from_str::<Config>(&config_content) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Failed to parse config file '{config_path}': {e}");
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
        let settings_item = MenuItem::with_id("settings", "Settings", true, None);
        let separator = PredefinedMenuItem::separator();
        let quit_item = MenuItem::with_id("quit", "Quit", true, None);

        let _ = tray_menu.append(&settings_item);
        let _ = tray_menu.append(&separator);
        let _ = tray_menu.append(&quit_item);

        // Initialize tray icon with default icon
        let tray_icon = init_tray_icon(
            tray_menu.clone(),
            None, // Uses default icon
        );

        // Set the menu to be shown on both left and right click
        tray_icon.set_menu(Some(Box::new(tray_menu)));

        // Set tooltip
        let _ = tray_icon.set_tooltip(Some("Hotkey Manager"));

        println!("Tray icon initialized");
    });

    // Handle tray menu click events
    use_muda_event_handler(move |event| {
        match event.id().as_ref() {
            "settings" => {
                println!("Settings menu clicked");
                // Create settings window with explicit window configuration
                let dom = VirtualDom::new(settings::SettingsWindow);
                let window_builder = WindowBuilder::new()
                    .with_title("Settings")
                    .with_decorations(true)
                    .with_resizable(true)
                    .with_always_on_top(false)
                    .with_visible(true)
                    .with_inner_size(LogicalSize::new(600.0, 500.0));
                let config = DioxusConfig::new().with_window(window_builder);
                window().new_window(dom, config);
                println!("Settings window created");
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
