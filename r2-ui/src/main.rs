//! r2 — S3-kompatibler Object-Storage-Browser
//!
//! Entry point for the GTK4 application.

use tracing_subscriber::filter::EnvFilter;

mod app;
mod dialogs;
mod pane;
mod profile_manager;
mod widgets;

fn main() {
    // Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .init();

    tracing::info!("Starting r2 — S3 Object Storage Browser v{}", env!("CARGO_PKG_VERSION"));

    // Initialize GTK
    gtk4::init().expect("Failed to initialize GTK4");

    // Create and run the application
    let mut r2_app = app::R2App::new();
    r2_app.run();
}
