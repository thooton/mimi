// The private credential-service client. Passwords cross this boundary once,
// on registration or login; browser sessions belong to mimi_backend and are
// never sent to mimi_auth.

use std::fmt;
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::header::CONTENT_TYPE;
use hyper::{Method, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Deserialize, Serialize)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}

// The two credential edits. Both carry a `login` and the current password
// because mimi_auth has no sessions of its own: the password is what stands
// in for one, and the caller is expected to have just asked for it. The
// `login` is never the one the browser typed but the username the backend
// session names, so a request can only ever edit its own account.
#[derive(Deserialize, Serialize)]
pub struct ChangePasswordRequest {
    pub login: String,
    pub current_password: String,
    pub new_password: String,
}

#[derive(Deserialize, Serialize)]
pub struct ChangeEmailRequest {
    pub login: String,
    pub password: String,
    pub new_email: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct AuthUser {
    pub username: String,
    pub email: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
}

pub enum CredentialError {
    Rejected(StatusCode, String),
    Unavailable,
}

impl fmt::Debug for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(status, message) => f
                .debug_tuple("Rejected")
                .field(status)
                .field(message)
                .finish(),
            Self::Unavailable => f.write_str("Unavailable"),
        }
    }
}

pub struct CredentialService {
    base_url: String,
    client: Client<HttpConnector, Full<Bytes>>,
}

impl CredentialService {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::builder(TokioExecutor::new()).build_http(),
        }
    }

    pub async fn register(&self, request: &RegisterRequest) -> Result<AuthUser, CredentialError> {
        self.post("/v1/register", request).await
    }

    pub async fn login(&self, request: &LoginRequest) -> Result<AuthUser, CredentialError> {
        self.post("/v1/login", request).await
    }

    pub async fn change_password(
        &self,
        request: &ChangePasswordRequest,
    ) -> Result<AuthUser, CredentialError> {
        self.post("/v1/password", request).await
    }

    pub async fn change_email(
        &self,
        request: &ChangeEmailRequest,
    ) -> Result<AuthUser, CredentialError> {
        self.post("/v1/email", request).await
    }

    async fn post<T: Serialize>(&self, path: &str, value: &T) -> Result<AuthUser, CredentialError> {
        let uri = format!("{}{path}", self.base_url)
            .parse::<hyper::Uri>()
            .map_err(|_| CredentialError::Unavailable)?;
        let body = serde_json::to_vec(value).map_err(|_| CredentialError::Unavailable)?;
        let request = hyper::Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json")
            .body(Full::new(Bytes::from(body)))
            .map_err(|_| CredentialError::Unavailable)?;
        let response = tokio::time::timeout(Duration::from_secs(10), self.client.request(request))
            .await
            .map_err(|_| CredentialError::Unavailable)?
            .map_err(|_| CredentialError::Unavailable)?;
        let status = response.status();
        let body = response
            .into_body()
            .collect()
            .await
            .map_err(|_| CredentialError::Unavailable)?
            .to_bytes();
        if status.is_success() {
            return serde_json::from_slice(&body).map_err(|_| CredentialError::Unavailable);
        }
        if status.is_client_error() {
            let message = serde_json::from_slice::<ErrorResponse>(&body)
                .map(|body| body.error)
                .unwrap_or_else(|_| "authentication request was rejected".to_string());
            return Err(CredentialError::Rejected(status, message));
        }
        Err(CredentialError::Unavailable)
    }
}
