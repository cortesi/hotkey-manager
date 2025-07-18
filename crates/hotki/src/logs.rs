use crate::ringbuffer::get_logs;
use dioxus::prelude::*;
use dioxus::desktop::{use_window, LogicalSize};

/// Signal to control logs window visibility
pub static SHOW_LOGS_WINDOW: GlobalSignal<bool> = Signal::global(|| false);

#[component]
pub fn LogsWindow() -> Element {
    let show_logs = *SHOW_LOGS_WINDOW.read();
    
    if !show_logs {
        return rsx! {};
    }
    
    let window = use_window();
    
    // Configure the logs window when it becomes visible/hidden
    use_effect(move || {
        if show_logs {
            window.set_title("Hotki - Logs (close via tray menu)");
            window.set_inner_size(LogicalSize::new(800.0, 600.0));
            window.set_minimizable(true);
            window.set_maximizable(true);
            window.set_resizable(true);
            window.set_visible(true);
            window.set_always_on_top(false);
            window.set_decorations(true); // Enable window decorations for logs
            // Note: We can't easily prevent window close, so instruct user via title
        } else {
            // When logs window is hidden, restore HUD window properties
            window.set_visible(false);
            window.set_decorations(false);
            window.set_always_on_top(true);
            window.set_resizable(false);
        }
    });

    // Get logs and reverse them (newest first)
    let logs = use_resource(move || async move {
        let mut logs = get_logs();
        logs.reverse(); // Show newest logs first
        logs
    });

    match &*logs.read_unchecked() {
        Some(log_lines) => {
            rsx! {
                div {
                    class: "logs-container",
                    style: "
                        width: 100vw;
                        height: 100vh;
                        background: #1e1e1e;
                        color: #d4d4d4;
                        font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', monospace;
                        font-size: 12px;
                        overflow-y: auto;
                        padding: 16px;
                        box-sizing: border-box;
                    ",
                    div {
                        class: "logs-header",
                        style: "
                            border-bottom: 1px solid #333;
                            padding-bottom: 8px;
                            margin-bottom: 16px;
                            color: #888;
                            font-weight: 600;
                        ",
                        "Application Logs ({log_lines.len()} entries) - Close via tray menu"
                    }
                    div {
                        class: "logs-content",
                        for (index, line) in log_lines.iter().enumerate() {
                            div {
                                key: "{index}",
                                class: "log-line",
                                style: "
                                    padding: 4px 8px;
                                    border-bottom: 1px solid #2a2a2a;
                                    line-height: 1.4;
                                    white-space: pre-wrap;
                                    word-break: break-all;
                                ",
                                "{line}"
                            }
                        }
                    }
                }
            }
        }
        None => {
            rsx! {
                div {
                    class: "logs-loading",
                    style: "
                        width: 100vw;
                        height: 100vh;
                        background: #1e1e1e;
                        color: #d4d4d4;
                        display: flex;
                        align-items: center;
                        justify-content: center;
                        font-family: system-ui;
                    ",
                    "Loading logs..."
                }
            }
        }
    }
}

/// Show the logs window
pub fn show_logs_window() {
    *SHOW_LOGS_WINDOW.write() = true;
}

/// Hide the logs window
pub fn hide_logs_window() {
    *SHOW_LOGS_WINDOW.write() = false;
}

/// Toggle the logs window visibility
pub fn toggle_logs_window() {
    let current_state = *SHOW_LOGS_WINDOW.read();
    *SHOW_LOGS_WINDOW.write() = !current_state;
}