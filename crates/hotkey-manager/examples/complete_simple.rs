//! Complete example showing how simple hotkey management has become

use hotkey_manager::{Key, ManagedClientBuilder, run_server_on};
use std::env;

const SOCKET_PATH: &str = "/tmp/my-app-hotkeys.sock";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Server mode: just one line!
    if env::args().any(|arg| arg == "--server") {
        return run_server_on(SOCKET_PATH).map_err(|e| e.into());
    }

    // Client mode: automatic server management
    let mut client = ManagedClientBuilder::new(SOCKET_PATH)
        .with_server_executable(env::current_exe()?)
        .connect()
        .await?;

    // Use the connection
    let connection = client.connection()?;
    
    // Bind some hotkeys
    connection.rebind(&[
        ("quit".to_string(), Key::parse("cmd+q")?),
        ("save".to_string(), Key::parse("cmd+s")?),
    ]).await?;

    println!("Hotkeys registered! Press Cmd+Q to quit, Cmd+S to save");

    // Listen for events
    loop {
        if let hotkey_manager::ipc::IPCResponse::HotkeyTriggered { identifier } = connection.recv_event().await? {
            match identifier.as_str() {
                "quit" => {
                    println!("Quit hotkey pressed");
                    break;
                }
                "save" => println!("Save hotkey pressed"),
                _ => {}
            }
        }
    }

    // Cleanup is automatic
    client.disconnect(true).await?;
    Ok(())
}