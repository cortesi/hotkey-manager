// Since we only support macOS, no need for cfg attributes
#![allow(unexpected_cfgs)]


pub fn configure_as_agent_app() {
    // unsafe {
    //     let app = NSApp();
    //
    //     // Set activation policy to Accessory (hides from dock and app switcher)
    //     app.setActivationPolicy_(
    //         NSApplicationActivationPolicy::NSApplicationActivationPolicyAccessory,
    //     );
    //
    //     // Remove the application icon entirely
    //     let _: () = msg_send![app, setApplicationIconImage:nil];
    //
    //     // Re-assert the activation policy using msg_send
    //     let _: () = msg_send![app, setActivationPolicy:1i64];
    //     // Hide the app again
    //     let _: () = msg_send![app, hide:nil];
    // }
}

pub fn hide_from_dock_for_window(_window: &dioxus::desktop::DesktopContext) {
    // // Additional attempt to hide from dock
    // // Since Dioxus doesn't expose raw window handle easily,
    // // we'll rely on the app-level activation policy
    // unsafe {
    //     let app = NSApp();
    //     // Re-assert the activation policy using msg_send
    //     let _: () = msg_send![app, setActivationPolicy:1i64];
    //     // Hide the app again
    //     let _: () = msg_send![app, hide:nil];
    // }
}
