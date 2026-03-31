//! Temporary localhost HTTP server that captures the OAuth authorization-code redirect.
//! Corresponds to TS `src/services/oauth/auth-code-listener.ts`.

use anyhow::{bail, Context, Result};
use std::net::SocketAddr;
use tokio::{
    net::TcpListener,
    sync::oneshot,
};

pub struct AuthCodeListener {
    listener: TcpListener,
}

impl AuthCodeListener {
    /// Bind to an OS-assigned port and return the listener.
    pub async fn bind() -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind OAuth callback listener")?;
        Ok(Self { listener })
    }

    /// Port the OS assigned.
    pub fn port(&self) -> u16 {
        self.listener.local_addr().unwrap().port()
    }

    /// Wait for exactly one `/callback?code=…&state=…` request.
    /// Validates `state`, sends a plain-text acknowledgement to the browser,
    /// and returns the authorization code.
    pub async fn wait_for_code(self, expected_state: &str) -> Result<String> {
        use axum::{
            extract::Query,
            http::StatusCode,
            response::IntoResponse,
            routing::get,
            Router,
        };
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            code: Option<String>,
            state: Option<String>,
            error: Option<String>,
        }

        // Channel: handler → wait_for_code
        let (tx, rx) = oneshot::channel::<Result<String, String>>();
        let tx = std::sync::Arc::new(tokio::sync::Mutex::new(Some(tx)));
        let expected_state = expected_state.to_owned();

        let tx_clone = tx.clone();
        let app = Router::new().route(
            "/callback",
            get(move |Query(params): Query<Params>| {
                let tx = tx_clone.clone();
                let expected = expected_state.clone();
                async move {
                    let result = (|| {
                        if let Some(err) = params.error {
                            return Err(format!("OAuth error: {err}"));
                        }
                        let state = params.state.unwrap_or_default();
                        if state != expected {
                            return Err("Invalid state parameter".into());
                        }
                        params.code.ok_or_else(|| "Missing code parameter".to_string())
                    })();

                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(result);
                    }

                    (
                        StatusCode::OK,
                        "Authorization successful. You can close this window.",
                    )
                        .into_response()
                }
            }),
        );

        let server = axum::serve(
            self.listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        );

        // Run server until the code arrives, then shut down.
        tokio::select! {
            res = server.with_graceful_shutdown(async {
                // We'll abort once the channel fires below — just block forever here.
                std::future::pending::<()>().await
            }) => {
                res.context("OAuth callback server error")?;
            }
            code_result = rx => {
                // Server is dropped / GC-ed; return the result.
                return code_result
                    .context("callback channel dropped")?
                    .map_err(|e| anyhow::anyhow!(e));
            }
        }

        bail!("OAuth callback server exited unexpectedly")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn binds_and_reports_port() {
        let l = AuthCodeListener::bind().await.unwrap();
        assert!(l.port() > 0);
    }
}
