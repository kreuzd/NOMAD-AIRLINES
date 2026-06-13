//! NOMAD Airlines — Tauri shell (desktop + Android/iOS).
//!
//! Rather than talking to Rust over Tauri IPC, this shell starts the very same
//! HTTP backend used by the Docker/web target on a localhost port, then points
//! the webview at it. That keeps a single frontend code path (`fetch("/api")`)
//! across every target.
//!
//! Flow in [`run`]:
//!   1. Resolve the bundled frontend directory and a writable DB path.
//!   2. Configure the backend via environment variables.
//!   3. Spawn the backend on a background Tokio runtime.
//!   4. Wait until it accepts connections, then open the main window at it.

use std::time::Duration;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Default localhost port for the embedded backend (override with `NOMAD_PORT`).
const DEFAULT_PORT: u16 = 8787;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let frontend_dir = resolve_frontend_dir(app);

            // Per-user writable location for the SQLite database.
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let _ = std::fs::create_dir_all(&data_dir);
            let db_path = data_dir.join("nomad.db");

            let port: u16 = std::env::var("NOMAD_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_PORT);
            let bind_addr = format!("127.0.0.1:{port}");

            // The backend reads its configuration from the environment.
            std::env::set_var("NOMAD_BIND_ADDR", &bind_addr);
            std::env::set_var("NOMAD_FRONTEND_DIR", &frontend_dir);
            std::env::set_var("NOMAD_DB_PATH", db_path.to_string_lossy().to_string());

            // Run the backend on its own multi-threaded runtime/thread so it
            // doesn't block the Tauri/main event loop.
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build Tokio runtime");
                rt.block_on(async move {
                    if let Err(e) = nomad_backend::run().await {
                        eprintln!("[nomad] backend exited with error: {e}");
                    }
                });
            });

            wait_for_server(&bind_addr, Duration::from_secs(15));

            let url = format!("http://{bind_addr}/");
            WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(url.parse().expect("valid backend URL")),
            )
            .title("NOMAD Airlines")
            .inner_size(1200.0, 820.0)
            .min_inner_size(800.0, 600.0)
            .build()?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running NOMAD Airlines");
}

/// Find the frontend directory: bundled resources in a packaged app, else the
/// repo's `../frontend` during `cargo tauri dev`.
fn resolve_frontend_dir(app: &tauri::App) -> String {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join("frontend");
        if candidate.join("index.html").exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend")
        .to_string_lossy()
        .into_owned()
}

/// Block until the backend accepts TCP connections (or the timeout elapses).
fn wait_for_server(addr: &str, timeout: Duration) {
    let start = std::time::Instant::now();
    loop {
        if std::net::TcpStream::connect(addr).is_ok() {
            return;
        }
        if start.elapsed() > timeout {
            eprintln!("[nomad] backend did not become ready within {timeout:?}");
            return;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}
