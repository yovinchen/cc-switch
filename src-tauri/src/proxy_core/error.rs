use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxyCoreError {
    #[error("invalid proxy request: {0}")]
    InvalidRequest(String),
    #[error("proxy configuration error: {0}")]
    Config(String),
    #[error("proxy authentication error: {0}")]
    Auth(String),
    #[error("upstream proxy error: {0}")]
    Upstream(String),
    #[error("proxy route unavailable: {0}")]
    Unavailable(String),
    #[error("proxy core feature is not supported yet: {0}")]
    Unsupported(String),
    #[error("proxy core internal error: {0}")]
    Internal(String),
}

pub type ProxyCoreResult<T> = Result<T, ProxyCoreError>;
