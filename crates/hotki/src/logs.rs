use crate::ringbuffer::get_logs;
use dioxus::prelude::*;

#[component]
pub fn LogsWindow() -> Element {
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
                        "Logs ({log_lines.len()} entries)"
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

