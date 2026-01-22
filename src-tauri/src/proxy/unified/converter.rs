//! 多协议转换器
//!
//! 提供不同协议格式之间的转换功能

use super::protocol::{ProtocolConfig, ProtocolFormat, TransformMatrix};
use crate::provider::Provider;
use crate::proxy::error::ProxyError;
use serde_json::{json, Value};

/// 多协议转换器
#[allow(dead_code)]
pub struct ProtocolConverter;

#[allow(dead_code)]
impl ProtocolConverter {
    /// 转换请求
    pub fn convert_request(
        body: Value,
        config: &ProtocolConfig,
        provider: &Provider,
    ) -> Result<Value, ProxyError> {
        if !config.needs_transform() {
            return Ok(body);
        }

        // 获取转换路径
        let path = TransformMatrix::get_transform_path(config.source_format, config.target_format)
            .ok_or_else(|| {
                ProxyError::TransformError(format!(
                    "Unsupported transform: {} -> {}",
                    config.source_format, config.target_format
                ))
            })?;

        // 逐步转换
        let mut current = body;
        for i in 0..path.len() - 1 {
            current = Self::transform_step(current, path[i], path[i + 1], provider, config)?;
        }

        Ok(current)
    }

    /// 转换响应
    pub fn convert_response(body: Value, config: &ProtocolConfig) -> Result<Value, ProxyError> {
        if !config.needs_transform() {
            return Ok(body);
        }

        // 响应转换是请求转换的逆过程
        let path = TransformMatrix::get_transform_path(config.target_format, config.source_format)
            .ok_or_else(|| {
                ProxyError::TransformError(format!(
                    "Unsupported response transform: {} -> {}",
                    config.target_format, config.source_format
                ))
            })?;

        let mut current = body;
        for i in 0..path.len() - 1 {
            current = Self::transform_response_step(current, path[i], path[i + 1], config)?;
        }

        Ok(current)
    }

    /// 单步请求转换
    fn transform_step(
        body: Value,
        source: ProtocolFormat,
        target: ProtocolFormat,
        provider: &Provider,
        config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        match (source, target) {
            // Anthropic → OpenAI Chat
            (ProtocolFormat::Anthropic, ProtocolFormat::OpenAIChat)
            | (ProtocolFormat::Anthropic, ProtocolFormat::DeepSeek)
            | (ProtocolFormat::Anthropic, ProtocolFormat::Mistral)
            | (ProtocolFormat::Anthropic, ProtocolFormat::Groq)
            | (ProtocolFormat::Anthropic, ProtocolFormat::XAI)
            | (ProtocolFormat::Anthropic, ProtocolFormat::Ollama) => {
                Self::anthropic_to_openai(body, provider, config)
            }

            // Anthropic → OpenAI Responses
            (ProtocolFormat::Anthropic, ProtocolFormat::OpenAIResponses) => {
                Self::anthropic_to_openai_responses(body, provider, config)
            }

            // Anthropic → Gemini
            (ProtocolFormat::Anthropic, ProtocolFormat::Gemini) => {
                Self::anthropic_to_gemini(body, provider, config)
            }

            // Anthropic → Cohere
            (ProtocolFormat::Anthropic, ProtocolFormat::Cohere) => {
                Self::anthropic_to_cohere(body, provider, config)
            }

            // OpenAI Chat → Anthropic
            (ProtocolFormat::OpenAIChat, ProtocolFormat::Anthropic) => {
                Self::openai_to_anthropic_request(body, provider, config)
            }

            // OpenAI Chat → Gemini
            (ProtocolFormat::OpenAIChat, ProtocolFormat::Gemini) => {
                Self::openai_to_gemini(body, provider, config)
            }

            // OpenAI Chat → OpenAI Responses
            (ProtocolFormat::OpenAIChat, ProtocolFormat::OpenAIResponses) => {
                Self::openai_chat_to_responses(body, provider, config)
            }

            // OpenAI Responses → Anthropic
            (ProtocolFormat::OpenAIResponses, ProtocolFormat::Anthropic) => {
                Self::openai_responses_to_anthropic(body, provider, config)
            }

            // OpenAI Responses → OpenAI Chat
            (ProtocolFormat::OpenAIResponses, ProtocolFormat::OpenAIChat) => {
                Self::openai_responses_to_chat(body, provider, config)
            }

            // Gemini → Anthropic
            (ProtocolFormat::Gemini, ProtocolFormat::Anthropic) => {
                Self::gemini_to_anthropic(body, provider, config)
            }

            // Gemini → OpenAI Chat
            (ProtocolFormat::Gemini, ProtocolFormat::OpenAIChat) => {
                Self::gemini_to_openai(body, provider, config)
            }

            // OpenAI 兼容格式之间直接透传
            (s, t) if s.is_openai_compatible() && t.is_openai_compatible() => Ok(body),

            // 相同格式
            (s, t) if s == t => Ok(body),

            _ => Err(ProxyError::TransformError(format!(
                "Unsupported transform step: {} -> {}",
                source, target
            ))),
        }
    }

    /// 单步响应转换
    fn transform_response_step(
        body: Value,
        source: ProtocolFormat,
        target: ProtocolFormat,
        config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        match (source, target) {
            // OpenAI Chat → Anthropic
            (ProtocolFormat::OpenAIChat, ProtocolFormat::Anthropic)
            | (ProtocolFormat::DeepSeek, ProtocolFormat::Anthropic)
            | (ProtocolFormat::Mistral, ProtocolFormat::Anthropic)
            | (ProtocolFormat::Groq, ProtocolFormat::Anthropic)
            | (ProtocolFormat::XAI, ProtocolFormat::Anthropic)
            | (ProtocolFormat::Ollama, ProtocolFormat::Anthropic) => {
                Self::openai_to_anthropic_response(body, config)
            }

            // OpenAI Responses → Anthropic
            (ProtocolFormat::OpenAIResponses, ProtocolFormat::Anthropic) => {
                Self::openai_responses_to_anthropic_response(body, config)
            }

            // Gemini → Anthropic
            (ProtocolFormat::Gemini, ProtocolFormat::Anthropic) => {
                Self::gemini_to_anthropic_response(body, config)
            }

            // Anthropic → OpenAI Chat
            (ProtocolFormat::Anthropic, ProtocolFormat::OpenAIChat) => {
                Self::anthropic_to_openai_response(body, config)
            }

            // Anthropic → OpenAI Responses
            (ProtocolFormat::Anthropic, ProtocolFormat::OpenAIResponses) => {
                Self::anthropic_to_openai_responses_response(body, config)
            }

            // OpenAI Chat ↔ OpenAI Responses
            (ProtocolFormat::OpenAIChat, ProtocolFormat::OpenAIResponses) => {
                Self::openai_chat_to_responses_response(body, config)
            }
            (ProtocolFormat::OpenAIResponses, ProtocolFormat::OpenAIChat) => {
                Self::openai_responses_to_chat_response(body, config)
            }

            // OpenAI 兼容格式之间直接透传
            (s, t) if s.is_openai_compatible() && t.is_openai_compatible() => Ok(body),

            // 相同格式
            (s, t) if s == t => Ok(body),

            _ => Err(ProxyError::TransformError(format!(
                "Unsupported response transform step: {} -> {}",
                source, target
            ))),
        }
    }

    // ========== Anthropic → OpenAI ==========

    /// Anthropic 请求 → OpenAI 请求
    ///
    /// 注意：模型映射已在 forwarder.rs 中通过 model_mapper.rs 完成，
    /// 此处接收的 body 中的 model 字段已经是映射后的结果。
    fn anthropic_to_openai(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});

        // 模型名称直接使用（已在 model_mapper.rs 中完成映射）
        if let Some(model) = body.get("model") {
            result["model"] = model.clone();
        }

        // 处理 system prompt
        let mut messages = Vec::new();
        if let Some(system) = body.get("system") {
            if let Some(text) = system.as_str() {
                messages.push(json!({"role": "system", "content": text}));
            } else if let Some(arr) = system.as_array() {
                for msg in arr {
                    if let Some(text) = msg.get("text").and_then(|t| t.as_str()) {
                        messages.push(json!({"role": "system", "content": text}));
                    }
                }
            }
        }

        // 转换 messages
        if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
            for msg in msgs {
                let converted = Self::convert_anthropic_message_to_openai(msg)?;
                messages.extend(converted);
            }
        }

        result["messages"] = json!(messages);

        // 复制参数
        Self::copy_common_params(&body, &mut result);

        // 转换 tools
        if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
            let openai_tools: Vec<Value> = tools
                .iter()
                .filter(|t| t.get("type").and_then(|v| v.as_str()) != Some("BatchTool"))
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.get("name"),
                            "description": t.get("description"),
                            "parameters": crate::proxy::providers::transform::clean_schema(
                                t.get("input_schema").cloned().unwrap_or(json!({}))
                            )
                        }
                    })
                })
                .collect();

            if !openai_tools.is_empty() {
                result["tools"] = json!(openai_tools);
            }
        }

        Ok(result)
    }

    fn convert_anthropic_message_to_openai(msg: &Value) -> Result<Vec<Value>, ProxyError> {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = msg.get("content");

        let mut result = Vec::new();

        let content = match content {
            Some(c) => c,
            None => {
                result.push(json!({"role": role, "content": null}));
                return Ok(result);
            }
        };

        // 字符串内容
        if let Some(text) = content.as_str() {
            result.push(json!({"role": role, "content": text}));
            return Ok(result);
        }

        // 数组内容
        if let Some(blocks) = content.as_array() {
            let mut content_parts = Vec::new();
            let mut tool_calls = Vec::new();

            for block in blocks {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            content_parts.push(json!({"type": "text", "text": text}));
                        }
                    }
                    "image" => {
                        if let Some(source) = block.get("source") {
                            let media_type = source
                                .get("media_type")
                                .and_then(|m| m.as_str())
                                .unwrap_or("image/png");
                            let data = source.get("data").and_then(|d| d.as_str()).unwrap_or("");
                            content_parts.push(json!({
                                "type": "image_url",
                                "image_url": {"url": format!("data:{};base64,{}", media_type, data)}
                            }));
                        }
                    }
                    "tool_use" => {
                        let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let input = block.get("input").cloned().unwrap_or(json!({}));
                        tool_calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(&input).unwrap_or_default()
                            }
                        }));
                    }
                    "tool_result" => {
                        let tool_use_id = block
                            .get("tool_use_id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("");
                        let content_val = block.get("content");
                        let content_str = match content_val {
                            Some(Value::String(s)) => s.clone(),
                            Some(v) => serde_json::to_string(v).unwrap_or_default(),
                            None => String::new(),
                        };
                        result.push(json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": content_str
                        }));
                    }
                    "thinking" => {
                        // 跳过 thinking blocks（OpenAI 不支持）
                    }
                    _ => {}
                }
            }

            // 添加带内容和/或工具调用的消息
            if !content_parts.is_empty() || !tool_calls.is_empty() {
                let mut msg = json!({"role": role});

                if content_parts.is_empty() {
                    msg["content"] = Value::Null;
                } else if content_parts.len() == 1 {
                    if let Some(text) = content_parts[0].get("text") {
                        msg["content"] = text.clone();
                    } else {
                        msg["content"] = json!(content_parts);
                    }
                } else {
                    msg["content"] = json!(content_parts);
                }

                if !tool_calls.is_empty() {
                    msg["tool_calls"] = json!(tool_calls);
                }

                result.push(msg);
            }
        }

        Ok(result)
    }

    // ========== Anthropic → Gemini ==========

    fn anthropic_to_gemini(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});
        let mut contents = Vec::new();

        // 转换 messages
        if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
            for msg in msgs {
                let gemini_content = Self::convert_anthropic_message_to_gemini(msg)?;
                contents.push(gemini_content);
            }
        }

        result["contents"] = json!(contents);

        // 处理 system instruction
        if let Some(system) = body.get("system") {
            if let Some(text) = system.as_str() {
                result["systemInstruction"] = json!({
                    "parts": [{"text": text}]
                });
            }
        }

        // 转换参数
        let mut gen_config = json!({});
        if let Some(v) = body.get("max_tokens") {
            gen_config["maxOutputTokens"] = v.clone();
        }
        if let Some(v) = body.get("temperature") {
            gen_config["temperature"] = v.clone();
        }
        if let Some(v) = body.get("top_p") {
            gen_config["topP"] = v.clone();
        }
        if !gen_config.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            result["generationConfig"] = gen_config;
        }

        // 转换 tools
        if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
            let gemini_tools: Vec<Value> = tools
                .iter()
                .filter(|t| t.get("type").and_then(|v| v.as_str()) != Some("BatchTool"))
                .map(|t| {
                    json!({
                        "name": t.get("name"),
                        "description": t.get("description"),
                        "parameters": crate::proxy::providers::transform::clean_schema(
                            t.get("input_schema").cloned().unwrap_or(json!({}))
                        )
                    })
                })
                .collect();

            if !gemini_tools.is_empty() {
                result["tools"] = json!([{
                    "functionDeclarations": gemini_tools
                }]);
            }
        }

        Ok(result)
    }

    fn convert_anthropic_message_to_gemini(msg: &Value) -> Result<Value, ProxyError> {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let gemini_role = match role {
            "user" => "user",
            "assistant" => "model",
            _ => "user",
        };

        let mut parts = Vec::new();

        if let Some(content) = msg.get("content") {
            if let Some(text) = content.as_str() {
                parts.push(json!({"text": text}));
            } else if let Some(blocks) = content.as_array() {
                for block in blocks {
                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match block_type {
                        "text" => {
                            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                parts.push(json!({"text": text}));
                            }
                        }
                        "image" => {
                            if let Some(source) = block.get("source") {
                                let media_type = source
                                    .get("media_type")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("image/png");
                                let data =
                                    source.get("data").and_then(|d| d.as_str()).unwrap_or("");
                                parts.push(json!({
                                    "inlineData": {
                                        "mimeType": media_type,
                                        "data": data
                                    }
                                }));
                            }
                        }
                        "tool_use" => {
                            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let input = block.get("input").cloned().unwrap_or(json!({}));
                            parts.push(json!({
                                "functionCall": {
                                    "name": name,
                                    "args": input
                                }
                            }));
                        }
                        "tool_result" => {
                            let name = block
                                .get("tool_use_id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("");
                            let content_val = block.get("content");
                            parts.push(json!({
                                "functionResponse": {
                                    "name": name,
                                    "response": content_val
                                }
                            }));
                        }
                        "thinking" => {
                            if let Some(thinking) = block.get("thinking").and_then(|t| t.as_str()) {
                                parts.push(json!({
                                    "thought": true,
                                    "text": thinking
                                }));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(json!({
            "role": gemini_role,
            "parts": parts
        }))
    }

    // ========== Anthropic → Cohere ==========

    fn anthropic_to_cohere(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});

        // 模型
        if let Some(model) = body.get("model").and_then(|m| m.as_str()) {
            result["model"] = json!(model);
        }

        // 处理 system prompt (preamble)
        if let Some(system) = body.get("system") {
            if let Some(text) = system.as_str() {
                result["preamble"] = json!(text);
            }
        }

        // 转换 messages 到 chat_history 和 message
        let mut chat_history = Vec::new();
        let mut last_user_message = String::new();

        if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
            for (i, msg) in msgs.iter().enumerate() {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                let content = Self::extract_text_content(msg);

                let cohere_role = match role {
                    "user" => "USER",
                    "assistant" => "CHATBOT",
                    _ => "USER",
                };

                // 最后一条用户消息作为 message
                if i == msgs.len() - 1 && role == "user" {
                    last_user_message = content;
                } else {
                    chat_history.push(json!({
                        "role": cohere_role,
                        "message": content
                    }));
                }
            }
        }

        result["message"] = json!(last_user_message);
        if !chat_history.is_empty() {
            result["chat_history"] = json!(chat_history);
        }

        // 转换参数
        if let Some(v) = body.get("max_tokens") {
            result["max_tokens"] = v.clone();
        }
        if let Some(v) = body.get("temperature") {
            result["temperature"] = v.clone();
        }

        // 转换 tools
        if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
            let cohere_tools: Vec<Value> = tools
                .iter()
                .filter(|t| t.get("type").and_then(|v| v.as_str()) != Some("BatchTool"))
                .map(|t| {
                    json!({
                        "name": t.get("name"),
                        "description": t.get("description"),
                        "parameter_definitions": t.get("input_schema").and_then(|s| s.get("properties"))
                    })
                })
                .collect();

            if !cohere_tools.is_empty() {
                result["tools"] = json!(cohere_tools);
            }
        }

        Ok(result)
    }

    // ========== OpenAI → Anthropic ==========

    fn openai_to_anthropic_request(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});

        // 模型
        if let Some(model) = body.get("model").and_then(|m| m.as_str()) {
            result["model"] = json!(model);
        }

        let mut messages = Vec::new();

        if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
            for msg in msgs {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");

                match role {
                    "system" => {
                        // 提取 system 消息
                        if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                            result["system"] = json!(content);
                        }
                    }
                    "tool" => {
                        // 转换 tool 消息为 tool_result
                        let tool_call_id = msg
                            .get("tool_call_id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("");
                        let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                        messages.push(json!({
                            "role": "user",
                            "content": [{
                                "type": "tool_result",
                                "tool_use_id": tool_call_id,
                                "content": content
                            }]
                        }));
                    }
                    _ => {
                        let anthropic_msg = Self::convert_openai_message_to_anthropic(msg)?;
                        messages.push(anthropic_msg);
                    }
                }
            }
        }

        result["messages"] = json!(messages);

        // 复制参数
        if let Some(v) = body.get("max_tokens") {
            result["max_tokens"] = v.clone();
        } else {
            result["max_tokens"] = json!(4096); // Anthropic 要求必须设置
        }
        if let Some(v) = body.get("temperature") {
            result["temperature"] = v.clone();
        }
        if let Some(v) = body.get("top_p") {
            result["top_p"] = v.clone();
        }
        if let Some(v) = body.get("stream") {
            result["stream"] = v.clone();
        }

        // 转换 tools
        if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
            let anthropic_tools: Vec<Value> = tools
                .iter()
                .filter_map(|t| {
                    let func = t.get("function")?;
                    Some(json!({
                        "name": func.get("name"),
                        "description": func.get("description"),
                        "input_schema": func.get("parameters")
                    }))
                })
                .collect();

            if !anthropic_tools.is_empty() {
                result["tools"] = json!(anthropic_tools);
            }
        }

        Ok(result)
    }

    fn convert_openai_message_to_anthropic(msg: &Value) -> Result<Value, ProxyError> {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let mut content = Vec::new();

        // 处理文本内容
        if let Some(c) = msg.get("content") {
            if let Some(text) = c.as_str() {
                if !text.is_empty() {
                    content.push(json!({"type": "text", "text": text}));
                }
            } else if let Some(parts) = c.as_array() {
                for part in parts {
                    let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match part_type {
                        "text" => {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                content.push(json!({"type": "text", "text": text}));
                            }
                        }
                        "image_url" => {
                            if let Some(url) = part
                                .get("image_url")
                                .and_then(|u| u.get("url"))
                                .and_then(|u| u.as_str())
                            {
                                // 解析 data URL
                                if url.starts_with("data:") {
                                    if let Some((media_type, data)) = Self::parse_data_url(url) {
                                        content.push(json!({
                                            "type": "image",
                                            "source": {
                                                "type": "base64",
                                                "media_type": media_type,
                                                "data": data
                                            }
                                        }));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // 处理 tool_calls
        if let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tool_calls {
                let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                let func = tc.get("function");
                let name = func
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let args_str = func
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}");
                let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));

                content.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input
                }));
            }
        }

        Ok(json!({
            "role": role,
            "content": content
        }))
    }

    // ========== OpenAI → Gemini ==========

    fn openai_to_gemini(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});
        let mut contents = Vec::new();

        if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
            for msg in msgs {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");

                match role {
                    "system" => {
                        if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                            result["systemInstruction"] = json!({
                                "parts": [{"text": content}]
                            });
                        }
                    }
                    _ => {
                        let gemini_role = match role {
                            "assistant" => "model",
                            _ => "user",
                        };

                        let mut parts = Vec::new();

                        if let Some(content) = msg.get("content") {
                            if let Some(text) = content.as_str() {
                                parts.push(json!({"text": text}));
                            } else if let Some(arr) = content.as_array() {
                                for part in arr {
                                    let part_type =
                                        part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                    match part_type {
                                        "text" => {
                                            if let Some(text) =
                                                part.get("text").and_then(|t| t.as_str())
                                            {
                                                parts.push(json!({"text": text}));
                                            }
                                        }
                                        "image_url" => {
                                            if let Some(url) = part
                                                .get("image_url")
                                                .and_then(|u| u.get("url"))
                                                .and_then(|u| u.as_str())
                                            {
                                                if let Some((media_type, data)) =
                                                    Self::parse_data_url(url)
                                                {
                                                    parts.push(json!({
                                                        "inlineData": {
                                                            "mimeType": media_type,
                                                            "data": data
                                                        }
                                                    }));
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }

                        // 处理 tool_calls
                        if let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                            for tc in tool_calls {
                                let func = tc.get("function");
                                let name = func
                                    .and_then(|f| f.get("name"))
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("");
                                let args_str = func
                                    .and_then(|f| f.get("arguments"))
                                    .and_then(|a| a.as_str())
                                    .unwrap_or("{}");
                                let args: Value =
                                    serde_json::from_str(args_str).unwrap_or(json!({}));

                                parts.push(json!({
                                    "functionCall": {
                                        "name": name,
                                        "args": args
                                    }
                                }));
                            }
                        }

                        if !parts.is_empty() {
                            contents.push(json!({
                                "role": gemini_role,
                                "parts": parts
                            }));
                        }
                    }
                }
            }
        }

        result["contents"] = json!(contents);

        // 转换参数
        let mut gen_config = json!({});
        if let Some(v) = body.get("max_tokens") {
            gen_config["maxOutputTokens"] = v.clone();
        }
        if let Some(v) = body.get("temperature") {
            gen_config["temperature"] = v.clone();
        }
        if let Some(v) = body.get("top_p") {
            gen_config["topP"] = v.clone();
        }
        if !gen_config.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            result["generationConfig"] = gen_config;
        }

        Ok(result)
    }

    // ========== Gemini → Anthropic ==========

    fn gemini_to_anthropic(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});
        let mut messages = Vec::new();

        // 处理 systemInstruction
        if let Some(system) = body.get("systemInstruction") {
            if let Some(parts) = system.get("parts").and_then(|p| p.as_array()) {
                let text: String = parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    result["system"] = json!(text);
                }
            }
        }

        // 转换 contents
        if let Some(contents) = body.get("contents").and_then(|c| c.as_array()) {
            for content in contents {
                let role = content
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("user");
                let anthropic_role = match role {
                    "model" => "assistant",
                    _ => "user",
                };

                let mut anthropic_content = Vec::new();

                if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            let is_thought = part.get("thought").and_then(|t| t.as_bool()).unwrap_or(false);
                            if is_thought {
                                anthropic_content.push(json!({
                                    "type": "thinking",
                                    "thinking": text
                                }));
                            } else {
                                anthropic_content.push(json!({
                                    "type": "text",
                                    "text": text
                                }));
                            }
                        }

                        if let Some(inline_data) = part.get("inlineData") {
                            let mime_type = inline_data
                                .get("mimeType")
                                .and_then(|m| m.as_str())
                                .unwrap_or("image/png");
                            let data = inline_data
                                .get("data")
                                .and_then(|d| d.as_str())
                                .unwrap_or("");
                            anthropic_content.push(json!({
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": mime_type,
                                    "data": data
                                }
                            }));
                        }

                        if let Some(func_call) = part.get("functionCall") {
                            let name = func_call
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("");
                            let args = func_call.get("args").cloned().unwrap_or(json!({}));
                            anthropic_content.push(json!({
                                "type": "tool_use",
                                "id": format!("call_{}", uuid::Uuid::new_v4()),
                                "name": name,
                                "input": args
                            }));
                        }

                        if let Some(func_resp) = part.get("functionResponse") {
                            let name = func_resp
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("");
                            let response = func_resp.get("response");
                            anthropic_content.push(json!({
                                "type": "tool_result",
                                "tool_use_id": name,
                                "content": response
                            }));
                        }
                    }
                }

                if !anthropic_content.is_empty() {
                    messages.push(json!({
                        "role": anthropic_role,
                        "content": anthropic_content
                    }));
                }
            }
        }

        result["messages"] = json!(messages);

        // 转换参数
        if let Some(gen_config) = body.get("generationConfig") {
            if let Some(v) = gen_config.get("maxOutputTokens") {
                result["max_tokens"] = v.clone();
            }
            if let Some(v) = gen_config.get("temperature") {
                result["temperature"] = v.clone();
            }
            if let Some(v) = gen_config.get("topP") {
                result["top_p"] = v.clone();
            }
        }

        // 默认 max_tokens
        if result.get("max_tokens").is_none() {
            result["max_tokens"] = json!(4096);
        }

        Ok(result)
    }

    // ========== Gemini → OpenAI ==========

    fn gemini_to_openai(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});
        let mut messages = Vec::new();

        // 处理 systemInstruction
        if let Some(system) = body.get("systemInstruction") {
            if let Some(parts) = system.get("parts").and_then(|p| p.as_array()) {
                let text: String = parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    messages.push(json!({"role": "system", "content": text}));
                }
            }
        }

        // 转换 contents
        if let Some(contents) = body.get("contents").and_then(|c| c.as_array()) {
            for content in contents {
                let role = content
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("user");
                let openai_role = match role {
                    "model" => "assistant",
                    _ => "user",
                };

                let mut content_parts = Vec::new();
                let mut tool_calls = Vec::new();

                if let Some(parts) = content.get("parts").and_then(|p| p.as_array()) {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            content_parts.push(json!({"type": "text", "text": text}));
                        }

                        if let Some(inline_data) = part.get("inlineData") {
                            let mime_type = inline_data
                                .get("mimeType")
                                .and_then(|m| m.as_str())
                                .unwrap_or("image/png");
                            let data = inline_data
                                .get("data")
                                .and_then(|d| d.as_str())
                                .unwrap_or("");
                            content_parts.push(json!({
                                "type": "image_url",
                                "image_url": {
                                    "url": format!("data:{};base64,{}", mime_type, data)
                                }
                            }));
                        }

                        if let Some(func_call) = part.get("functionCall") {
                            let name = func_call
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("");
                            let args = func_call.get("args").cloned().unwrap_or(json!({}));
                            tool_calls.push(json!({
                                "id": format!("call_{}", uuid::Uuid::new_v4()),
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": serde_json::to_string(&args).unwrap_or_default()
                                }
                            }));
                        }
                    }
                }

                let mut msg = json!({"role": openai_role});

                if content_parts.len() == 1 {
                    if let Some(text) = content_parts[0].get("text") {
                        msg["content"] = text.clone();
                    } else {
                        msg["content"] = json!(content_parts);
                    }
                } else if !content_parts.is_empty() {
                    msg["content"] = json!(content_parts);
                } else {
                    msg["content"] = Value::Null;
                }

                if !tool_calls.is_empty() {
                    msg["tool_calls"] = json!(tool_calls);
                }

                messages.push(msg);
            }
        }

        result["messages"] = json!(messages);

        // 转换参数
        if let Some(gen_config) = body.get("generationConfig") {
            if let Some(v) = gen_config.get("maxOutputTokens") {
                result["max_tokens"] = v.clone();
            }
            if let Some(v) = gen_config.get("temperature") {
                result["temperature"] = v.clone();
            }
            if let Some(v) = gen_config.get("topP") {
                result["top_p"] = v.clone();
            }
        }

        Ok(result)
    }

    // ========== 响应转换 ==========

    fn openai_to_anthropic_response(body: Value, _config: &ProtocolConfig) -> Result<Value, ProxyError> {
        let choices = body
            .get("choices")
            .and_then(|c| c.as_array())
            .ok_or_else(|| ProxyError::TransformError("No choices in response".to_string()))?;

        let choice = choices
            .first()
            .ok_or_else(|| ProxyError::TransformError("Empty choices array".to_string()))?;

        let message = choice
            .get("message")
            .ok_or_else(|| ProxyError::TransformError("No message in choice".to_string()))?;

        let mut content = Vec::new();

        // 文本内容
        if let Some(text) = message.get("content").and_then(|c| c.as_str()) {
            if !text.is_empty() {
                content.push(json!({"type": "text", "text": text}));
            }
        }

        // 工具调用
        if let Some(tool_calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tool_calls {
                let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                let func = tc.get("function");
                let name = func
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let args_str = func
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}");
                let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));

                content.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": input
                }));
            }
        }

        // 映射 finish_reason → stop_reason
        let stop_reason = choice
            .get("finish_reason")
            .and_then(|r| r.as_str())
            .map(|r| match r {
                "stop" => "end_turn",
                "length" => "max_tokens",
                "tool_calls" => "tool_use",
                other => other,
            });

        // usage
        let usage = body.get("usage").cloned().unwrap_or(json!({}));
        let input_tokens = usage
            .get("prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let output_tokens = usage
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok(json!({
            "id": body.get("id").and_then(|i| i.as_str()).unwrap_or(""),
            "type": "message",
            "role": "assistant",
            "content": content,
            "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(""),
            "stop_reason": stop_reason,
            "stop_sequence": null,
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": output_tokens
            }
        }))
    }

    fn gemini_to_anthropic_response(body: Value, _config: &ProtocolConfig) -> Result<Value, ProxyError> {
        let candidates = body
            .get("candidates")
            .and_then(|c| c.as_array())
            .ok_or_else(|| ProxyError::TransformError("No candidates in response".to_string()))?;

        let candidate = candidates
            .first()
            .ok_or_else(|| ProxyError::TransformError("Empty candidates array".to_string()))?;

        let gemini_content = candidate
            .get("content")
            .ok_or_else(|| ProxyError::TransformError("No content in candidate".to_string()))?;

        let mut content = Vec::new();

        if let Some(parts) = gemini_content.get("parts").and_then(|p| p.as_array()) {
            for part in parts {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    let is_thought = part.get("thought").and_then(|t| t.as_bool()).unwrap_or(false);
                    if is_thought {
                        content.push(json!({
                            "type": "thinking",
                            "thinking": text
                        }));
                    } else {
                        content.push(json!({
                            "type": "text",
                            "text": text
                        }));
                    }
                }

                if let Some(func_call) = part.get("functionCall") {
                    let name = func_call
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("");
                    let args = func_call.get("args").cloned().unwrap_or(json!({}));
                    content.push(json!({
                        "type": "tool_use",
                        "id": format!("call_{}", uuid::Uuid::new_v4()),
                        "name": name,
                        "input": args
                    }));
                }
            }
        }

        // 映射 finishReason
        let stop_reason = candidate
            .get("finishReason")
            .and_then(|r| r.as_str())
            .map(|r| match r {
                "STOP" => "end_turn",
                "MAX_TOKENS" => "max_tokens",
                "SAFETY" => "end_turn",
                other => other,
            });

        // usage
        let usage_metadata = body.get("usageMetadata").cloned().unwrap_or(json!({}));
        let input_tokens = usage_metadata
            .get("promptTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let output_tokens = usage_metadata
            .get("candidatesTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok(json!({
            "id": format!("msg_{}", uuid::Uuid::new_v4()),
            "type": "message",
            "role": "assistant",
            "content": content,
            "model": body.get("modelVersion").and_then(|m| m.as_str()).unwrap_or(""),
            "stop_reason": stop_reason,
            "stop_sequence": null,
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": output_tokens
            }
        }))
    }

    fn anthropic_to_openai_response(body: Value, _config: &ProtocolConfig) -> Result<Value, ProxyError> {
        let content = body.get("content").and_then(|c| c.as_array());

        let mut text_content = String::new();
        let mut tool_calls = Vec::new();

        if let Some(blocks) = content {
            for block in blocks {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            if !text_content.is_empty() {
                                text_content.push('\n');
                            }
                            text_content.push_str(text);
                        }
                    }
                    "tool_use" => {
                        let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let input = block.get("input").cloned().unwrap_or(json!({}));
                        tool_calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(&input).unwrap_or_default()
                            }
                        }));
                    }
                    _ => {}
                }
            }
        }

        // 映射 stop_reason → finish_reason
        let finish_reason = body
            .get("stop_reason")
            .and_then(|r| r.as_str())
            .map(|r| match r {
                "end_turn" => "stop",
                "max_tokens" => "length",
                "tool_use" => "tool_calls",
                other => other,
            });

        // usage
        let usage = body.get("usage").cloned().unwrap_or(json!({}));
        let prompt_tokens = usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let completion_tokens = usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let mut message = json!({
            "role": "assistant",
            "content": if text_content.is_empty() { Value::Null } else { json!(text_content) }
        });

        if !tool_calls.is_empty() {
            message["tool_calls"] = json!(tool_calls);
        }

        Ok(json!({
            "id": body.get("id").and_then(|i| i.as_str()).unwrap_or(""),
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(""),
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": finish_reason
            }],
            "usage": {
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
                "total_tokens": prompt_tokens + completion_tokens
            }
        }))
    }

    // ========== Anthropic ↔ OpenAI Responses ==========

    /// Anthropic 请求 → OpenAI Responses 请求
    fn anthropic_to_openai_responses(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});

        // 模型
        if let Some(model) = body.get("model") {
            result["model"] = model.clone();
        }

        // system → instructions
        if let Some(system) = body.get("system") {
            if let Some(text) = system.as_str() {
                result["instructions"] = json!(text);
            } else if let Some(arr) = system.as_array() {
                let text: String = arr
                    .iter()
                    .filter_map(|m| m.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    result["instructions"] = json!(text);
                }
            }
        }

        // messages → input
        let mut input = Vec::new();
        if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
            for msg in msgs {
                let converted = Self::convert_anthropic_message_to_responses(msg)?;
                input.extend(converted);
            }
        }
        result["input"] = json!(input);

        // 复制参数
        if let Some(v) = body.get("max_tokens") {
            result["max_output_tokens"] = v.clone();
        }
        if let Some(v) = body.get("temperature") {
            result["temperature"] = v.clone();
        }
        if let Some(v) = body.get("top_p") {
            result["top_p"] = v.clone();
        }

        // 转换 tools
        if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
            let responses_tools: Vec<Value> = tools
                .iter()
                .filter(|t| t.get("type").and_then(|v| v.as_str()) != Some("BatchTool"))
                .map(|t| {
                    json!({
                        "type": "function",
                        "name": t.get("name"),
                        "description": t.get("description"),
                        "parameters": crate::proxy::providers::transform::clean_schema(
                            t.get("input_schema").cloned().unwrap_or(json!({}))
                        )
                    })
                })
                .collect();

            if !responses_tools.is_empty() {
                result["tools"] = json!(responses_tools);
            }
        }

        Ok(result)
    }

    fn convert_anthropic_message_to_responses(msg: &Value) -> Result<Vec<Value>, ProxyError> {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = msg.get("content");
        let mut result = Vec::new();

        let content = match content {
            Some(c) => c,
            None => return Ok(result),
        };

        // 字符串内容
        if let Some(text) = content.as_str() {
            let responses_role = if role == "assistant" { "assistant" } else { "user" };
            let content_type = if role == "assistant" { "output_text" } else { "input_text" };
            result.push(json!({
                "role": responses_role,
                "type": "message",
                "content": [{ "type": content_type, "text": text }]
            }));
            return Ok(result);
        }

        // 数组内容
        if let Some(blocks) = content.as_array() {
            let mut content_items = Vec::new();
            let responses_role = if role == "assistant" { "assistant" } else { "user" };

            for block in blocks {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            let content_type = if role == "assistant" { "output_text" } else { "input_text" };
                            content_items.push(json!({ "type": content_type, "text": text }));
                        }
                    }
                    "image" => {
                        if let Some(source) = block.get("source") {
                            let media_type = source
                                .get("media_type")
                                .and_then(|m| m.as_str())
                                .unwrap_or("image/png");
                            let data = source.get("data").and_then(|d| d.as_str()).unwrap_or("");
                            content_items.push(json!({
                                "type": "input_image",
                                "image_url": format!("data:{};base64,{}", media_type, data)
                            }));
                        }
                    }
                    "tool_use" => {
                        // tool_use 在 Responses API 中由模型输出，不在 input 中
                        // 跳过
                    }
                    "tool_result" => {
                        let tool_use_id = block
                            .get("tool_use_id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("");
                        let content_val = block.get("content");
                        let content_str = match content_val {
                            Some(Value::String(s)) => s.clone(),
                            Some(v) => serde_json::to_string(v).unwrap_or_default(),
                            None => String::new(),
                        };
                        result.push(json!({
                            "type": "function_call_output",
                            "call_id": tool_use_id,
                            "output": content_str
                        }));
                    }
                    "thinking" => {
                        // Responses API 支持 reasoning，但格式不同，暂跳过
                    }
                    _ => {}
                }
            }

            if !content_items.is_empty() {
                result.push(json!({
                    "role": responses_role,
                    "type": "message",
                    "content": content_items
                }));
            }
        }

        Ok(result)
    }

    /// OpenAI Responses 请求 → Anthropic 请求
    fn openai_responses_to_anthropic(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});

        // 模型
        if let Some(model) = body.get("model") {
            result["model"] = model.clone();
        }

        // instructions → system
        if let Some(instructions) = body.get("instructions").and_then(|i| i.as_str()) {
            result["system"] = json!(instructions);
        }

        // input → messages
        let mut messages = Vec::new();
        if let Some(input) = body.get("input").and_then(|i| i.as_array()) {
            for item in input {
                let converted = Self::convert_responses_input_to_anthropic(item)?;
                if let Some(msg) = converted {
                    messages.push(msg);
                }
            }
        }
        result["messages"] = json!(messages);

        // 参数转换
        if let Some(v) = body.get("max_output_tokens") {
            result["max_tokens"] = v.clone();
        } else {
            result["max_tokens"] = json!(4096);
        }
        if let Some(v) = body.get("temperature") {
            result["temperature"] = v.clone();
        }
        if let Some(v) = body.get("top_p") {
            result["top_p"] = v.clone();
        }

        // 转换 tools
        if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
            let anthropic_tools: Vec<Value> = tools
                .iter()
                .filter_map(|t| {
                    Some(json!({
                        "name": t.get("name"),
                        "description": t.get("description"),
                        "input_schema": t.get("parameters")
                    }))
                })
                .collect();

            if !anthropic_tools.is_empty() {
                result["tools"] = json!(anthropic_tools);
            }
        }

        Ok(result)
    }

    fn convert_responses_input_to_anthropic(item: &Value) -> Result<Option<Value>, ProxyError> {
        // 检查是否是 function_call_output
        if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
            if item_type == "function_call_output" {
                let call_id = item.get("call_id").and_then(|i| i.as_str()).unwrap_or("");
                let output = item.get("output").and_then(|o| o.as_str()).unwrap_or("");
                return Ok(Some(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": call_id,
                        "content": output
                    }]
                })));
            }
        }

        // Message 类型
        let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let anthropic_role = if role == "assistant" { "assistant" } else { "user" };

        let mut content = Vec::new();

        if let Some(items) = item.get("content").and_then(|c| c.as_array()) {
            for c in items {
                let content_type = c.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match content_type {
                    "input_text" | "output_text" => {
                        if let Some(text) = c.get("text").and_then(|t| t.as_str()) {
                            content.push(json!({"type": "text", "text": text}));
                        }
                    }
                    "input_image" => {
                        if let Some(url) = c.get("image_url").and_then(|u| u.as_str()) {
                            if let Some((media_type, data)) = Self::parse_data_url(url) {
                                content.push(json!({
                                    "type": "image",
                                    "source": {
                                        "type": "base64",
                                        "media_type": media_type,
                                        "data": data
                                    }
                                }));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if content.is_empty() {
            return Ok(None);
        }

        Ok(Some(json!({
            "role": anthropic_role,
            "content": content
        })))
    }

    // ========== OpenAI Chat ↔ OpenAI Responses ==========

    /// OpenAI Chat 请求 → OpenAI Responses 请求
    fn openai_chat_to_responses(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});

        // 模型
        if let Some(model) = body.get("model") {
            result["model"] = model.clone();
        }

        // messages → input + instructions
        let mut input = Vec::new();
        if let Some(msgs) = body.get("messages").and_then(|m| m.as_array()) {
            for msg in msgs {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");

                if role == "system" {
                    // system message → instructions
                    if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                        result["instructions"] = json!(content);
                    }
                } else if role == "tool" {
                    // tool message → function_call_output
                    let tool_call_id = msg.get("tool_call_id").and_then(|i| i.as_str()).unwrap_or("");
                    let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": tool_call_id,
                        "output": content
                    }));
                } else {
                    // user/assistant message
                    let converted = Self::convert_openai_message_to_responses(msg)?;
                    if let Some(m) = converted {
                        input.push(m);
                    }
                }
            }
        }
        result["input"] = json!(input);

        // 参数
        if let Some(v) = body.get("max_tokens") {
            result["max_output_tokens"] = v.clone();
        }
        if let Some(v) = body.get("temperature") {
            result["temperature"] = v.clone();
        }
        if let Some(v) = body.get("top_p") {
            result["top_p"] = v.clone();
        }

        // 转换 tools
        if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
            let responses_tools: Vec<Value> = tools
                .iter()
                .filter_map(|t| {
                    let func = t.get("function")?;
                    Some(json!({
                        "type": "function",
                        "name": func.get("name"),
                        "description": func.get("description"),
                        "parameters": func.get("parameters")
                    }))
                })
                .collect();

            if !responses_tools.is_empty() {
                result["tools"] = json!(responses_tools);
            }
        }

        Ok(result)
    }

    fn convert_openai_message_to_responses(msg: &Value) -> Result<Option<Value>, ProxyError> {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let responses_role = if role == "assistant" { "assistant" } else { "user" };

        let mut content_items = Vec::new();

        // 处理 content
        if let Some(content) = msg.get("content") {
            if let Some(text) = content.as_str() {
                let content_type = if role == "assistant" { "output_text" } else { "input_text" };
                content_items.push(json!({ "type": content_type, "text": text }));
            } else if let Some(parts) = content.as_array() {
                for part in parts {
                    let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match part_type {
                        "text" => {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                let content_type = if role == "assistant" { "output_text" } else { "input_text" };
                                content_items.push(json!({ "type": content_type, "text": text }));
                            }
                        }
                        "image_url" => {
                            if let Some(url) = part
                                .get("image_url")
                                .and_then(|u| u.get("url"))
                                .and_then(|u| u.as_str())
                            {
                                content_items.push(json!({
                                    "type": "input_image",
                                    "image_url": url
                                }));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // 注意：tool_calls 在 Responses API 中由模型输出，不在 input 中传递

        if content_items.is_empty() {
            return Ok(None);
        }

        Ok(Some(json!({
            "role": responses_role,
            "type": "message",
            "content": content_items
        })))
    }

    /// OpenAI Responses 请求 → OpenAI Chat 请求
    fn openai_responses_to_chat(
        body: Value,
        _provider: &Provider,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let mut result = json!({});

        // 模型
        if let Some(model) = body.get("model") {
            result["model"] = model.clone();
        }

        let mut messages = Vec::new();

        // instructions → system message
        if let Some(instructions) = body.get("instructions").and_then(|i| i.as_str()) {
            messages.push(json!({"role": "system", "content": instructions}));
        }

        // input → messages
        if let Some(input) = body.get("input").and_then(|i| i.as_array()) {
            for item in input {
                // function_call_output → tool message
                if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                    if item_type == "function_call_output" {
                        let call_id = item.get("call_id").and_then(|i| i.as_str()).unwrap_or("");
                        let output = item.get("output").and_then(|o| o.as_str()).unwrap_or("");
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": output
                        }));
                        continue;
                    }
                }

                // Message → user/assistant message
                let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                let mut content_parts = Vec::new();

                if let Some(contents) = item.get("content").and_then(|c| c.as_array()) {
                    for c in contents {
                        let content_type = c.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match content_type {
                            "input_text" | "output_text" => {
                                if let Some(text) = c.get("text").and_then(|t| t.as_str()) {
                                    content_parts.push(json!({"type": "text", "text": text}));
                                }
                            }
                            "input_image" => {
                                if let Some(url) = c.get("image_url").and_then(|u| u.as_str()) {
                                    content_parts.push(json!({
                                        "type": "image_url",
                                        "image_url": {"url": url}
                                    }));
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if !content_parts.is_empty() {
                    let mut msg = json!({"role": role});
                    if content_parts.len() == 1 {
                        if let Some(text) = content_parts[0].get("text") {
                            msg["content"] = text.clone();
                        } else {
                            msg["content"] = json!(content_parts);
                        }
                    } else {
                        msg["content"] = json!(content_parts);
                    }
                    messages.push(msg);
                }
            }
        }

        result["messages"] = json!(messages);

        // 参数
        if let Some(v) = body.get("max_output_tokens") {
            result["max_tokens"] = v.clone();
        }
        if let Some(v) = body.get("temperature") {
            result["temperature"] = v.clone();
        }
        if let Some(v) = body.get("top_p") {
            result["top_p"] = v.clone();
        }

        // 转换 tools
        if let Some(tools) = body.get("tools").and_then(|t| t.as_array()) {
            let chat_tools: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.get("name"),
                            "description": t.get("description"),
                            "parameters": t.get("parameters")
                        }
                    })
                })
                .collect();

            if !chat_tools.is_empty() {
                result["tools"] = json!(chat_tools);
            }
        }

        Ok(result)
    }

    // ========== 响应转换：OpenAI Responses ==========

    /// OpenAI Responses 响应 → Anthropic 响应
    fn openai_responses_to_anthropic_response(
        body: Value,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let output = body
            .get("output")
            .and_then(|o| o.as_array())
            .ok_or_else(|| ProxyError::TransformError("No output in response".to_string()))?;

        let mut content = Vec::new();

        for item in output {
            let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match item_type {
                "message" => {
                    if let Some(contents) = item.get("content").and_then(|c| c.as_array()) {
                        for c in contents {
                            let content_type = c.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            if content_type == "output_text" {
                                if let Some(text) = c.get("text").and_then(|t| t.as_str()) {
                                    content.push(json!({"type": "text", "text": text}));
                                }
                            }
                        }
                    }
                }
                "function_call" => {
                    let id = item.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let arguments = item.get("arguments").cloned().unwrap_or(json!({}));
                    content.push(json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": arguments
                    }));
                }
                "reasoning" => {
                    // 提取 reasoning summary
                    if let Some(summaries) = item.get("summary").and_then(|s| s.as_array()) {
                        let text: String = summaries
                            .iter()
                            .filter_map(|s| s.get("text").and_then(|t| t.as_str()))
                            .collect::<Vec<_>>()
                            .join("\n");
                        if !text.is_empty() {
                            content.push(json!({
                                "type": "thinking",
                                "thinking": text
                            }));
                        }
                    }
                }
                _ => {}
            }
        }

        // 映射 status → stop_reason
        let stop_reason = body
            .get("status")
            .and_then(|s| s.as_str())
            .map(|s| match s {
                "completed" => "end_turn",
                "incomplete" => "max_tokens",
                "failed" => "end_turn",
                other => other,
            });

        // usage
        let usage = body.get("usage").cloned().unwrap_or(json!({}));
        let input_tokens = usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let output_tokens = usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok(json!({
            "id": body.get("id").and_then(|i| i.as_str()).unwrap_or(""),
            "type": "message",
            "role": "assistant",
            "content": content,
            "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(""),
            "stop_reason": stop_reason,
            "stop_sequence": null,
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": output_tokens
            }
        }))
    }

    /// Anthropic 响应 → OpenAI Responses 响应
    fn anthropic_to_openai_responses_response(
        body: Value,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let content = body.get("content").and_then(|c| c.as_array());
        let mut output = Vec::new();

        let mut message_content = Vec::new();

        if let Some(blocks) = content {
            for block in blocks {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            message_content.push(json!({"type": "output_text", "text": text}));
                        }
                    }
                    "tool_use" => {
                        let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let input = block.get("input").cloned().unwrap_or(json!({}));
                        output.push(json!({
                            "type": "function_call",
                            "id": id,
                            "call_id": id,
                            "name": name,
                            "arguments": input,
                            "status": "completed"
                        }));
                    }
                    "thinking" => {
                        if let Some(thinking) = block.get("thinking").and_then(|t| t.as_str()) {
                            output.push(json!({
                                "type": "reasoning",
                                "id": format!("rs_{}", uuid::Uuid::new_v4()),
                                "summary": [{"type": "summary_text", "text": thinking}]
                            }));
                        }
                    }
                    _ => {}
                }
            }
        }

        // 添加 message output
        if !message_content.is_empty() {
            output.insert(0, json!({
                "type": "message",
                "id": format!("msg_{}", uuid::Uuid::new_v4()),
                "role": "assistant",
                "status": "completed",
                "content": message_content
            }));
        }

        // 映射 stop_reason → status
        let status = body
            .get("stop_reason")
            .and_then(|r| r.as_str())
            .map(|r| match r {
                "end_turn" => "completed",
                "max_tokens" => "incomplete",
                "tool_use" => "completed",
                _ => "completed",
            })
            .unwrap_or("completed");

        // usage
        let usage = body.get("usage").cloned().unwrap_or(json!({}));
        let input_tokens = usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output_tokens = usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok(json!({
            "id": body.get("id").and_then(|i| i.as_str()).unwrap_or(""),
            "object": "response",
            "created_at": chrono::Utc::now().timestamp(),
            "status": status,
            "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(""),
            "output": output,
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "total_tokens": input_tokens + output_tokens
            }
        }))
    }

    /// OpenAI Chat 响应 → OpenAI Responses 响应
    fn openai_chat_to_responses_response(
        body: Value,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let choices = body
            .get("choices")
            .and_then(|c| c.as_array())
            .ok_or_else(|| ProxyError::TransformError("No choices in response".to_string()))?;

        let choice = choices
            .first()
            .ok_or_else(|| ProxyError::TransformError("Empty choices array".to_string()))?;

        let message = choice
            .get("message")
            .ok_or_else(|| ProxyError::TransformError("No message in choice".to_string()))?;

        let mut output = Vec::new();
        let mut message_content = Vec::new();

        // 文本内容
        if let Some(text) = message.get("content").and_then(|c| c.as_str()) {
            if !text.is_empty() {
                message_content.push(json!({"type": "output_text", "text": text}));
            }
        }

        // 添加 message output
        if !message_content.is_empty() {
            output.push(json!({
                "type": "message",
                "id": format!("msg_{}", uuid::Uuid::new_v4()),
                "role": "assistant",
                "status": "completed",
                "content": message_content
            }));
        }

        // 工具调用
        if let Some(tool_calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tool_calls {
                let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                let func = tc.get("function");
                let name = func
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let args_str = func
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}");
                let arguments: Value = serde_json::from_str(args_str).unwrap_or(json!({}));

                output.push(json!({
                    "type": "function_call",
                    "id": id,
                    "call_id": id,
                    "name": name,
                    "arguments": arguments,
                    "status": "completed"
                }));
            }
        }

        // 映射 finish_reason → status
        let status = choice
            .get("finish_reason")
            .and_then(|r| r.as_str())
            .map(|r| match r {
                "stop" => "completed",
                "length" => "incomplete",
                "tool_calls" => "completed",
                _ => "completed",
            })
            .unwrap_or("completed");

        // usage
        let usage = body.get("usage").cloned().unwrap_or(json!({}));
        let prompt_tokens = usage
            .get("prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let completion_tokens = usage
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok(json!({
            "id": body.get("id").and_then(|i| i.as_str()).unwrap_or(""),
            "object": "response",
            "created_at": chrono::Utc::now().timestamp(),
            "status": status,
            "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(""),
            "output": output,
            "usage": {
                "input_tokens": prompt_tokens,
                "output_tokens": completion_tokens,
                "total_tokens": prompt_tokens + completion_tokens
            }
        }))
    }

    /// OpenAI Responses 响应 → OpenAI Chat 响应
    fn openai_responses_to_chat_response(
        body: Value,
        _config: &ProtocolConfig,
    ) -> Result<Value, ProxyError> {
        let output = body
            .get("output")
            .and_then(|o| o.as_array())
            .ok_or_else(|| ProxyError::TransformError("No output in response".to_string()))?;

        let mut text_content = String::new();
        let mut tool_calls = Vec::new();

        for item in output {
            let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match item_type {
                "message" => {
                    if let Some(contents) = item.get("content").and_then(|c| c.as_array()) {
                        for c in contents {
                            let content_type = c.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            if content_type == "output_text" {
                                if let Some(text) = c.get("text").and_then(|t| t.as_str()) {
                                    if !text_content.is_empty() {
                                        text_content.push('\n');
                                    }
                                    text_content.push_str(text);
                                }
                            }
                        }
                    }
                }
                "function_call" => {
                    let id = item.get("call_id").and_then(|i| i.as_str()).unwrap_or("");
                    let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let arguments = item.get("arguments").cloned().unwrap_or(json!({}));
                    tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(&arguments).unwrap_or_default()
                        }
                    }));
                }
                "reasoning" => {
                    // OpenAI Chat 不支持 reasoning，跳过
                }
                _ => {}
            }
        }

        // 映射 status → finish_reason
        let finish_reason = body
            .get("status")
            .and_then(|s| s.as_str())
            .map(|s| match s {
                "completed" => if tool_calls.is_empty() { "stop" } else { "tool_calls" },
                "incomplete" => "length",
                "failed" => "stop",
                _ => "stop",
            });

        // usage
        let usage = body.get("usage").cloned().unwrap_or(json!({}));
        let input_tokens = usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output_tokens = usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let mut message = json!({
            "role": "assistant",
            "content": if text_content.is_empty() { Value::Null } else { json!(text_content) }
        });

        if !tool_calls.is_empty() {
            message["tool_calls"] = json!(tool_calls);
        }

        Ok(json!({
            "id": body.get("id").and_then(|i| i.as_str()).unwrap_or(""),
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": body.get("model").and_then(|m| m.as_str()).unwrap_or(""),
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": finish_reason
            }],
            "usage": {
                "prompt_tokens": input_tokens,
                "completion_tokens": output_tokens,
                "total_tokens": input_tokens + output_tokens
            }
        }))
    }

    // ========== 辅助方法 ==========

    // 注意：模型映射已统一移至 model_mapper.rs
    // 本模块不再执行模型映射，只负责格式转换

    fn copy_common_params(source: &Value, target: &mut Value) {
        for key in &["max_tokens", "temperature", "top_p", "stream", "tool_choice"] {
            if let Some(v) = source.get(key) {
                target[key] = v.clone();
            }
        }

        // 转换 stop_sequences → stop
        if let Some(v) = source.get("stop_sequences") {
            target["stop"] = v.clone();
        }
    }

    fn extract_text_content(msg: &Value) -> String {
        if let Some(content) = msg.get("content") {
            if let Some(text) = content.as_str() {
                return text.to_string();
            }
            if let Some(blocks) = content.as_array() {
                return blocks
                    .iter()
                    .filter_map(|b| {
                        if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                            b.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
        String::new()
    }

    fn parse_data_url(url: &str) -> Option<(String, String)> {
        if !url.starts_with("data:") {
            return None;
        }

        let rest = &url[5..];
        let parts: Vec<&str> = rest.splitn(2, ',').collect();
        if parts.len() != 2 {
            return None;
        }

        let meta = parts[0];
        let data = parts[1];

        let media_type = if meta.contains(';') {
            meta.split(';').next().unwrap_or("image/png")
        } else {
            meta
        };

        Some((media_type.to_string(), data.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_provider() -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Provider".to_string(),
            settings_config: json!({}),
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn test_anthropic_to_openai() {
        let provider = create_test_provider();
        let config = ProtocolConfig::transform(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIChat);

        let input = json!({
            "model": "claude-3-opus",
            "max_tokens": 1024,
            "system": "You are a helpful assistant.",
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        });

        let result = ProtocolConverter::convert_request(input, &config, &provider).unwrap();

        assert!(result.get("messages").is_some());
        assert_eq!(result["messages"][0]["role"], "system");
        assert_eq!(result["messages"][1]["role"], "user");
    }

    #[test]
    fn test_openai_to_anthropic_response() {
        let config = ProtocolConfig::transform(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIChat);

        let input = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5
            }
        });

        let result = ProtocolConverter::convert_response(input, &config).unwrap();

        assert_eq!(result["type"], "message");
        assert_eq!(result["role"], "assistant");
        assert_eq!(result["stop_reason"], "end_turn");
    }

    #[test]
    fn test_passthrough() {
        let provider = create_test_provider();
        let config = ProtocolConfig::passthrough(ProtocolFormat::Anthropic);

        let input = json!({
            "model": "claude-3-opus",
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let result = ProtocolConverter::convert_request(input.clone(), &config, &provider).unwrap();

        assert_eq!(result, input);
    }

    #[test]
    fn test_parse_data_url() {
        let url = "data:image/png;base64,iVBORw0KGgo=";
        let (media_type, data) = ProtocolConverter::parse_data_url(url).unwrap();
        assert_eq!(media_type, "image/png");
        assert_eq!(data, "iVBORw0KGgo=");
    }

    // ========== OpenAI Responses API 测试 ==========

    #[test]
    fn test_anthropic_to_openai_responses() {
        let provider = create_test_provider();
        let config =
            ProtocolConfig::transform(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIResponses);

        let input = json!({
            "model": "claude-3-opus",
            "max_tokens": 1024,
            "system": "You are a helpful assistant.",
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        });

        let result = ProtocolConverter::convert_request(input, &config, &provider).unwrap();

        assert_eq!(result["instructions"], "You are a helpful assistant.");
        assert!(result.get("input").is_some());
        assert_eq!(result["max_output_tokens"], 1024);
    }

    #[test]
    fn test_openai_responses_to_anthropic() {
        let provider = create_test_provider();
        let config =
            ProtocolConfig::transform(ProtocolFormat::OpenAIResponses, ProtocolFormat::Anthropic);

        let input = json!({
            "model": "gpt-4o",
            "instructions": "You are a helpful assistant.",
            "input": [
                {
                    "role": "user",
                    "type": "message",
                    "content": [{"type": "input_text", "text": "Hello"}]
                }
            ],
            "max_output_tokens": 1024
        });

        let result = ProtocolConverter::convert_request(input, &config, &provider).unwrap();

        assert_eq!(result["system"], "You are a helpful assistant.");
        assert!(result.get("messages").is_some());
        assert_eq!(result["max_tokens"], 1024);
    }

    #[test]
    fn test_openai_chat_to_responses() {
        let provider = create_test_provider();
        let config =
            ProtocolConfig::transform(ProtocolFormat::OpenAIChat, ProtocolFormat::OpenAIResponses);

        let input = json!({
            "model": "gpt-4",
            "max_tokens": 1024,
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Hello"}
            ]
        });

        let result = ProtocolConverter::convert_request(input, &config, &provider).unwrap();

        assert_eq!(result["instructions"], "You are a helpful assistant.");
        assert!(result.get("input").is_some());
        let input_arr = result["input"].as_array().unwrap();
        assert_eq!(input_arr.len(), 1); // Only user message, system is in instructions
    }

    #[test]
    fn test_openai_responses_to_chat() {
        let provider = create_test_provider();
        let config =
            ProtocolConfig::transform(ProtocolFormat::OpenAIResponses, ProtocolFormat::OpenAIChat);

        let input = json!({
            "model": "gpt-4o",
            "instructions": "You are a helpful assistant.",
            "input": [
                {
                    "role": "user",
                    "type": "message",
                    "content": [{"type": "input_text", "text": "Hello"}]
                }
            ],
            "max_output_tokens": 1024
        });

        let result = ProtocolConverter::convert_request(input, &config, &provider).unwrap();

        assert!(result.get("messages").is_some());
        let messages = result["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are a helpful assistant.");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(result["max_tokens"], 1024);
    }

    #[test]
    fn test_openai_responses_to_anthropic_response() {
        let config =
            ProtocolConfig::transform(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIResponses);

        let input = json!({
            "id": "resp_123",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [
                {
                    "type": "message",
                    "id": "msg_123",
                    "role": "assistant",
                    "status": "completed",
                    "content": [{"type": "output_text", "text": "Hello!"}]
                }
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "total_tokens": 15
            }
        });

        let result = ProtocolConverter::convert_response(input, &config).unwrap();

        assert_eq!(result["type"], "message");
        assert_eq!(result["role"], "assistant");
        assert_eq!(result["stop_reason"], "end_turn");
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(result["content"][0]["text"], "Hello!");
    }

    #[test]
    fn test_anthropic_to_openai_responses_response() {
        let config =
            ProtocolConfig::transform(ProtocolFormat::OpenAIResponses, ProtocolFormat::Anthropic);

        let input = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Hello!"}],
            "model": "claude-3-opus",
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5
            }
        });

        let result = ProtocolConverter::convert_response(input, &config).unwrap();

        assert_eq!(result["object"], "response");
        assert_eq!(result["status"], "completed");
        assert!(result.get("output").is_some());
        let output = result["output"].as_array().unwrap();
        assert!(!output.is_empty());
    }

    #[test]
    fn test_openai_responses_with_function_call() {
        let config =
            ProtocolConfig::transform(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIResponses);

        let input = json!({
            "id": "resp_123",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [
                {
                    "type": "function_call",
                    "id": "fc_123",
                    "call_id": "call_123",
                    "name": "search",
                    "arguments": {"query": "rust programming"},
                    "status": "completed"
                }
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20,
                "total_tokens": 30
            }
        });

        let result = ProtocolConverter::convert_response(input, &config).unwrap();

        assert_eq!(result["type"], "message");
        assert_eq!(result["content"][0]["type"], "tool_use");
        assert_eq!(result["content"][0]["name"], "search");
    }

    #[test]
    fn test_openai_responses_with_reasoning() {
        let config =
            ProtocolConfig::transform(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIResponses);

        let input = json!({
            "id": "resp_123",
            "object": "response",
            "status": "completed",
            "model": "o1-preview",
            "output": [
                {
                    "type": "reasoning",
                    "id": "rs_123",
                    "summary": [{"type": "summary_text", "text": "Let me think about this..."}]
                },
                {
                    "type": "message",
                    "id": "msg_123",
                    "role": "assistant",
                    "status": "completed",
                    "content": [{"type": "output_text", "text": "The answer is 42."}]
                }
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 50,
                "total_tokens": 60
            }
        });

        let result = ProtocolConverter::convert_response(input, &config).unwrap();

        assert_eq!(result["type"], "message");
        // 检查是否包含 thinking 和 text
        let content = result["content"].as_array().unwrap();
        assert!(content.iter().any(|c| c["type"] == "thinking"));
        assert!(content.iter().any(|c| c["type"] == "text"));
    }

    #[test]
    fn test_function_call_output_conversion() {
        let provider = create_test_provider();
        let config =
            ProtocolConfig::transform(ProtocolFormat::Anthropic, ProtocolFormat::OpenAIResponses);

        let input = json!({
            "model": "claude-3-opus",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "Search for rust"},
                {
                    "role": "assistant",
                    "content": [
                        {
                            "type": "tool_use",
                            "id": "call_123",
                            "name": "search",
                            "input": {"query": "rust"}
                        }
                    ]
                },
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": "call_123",
                            "content": "Found: Rust is a programming language"
                        }
                    ]
                }
            ]
        });

        let result = ProtocolConverter::convert_request(input, &config, &provider).unwrap();

        let input_arr = result["input"].as_array().unwrap();
        // 应该有 function_call_output
        assert!(input_arr
            .iter()
            .any(|i| i["type"] == "function_call_output"));
    }
}
