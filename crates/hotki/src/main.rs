mod hud;
mod platform_specific;
mod settings;

use clap::Parser;
use dioxus::desktop::trayicon::menu::{Menu, MenuItem};
use dioxus::desktop::{use_muda_event_handler, use_window, Config};
use dioxus::prelude::*;
use hotkey_manager::{Client, IPCResponse, Key, Server};
use keymode::{Mode, State};
use std::{env, fs};

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
            std::process::exit(1);
        }
    } else {
        // Run GUI mode
        // Configure the app as a background agent before anything else
        platform_specific::configure_as_agent_app();
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new()
                    .with_window(
                        dioxus::desktop::WindowBuilder::new()
                            .with_transparent(true)
                            .with_visible(false)
                            // Position window off-screen initially to prevent flicker
                            .with_position(dioxus::desktop::LogicalPosition::new(-1000.0, -1000.0)),
                    )
                    .with_custom_head(
                        r#"<style>
                        #app { background: transparent; }
                    </style>"#
                            .to_string(),
                    ),
            )
            .launch(App);
    }
}

#[component]
fn App() -> Element {
    let window = use_window();

    // Load config from environment variable (required)
    let config_path = match env::var("HOTKI_CONFIG") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("Error: HOTKI_CONFIG environment variable not set");
            eprintln!("Please set HOTKI_CONFIG to the path of your RON configuration file");
            eprintln!("Example: HOTKI_CONFIG=/path/to/config.ron hotki");
            std::process::exit(1);
        }
    };

    let config_content = match fs::read_to_string(&config_path) {
        Ok(content) => {
            println!("Loaded config from: {config_path}");
            content
        }
        Err(e) => {
            eprintln!("Failed to read config file '{config_path}': {e}");
            std::process::exit(1);
        }
    };

    // Parse the config
    let mode = match Mode::from_ron(&config_content) {
        Ok(mode) => mode,
        Err(e) => {
            eprintln!("Failed to parse config file '{config_path}': {e}");
            std::process::exit(1);
        }
    };

    let keymode_state = use_signal(|| State::new(mode));
    let current_keys = use_signal(Vec::<(Key, String, keymode::Attrs)>::new);
    let error_msg = use_signal(String::new);
    let is_connected = use_signal(|| false);
    let should_rebind = use_signal(|| false);

    // Configure the HUD window properties
    use_effect({
        let window = window.clone();
        move || {
            // Hide window from dock on macOS
            platform_specific::hide_from_dock_for_window(&window);

            // Set HUD window properties
            window.set_decorations(false);
            window.set_always_on_top(true);
            window.set_resizable(false);
            window.set_visible_on_all_workspaces(true);
            window.set_inner_size(dioxus::desktop::LogicalSize::new(400.0, 300.0));

            // Position window at correct location
            if let Some(monitor) = window.current_monitor() {
                let screen_size = monitor.size();
                let scale_factor = monitor.scale_factor();
                let window_width = 400.0;
                let padding = 20.0;

                let physical_x = screen_size.width as f64
                    - (window_width * scale_factor)
                    - (padding * scale_factor);
                let physical_y = padding * scale_factor;

                let logical_x = physical_x / scale_factor;
                let logical_y = physical_y / scale_factor;

                window.set_outer_position(dioxus::desktop::LogicalPosition::new(
                    logical_x, logical_y,
                ));
            }

            window.set_visible(false);
        }
    });

    // Initialize system tray icon
    use_effect(move || {
        // Create a simple tray menu
        let tray_menu = Menu::new();

        // Add menu items with IDs to handle click events
        let settings_item = MenuItem::with_id("settings", "Settings", true, None);
        let separator = dioxus::desktop::trayicon::menu::PredefinedMenuItem::separator();
        let quit_item = MenuItem::with_id("quit", "Quit", true, None);

        let _ = tray_menu.append(&settings_item);
        let _ = tray_menu.append(&separator);
        let _ = tray_menu.append(&quit_item);

        // Initialize tray icon with default icon
        let tray_icon = dioxus::desktop::trayicon::init_tray_icon(
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
                let window_builder = dioxus::desktop::WindowBuilder::new()
                    .with_title("Settings")
                    .with_decorations(true)
                    .with_resizable(true)
                    .with_always_on_top(false)
                    .with_visible(true)
                    .with_inner_size(dioxus::desktop::LogicalSize::new(600.0, 500.0));
                let config = dioxus::desktop::Config::new().with_window(window_builder);
                dioxus::desktop::window().new_window(dom, config);
                println!("Settings window created");
            }
            "quit" => {
                // Quit the application
                std::process::exit(0);
            }
            _ => {}
        }
    });

    // Connect to hotkey server and handle events
    use_coroutine({
        let window = window.clone();

        move |_: UnboundedReceiver<()>| {
            let window = window.clone();
            let mut keymode_state = keymode_state;
            let mut current_keys = current_keys;
            let mut error_msg = error_msg;
            let mut is_connected = is_connected;
            let mut should_rebind = should_rebind;

            async move {
                // Try to connect to the server
                match Client::new().with_auto_spawn_server().connect().await {
                    Ok(mut client) => {
                        println!("Connected to hotkey server");
                        is_connected.set(true);

                        // Get connection and use it
                        match client.connection() {
                            Ok(connection) => {
                                // Initial key binding
                                let keys = keymode_state.read().keys();
                                current_keys.set(keys.clone());
                                let key_refs: Vec<Key> =
                                    keys.iter().map(|(k, _, _)| k.clone()).collect();

                                if let Err(e) = connection.rebind(&key_refs).await {
                                    error_msg.set(format!("Failed to bind keys: {e}"));
                                }

                                // Event loop
                                loop {
                                    // Check if we need to rebind keys
                                    if *should_rebind.read() {
                                        should_rebind.set(false);
                                        let keys = keymode_state.read().keys();
                                        let key_refs: Vec<Key> =
                                            keys.iter().map(|(k, _, _)| k.clone()).collect();

                                        if let Err(e) = connection.rebind(&key_refs).await {
                                            error_msg.set(format!("Failed to rebind keys: {e}"));
                                        }
                                    }

                                    // Process events with timeout
                                    match tokio::time::timeout(
                                        std::time::Duration::from_millis(100),
                                        connection.recv_event(),
                                    )
                                    .await
                                    {
                                        Ok(Ok(IPCResponse::HotkeyTriggered(key))) => {
                                            // Handle the key
                                            let result = keymode_state.write().handle_key(&key);
                                            match result {
                                                Ok(_handled) => {
                                                    // Update current keys after handling
                                                    let keys = keymode_state.read().keys();
                                                    current_keys.set(keys.clone());

                                                    // Request rebind
                                                    should_rebind.set(true);

                                                    // Check depth to show/hide window
                                                    let depth = keymode_state.read().depth();
                                                    let window_ref = window.clone();
                                                    if depth > 0 && !window_ref.is_visible() {
                                                        window_ref.set_visible(true);
                                                    } else if depth == 0 && window_ref.is_visible()
                                                    {
                                                        window_ref.set_visible(false);
                                                    }
                                                }
                                                Err(e) => {
                                                    error_msg
                                                        .set(format!("Error handling key: {e}"));
                                                }
                                            }
                                        }
                                        Ok(Ok(_)) => {}
                                        Ok(Err(e)) => {
                                            error_msg.set(format!("Connection error: {e}"));
                                            is_connected.set(false);
                                            break;
                                        }
                                        Err(_) => {
                                            // Timeout, continue loop
                                        }
                                    }
                                }

                                // Disconnect on exit
                                let _ = client.disconnect(true).await;
                            }
                            Err(e) => {
                                error_msg.set(format!("Failed to get connection: {e}"));
                                is_connected.set(false);
                            }
                        }
                    }
                    Err(e) => {
                        error_msg.set(format!("Failed to connect to server: {e}"));
                        is_connected.set(false);
                    }
                }
            }
        }
    });

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        div { class: "p-4",
            if !error_msg.read().is_empty() {
                div { class: "text-red-500 mb-4",
                    {error_msg.read().clone()}
                }
            }

            if !*is_connected.read() {
                div { class: "text-yellow-500 mb-4",
                    "Connecting to hotkey server..."
                }
            }

            div { class: "text-white",
                h2 { class: "text-2xl font-bold mb-4", "Available Keys" }

                div { class: "space-y-2",
                    for (key, desc, attrs) in current_keys.read().iter() {
                        if !attrs.hide {
                            div { class: "flex items-center space-x-4",
                                span { class: "font-mono bg-gray-700 px-2 py-1 rounded",
                                    {key.to_string()}
                                }
                                span { class: "text-gray-300",
                                    {desc.clone()}
                                }
                            }
                        }
                    }
                }

                if keymode_state.read().depth() > 0 {
                    div { class: "mt-4 text-sm text-gray-400",
                        "Depth: {keymode_state.read().depth()}"
                    }
                }
            }
        }
    }
}
