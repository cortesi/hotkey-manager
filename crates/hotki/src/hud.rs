use dioxus::prelude::*;

#[component]
pub fn Hero() -> Element {
    rsx! {
        div {
            class: "hud-container relative",
            h1 { class: "text-2xl font-bold mb-4", "AI HUD" }
            p { "This is a placeholder for your AI HUD content." }
        }
    }
}
