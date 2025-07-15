use crate::config::{Config, Pos};
use dioxus::desktop::{use_window, LogicalPosition, LogicalSize};
use dioxus::prelude::*;
use hotkey_manager::{Client, IPCResponse, Key};
use keymode::State;

const WINDOW_WIDTH: f64 = 400.0;
const WINDOW_PADDING: f64 = 20.0;

fn calculate_window_height(visible_count: usize, has_error: bool, is_connected: bool) -> f64 {
    let padding = 40.0; // Total padding (20px top + 20px bottom)
    let item_height = 36.0; // Height per item including spacing
    let error_height = if has_error { 40.0 } else { 0.0 };
    let connection_height = if !is_connected { 40.0 } else { 0.0 };

    let content_height = (visible_count as f64 * item_height) + error_height + connection_height;
    content_height + padding
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
            // Hide window from dock on macOS
            crate::platform_specific::hide_from_dock_for_window(&window);

            // Set HUD window properties
            window.set_decorations(false);
            window.set_always_on_top(true);
            window.set_resizable(false);
            window.set_visible_on_all_workspaces(true);
            window.set_visible(false);
        }
    });

    // Update window size when content changes (but not position)
    use_effect({
        let window = window.clone();

        move || {
            // Only update if window is visible to avoid unnecessary calculations
            if !window.is_visible() {
                return;
            }

            // Calculate the number of visible items
            let visible_count = current_keys
                .read()
                .iter()
                .filter(|(_, _, attrs)| !attrs.hide)
                .count();

            // Calculate window height using helper function
            let window_height = calculate_window_height(
                visible_count,
                !error_msg.read().is_empty(),
                *is_connected.read(),
            );

            // Only update size, not position
            window.set_inner_size(LogicalSize::new(WINDOW_WIDTH, window_height));
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
                                                        // Calculate and set window size before showing
                                                        let visible_count = current_keys
                                                            .read()
                                                            .iter()
                                                            .filter(|(_, _, attrs)| !attrs.hide)
                                                            .count();

                                                        let window_height = calculate_window_height(
                                                            visible_count,
                                                            !error_msg.read().is_empty(),
                                                            *is_connected.read(),
                                                        );

                                                        window_ref.set_inner_size(
                                                            LogicalSize::new(
                                                                WINDOW_WIDTH,
                                                                window_height,
                                                            ),
                                                        );

                                                        // Position window
                                                        if let Some(monitor) =
                                                            window_ref.current_monitor()
                                                        {
                                                            let screen_size = monitor.size();
                                                            let scale_factor =
                                                                monitor.scale_factor();

                                                            let (physical_x, physical_y) =
                                                                calculate_window_position(
                                                                    initial_config.pos,
                                                                    screen_size.width as f64,
                                                                    screen_size.height as f64,
                                                                    WINDOW_WIDTH * scale_factor,
                                                                    window_height * scale_factor,
                                                                    WINDOW_PADDING * scale_factor,
                                                                );

                                                            let logical_x =
                                                                physical_x / scale_factor;
                                                            let logical_y =
                                                                physical_y / scale_factor;

                                                            window_ref.set_outer_position(
                                                                LogicalPosition::new(
                                                                    logical_x, logical_y,
                                                                ),
                                                            );
                                                        }

                                                        // Now show the window
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
