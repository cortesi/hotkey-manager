//! The simplest possible hotkey server

use hotkey_manager::run_server;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // That's it! One line to run a fully functional hotkey server
    run_server()?;
    Ok(())
}