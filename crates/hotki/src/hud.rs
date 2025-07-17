use dioxus::{
    desktop::{use_window, DesktopService, LogicalPosition, LogicalSize},
    prelude::*,
};
use std::rc::Rc;

use hotkey_manager::{Client, IPCResponse, Key};
use keymode::State;

use crate::{
    config::{Config, Pos},
    platform_specific,
};

const WINDOW_WIDTH: f64 = 400.0;
const WINDOW_PADDING: f64 = 20.0;
const AUTO_HIDE_TIMEOUT_MS: u64 = 3000; // 3 seconds

/// Calculates the exact window height needed to contain the HUD content without clipping.
///
/// This function must precisely match the CSS layout to prevent content from being clipped.
/// All values correspond to specific CSS rules and DOM structure.
///
/// # Layout Structure
/// ```
/// Window
/// └── .hud-container (CSS: margin: 20px, padding: 20px)
///     ├── Error message (optional, CSS: mb-4)
///     ├── Connection status (optional, CSS: mb-4)
///     └── .space-y-2 container
///         └── Key items (CSS: .flex.items-center with .space-y-2 spacing)
/// ```
///
/// # CSS Sources
/// - `.hud-container` margin: 20px → 40px total vertical margin (assets/main.css:32)
/// - `.hud-container` padding: 20px → 40px total vertical padding (assets/main.css:31)
/// - `.mb-4` margin-bottom: 16px (tailwind.css:186, --spacing * 4 = 4px * 4)
/// - `.space-y-2` margin: 8px between items (tailwind.css:219, --spacing * 2 = 4px * 2)
/// - Base line-height: 1.5 → 24px for 16px font (tailwind.css:41)
/// - `.py-1` padding: 4px top+bottom (tailwind.css:257, --spacing * 1 = 4px * 1)
fn calculate_window_height(visible_count: usize, has_error: bool, is_connected: bool) -> f64 {
    // CSS .hud-container padding: 20px (top) + 20px (bottom) = 40px total
    let padding = 40.0;

    // CSS .hud-container margin: 20px (top) + 20px (bottom) = 40px total
    let margin = 40.0;

    // Each key item height calculation (increased to prevent clipping):
    // - .flex.items-center container with default line-height: 1.5
    // - Key span: 16px font × 1.5 line-height = 24px + .py-1 (4px top+bottom) = 32px
    // - Description span: 16px font × 1.5 line-height = 24px
    // - .space-y-2 adds 8px margin-bottom between items
    // - Total per item: max(32px, 24px) + 8px = 40px
    // - Adding extra padding to ensure no clipping
    let item_height = 44.0;

    // Error message height: 16px font × 1.5 line-height = 24px + .mb-4 (16px) = 40px
    let error_height = if has_error { 40.0 } else { 0.0 };

    // Connection status height: 16px font × 1.5 line-height = 24px + .mb-4 (16px) = 40px
    let connection_height = if !is_connected { 40.0 } else { 0.0 };

    let content_height = (visible_count as f64 * item_height) + error_height + connection_height;
    content_height + padding + margin
}

fn calculate_window_position(
    pos: Pos,
    screen_width: f64,
    screen_height: f64,
    window_width: f64,
    window_height: f64,
    padding: f64,
) -> (f64, f64) {
    match pos {
        Pos::N => {
            let x = (screen_width / 2.0) - (window_width / 2.0);
            let y = padding;
            (x, y)
        }
        Pos::NE => {
            let x = screen_width - window_width - padding;
            let y = padding;
            (x, y)
        }
        Pos::E => {
            let x = screen_width - window_width - padding;
            let y = (screen_height / 2.0) - (window_height / 2.0);
            (x, y)
        }
        Pos::SE => {
            let x = screen_width - window_width - padding;
            let y = screen_height - window_height - padding;
            (x, y)
        }
        Pos::S => {
            let x = (screen_width / 2.0) - (window_width / 2.0);
            let y = screen_height - window_height - padding;
            (x, y)
        }
        Pos::SW => {
            let x = padding;
            let y = screen_height - window_height - padding;
            (x, y)
        }
        Pos::W => {
            let x = padding;
            let y = (screen_height / 2.0) - (window_height / 2.0);
            (x, y)
        }
        Pos::NW => {
            let x = padding;
            let y = padding;
            (x, y)
        }
        Pos::Center => {
            let x = (screen_width / 2.0) - (window_width / 2.0);
            let y = (screen_height / 2.0) - (window_height / 2.0);
            (x, y)
        }
    }
}

/// Configure HUD window properties (decorations, positioning, visibility, etc.)
fn setup_hud_window(window: &Rc<DesktopService>) {
    // Hide window from dock on macOS
    platform_specific::hide_from_dock_for_window(window);

    // Set HUD window properties
    window.set_decorations(false);
    window.set_always_on_top(true);
    window.set_resizable(false);
    window.set_visible_on_all_workspaces(true);
    window.set_visible(false);
}

/// Position and size the window based on current content and configuration
fn position_and_size_window(
    window: &Rc<DesktopService>,
    visible_count: usize,
    has_error: bool,
    is_connected: bool,
    config: &Config,
) {
    let window_height = calculate_window_height(visible_count, has_error, is_connected);

    // Debug output to understand initial sizing
    println!(
        "DEBUG: initial show - visible_count: {visible_count}, calculated height: {window_height}"
    );

    window.set_inner_size(LogicalSize::new(WINDOW_WIDTH, window_height));

    // Position window
    if let Some(monitor) = window.current_monitor() {
        let screen_size = monitor.size();
        let scale_factor = monitor.scale_factor();

        let (physical_x, physical_y) = calculate_window_position(
            config.pos,
            screen_size.width as f64,
            screen_size.height as f64,
            WINDOW_WIDTH * scale_factor,
            window_height * scale_factor,
            WINDOW_PADDING * scale_factor,
        );

        let logical_x = physical_x / scale_factor;
        let logical_y = physical_y / scale_factor;

        window.set_outer_position(LogicalPosition::new(logical_x, logical_y));
    }
}

/// State container for HUD signals
struct HudState {
    keymode_state: Signal<State>,
    current_keys: Signal<Vec<(Key, String, keymode::Attrs)>>,
    error_msg: Signal<String>,
    is_connected: Signal<bool>,
    should_rebind: Signal<bool>,
}

/// Handle a triggered hotkey and update window state accordingly
fn handle_triggered_key(
    key: &Key,
    window: &Rc<DesktopService>,
    initial_config: &Config,
    state: &mut HudState,
) {
    // Handle the key
    let result = state.keymode_state.write().handle_key(key);
    match result {
        Ok(_handled) => {
            // Update current keys after handling
            let keys = state.keymode_state.read().keys();
            state.current_keys.set(keys.clone());

            // Hide current window
            window.set_visible(false);

            // Request rebind
            state.should_rebind.set(true);

            // Check depth to show/hide window
            let depth = state.keymode_state.read().depth();
            let window_ref = window.clone();
            if depth > 0 && !window_ref.is_visible() {
                // Calculate and set window size before showing
                let visible_count = state
                    .current_keys
                    .read()
                    .iter()
                    .filter(|(_, _, attrs)| !attrs.hide)
                    .count();

                position_and_size_window(
                    &window_ref,
                    visible_count,
                    !state.error_msg.read().is_empty(),
                    *state.is_connected.read(),
                    initial_config,
                );

                // Now show the window
                window_ref.set_visible(true);
            } else if depth == 0 && window_ref.is_visible() {
                window_ref.set_visible(false);
            }
        }
        Err(e) => {
            state.error_msg.set(format!("Error handling key: {e}"));
        }
    }
}

/// Bind or rebind keys with the hotkey server
async fn bind_keys(connection: &mut hotkey_manager::IPCConnection, state: &mut HudState) {
    let keys = state.keymode_state.read().keys();
    state.current_keys.set(keys.clone());
    let key_refs: Vec<Key> = keys.iter().map(|(k, _, _)| k.clone()).collect();

    if let Err(e) = connection.rebind(&key_refs).await {
        state.error_msg.set(format!("Failed to bind keys: {e}"));
    }
}

/// Main event processing loop for handling hotkey triggers
async fn run_event_loop(
    connection: &mut hotkey_manager::IPCConnection,
    window: &Rc<DesktopService>,
    initial_config: &Config,
    state: &mut HudState,
) {
    // Initial key binding
    bind_keys(connection, state).await;

    loop {
        // Check if we need to rebind keys
        if *state.should_rebind.read() {
            state.should_rebind.set(false);
            bind_keys(connection, state).await;
        }

        // Process events with timeout
        match tokio::time::timeout(
            std::time::Duration::from_millis(100),
            connection.recv_event(),
        )
        .await
        {
            Ok(Ok(IPCResponse::HotkeyTriggered(key))) => {
                handle_triggered_key(&key, window, initial_config, state);
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                state.error_msg.set(format!("Connection error: {e}"));
                state.is_connected.set(false);
                break;
            }
            Err(_) => {
                // Timeout, continue loop
            }
        }
    }
}

/// Handle server connection and key event processing
async fn handle_server_connection(
    window: Rc<DesktopService>,
    initial_config: Config,
    keymode_state: Signal<State>,
    current_keys: Signal<Vec<(Key, String, keymode::Attrs)>>,
    mut error_msg: Signal<String>,
    mut is_connected: Signal<bool>,
    should_rebind: Signal<bool>,
) {
    // Try to connect to the server
    match Client::new().with_auto_spawn_server().connect().await {
        Ok(mut client) => {
            println!("Connected to hotkey server");
            is_connected.set(true);

            // Get connection and use it
            match client.connection() {
                Ok(connection) => {
                    // Event loop (includes initial key binding)
                    let mut state = HudState {
                        keymode_state,
                        current_keys,
                        error_msg,
                        is_connected,
                        should_rebind,
                    };
                    run_event_loop(connection, &window, &initial_config, &mut state).await;

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

#[component]
pub fn HudWindow() -> Element {
    let window = use_window();
    let initial_config = use_context::<Config>();

    let keymode_state = use_signal(|| State::new(initial_config.keys.clone()));
    let current_keys = use_signal(Vec::<(Key, String, keymode::Attrs)>::new);
    let error_msg = use_signal(String::new);
    let is_connected = use_signal(|| false);
    let should_rebind = use_signal(|| false);

    // Configure the HUD window properties
    use_effect({
        let window = window.clone();
        move || {
            setup_hud_window(&window);
        }
    });

    // Connect to hotkey server and handle events
    use_coroutine({
        let window = window.clone();

        move |_: UnboundedReceiver<()>| {
            let window = window.clone();
            handle_server_connection(
                window,
                initial_config.clone(),
                keymode_state,
                current_keys,
                error_msg,
                is_connected,
                should_rebind,
            )
        }
    });

    // Monitor window visibility and auto-hide when depth is 0
    use_coroutine({
        let window = window.clone();
        move |_: UnboundedReceiver<()>| {
            let window = window.clone();
            async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                    if window.is_visible() && keymode_state.read().depth() == 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(AUTO_HIDE_TIMEOUT_MS))
                            .await;
                        // Check again in case depth changed while waiting
                        if window.is_visible() && keymode_state.read().depth() == 0 {
                            window.set_visible(false);
                        }
                    }
                }
            }
        }
    });

    rsx! {
        div {
            class: "hud-container",
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
            }
        }
    }
}
