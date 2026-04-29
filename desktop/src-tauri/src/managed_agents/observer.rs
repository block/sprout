pub struct ObserverEndpoint {
    pub addr: String,
    pub token: String,
    pub url: String,
}

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
