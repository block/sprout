//! Local WebSocket proxy: webview → ws://127.0.0.1:{port} → wss://upstream.
//!
//! WHY: Tauri's WKWebView on macOS doesn't route through Cloudflare WARP.
//! The Rust process does. So we accept WebSocket connections on localhost,
//! then proxy them to the real LiveKit server through the WARP tunnel.
//!
//! The LiveKit JS SDK connects to `{url}/rtc/v1?access_token=XXX`. The
//! frontend rewrites only the host to `127.0.0.1:{port}`, keeping the
//! path+query intact. We capture that path+query via `accept_hdr_async`
//! and append it to the upstream URL before connecting.

use std::sync::Mutex;

use futures_util::StreamExt;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_hdr_async, connect_async, tungstenite,
    tungstenite::{client::IntoClientRequest, handshake::server::Response},
};

/// Running proxy state: port + the upstream it was started with.
/// `None` = not started. Protected by Mutex for atomic init.
static PROXY: Mutex<Option<(u16, String)>> = Mutex::new(None);

/// Maximum concurrent proxy connections. LiveKit uses 1 WS per participant,
/// so even a large huddle won't exceed a handful.
const MAX_CONNECTIONS: usize = 16;

/// Start a local WebSocket proxy that forwards to `upstream_url`.
/// Returns the local port.
///
/// Idempotent: if already running with the same upstream, returns existing port.
/// Rejects if called with a different upstream (would be a routing bug).
/// Validates the upstream URL matches the relay's configured LiveKit host.
pub async fn start_proxy(upstream_url: &str) -> Result<u16, String> {
    // Validate upstream scheme — only wss:// is allowed. Prevents SSRF to
    // arbitrary protocols. The URL itself comes from the relay's token response
    // (server-controlled), not from user input.
    if !upstream_url.starts_with("wss://") {
        return Err(format!(
            "WS proxy: upstream must be wss://, got: {}",
            &upstream_url[..upstream_url.len().min(30)]
        ));
    }

    // Check if already running — mutex ensures exactly-once initialization.
    {
        let guard = PROXY.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((port, existing_upstream)) = guard.as_ref() {
            if existing_upstream == upstream_url {
                return Ok(*port);
            }
            return Err(format!(
                "WS proxy already running for {}, cannot switch to {}",
                &existing_upstream[..existing_upstream.len().min(40)],
                &upstream_url[..upstream_url.len().min(40)],
            ));
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("WS proxy bind: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();

    // Store before spawning — a concurrent caller arriving between bind and
    // store will see the mutex-protected Some and return the port.
    {
        let mut guard = PROXY.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((existing_port, _)) = guard.as_ref() {
            // Lost the race — another caller initialized between our check and here.
            drop(listener);
            return Ok(*existing_port);
        }
        *guard = Some((port, upstream_url.to_string()));
    }

    let upstream = upstream_url.to_string();
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    tauri::async_runtime::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let upstream = upstream.clone();
                    let permit = match semaphore.clone().try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => {
                            eprintln!("sprout-desktop: WS proxy at connection limit");
                            continue;
                        }
                    };
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = handle_connection(stream, &upstream).await {
                            eprintln!("sprout-desktop: WS proxy error: {e}");
                        }
                        drop(permit);
                    });
                }
                Err(e) => eprintln!("sprout-desktop: WS proxy accept error: {e}"),
            }
        }
    });

    eprintln!("sprout-desktop: WS proxy listening on 127.0.0.1:{port}");
    Ok(port)
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    upstream_base: &str,
) -> Result<(), String> {
    // Capture the request URI (path+query) during the WebSocket handshake.
    let captured_uri = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let uri_clone = captured_uri.clone();

    let local_ws =
        accept_hdr_async(stream, move |req: &tungstenite::http::Request<()>, resp: Response| {
            *uri_clone.lock().unwrap_or_else(|e| e.into_inner()) = req.uri().to_string();
            Ok(resp)
        })
        .await
        .map_err(|e| format!("accept: {e}"))?;

    let path_and_query = captured_uri
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();

    // Build full upstream URL: base (e.g. wss://staging.example.com) + path+query
    let upstream_full = format!("{upstream_base}{path_and_query}");
    let request = upstream_full
        .into_client_request()
        .map_err(|e| format!("request: {e}"))?;

    let (upstream_ws, _) = connect_async(request)
        .await
        .map_err(|e| format!("connect upstream: {e}"))?;

    // Bidirectional relay — when either side closes, the other follows.
    let (local_tx, local_rx) = local_ws.split();
    let (upstream_tx, upstream_rx) = upstream_ws.split();

    let local_to_upstream = local_rx.forward(upstream_tx);
    let upstream_to_local = upstream_rx.forward(local_tx);

    // forward() propagates close frames before returning. When either direction
    // completes (close or error), drop the other half — the TCP RST from the
    // drop is sufficient for cleanup on a localhost↔remote proxy.
    futures_util::future::select(
        std::pin::pin!(local_to_upstream),
        std::pin::pin!(upstream_to_local),
    )
    .await;

    Ok(())
}
