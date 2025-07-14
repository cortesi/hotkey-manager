use dioxus::prelude::*;

const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

#[component]
pub fn SettingsWindow() -> Element {
    // Log when settings window renders
    use_effect(move || {
        println!("SettingsWindow component mounted");
    });

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }

        div {
            class: "settings-container p-8",
            h1 { class: "text-3xl font-bold mb-6", "Settings" }

            div { class: "space-y-4",
                div { class: "border-b pb-4",
                    h2 { class: "text-xl font-semibold mb-2", "General" }
                    label { class: "flex items-center gap-2",
                        input { type: "checkbox", class: "w-4 h-4" }
                        span { "Start at login" }
                    }
                }

                div { class: "border-b pb-4",
                    h2 { class: "text-xl font-semibold mb-2", "Appearance" }
                    p { class: "text-gray-600", "Appearance options coming soon..." }
                }

                div { class: "border-b pb-4",
                    h2 { class: "text-xl font-semibold mb-2", "Hotkeys" }
                    div { class: "space-y-2",
                        p { "Show HUD: Cmd+Shift+0" }
                        p { "Show Settings: Click the gear icon in HUD or via tray menu" }
                    }
                }

                div { class: "pt-4",
                    p { class: "text-sm text-gray-500", "AI HUD v0.1.0" }
                }
            }
        }
    }
}
