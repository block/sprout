//! In-process mesh-llm runtime adapter.
//!
//! Sprout uses this module to stand up the same localhost OpenAI surface that
//! `mesh-llm serve` would expose, without spawning a sidecar binary.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;

use mesh_llm_host_runtime::sdk::{
    EmbeddedMeshDiscoveryMode, EmbeddedMeshNodeConfig, EmbeddedMeshNodeHandle,
    EmbeddedMeshNodeMode, start_embedded_node,
};
use serde::{Deserialize, Serialize};

use super::ComputeSharingPrefs;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMeshNodeRequest {
    pub mode: Option<String>,
    pub model: Option<String>,
    pub join_token: Option<String>,
    pub auto: Option<bool>,
    pub discovery_mode: Option<String>,
    pub api_port: Option<u16>,
    pub console_port: Option<u16>,
    pub mesh_name: Option<String>,
    pub region: Option<String>,
    pub node_name: Option<String>,
    pub max_vram_gb: Option<f64>,
    pub publish: Option<bool>,
    pub relay_urls: Option<Vec<String>>,
    pub relay_auth: Option<BTreeMap<String, String>>,
    pub nostr_relays: Option<Vec<String>>,
    pub bind_ip: Option<String>,
    pub bind_port: Option<u16>,
    pub listen_all: Option<bool>,
    pub enumerate_host: Option<bool>,
    pub console_ui: Option<bool>,
    pub startup_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshNodeStatus {
    pub running: bool,
    pub api_base_url: Option<String>,
    pub console_url: Option<String>,
    pub invite_token: Option<String>,
    pub status: Option<serde_json::Value>,
}

pub struct SproutMeshRuntime {
    handle: EmbeddedMeshNodeHandle,
}

impl SproutMeshRuntime {
    pub async fn start(
        request: StartMeshNodeRequest,
        prefs: &ComputeSharingPrefs,
    ) -> anyhow::Result<Self> {
        let handle = start_embedded_node(build_config(request, prefs)?).await?;
        Ok(Self { handle })
    }

    pub async fn status(&self) -> anyhow::Result<MeshNodeStatus> {
        let status = self.handle.status().await?;
        Ok(MeshNodeStatus {
            running: true,
            api_base_url: Some(status.api_base_url),
            console_url: Some(status.console_url),
            invite_token: status.invite_token,
            status: Some(status.payload),
        })
    }

    pub async fn stop(self) -> anyhow::Result<()> {
        self.handle.stop().await
    }
}

pub fn stopped_status() -> MeshNodeStatus {
    MeshNodeStatus {
        running: false,
        api_base_url: None,
        console_url: None,
        invite_token: None,
        status: None,
    }
}

fn build_config(
    request: StartMeshNodeRequest,
    prefs: &ComputeSharingPrefs,
) -> anyhow::Result<EmbeddedMeshNodeConfig> {
    let mode = match request.mode.as_deref().unwrap_or("serve") {
        "serve" => EmbeddedMeshNodeMode::Serve,
        "client" => EmbeddedMeshNodeMode::Client,
        other => anyhow::bail!("unsupported mesh-llm mode: {other}"),
    };

    let mut models = Vec::new();
    if let Some(model) = request.model.filter(|model| !model.trim().is_empty()) {
        models.push(model);
    } else if mode == EmbeddedMeshNodeMode::Serve {
        models.extend(prefs.models.iter().map(|model| model.id.clone()));
    }

    let mut join = Vec::new();
    if let Some(token) = request.join_token.filter(|token| !token.trim().is_empty()) {
        join.push(token);
    }

    let mut builder = EmbeddedMeshNodeConfig::builder()
        .mode(mode)
        .models(models)
        .join_tokens(join)
        .auto_join(request.auto.unwrap_or(false))
        .api_port(request.api_port.unwrap_or(9337))
        .console_port(request.console_port.unwrap_or(3131))
        .publish(request.publish.unwrap_or(false))
        .isolated_config(true)
        .startup_timeout(Duration::from_millis(
            request.startup_timeout_ms.unwrap_or(30_000),
        ))
        .console_ui(request.console_ui.unwrap_or(false))
        .listen_all(request.listen_all.unwrap_or(false))
        .enumerate_host(request.enumerate_host.unwrap_or(true));

    if let Some(mode) = parse_discovery_mode(request.discovery_mode.as_deref())? {
        builder = builder.discovery_mode(mode);
    }
    if let Some(mesh_name) = request.mesh_name.or_else(|| Some("sprout".to_string())) {
        builder = builder.mesh_name(mesh_name);
    }
    if let Some(region) = request.region.filter(|value| !value.trim().is_empty()) {
        builder = builder.region(region);
    }
    if let Some(node_name) = request.node_name.filter(|value| !value.trim().is_empty()) {
        builder = builder.node_name(node_name);
    }
    if let Some(max_vram_gb) = request.max_vram_gb.or_else(|| {
        prefs
            .caps
            .max_vram_mb
            .map(|max_vram_mb| max_vram_mb as f64 / 1024.0)
    }) {
        builder = builder.max_vram_gb(max_vram_gb);
    }
    for relay in request.relay_urls.unwrap_or_default() {
        if !relay.trim().is_empty() {
            builder = builder.iroh_relay(relay);
        }
    }
    for (relay, token) in request.relay_auth.unwrap_or_default() {
        if !relay.trim().is_empty() && !token.trim().is_empty() {
            builder = builder.iroh_relay_auth(relay, token);
        }
    }
    for relay in request.nostr_relays.unwrap_or_default() {
        if !relay.trim().is_empty() {
            builder = builder.nostr_relay(relay);
        }
    }
    if let Some(ip) = request.bind_ip.filter(|value| !value.trim().is_empty()) {
        builder = builder.bind_ip(ip.parse::<IpAddr>()?);
    }
    if let Some(port) = request.bind_port {
        builder = builder.bind_port(port);
    }

    Ok(builder.build())
}

fn parse_discovery_mode(value: Option<&str>) -> anyhow::Result<Option<EmbeddedMeshDiscoveryMode>> {
    match value {
        None => Ok(None),
        Some("nostr") => Ok(Some(EmbeddedMeshDiscoveryMode::Nostr)),
        Some("mdns") => Ok(Some(EmbeddedMeshDiscoveryMode::Mdns)),
        Some(other) => anyhow::bail!("unsupported mesh discovery mode: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::time::Duration;

    use sprout_core::mesh_llm::{ModelOffer, ResourceCaps};

    #[test]
    fn serve_config_uses_request_model_as_model_ref() {
        let prefs = ComputeSharingPrefs::default();
        let config = build_config(
            StartMeshNodeRequest {
                mode: None,
                model: Some("unsloth/Qwen3-4B-128K-GGUF:Q4_K_M".to_string()),
                join_token: Some("join-token".to_string()),
                auto: Some(true),
                api_port: Some(19337),
                console_port: Some(13131),
                mesh_name: None,
                max_vram_gb: Some(3.0),
                publish: None,
                startup_timeout_ms: Some(1_000),
                ..start_request()
            },
            &prefs,
        )
        .expect("config");

        assert_eq!(config.mode, EmbeddedMeshNodeMode::Serve);
        assert_eq!(
            config.serving.models,
            vec!["unsloth/Qwen3-4B-128K-GGUF:Q4_K_M".to_string()]
        );
        assert_eq!(config.network.join_tokens, vec!["join-token".to_string()]);
        assert!(config.network.auto_join);
        assert_eq!(config.network.mesh_name.as_deref(), Some("sprout"));
        assert_eq!(config.serving.max_vram_gb, Some(3.0));
    }

    #[test]
    fn serve_config_can_fall_back_to_prefs_models_and_caps() {
        let prefs = ComputeSharingPrefs {
            enabled: true,
            caps: ResourceCaps {
                max_vram_mb: Some(3072),
                max_ram_mb: None,
                max_concurrency: Some(1),
            },
            models: vec![ModelOffer {
                id: "meshllm/Qwen3-8B-Q4_K_M-layers".to_string(),
                label: None,
                context_tokens: None,
            }],
            d_tag: "default".to_string(),
        };

        let config = build_config(
            StartMeshNodeRequest {
                mode: Some("serve".to_string()),
                model: None,
                join_token: None,
                auto: None,
                api_port: None,
                console_port: None,
                mesh_name: Some("local-sprout".to_string()),
                max_vram_gb: None,
                publish: Some(true),
                startup_timeout_ms: None,
                ..start_request()
            },
            &prefs,
        )
        .expect("config");

        assert_eq!(
            config.serving.models,
            vec!["meshllm/Qwen3-8B-Q4_K_M-layers".to_string()]
        );
        assert_eq!(config.serving.max_vram_gb, Some(3.0));
        assert_eq!(config.network.mesh_name.as_deref(), Some("local-sprout"));
        assert!(config.network.publish);
    }

    #[test]
    fn config_accepts_relay_and_discovery_options() {
        let mut relay_auth = BTreeMap::new();
        relay_auth.insert("https://relay.example".to_string(), "secret".to_string());

        let config = build_config(
            StartMeshNodeRequest {
                mode: Some("client".to_string()),
                auto: Some(true),
                discovery_mode: Some("mdns".to_string()),
                relay_urls: Some(vec!["https://relay.example".to_string()]),
                relay_auth: Some(relay_auth),
                nostr_relays: Some(vec!["wss://nostr.example".to_string()]),
                bind_ip: Some("127.0.0.1".to_string()),
                bind_port: Some(17777),
                listen_all: Some(true),
                enumerate_host: Some(false),
                console_ui: Some(true),
                region: Some("AU".to_string()),
                node_name: Some("sprout-node".to_string()),
                ..start_request()
            },
            &ComputeSharingPrefs::default(),
        )
        .expect("config");

        assert_eq!(config.mode, EmbeddedMeshNodeMode::Client);
        assert_eq!(
            config.network.discovery_mode,
            EmbeddedMeshDiscoveryMode::Mdns
        );
        assert_eq!(
            config.network.iroh_relays,
            vec!["https://relay.example".to_string()]
        );
        assert_eq!(
            config.network.iroh_relay_auth.get("https://relay.example"),
            Some(&"secret".to_string())
        );
        assert_eq!(
            config.network.nostr_relays,
            vec!["wss://nostr.example".to_string()]
        );
        assert_eq!(config.network.bind_ip, Some("127.0.0.1".parse().unwrap()));
        assert_eq!(config.network.bind_port, Some(17777));
        assert!(config.network.listen_all);
        assert!(!config.network.enumerate_host);
        assert!(config.http.console_ui);
        assert_eq!(config.network.region.as_deref(), Some("AU"));
        assert_eq!(config.network.node_name.as_deref(), Some("sprout-node"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "opens localhost mesh runtime sockets"]
    async fn client_runtime_start_stop_exposes_local_status() {
        let runtime = SproutMeshRuntime::start(
            StartMeshNodeRequest {
                mode: Some("client".to_string()),
                model: None,
                join_token: None,
                auto: None,
                api_port: Some(unused_loopback_port()),
                console_port: Some(unused_loopback_port()),
                mesh_name: Some("sprout-sdk-smoke".to_string()),
                max_vram_gb: None,
                publish: Some(false),
                startup_timeout_ms: Some(45_000),
                ..start_request()
            },
            &ComputeSharingPrefs::default(),
        )
        .await
        .expect("start embedded mesh runtime");

        let status = runtime.status().await.expect("status");
        assert!(status.running);
        assert!(status.api_base_url.as_deref().is_some_and(|url| {
            url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:")
        }));
        assert!(status.console_url.as_deref().is_some_and(|url| {
            url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:")
        }));

        runtime.stop().await.expect("stop embedded mesh runtime");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "joins the public mesh and sends a real inference request"]
    async fn client_auto_runtime_can_infer_through_public_mesh() {
        let runtime = SproutMeshRuntime::start(
            StartMeshNodeRequest {
                mode: Some("client".to_string()),
                model: None,
                join_token: None,
                auto: Some(true),
                api_port: Some(unused_loopback_port()),
                console_port: Some(unused_loopback_port()),
                mesh_name: None,
                max_vram_gb: None,
                publish: Some(false),
                startup_timeout_ms: Some(90_000),
                ..start_request()
            },
            &ComputeSharingPrefs::default(),
        )
        .await
        .expect("start public auto mesh client");

        let result = chat_completion(
            &runtime,
            "mesh",
            "Reply with exactly: OK",
            Duration::from_secs(120),
        )
        .await;
        let _ = runtime.stop().await;

        let content = result.expect("public mesh inference response");
        assert!(!content.trim().is_empty(), "empty mesh response");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "loads a local/cached small GGUF through mesh and sends a real inference request"]
    async fn serve_small_cached_model_can_infer_locally() {
        let model = "unsloth/Qwen3-0.6B-GGUF:Q4_K_M";
        let runtime = SproutMeshRuntime::start(
            StartMeshNodeRequest {
                mode: Some("serve".to_string()),
                model: Some(model.to_string()),
                join_token: None,
                auto: None,
                api_port: Some(unused_loopback_port()),
                console_port: Some(unused_loopback_port()),
                mesh_name: Some("sprout-local-smoke".to_string()),
                max_vram_gb: Some(6.0),
                publish: Some(false),
                startup_timeout_ms: Some(180_000),
                ..start_request()
            },
            &ComputeSharingPrefs::default(),
        )
        .await
        .expect("start local model mesh runtime");

        let result = chat_completion(
            &runtime,
            model,
            "Reply with exactly: OK",
            Duration::from_secs(120),
        )
        .await;
        let _ = runtime.stop().await;

        let content = result.expect("local model inference response");
        assert!(!content.trim().is_empty(), "empty local model response");
    }

    async fn chat_completion(
        runtime: &SproutMeshRuntime,
        model: &str,
        prompt: &str,
        timeout: Duration,
    ) -> anyhow::Result<String> {
        let status = runtime.status().await?;
        let api_base_url = status
            .api_base_url
            .ok_or_else(|| anyhow::anyhow!("missing API base URL"))?;
        let response = reqwest::Client::builder()
            .timeout(timeout)
            .build()?
            .post(format!("{api_base_url}/chat/completions"))
            .json(&serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 16,
                "stream": false,
            }))
            .send()
            .await?
            .error_for_status()?;
        let payload = response.json::<serde_json::Value>().await?;
        payload["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("missing completion content: {payload}"))
    }

    fn unused_loopback_port() -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral loopback port");
        listener
            .local_addr()
            .expect("loopback socket address")
            .port()
    }

    fn start_request() -> StartMeshNodeRequest {
        StartMeshNodeRequest {
            mode: None,
            model: None,
            join_token: None,
            auto: None,
            discovery_mode: None,
            api_port: None,
            console_port: None,
            mesh_name: None,
            region: None,
            node_name: None,
            max_vram_gb: None,
            publish: None,
            relay_urls: None,
            relay_auth: None,
            nostr_relays: None,
            bind_ip: None,
            bind_port: None,
            listen_all: None,
            enumerate_host: None,
            console_ui: None,
            startup_timeout_ms: None,
        }
    }
}
