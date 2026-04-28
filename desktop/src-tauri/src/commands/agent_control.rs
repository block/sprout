use serde::{Deserialize, Serialize};
use tauri::State;

use crate::{app_state::AppState, relay::relay_error_message};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelManagedAgentTurnResponse {
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObserverCancelTurnResponse {
    status: String,
}

#[tauri::command]
pub async fn cancel_managed_agent_turn(
    pubkey: String,
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<CancelManagedAgentTurnResponse, String> {
    let observer_url = {
        let runtimes = state
            .managed_agent_processes
            .lock()
            .map_err(|error| error.to_string())?;
        let runtime = runtimes
            .get(&pubkey)
            .ok_or_else(|| format!("agent {pubkey} is not running locally"))?;
        runtime
            .observer_url
            .clone()
            .ok_or_else(|| format!("agent {pubkey} does not expose an observer control endpoint"))?
    };

    let (control_url, token) = observer_control_url(&observer_url)?;
    let request = state
        .http_client
        .post(control_url)
        .bearer_auth(token)
        .json(&serde_json::json!({ "channelId": channel_id }));

    let response = request
        .send()
        .await
        .map_err(|error| format!("observer cancel request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    let body = response
        .json::<ObserverCancelTurnResponse>()
        .await
        .map_err(|error| format!("observer cancel response parse failed: {error}"))?;

    Ok(CancelManagedAgentTurnResponse {
        status: body.status,
    })
}

fn observer_control_url(observer_url: &str) -> Result<(String, String), String> {
    let mut url = url::Url::parse(observer_url)
        .map_err(|error| format!("invalid observer URL for agent: {error}"))?;
    let token = url
        .query_pairs()
        .find_map(|(key, value)| (key == "token").then(|| value.into_owned()))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "observer URL is missing its control token".to_string())?;

    url.set_path("/control/cancel");
    url.set_query(None);

    Ok((url.to_string(), token))
}

#[cfg(test)]
mod tests {
    use super::observer_control_url;

    #[test]
    fn derives_control_url_and_token_from_events_url() {
        let (url, token) =
            observer_control_url("http://127.0.0.1:1234/events?token=abc").expect("control url");
        assert_eq!(url, "http://127.0.0.1:1234/control/cancel");
        assert_eq!(token, "abc");
    }
}
