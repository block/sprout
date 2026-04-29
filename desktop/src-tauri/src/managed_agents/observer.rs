/// Loopback observer endpoint assigned to one local ACP harness process.
pub struct ObserverEndpoint {
    /// Host:port bind address passed to `sprout-acp`.
    pub addr: String,
    /// Token required by observer requests.
    pub token: String,
    /// Event-stream URL including the observer token.
    pub url: String,
}

/// Allocate an unused loopback port and random observer token.
pub fn allocate_observer_endpoint() -> Result<ObserverEndpoint, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("failed to allocate observer port: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect observer port: {error}"))?
        .port();
    drop(listener);

    let token = uuid::Uuid::new_v4().to_string();
    let addr = format!("127.0.0.1:{port}");
    let url = format!("http://127.0.0.1:{port}/events?token={token}");
    Ok(ObserverEndpoint { addr, token, url })
}
