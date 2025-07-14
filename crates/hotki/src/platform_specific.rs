// Since we only support macOS, no need for cfg attributes
#![allow(unexpected_cfgs)]

use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicy};
use cocoa::base::nil;
use objc::{msg_send, sel, sel_impl};

pub fn configure_as_agent_app() {
    unsafe {
        let app = NSApp();
        
        // Set activation policy to Accessory (hides from dock and app switcher)
        app.setActivationPolicy_(NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory);
        
        // Also use msg_send to ensure it takes effect
        let _: () = msg_send![app, setActivationPolicy:1i64];
        
        // Hide the app
        let _: () = msg_send![app, hide:nil];
    }
}

pub fn hide_from_dock_for_window(_window: &dioxus::desktop::DesktopContext) {
    // Additional attempt to hide from dock
    // Since Dioxus doesn't expose raw window handle easily,
    // we'll rely on the app-level activation policy
    unsafe {
        let app = NSApp();
        // Re-assert the activation policy using msg_send
        let _: () = msg_send![app, setActivationPolicy:1i64];
        // Hide the app again
        let _: () = msg_send![app, hide:nil];
    }
}