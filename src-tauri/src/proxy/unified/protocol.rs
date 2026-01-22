//! 协议配置模块
//!
//! 定义支持的协议类型和转换配置

use serde::{Deserialize, Serialize};

/// 支持的协议/格式类型
///
/// 参考 Rig 项目支持的提供商
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolFormat {
    /// Anthropic Claude API 格式
    /// - Messages API: /v1/messages
    /// - 支持: text, image, tool_use, tool_result, thinking
    Anthropic,

    /// OpenAI Chat Completions API 格式
    /// - Endpoint: /v1/chat/completions
    /// - 支持: system, user, assistant, tool messages
    /// - 不支持 Reasoning/Thinking
    #[serde(alias = "openai")]
    OpenAIChat,

    /// OpenAI Responses API 格式
    /// - Endpoint: /v1/responses
    /// - 支持: input, output, reasoning, function_call
    /// - 支持 Reasoning/Thinking
    OpenAIResponses,

    /// Google Gemini API 格式
    /// - Endpoint: /v1beta/models/{model}:generateContent
    /// - 支持: text, inlineData, functionCall, functionResponse, thought
    Gemini,

    /// Cohere API 格式
    /// - Endpoint: /v1/chat
    /// - 支持: USER, CHATBOT, SYSTEM, TOOL_RESULT roles
    Cohere,

    /// DeepSeek API 格式
    /// - 兼容 OpenAI 格式
    /// - 额外支持: reasoning_content (思维链)
    DeepSeek,

    /// Mistral API 格式
    /// - 兼容 OpenAI 格式
    /// - 支持: tool_calls
    Mistral,

    /// Groq API 格式
    /// - 兼容 OpenAI 格式
    /// - 高速推理优化
    Groq,

    /// OpenRouter API 格式
    /// - 支持多种后端格式
    /// - 可选: Anthropic 兼容或 OpenAI 兼容
    OpenRouter,

    /// xAI Grok API 格式
    /// - 兼容 OpenAI 格式
    XAI,

    /// Ollama 本地模型格式
    /// - 兼容 OpenAI 格式
    /// - 支持本地部署
    Ollama,
}

impl ProtocolFormat {
    /// 获取协议的默认端点
    pub fn default_endpoint(&self) -> &'static str {
        match self {
            ProtocolFormat::Anthropic => "/v1/messages",
            ProtocolFormat::OpenAIChat => "/v1/chat/completions",
            ProtocolFormat::OpenAIResponses => "/v1/responses",
            ProtocolFormat::Gemini => "/v1beta/models",
            ProtocolFormat::Cohere => "/v1/chat",
            ProtocolFormat::DeepSeek => "/v1/chat/completions",
            ProtocolFormat::Mistral => "/v1/chat/completions",
            ProtocolFormat::Groq => "/openai/v1/chat/completions",
            ProtocolFormat::OpenRouter => "/api/v1/chat/completions",
            ProtocolFormat::XAI => "/v1/chat/completions",
            ProtocolFormat::Ollama => "/api/chat",
        }
    }

    /// 获取协议的默认 base URL
    pub fn default_base_url(&self) -> &'static str {
        match self {
            ProtocolFormat::Anthropic => "https://api.anthropic.com",
            ProtocolFormat::OpenAIChat | ProtocolFormat::OpenAIResponses => "https://api.openai.com",
            ProtocolFormat::Gemini => "https://generativelanguage.googleapis.com",
            ProtocolFormat::Cohere => "https://api.cohere.ai",
            ProtocolFormat::DeepSeek => "https://api.deepseek.com",
            ProtocolFormat::Mistral => "https://api.mistral.ai",
            ProtocolFormat::Groq => "https://api.groq.com",
            ProtocolFormat::OpenRouter => "https://openrouter.ai",
            ProtocolFormat::XAI => "https://api.x.ai",
            ProtocolFormat::Ollama => "http://localhost:11434",
        }
    }

    /// 是否兼容 OpenAI Chat Completions 格式
    pub fn is_openai_compatible(&self) -> bool {
        matches!(
            self,
            ProtocolFormat::OpenAIChat
                | ProtocolFormat::DeepSeek
                | ProtocolFormat::Mistral
                | ProtocolFormat::Groq
                | ProtocolFormat::XAI
                | ProtocolFormat::Ollama
        )
    }

    /// 是否为 OpenAI 系列格式（Chat 或 Responses）
    pub fn is_openai_family(&self) -> bool {
        matches!(
            self,
            ProtocolFormat::OpenAIChat | ProtocolFormat::OpenAIResponses
        )
    }

    /// 是否支持思维链 (Reasoning/Thinking)
    pub fn supports_reasoning(&self) -> bool {
        matches!(
            self,
            ProtocolFormat::Anthropic
                | ProtocolFormat::Gemini
                | ProtocolFormat::DeepSeek
                | ProtocolFormat::OpenAIResponses // Responses API 支持 reasoning
        )
    }

    /// 是否支持工具调用
    pub fn supports_tool_calls(&self) -> bool {
        matches!(
            self,
            ProtocolFormat::Anthropic
                | ProtocolFormat::OpenAIChat
                | ProtocolFormat::OpenAIResponses
                | ProtocolFormat::Gemini
                | ProtocolFormat::Cohere
                | ProtocolFormat::DeepSeek
                | ProtocolFormat::Mistral
                | ProtocolFormat::Groq
        )
    }

    /// 是否支持多模态 (图片)
    pub fn supports_images(&self) -> bool {
        matches!(
            self,
            ProtocolFormat::Anthropic
                | ProtocolFormat::OpenAIChat
                | ProtocolFormat::OpenAIResponses
                | ProtocolFormat::Gemini
                | ProtocolFormat::Cohere
        )
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "anthropic" | "claude" => Some(ProtocolFormat::Anthropic),
            "openai" | "gpt" | "openai_chat" | "chat_completions" => Some(ProtocolFormat::OpenAIChat),
            "openai_responses" | "responses" | "responses_api" => Some(ProtocolFormat::OpenAIResponses),
            "gemini" | "google" => Some(ProtocolFormat::Gemini),
            "cohere" => Some(ProtocolFormat::Cohere),
            "deepseek" => Some(ProtocolFormat::DeepSeek),
            "mistral" => Some(ProtocolFormat::Mistral),
            "groq" => Some(ProtocolFormat::Groq),
            "openrouter" => Some(ProtocolFormat::OpenRouter),
            "xai" | "grok" => Some(ProtocolFormat::XAI),
            "ollama" => Some(ProtocolFormat::Ollama),
            _ => None,
        }
    }

    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            ProtocolFormat::Anthropic => "anthropic",
            ProtocolFormat::OpenAIChat => "openai_chat",
            ProtocolFormat::OpenAIResponses => "openai_responses",
            ProtocolFormat::Gemini => "gemini",
            ProtocolFormat::Cohere => "cohere",
            ProtocolFormat::DeepSeek => "deepseek",
            ProtocolFormat::Mistral => "mistral",
            ProtocolFormat::Groq => "groq",
            ProtocolFormat::OpenRouter => "openrouter",
            ProtocolFormat::XAI => "xai",
            ProtocolFormat::Ollama => "ollama",
        }
    }

    /// 获取显示名称（用于 UI）
    pub fn display_name(&self) -> &'static str {
        match self {
            ProtocolFormat::Anthropic => "Anthropic (Claude)",
            ProtocolFormat::OpenAIChat => "OpenAI Chat Completions",
            ProtocolFormat::OpenAIResponses => "OpenAI Responses API",
            ProtocolFormat::Gemini => "Google Gemini",
            ProtocolFormat::Cohere => "Cohere",
            ProtocolFormat::DeepSeek => "DeepSeek",
            ProtocolFormat::Mistral => "Mistral",
            ProtocolFormat::Groq => "Groq",
            ProtocolFormat::OpenRouter => "OpenRouter",
            ProtocolFormat::XAI => "xAI (Grok)",
            ProtocolFormat::Ollama => "Ollama",
        }
    }
}

impl std::fmt::Display for ProtocolFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 协议转换配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// 源协议格式（客户端发送的格式）
    pub source_format: ProtocolFormat,

    /// 目标协议格式（上游 API 期望的格式）
    pub target_format: ProtocolFormat,

    /// 是否启用转换
    pub transform_enabled: bool,

    /// 模型映射配置
    #[serde(default)]
    pub model_mapping: std::collections::HashMap<String, String>,

    /// 是否保留思维链内容
    #[serde(default = "default_true")]
    pub preserve_reasoning: bool,

    /// 是否转换工具调用格式
    #[serde(default = "default_true")]
    pub transform_tools: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            source_format: ProtocolFormat::Anthropic,
            target_format: ProtocolFormat::Anthropic,
            transform_enabled: false,
            model_mapping: std::collections::HashMap::new(),
            preserve_reasoning: true,
            transform_tools: true,
        }
    }
}

impl ProtocolConfig {
    /// 创建透传配置（不转换）
    pub fn passthrough(format: ProtocolFormat) -> Self {
        Self {
            source_format: format,
            target_format: format,
            transform_enabled: false,
            ..Default::default()
        }
    }

    /// 创建转换配置
    pub fn transform(source: ProtocolFormat, target: ProtocolFormat) -> Self {
        Self {
            source_format: source,
            target_format: target,
            transform_enabled: source != target,
            ..Default::default()
        }
    }

    /// 是否需要转换
    pub fn needs_transform(&self) -> bool {
        self.transform_enabled && self.source_format != self.target_format
    }

    /// 检查转换是否支持
    pub fn is_transform_supported(&self) -> bool {
        // 所有格式都可以转换到 OpenAI 兼容格式
        if self.target_format.is_openai_compatible() {
            return true;
        }

        // Anthropic 和 Gemini 之间可以互转
        // OpenAI Chat/Responses 可以转换到 Anthropic 和 Gemini
        matches!(
            (&self.source_format, &self.target_format),
            (ProtocolFormat::Anthropic, ProtocolFormat::Gemini)
                | (ProtocolFormat::Gemini, ProtocolFormat::Anthropic)
                | (ProtocolFormat::OpenAIChat, ProtocolFormat::Anthropic)
                | (ProtocolFormat::OpenAIChat, ProtocolFormat::Gemini)
                | (ProtocolFormat::OpenAIResponses, ProtocolFormat::Anthropic)
                | (ProtocolFormat::OpenAIResponses, ProtocolFormat::Gemini)
                | (ProtocolFormat::Anthropic, ProtocolFormat::OpenAIResponses)
                | (ProtocolFormat::Gemini, ProtocolFormat::OpenAIResponses)
        )
    }
}

/// 转换能力矩阵
///
/// 定义哪些格式之间可以互相转换
#[derive(Debug)]
pub struct TransformMatrix;

impl TransformMatrix {
    /// 检查是否支持从 source 转换到 target
    pub fn is_supported(source: ProtocolFormat, target: ProtocolFormat) -> bool {
        if source == target {
            return true; // 相同格式，无需转换
        }

        match (source, target) {
            // Anthropic 可以转换到所有格式
            (ProtocolFormat::Anthropic, _) => true,

            // OpenAI Chat 可以转换到 Anthropic、Gemini 和 OpenAI Responses
            (ProtocolFormat::OpenAIChat, ProtocolFormat::Anthropic) => true,
            (ProtocolFormat::OpenAIChat, ProtocolFormat::Gemini) => true,
            (ProtocolFormat::OpenAIChat, ProtocolFormat::OpenAIResponses) => true,

            // OpenAI Responses 可以转换到 Anthropic、Gemini 和 OpenAI Chat
            (ProtocolFormat::OpenAIResponses, ProtocolFormat::Anthropic) => true,
            (ProtocolFormat::OpenAIResponses, ProtocolFormat::Gemini) => true,
            (ProtocolFormat::OpenAIResponses, ProtocolFormat::OpenAIChat) => true,

            // Gemini 可以转换到 Anthropic 和 OpenAI 系列
            (ProtocolFormat::Gemini, ProtocolFormat::Anthropic) => true,
            (ProtocolFormat::Gemini, ProtocolFormat::OpenAIChat) => true,
            (ProtocolFormat::Gemini, ProtocolFormat::OpenAIResponses) => true,

            // OpenAI 兼容格式之间可以互转
            (s, t) if s.is_openai_compatible() && t.is_openai_compatible() => true,

            // 其他情况暂不支持
            _ => false,
        }
    }

    /// 获取转换路径
    ///
    /// 如果直接转换不支持，尝试通过中间格式转换
    pub fn get_transform_path(
        source: ProtocolFormat,
        target: ProtocolFormat,
    ) -> Option<Vec<ProtocolFormat>> {
        if source == target {
            return Some(vec![source]);
        }

        // 直接转换
        if Self::is_supported(source, target) {
            return Some(vec![source, target]);
        }

        // 通过 OpenAI Chat 格式中转
        if Self::is_supported(source, ProtocolFormat::OpenAIChat)
            && Self::is_supported(ProtocolFormat::OpenAIChat, target)
        {
            return Some(vec![source, ProtocolFormat::OpenAIChat, target]);
        }

        // 通过 Anthropic 格式中转
        if Self::is_supported(source, ProtocolFormat::Anthropic)
            && Self::is_supported(ProtocolFormat::Anthropic, target)
        {
            return Some(vec![source, ProtocolFormat::Anthropic, target]);
        }

        None
    }
}

/// 功能支持矩阵
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSupport {
    pub text: bool,
    pub images: bool,
    pub tool_calls: bool,
    pub tool_results: bool,
    pub reasoning: bool,
    pub streaming: bool,
    pub system_message: bool,
}

impl FeatureSupport {
    /// 获取指定协议的功能支持
    pub fn for_protocol(protocol: ProtocolFormat) -> Self {
        match protocol {
            ProtocolFormat::Anthropic => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: true,
                streaming: true,
                system_message: true, // 通过 system 字段
            },
            ProtocolFormat::OpenAIChat => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: false, // Chat Completions 原生不支持
                streaming: true,
                system_message: true,
            },
            ProtocolFormat::OpenAIResponses => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: true, // Responses API 支持 reasoning
                streaming: true,
                system_message: true, // 通过 instructions 字段
            },
            ProtocolFormat::Gemini => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: true, // thought 字段
                streaming: true,
                system_message: true, // systemInstruction
            },
            ProtocolFormat::Cohere => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: false,
                streaming: true,
                system_message: true, // preamble
            },
            ProtocolFormat::DeepSeek => Self {
                text: true,
                images: false,
                tool_calls: true,
                tool_results: true,
                reasoning: true, // reasoning_content
                streaming: true,
                system_message: true,
            },
            ProtocolFormat::Mistral => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: false,
                streaming: true,
                system_message: true,
            },
            ProtocolFormat::Groq => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: false,
                streaming: true,
                system_message: true,
            },
            ProtocolFormat::OpenRouter => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: true, // 取决于后端模型
                streaming: true,
                system_message: true,
            },
            ProtocolFormat::XAI => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: false,
                streaming: true,
                system_message: true,
            },
            ProtocolFormat::Ollama => Self {
                text: true,
                images: true,
                tool_calls: true,
                tool_results: true,
                reasoning: false,
                streaming: true,
                system_message: true,
            },
        }
    }

    /// 检查转换是否会丢失功能
    pub fn check_feature_loss(source: ProtocolFormat, target: ProtocolFormat) -> Vec<&'static str> {
        let source_features = Self::for_protocol(source);
        let target_features = Self::for_protocol(target);
        let mut lost = Vec::new();

        if source_features.reasoning && !target_features.reasoning {
            lost.push("reasoning");
        }
        if source_features.images && !target_features.images {
            lost.push("images");
        }
        if source_features.tool_calls && !target_features.tool_calls {
            lost.push("tool_calls");
        }

        lost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_format_from_str() {
        assert_eq!(
            ProtocolFormat::from_str("anthropic"),
            Some(ProtocolFormat::Anthropic)
        );
        assert_eq!(
            ProtocolFormat::from_str("claude"),
            Some(ProtocolFormat::Anthropic)
        );
        // "openai" 解析为 OpenAIChat（向后兼容）
        assert_eq!(
            ProtocolFormat::from_str("openai"),
            Some(ProtocolFormat::OpenAIChat)
        );
        assert_eq!(
            ProtocolFormat::from_str("openai_chat"),
            Some(ProtocolFormat::OpenAIChat)
        );
        assert_eq!(
            ProtocolFormat::from_str("openai_responses"),
            Some(ProtocolFormat::OpenAIResponses)
        );
        assert_eq!(
            ProtocolFormat::from_str("responses"),
            Some(ProtocolFormat::OpenAIResponses)
        );
        assert_eq!(
            ProtocolFormat::from_str("gemini"),
            Some(ProtocolFormat::Gemini)
        );
        assert_eq!(ProtocolFormat::from_str("unknown"), None);
    }

    #[test]
    fn test_openai_compatible() {
        assert!(ProtocolFormat::OpenAIChat.is_openai_compatible());
        assert!(ProtocolFormat::DeepSeek.is_openai_compatible());
        assert!(ProtocolFormat::Mistral.is_openai_compatible());
        assert!(!ProtocolFormat::Anthropic.is_openai_compatible());
        assert!(!ProtocolFormat::Gemini.is_openai_compatible());
        // OpenAI Responses 不属于 "OpenAI 兼容"（它使用不同格式）
        assert!(!ProtocolFormat::OpenAIResponses.is_openai_compatible());
    }

    #[test]
    fn test_openai_family() {
        assert!(ProtocolFormat::OpenAIChat.is_openai_family());
        assert!(ProtocolFormat::OpenAIResponses.is_openai_family());
        assert!(!ProtocolFormat::Anthropic.is_openai_family());
        assert!(!ProtocolFormat::DeepSeek.is_openai_family());
    }

    #[test]
    fn test_reasoning_support() {
        assert!(ProtocolFormat::Anthropic.supports_reasoning());
        assert!(ProtocolFormat::Gemini.supports_reasoning());
        assert!(ProtocolFormat::OpenAIResponses.supports_reasoning());
        assert!(!ProtocolFormat::OpenAIChat.supports_reasoning());
    }

    #[test]
    fn test_transform_matrix() {
        // 直接支持
        assert!(TransformMatrix::is_supported(
            ProtocolFormat::Anthropic,
            ProtocolFormat::OpenAIChat
        ));
        assert!(TransformMatrix::is_supported(
            ProtocolFormat::OpenAIChat,
            ProtocolFormat::Anthropic
        ));
        assert!(TransformMatrix::is_supported(
            ProtocolFormat::Anthropic,
            ProtocolFormat::Gemini
        ));

        // OpenAI Chat ↔ OpenAI Responses
        assert!(TransformMatrix::is_supported(
            ProtocolFormat::OpenAIChat,
            ProtocolFormat::OpenAIResponses
        ));
        assert!(TransformMatrix::is_supported(
            ProtocolFormat::OpenAIResponses,
            ProtocolFormat::OpenAIChat
        ));

        // 相同格式
        assert!(TransformMatrix::is_supported(
            ProtocolFormat::Anthropic,
            ProtocolFormat::Anthropic
        ));
    }

    #[test]
    fn test_transform_path() {
        // 直接路径
        let path = TransformMatrix::get_transform_path(
            ProtocolFormat::Anthropic,
            ProtocolFormat::OpenAIChat,
        );
        assert_eq!(
            path,
            Some(vec![ProtocolFormat::Anthropic, ProtocolFormat::OpenAIChat])
        );

        // 相同格式
        let path = TransformMatrix::get_transform_path(
            ProtocolFormat::Anthropic,
            ProtocolFormat::Anthropic,
        );
        assert_eq!(path, Some(vec![ProtocolFormat::Anthropic]));
    }

    #[test]
    fn test_feature_loss() {
        // Anthropic → OpenAI Chat 会丢失 reasoning
        let lost =
            FeatureSupport::check_feature_loss(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIChat);
        assert!(lost.contains(&"reasoning"));

        // Anthropic → OpenAI Responses 不会丢失 reasoning
        let lost =
            FeatureSupport::check_feature_loss(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIResponses);
        assert!(!lost.contains(&"reasoning"));

        // OpenAI Chat → Anthropic 不会丢失功能
        let lost =
            FeatureSupport::check_feature_loss(ProtocolFormat::OpenAIChat, ProtocolFormat::Anthropic);
        assert!(lost.is_empty());
    }

    #[test]
    fn test_protocol_config() {
        let config = ProtocolConfig::passthrough(ProtocolFormat::Anthropic);
        assert!(!config.needs_transform());

        let config =
            ProtocolConfig::transform(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIChat);
        assert!(config.needs_transform());
    }
}
