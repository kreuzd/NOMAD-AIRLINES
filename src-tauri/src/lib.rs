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

/// Injected into the webview on mobile only. The Android/iOS webview renders
/// edge-to-edge, so the system status bar and navigation/gesture bar overlap
/// jspaint's top menu bar and bottom color palette. This adds `viewport-fit=cover`
/// (required for `env(safe-area-inset-*)` to be populated) and pads jspaint's
/// root `.jspaint` container by the safe-area insets. `.jspaint` is
/// `box-sizing: border-box`, so the padding insets the content without
/// overflowing the viewport.
#[cfg(mobile)]
const SAFE_AREA_INIT_SCRIPT: &str = r#"
(function () {
  function applySafeArea() {
    var vp = document.querySelector('meta[name="viewport"]');
    if (vp && vp.content.indexOf('viewport-fit') === -1) {
      vp.content = vp.content + ', viewport-fit=cover';
    }
    if (!document.getElementById('nomad-safe-area')) {
      var style = document.createElement('style');
      style.id = 'nomad-safe-area';
      style.textContent =
        '.jspaint{' +
          'box-sizing:border-box;' +
          'padding-top:env(safe-area-inset-top);' +
          'padding-bottom:env(safe-area-inset-bottom);' +
          'padding-left:env(safe-area-inset-left);' +
          'padding-right:env(safe-area-inset-right);' +
        '}';
      (document.head || document.documentElement).appendChild(style);
    }
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', applySafeArea);
  } else {
    applySafeArea();
  }
  window.addEventListener('load', applySafeArea);
})();
"#;

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
            #[allow(unused_mut)]
            let mut builder = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(url.parse().expect("valid backend URL")),
            )
            .title("NOMAD Airlines")
            .inner_size(1200.0, 820.0)
            .min_inner_size(800.0, 600.0);

            // On mobile the webview draws edge-to-edge, so the system status bar
            // and navigation/gesture bar overlap the app's top menu and bottom
            // palette. Inset the UI by the safe-area amounts. Scoped to mobile so
            // the shared web/desktop frontend is untouched.
            #[cfg(mobile)]
            {
                builder = builder.initialization_script(SAFE_AREA_INIT_SCRIPT);
            }

            builder.build()?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running NOMAD Airlines");
}

/// Find the frontend directory the HTTP backend should serve.
///
/// * **Desktop** — bundled resources are extracted to the filesystem, so the
///   `resource_dir/frontend` directory exists and is used directly.
/// * **Android/iOS** — bundled assets live *inside* the app package and are not
///   accessible as plain filesystem paths, so the backend's `ServeDir`
///   (`std::fs`) cannot read them. We instead extract the embedded
///   `frontendDist` assets to a writable, version-scoped directory once and
///   serve from there.
/// * **Dev** (`cargo tauri dev`) — fall back to the repo's `../frontend`.
fn resolve_frontend_dir(app: &tauri::App) -> String {
    // Desktop: resources are real files on disk.
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join("frontend");
        if candidate.join("index.html").exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }

    // Mobile: extract the embedded frontend to a writable directory.
    if let Ok(data_dir) = app.path().app_data_dir() {
        let dest = data_dir.join("frontend").join(env!("CARGO_PKG_VERSION"));
        if extract_frontend_assets(app, &dest) {
            return dest.to_string_lossy().into_owned();
        }
    }

    // Dev fallback: the repo's frontend directory next to the crate.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend")
        .to_string_lossy()
        .into_owned()
}

/// Extract the embedded `frontendDist` assets into `dest` so the HTTP backend
/// can serve them from the filesystem on platforms (Android/iOS) where bundled
/// assets are not real files.
///
/// Uses [`AssetResolver::get`] (which decompresses) rather than the raw
/// [`AssetResolver::iter`] bytes (which are brotli-compressed). With
/// `security.csp = null` the returned HTML is byte-identical to the source, so
/// this matches the desktop's real-file serving exactly.
///
/// Extraction is skipped when `dest/index.html` already exists, so it runs at
/// most once per app version. Returns `true` if the index is present afterward.
fn extract_frontend_assets(app: &tauri::App, dest: &std::path::Path) -> bool {
    let index = dest.join("index.html");
    if index.exists() {
        return true;
    }

    let resolver = app.asset_resolver();
    // Collect keys first to avoid holding the iterator while calling `get`.
    let keys: Vec<String> = resolver.iter().map(|(k, _)| k.into_owned()).collect();
    for key in keys {
        let rel = key.trim_start_matches(['/', '\\']);
        if rel.is_empty() {
            continue;
        }
        let out = dest.join(rel);
        // Refuse to write outside `dest` (guard against unexpected keys).
        if !out.starts_with(dest) {
            continue;
        }
        let Some(asset) = resolver.get(key.clone()) else {
            continue;
        };
        if let Some(parent) = out.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
        }
        let _ = std::fs::write(&out, &asset.bytes);
    }

    index.exists()
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
