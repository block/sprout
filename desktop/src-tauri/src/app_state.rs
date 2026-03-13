use std::{collections::HashMap, sync::Mutex};

use nostr::Keys;

use crate::managed_agents::ManagedAgentProcess;

pub struct AppState {
    pub keys: Mutex<Keys>,
    pub http_client: reqwest::Client,
    pub configured_api_token: Option<String>,
    pub session_token: Mutex<Option<String>>,
    pub managed_agents_store_lock: Mutex<()>,
    pub managed_agent_processes: Mutex<HashMap<String, ManagedAgentProcess>>,
}

pub fn build_app_state() -> AppState {
    // GUI app: warn on bad key but don't crash, and fall back to ephemeral.
    let (keys, source) = match std::env::var("SPROUT_PRIVATE_KEY") {
        Ok(nsec) => match Keys::parse(nsec.trim()) {
            Ok(keys) => (keys, "configured"),
            Err(error) => {
                eprintln!("sprout-desktop: invalid SPROUT_PRIVATE_KEY: {error}");
                (Keys::generate(), "ephemeral")
            }
        },
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!("sprout-desktop: SPROUT_PRIVATE_KEY contains invalid UTF-8");
            (Keys::generate(), "ephemeral")
        }
        Err(std::env::VarError::NotPresent) => (Keys::generate(), "ephemeral"),
    };

    eprintln!(
        "sprout-desktop: {source} identity pubkey {}",
        keys.public_key().to_hex()
    );

    let api_token = match std::env::var("SPROUT_API_TOKEN") {
        Ok(token) if !token.trim().is_empty() => Some(token),
        Ok(_) | Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!("sprout-desktop: SPROUT_API_TOKEN contains invalid UTF-8");
            None
        }
    };

    AppState {
        keys: Mutex::new(keys),
        http_client: reqwest::Client::new(),
        configured_api_token: api_token,
        session_token: Mutex::new(None),
        managed_agents_store_lock: Mutex::new(()),
        managed_agent_processes: Mutex::new(HashMap::new()),
    }
}
