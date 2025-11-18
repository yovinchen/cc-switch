//! 深链接导入模型
//!
//! 该模块实现了 `docs/ccswitch-deeplink-design.md` 3.1 节描述的 `ccswitch://`
//! 协议解析逻辑，用于统一 Provider 及完整配置导入的请求结构。

use crate::error::AppError;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

/// `ccswitch://` 深链接请求的统一表示
///
/// * `Provider` 变体覆盖 v1.0 已实现的供应商导入字段
/// * `Config` 变体覆盖 v1.1 设计中的完整配置导入规范
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum DeepLinkRequest {
    /// Provider 字段导入（resource=provider）
    Provider(ProviderImportRequest),
    /// 完整配置导入（resource=config）
    Config(ConfigImportRequest),
}

impl DeepLinkRequest {
    /// 从 `ccswitch://` URL 解析 `DeepLinkRequest`
    ///
    /// # 规范
    /// - 协议：`ccswitch://`
    /// - Host：协议版本（当前仅支持 v1）
    /// - Path：必须为 `/import`
    /// - Query：依据 `resource` 参数拆分 `provider` 与 `config`
    pub fn from_url(url: &str) -> Result<Self, AppError> {
        let url = Url::parse(url)
            .map_err(|e| AppError::InvalidInput(format!("Invalid deep link URL: {e}")))?;
        Self::require_scheme(&url)?;
        let version = Self::extract_version(&url)?;
        Self::require_import_path(&url)?;

        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let resource = params
            .get("resource")
            .ok_or_else(|| AppError::InvalidInput("Missing 'resource' parameter".to_string()))?
            .to_lowercase();

        match resource.as_str() {
            "provider" => {
                let request = ProviderImportRequest::from_query(&version, &params)?;
                Ok(DeepLinkRequest::Provider(request))
            }
            "config" => {
                let request = ConfigImportRequest::from_query(&params)?;
                request.validate()?;
                Ok(DeepLinkRequest::Config(request))
            }
            other => Err(AppError::InvalidInput(format!(
                "Unsupported resource type: {other}"
            ))),
        }
    }

    fn require_scheme(url: &Url) -> Result<(), AppError> {
        if url.scheme() == "ccswitch" {
            return Ok(());
        }
        Err(AppError::InvalidInput(format!(
            "Invalid scheme: expected 'ccswitch', got '{}'",
            url.scheme()
        )))
    }

    fn extract_version(url: &Url) -> Result<String, AppError> {
        let version = url
            .host_str()
            .ok_or_else(|| AppError::InvalidInput("Missing version in URL host".to_string()))?;
        if version != "v1" {
            return Err(AppError::InvalidInput(format!(
                "Unsupported protocol version: {version}"
            )));
        }
        Ok(version.to_string())
    }

    fn require_import_path(url: &Url) -> Result<(), AppError> {
        if url.path() == "/import" {
            return Ok(());
        }
        Err(AppError::InvalidInput(format!(
            "Invalid path: expected '/import', got '{}'",
            url.path()
        )))
    }
}

/// Provider 导入请求
///
/// 结构与 `src-tauri/src/deeplink.rs` 中的 `DeepLinkImportRequest` 保持一致，
/// 以兼容既有供应商导入逻辑。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderImportRequest {
    /// 协议版本（例如 v1）
    pub version: String,
    /// 资源类型，固定为 `provider`
    pub resource: String,
    /// 目标应用：claude/codex/gemini
    pub app: String,
    /// Provider 名称
    pub name: String,
    /// 供应商主页 URL
    pub homepage: String,
    /// API Endpoint/Base URL
    pub endpoint: String,
    /// 用于认证的 API Key
    pub api_key: String,
    /// 默认模型，可选
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 备注，可选
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl ProviderImportRequest {
    fn from_query(version: &str, params: &HashMap<String, String>) -> Result<Self, AppError> {
        let resource = params
            .get("resource")
            .cloned()
            .unwrap_or_else(|| "provider".to_string());

        let app = params
            .get("app")
            .ok_or_else(|| AppError::InvalidInput("Missing 'app' parameter".to_string()))?
            .to_lowercase();
        validate_app(&app)?;

        let name = params
            .get("name")
            .ok_or_else(|| AppError::InvalidInput("Missing 'name' parameter".to_string()))?
            .to_string();
        if name.trim().is_empty() {
            return Err(AppError::InvalidInput("Invalid provider name".to_string()));
        }

        let homepage = params
            .get("homepage")
            .ok_or_else(|| AppError::InvalidInput("Missing 'homepage' parameter".to_string()))?
            .to_string();
        validate_http_url(&homepage, "homepage")?;

        let endpoint = params
            .get("endpoint")
            .ok_or_else(|| AppError::InvalidInput("Missing 'endpoint' parameter".to_string()))?
            .to_string();
        validate_http_url(&endpoint, "endpoint")?;

        let api_key = params
            .get("apiKey")
            .ok_or_else(|| AppError::InvalidInput("Missing 'apiKey' parameter".to_string()))?
            .to_string();

        Ok(Self {
            version: version.to_string(),
            resource,
            app,
            name,
            homepage,
            endpoint,
            api_key,
            model: params.get("model").cloned(),
            notes: params.get("notes").cloned(),
        })
    }
}

/// 完整配置导入请求
///
/// 仅保留配置导入所需字段，格式约束见设计文档 3.1.2。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigImportRequest {
    /// 目标应用：claude/codex/gemini
    pub app: String,
    /// Base64 编码后的配置内容
    pub data: String,
    /// 配置格式（json/toml，可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

impl ConfigImportRequest {
    fn from_query(params: &HashMap<String, String>) -> Result<Self, AppError> {
        let app = params
            .get("app")
            .ok_or_else(|| AppError::InvalidInput("Missing 'app' parameter".to_string()))?
            .to_lowercase();

        let data = params
            .get("data")
            .ok_or_else(|| AppError::InvalidInput("Missing 'data' parameter".to_string()))?
            .to_string();

        let format = params.get("format").map(|value| value.to_lowercase());

        Ok(Self { app, data, format })
    }

    /// 校验 `app`、`data` 及可选 `format` 字段的有效性
    ///
    /// - `app` 必须为 `claude`/`codex`/`gemini`
    /// - `data` 必须可被 Base64 解码，并能转换为 UTF-8 字符串
    /// - `format` 存在时必须为 `json` 或 `toml`
    pub fn validate(&self) -> Result<(), AppError> {
        validate_app(&self.app)?;
        if self.data.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Config payload cannot be empty".to_string(),
            ));
        }

        let decoded = BASE64
            .decode(self.data.as_bytes())
            .map_err(|e| AppError::InvalidInput(format!("Invalid base64 payload: {e}")))?;
        String::from_utf8(decoded).map_err(|e| {
            AppError::InvalidInput(format!("Config payload must be valid UTF-8: {e}"))
        })?;

        if let Some(format) = self.format.as_deref() {
            match format {
                "json" | "toml" => {}
                other => {
                    return Err(AppError::InvalidInput(format!(
                        "Unsupported config format: {other}"
                    )))
                }
            }
        }

        Ok(())
    }
}

fn validate_http_url(value: &str, field: &str) -> Result<(), AppError> {
    let url = Url::parse(value)
        .map_err(|e| AppError::InvalidInput(format!("Invalid URL for '{field}': {e}")))?;
    match url.scheme() {
        "http" | "https" => Ok(()),
        other => Err(AppError::InvalidInput(format!(
            "Invalid URL scheme for '{field}': must be http or https, got '{other}'"
        ))),
    }
}

fn validate_app(app: &str) -> Result<(), AppError> {
    match app {
        "claude" | "codex" | "gemini" => Ok(()),
        other => Err(AppError::InvalidInput(format!("Invalid app type: {other}"))),
    }
}
