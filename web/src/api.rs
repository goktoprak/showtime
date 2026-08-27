//! Thin wrapper over the JSON API, replacing the `api()` helper that the
//! previous plain-JS frontend kept in app.js.
//!
//! Error handling mirrors what that helper did: on a non-2xx response, use
//! the `error` field from the JSON body if there is one, and otherwise fall
//! back to a generic message carrying the status code.

use gloo_net::http::{Request, Response};
use serde::{de::DeserializeOwned, Serialize};

/// A failed request, already reduced to something showable to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError(pub String);

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<gloo_net::Error> for ApiError {
    fn from(e: gloo_net::Error) -> Self {
        ApiError(e.to_string())
    }
}

fn url(path: &str) -> String {
    format!("/api{path}")
}

/// Turns a non-2xx response into an `ApiError`, preferring the server's own
/// message. The body is read as text first so that a non-JSON error (an
/// unexpected HTML error page, say) degrades to the generic message instead
/// of a confusing deserialization failure.
async fn error_from(resp: Response) -> ApiError {
    let status = resp.status();
    let fallback = format!("Request failed ({status})");
    match resp.text().await {
        Ok(body) => match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(v) => match v.get("error").and_then(|e| e.as_str()) {
                Some(msg) => ApiError(msg.to_string()),
                None => ApiError(fallback),
            },
            Err(_) => ApiError(fallback),
        },
        Err(_) => ApiError(fallback),
    }
}

async fn read<T: DeserializeOwned>(resp: Response) -> Result<T, ApiError> {
    if !resp.ok() {
        return Err(error_from(resp).await);
    }
    resp.json::<T>().await.map_err(Into::into)
}

pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    read(Request::get(&url(path)).send().await?).await
}

/// POST with no request body, for the endpoints that act on a path id alone.
pub async fn post<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    read(Request::post(&url(path)).send().await?).await
}

pub async fn post_json<B: Serialize, T: DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<T, ApiError> {
    read(Request::post(&url(path)).json(body)?.send().await?).await
}

/// DELETE, discarding the body. The delete-show endpoint answers 204, which
/// has no body to parse at all.
pub async fn delete(path: &str) -> Result<(), ApiError> {
    let resp = Request::delete(&url(path)).send().await?;
    if resp.ok() {
        Ok(())
    } else {
        Err(error_from(resp).await)
    }
}
