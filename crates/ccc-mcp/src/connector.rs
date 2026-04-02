use std::collections::HashMap;

use ccc_core::{
    McpConnectionSnapshot, McpConnectionStatus, McpSourceScope, McpTransportKind,
    config::McpServerConfig,
};
use http::{HeaderName, HeaderValue, Request};

use crate::client::McpClient;

pub struct McpConnectResult {
    pub snapshot: McpConnectionSnapshot,
    pub client: Option<McpClient>,
}

pub async fn connect_server(
    name: &str,
    source_scope: McpSourceScope,
    config: &McpServerConfig,
) -> McpConnectResult {
    match config {
        McpServerConfig::Stdio { command, args, env } => {
            connect_stdio(name, source_scope, command, args, env).await
        }
        McpServerConfig::Sse {
            url,
            headers,
            headers_helper: _,
        } => connect_http_like(name, source_scope, McpTransportKind::Sse, url, headers).await,
        McpServerConfig::Http {
            url,
            headers,
            headers_helper: _,
        } => connect_http_like(name, source_scope, McpTransportKind::Http, url, headers).await,
        McpServerConfig::Ws {
            url,
            headers,
            headers_helper: _,
        } => connect_ws(name, source_scope, url, headers).await,
        McpServerConfig::Sdk { name: sdk_name } => McpConnectResult {
            snapshot: snapshot(
                name,
                McpTransportKind::Sdk,
                McpConnectionStatus::Failed,
                source_scope,
                Some(format!(
                    "sdk transport `{sdk_name}` requires a host adapter and is not available in ccc-cli"
                )),
            ),
            client: None,
        },
        McpServerConfig::ClaudeAiProxy { url, .. } => {
            connect_http_like(name, source_scope, McpTransportKind::ClaudeAiProxy, url, &HashMap::new()).await
        }
    }
}

async fn connect_stdio(
    name: &str,
    source_scope: McpSourceScope,
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> McpConnectResult {
    match McpClient::spawn(command, args, env).await {
        Ok(mut client) => match client.initialize().await {
            Ok(()) => McpConnectResult {
                snapshot: snapshot(
                    name,
                    McpTransportKind::Stdio,
                    McpConnectionStatus::Connected,
                    source_scope,
                    None,
                ),
                client: Some(client),
            },
            Err(error) => McpConnectResult {
                snapshot: snapshot(
                    name,
                    McpTransportKind::Stdio,
                    McpConnectionStatus::Failed,
                    source_scope,
                    Some(error.to_string()),
                ),
                client: None,
            },
        },
        Err(error) => McpConnectResult {
            snapshot: snapshot(
                name,
                McpTransportKind::Stdio,
                McpConnectionStatus::Failed,
                source_scope,
                Some(error.to_string()),
            ),
            client: None,
        },
    }
}

async fn connect_http_like(
    name: &str,
    source_scope: McpSourceScope,
    transport: McpTransportKind,
    url: &str,
    headers: &HashMap<String, String>,
) -> McpConnectResult {
    let client = reqwest::Client::new();
    let mut request = client.get(url);
    for (key, value) in headers {
        request = request.header(key, value);
    }

    let result = match request.send().await {
        Ok(response) => {
            let (status, error) = classify_http_status(response.status());
            snapshot(name, transport, status, source_scope, error)
        }
        Err(error) => snapshot(
            name,
            transport,
            McpConnectionStatus::Failed,
            source_scope,
            Some(error.to_string()),
        ),
    };

    McpConnectResult {
        snapshot: result,
        client: None,
    }
}

fn classify_http_status(status: reqwest::StatusCode) -> (McpConnectionStatus, Option<String>) {
    if status.as_u16() == 401 || status.as_u16() == 403 {
        (
            McpConnectionStatus::NeedsAuth,
            Some(format!("transport requires authentication: {status}")),
        )
    } else if status.is_success() || status.is_redirection() {
        (McpConnectionStatus::Connected, None)
    } else {
        (
            McpConnectionStatus::Failed,
            Some(format!("transport probe returned {status}")),
        )
    }
}

async fn connect_ws(
    name: &str,
    source_scope: McpSourceScope,
    url: &str,
    headers: &HashMap<String, String>,
) -> McpConnectResult {
    let mut builder = Request::builder().method("GET").uri(url);
    for (key, value) in headers {
        let Ok(header_name) = HeaderName::try_from(key.as_str()) else {
            return McpConnectResult {
                snapshot: snapshot(
                    name,
                    McpTransportKind::Ws,
                    McpConnectionStatus::Failed,
                    source_scope,
                    Some(format!("invalid websocket header name: {key}")),
                ),
                client: None,
            };
        };
        let Ok(header_value) = HeaderValue::try_from(value.as_str()) else {
            return McpConnectResult {
                snapshot: snapshot(
                    name,
                    McpTransportKind::Ws,
                    McpConnectionStatus::Failed,
                    source_scope,
                    Some(format!("invalid websocket header value for {key}")),
                ),
                client: None,
            };
        };
        builder = builder.header(header_name, header_value);
    }

    let Ok(request) = builder.body(()) else {
        return McpConnectResult {
            snapshot: snapshot(
                name,
                McpTransportKind::Ws,
                McpConnectionStatus::Failed,
                source_scope,
                Some("failed to construct websocket request".into()),
            ),
            client: None,
        };
    };

    match tokio_tungstenite::connect_async(request).await {
        Ok((mut stream, _)) => {
            let _ = stream.close(None).await;
            McpConnectResult {
                snapshot: snapshot(
                    name,
                    McpTransportKind::Ws,
                    McpConnectionStatus::Connected,
                    source_scope,
                    None,
                ),
                client: None,
            }
        }
        Err(error) => {
            let error_text = error.to_string();
            let status = if error_text.contains("401") || error_text.contains("403") {
                McpConnectionStatus::NeedsAuth
            } else {
                McpConnectionStatus::Failed
            };
            McpConnectResult {
                snapshot: snapshot(
                    name,
                    McpTransportKind::Ws,
                    status,
                    source_scope,
                    Some(error_text),
                ),
                client: None,
            }
        }
    }
}

fn snapshot(
    name: &str,
    transport: McpTransportKind,
    status: McpConnectionStatus,
    source_scope: McpSourceScope,
    error: Option<String>,
) -> McpConnectionSnapshot {
    McpConnectionSnapshot {
        name: name.into(),
        transport,
        status,
        reconnect_attempt: None,
        max_reconnect_attempts: None,
        error,
        source_scope,
    }
}

#[cfg(test)]
mod tests {
    use ccc_core::McpSourceScope;

    use super::*;

    #[tokio::test]
    async fn sdk_transport_reports_failed_without_host_adapter() {
        let result = connect_server(
            "sdk-server",
            McpSourceScope::Plugin,
            &McpServerConfig::Sdk {
                name: "host-sdk".into(),
            },
        )
        .await;

        assert_eq!(result.snapshot.transport, McpTransportKind::Sdk);
        assert_eq!(result.snapshot.status, McpConnectionStatus::Failed);
        assert!(result.snapshot.error.unwrap().contains("host adapter"));
        assert!(result.client.is_none());
    }

    #[tokio::test]
    async fn http_401_is_mapped_to_needs_auth() {
        let (status, error) = classify_http_status(reqwest::StatusCode::UNAUTHORIZED);

        assert_eq!(status, McpConnectionStatus::NeedsAuth);
        assert_eq!(
            error.as_deref(),
            Some("transport requires authentication: 401 Unauthorized")
        );
    }
}
