mod hud;
mod platform_specific;
mod settings;

use dioxus::desktop::trayicon::menu::{Menu, MenuItem};
use dioxus::desktop::{use_muda_event_handler, use_window, Config};
use dioxus::prelude::*;
//use hotkey_manager::{Code, HotkeyManager, Modifiers};
use std::sync::{mpsc, Arc, Mutex, OnceLock};

const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

// Global static for the HotkeyManager
static HOTKEY_MANAGER: OnceLock<Arc<Mutex<HotkeyManager>>> = OnceLock::new();

fn main() {
    // Configure the app as a background agent before anything else
    platform_specific::configure_as_agent_app();

    // Create HotkeyManager on the main thread
    let hotkey_manager = Arc::new(Mutex::new(
        HotkeyManager::new().expect("Failed to create hotkey manager"),
    ));

    // Store it in the global static
    let _ = HOTKEY_MANAGER.set(hotkey_manager);

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

#[component]
fn App() -> Element {
    let window = use_window();
    let mut window_shown_intentionally = use_signal(|| false);
    let hotkey_manager = HOTKEY_MANAGER
        .get()
        .expect("HotkeyManager not initialized")
        .clone();
    let mut hud_hotkey_id = use_signal(|| None::<u32>);
    let hotkey_channel = use_signal(|| {
        let (tx, rx) = mpsc::channel::<()>();
        (Arc::new(tx), Arc::new(Mutex::new(rx)))
    });

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

            // Position window at correct location off-screen
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

    // Monitor window visibility to prevent unintentional shows
    use_effect({
        let window = window.clone();
        move || {
            // If window becomes visible without being triggered intentionally, hide it
            if window.is_visible() && !window_shown_intentionally() {
                println!("Window shown unintentionally - hiding it");
                window.set_visible(false);
            }
        }
    });

    // Initialize system tray icon
    use_effect(move || {
        // Create a simple tray menu
        let tray_menu = Menu::new();

        // Add menu items with IDs to handle click events
        let hud_item = MenuItem::with_id("show-hud", "Show HUD - Cmd+Shift+0", true, None);
        let settings_item = MenuItem::with_id("settings", "Settings", true, None);
        let separator = dioxus::desktop::trayicon::menu::PredefinedMenuItem::separator();
        let quit_item = MenuItem::with_id("quit", "Quit", true, None);

        let _ = tray_menu.append(&hud_item);
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
        let _ = tray_icon.set_tooltip(Some("AI HUD - Press Cmd+Shift+0 to toggle"));

        println!("Tray icon initialized");
    });

    // Handle tray menu click events
    let window_menu = window.clone();
    use_muda_event_handler(move |event| {
        match event.id().as_ref() {
            "show-hud" => {
                println!("Show HUD menu clicked");
                window_shown_intentionally.set(true);
                // Toggle HUD visibility
                if window_menu.is_visible() {
                    window_menu.set_visible(false);
                } else {
                    window_menu.set_visible(true);
                }
            }
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

    // Register global hotkey for HUD toggle
    use_effect({
        let manager = hotkey_manager.clone();
        let (tx, _) = hotkey_channel();
        let tx = tx.clone();
        move || {
            if let Ok(manager) = manager.lock() {
                let tx_clone = tx.clone();
                match manager.bind(
                    "toggle_hud",
                    Some(Modifiers::SUPER | Modifiers::SHIFT),
                    Code::Digit0,
                    move |identifier| {
                        match identifier {
                            "toggle_hud" => {
                                // Send signal through channel
                                let _ = tx_clone.send(());
                            }
                            _ => {
                                eprintln!("Warning: Unrecognized hotkey identifier: {identifier}");
                            }
                        }
                    },
                ) {
                    Ok(id) => {
                        hud_hotkey_id.set(Some(id));
                        println!("HUD toggle hotkey registered: Cmd+Shift+0");
                    }
                    Err(e) => {
                        eprintln!("Failed to register HUD hotkey: {e}");
                    }
                }
            }
        }
    });

    // Poll for hotkey events using a coroutine
    use_coroutine({
        let window_hud = window.clone();
        let (_, rx) = hotkey_channel();
        move |_rx_coroutine: UnboundedReceiver<()>| {
            let window_hud = window_hud.clone();
            let rx = rx.clone();
            async move {
                loop {
                    // Check for hotkey events
                    if let Ok(rx) = rx.lock() {
                        if rx.try_recv().is_ok() {
                            window_shown_intentionally.set(true);

                            // Toggle window visibility
                            if window_hud.is_visible() {
                                window_hud.set_visible(false);
                            } else {
                                window_hud.set_visible(true);
                            }
                        }
                    }
                    // Poll every 50ms
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        }
    });

    // Cleanup hotkeys on unmount
    use_drop(move || {
        if let Some(id) = hud_hotkey_id() {
            if let Ok(manager) = hotkey_manager.lock() {
                let _ = manager.unbind(id);
            }
        }
    });

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        hud::Hero {}
    }
}
