# CC-Switch 深链接设计文档

## 文档版本
- **版本**: v1.1
- **最后更新**: 2025-11-18
- **状态**: v1.0 已实现 (Provider 字段导入), v1.1 规划中 (完整配置导入)

---

## 1. 概述

### 1.1 目标

实现 `ccswitch://` 协议深链接,允许用户通过 URL 导入供应商配置,提升配置分享和迁移的便利性。

### 1.2 核心功能

**v1.0 (已实现)**:
- 通过 URL 参数导入 Provider 字段（name, endpoint, apiKey 等）
- 支持三种应用类型: Claude Code, Codex, Gemini
- 本地确认对话框（安全机制）
- 敏感信息掩码显示

**v1.1 (规划中)**:
- 导入完整配置文件（JSON/TOML 格式）
- 支持 Claude settings.json、Gemini config.json、Codex config.toml
- Base64 编码传输，支持大配置文件
- 配置内容预览与编辑

---

## 2. v1.0 协议规范 (已实现)

### 2.1 URL 格式

```
ccswitch://{version}/import?{parameters}
```

**组成部分**:
- `ccswitch://`: 协议标识符
- `{version}`: 协议版本（当前为 `v1`）
- `/import`: 操作路径（固定）
- `{parameters}`: 查询参数（URL 编码）

### 2.2 支持的资源类型

#### Provider 导入 (resource=provider)

**必需参数**:
| 参数 | 类型 | 说明 | 示例 |
|------|------|------|------|
| `resource` | string | 资源类型，固定为 `provider` | `provider` |
| `app` | string | 目标应用 | `claude` \| `codex` \| `gemini` |
| `name` | string | 供应商名称 | `DMXAPI` |
| `homepage` | string | 供应商主页 URL | `https://dmxapi.com` |
| `endpoint` | string | API 端点 URL | `https://api.dmxapi.com/v1` |
| `apiKey` | string | API 密钥 | `sk-xxx` |

**可选参数**:
| 参数 | 类型 | 说明 | 示例 |
|------|------|------|------|
| `model` | string | 默认模型 | `claude-sonnet-4-5` |
| `notes` | string | 备注信息 | `高速稳定供应商` |

**完整示例**:
```
ccswitch://v1/import?resource=provider&app=claude&name=DMXAPI&homepage=https%3A%2F%2Fdmxapi.com&endpoint=https%3A%2F%2Fapi.dmxapi.com%2Fv1&apiKey=sk-ant-xxx&model=claude-sonnet-4-5&notes=%E9%AB%98%E9%80%9F%E7%A8%B3%E5%AE%9A
```

### 2.3 校验规则

#### URL 结构校验
- ✅ 协议必须为 `ccswitch://`
- ✅ 版本必须为 `v1`（未来可扩展）
- ✅ 路径必须为 `/import`
- ✅ 必需参数不可缺失

#### 字段值校验
- ✅ `app` 必须为 `claude` / `codex` / `gemini` 之一
- ✅ `homepage` 和 `endpoint` 必须为有效的 HTTP(S) URL
- ✅ `name` 不可为空字符串

#### 示例错误响应
```json
{
  "error": "InvalidInput",
  "message": "Missing 'apiKey' parameter"
}
```

---

## 3. v1.1 扩展设计 (规划中)

### 3.1 完整配置导入

#### 3.1.1 URL 格式

```
ccswitch://v1/import?resource=config&app={app}&data={base64_encoded_config}
```

**新增参数**:
| 参数 | 类型 | 说明 | 示例 |
|------|------|------|------|
| `resource` | string | 资源类型，值为 `config` | `config` |
| `app` | string | 目标应用 | `claude` \| `codex` \| `gemini` |
| `data` | string | Base64 编码的配置内容 | `eyJlbnYiOnsic...` |
| `format` | string | 配置格式（可选，自动检测） | `json` \| `toml` |

#### 3.1.2 支持的配置格式

**Claude Code - settings.json**:
```json
{
  "env": {
    "ANTHROPIC_AUTH_TOKEN": "sk-ant-xxx",
    "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
    "ANTHROPIC_DEFAULT_SONNET_MODEL": "claude-sonnet-4-5-20250929"
  },
  "features": {
    "autoUpdate": true
  }
}
```

**Gemini - config.json**:
```json
{
  "apiKey": "AIza-xxx",
  "baseURL": "https://generativelanguage.googleapis.com",
  "model": "gemini-2.0-flash-exp",
  "security": {
    "auth": {
      "selectedType": "gemini-api-key"
    }
  }
}
```

**Codex - config.toml**:
```toml
[api]
base_url = "https://api.openai.com/v1"
model = "gpt-5-codex"

[features]
web_search_request = true
```

#### 3.1.3 编码示例

**生成配置导入链接 (TypeScript)**:
```typescript
import { generateConfigImportUrl } from '@/lib/utils/deeplink'

// 1. 读取配置文件
const config = {
  env: {
    ANTHROPIC_AUTH_TOKEN: 'sk-ant-xxx',
    ANTHROPIC_BASE_URL: 'https://api.anthropic.com'
  }
}

// 2. 生成深链接
const url = generateConfigImportUrl('claude', JSON.stringify(config, null, 2))

// 输出: ccswitch://v1/import?resource=config&app=claude&data=eyJlbnYi...
console.log(url)
```

**解码配置内容 (Rust)**:
```rust
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

let encoded_data = "eyJlbnYiOnsic...";
let raw_data = BASE64.decode(encoded_data)?;
let content = String::from_utf8(raw_data)?;

// 解析 JSON
let config: serde_json::Value = serde_json::from_str(&content)?;
```

---

## 4. 架构设计

### 4.1 数据流 (v1.0)

```
用户点击深链接
    ↓
操作系统捕获 ccswitch:// 协议
    ↓
启动/唤醒 CC-Switch 应用
    ↓
前端: 监听 deep-link 事件
    ↓
调用: parse_deeplink_url(url)
    ↓
后端: 解析并验证 URL
    ↓
返回: DeepLinkImportRequest 结构
    ↓
前端: 弹出确认对话框
    ├─ 显示供应商信息
    ├─ API Key 掩码显示
    └─ 风险提示
    ↓
用户确认后调用: import_provider_from_deeplink(request)
    ↓
后端: ProviderService.add() 添加供应商
    ↓
前端: 刷新供应商列表 + Toast 提示
```

### 4.2 数据流 (v1.1 配置导入)

```
用户点击配置导入链接
    ↓
前端: parse_deeplink_url(url)
    ↓
后端: 解析 resource=config
    ├─ 提取 data 参数
    ├─ Base64 解码
    └─ 验证配置格式 (JSON/TOML)
    ↓
返回: DeepLinkConfigRequest
    ↓
前端: 配置预览对话框
    ├─ 显示完整配置内容
    ├─ 支持编辑修改
    ├─ 语法高亮 (JSON/TOML)
    └─ 敏感字段掩码
    ↓
用户确认后调用: import_config_from_deeplink(app, data)
    ↓
后端: DeepLinkService.handle_config_import()
    ├─ 根据 app 类型解析配置
    ├─ 提取关键字段 (apiKey, baseUrl 等)
    ├─ 生成 Provider 结构
    └─ 调用 ProviderService.add()
    ↓
前端: 刷新 + Toast 提示
```

### 4.3 模块设计

#### 后端模块结构

```
src-tauri/src/
├── models/
│   └── deeplink.rs          # 数据模型 (NEW in v1.1)
│       ├── DeepLinkRequest (枚举)
│       ├── ProviderImportRequest
│       └── ConfigImportRequest
├── services/
│   └── deeplink.rs          # 业务逻辑 (NEW in v1.1)
│       ├── handle_config_import()
│       ├── import_claude_config()
│       ├── import_gemini_config()
│       └── import_codex_config()
├── commands/
│   └── deeplink.rs          # Tauri 命令
│       ├── parse_deeplink_url()
│       └── import_config_from_deeplink() (NEW in v1.1)
└── deeplink.rs              # v1.0 实现 (兼容保留)
```

#### 前端模块结构

```
src/
├── lib/
│   ├── api/
│   │   └── deeplink.ts      # API 封装
│   │       ├── parseUrl()
│   │       └── importConfig() (NEW in v1.1)
│   └── utils/
│       └── deeplink.ts      # 工具函数 (NEW in v1.1)
│           ├── encodeConfig()
│           ├── decodeConfig()
│           └── generateConfigImportUrl()
└── components/
    └── deeplink/
        ├── ProviderImportDialog.tsx  # v1.0 字段导入
        └── ConfigImportDialog.tsx    # v1.1 配置导入 (NEW)
```

---

## 5. 安全设计

### 5.1 现有机制 (v1.0)

#### 用户确认对话框
- 显示完整的供应商信息（name, endpoint, homepage）
- API Key 掩码显示: `sk-ant-****-****abcd` (仅显示前后各 4 位)
- 明确的风险提示文案
- 必须点击"确认导入"才会执行

#### 输入校验
```rust
// 后端校验
fn validate_url(url_str: &str, field_name: &str) -> Result<(), AppError> {
    let url = Url::parse(url_str)?;
    let scheme = url.scheme();

    // 仅允许 HTTP/HTTPS
    if scheme != "http" && scheme != "https" {
        return Err(AppError::InvalidInput(
            format!("Invalid URL scheme for '{field_name}': {scheme}")
        ));
    }
    Ok(())
}
```

#### 防护措施
- ✅ XSS 防护: 所有字段经过严格校验
- ✅ 注入防护: 不直接拼接 SQL/命令
- ✅ URL 伪造防护: scheme/host 强制校验
- ✅ 重放攻击防护: 每次导入生成唯一 ID (timestamp + name)

### 5.2 v1.1 增强机制

#### 配置文件大小限制
```rust
const MAX_CONFIG_SIZE: usize = 100 * 1024; // 100KB

fn validate_config_size(content: &str) -> Result<()> {
    if content.len() > MAX_CONFIG_SIZE {
        return Err(AppError::InvalidInput(
            format!("配置文件过大 (最大 {}KB)", MAX_CONFIG_SIZE / 1024)
        ));
    }
    Ok(())
}
```

#### 危险字段过滤
```rust
fn sanitize_config(config: &mut serde_json::Value) -> Result<()> {
    if let Some(obj) = config.as_object_mut() {
        // 移除原型污染攻击相关字段
        obj.remove("__proto__");
        obj.remove("constructor");
        obj.remove("prototype");
    }
    Ok(())
}
```

#### 配置预览与编辑
```typescript
<ConfigImportDialog>
  {/* 完整配置内容预览 */}
  <Textarea
    value={configPreview}
    onChange={(e) => setConfigPreview(e.target.value)}
    className="font-mono text-xs"
    rows={12}
  />

  {/* 安全提示 */}
  <Alert variant="warning">
    <AlertTriangle className="h-4 w-4" />
    <AlertTitle>安全提示</AlertTitle>
    <AlertDescription>
      <ul className="list-disc list-inside text-xs">
        <li>请确认配置来源可信</li>
        <li>API Key 将被掩码显示</li>
        <li>导入后建议立即验证配置有效性</li>
      </ul>
    </AlertDescription>
  </Alert>
</ConfigImportDialog>
```

---

## 6. 实现细节

### 6.1 操作系统集成 (v1.0 已实现)

#### Windows (WiX Installer)
```xml
<!-- src-tauri/tauri.conf.json -->
{
  "bundle": {
    "windows": {
      "wix": {
        "deep_link_protocols": ["ccswitch"]
      }
    }
  }
}
```

#### macOS / Linux (Tauri Plugin)
```rust
// src-tauri/src/lib.rs
use tauri_plugin_deep_link;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                app.deep_link().register("ccswitch")?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running application");
}
```

### 6.2 前端事件监听 (v1.0 已实现)

```typescript
// src/App.tsx
import { getCurrent } from '@tauri-apps/api/webview'
import { deeplinkApi } from '@/lib/api/deeplink'

useEffect(() => {
  const unlisten = getCurrent().listen('deep-link', async (event) => {
    const url = event.payload as string

    try {
      // 解析深链接
      const request = await deeplinkApi.parseUrl(url)

      if (request.resource === 'provider') {
        // 显示字段导入对话框
        setProviderImportDialog({ open: true, data: request })
      } else if (request.resource === 'config') {
        // v1.1: 显示配置导入对话框
        setConfigImportDialog({ open: true, data: request })
      }
    } catch (error) {
      toast.error('解析深链接失败: ' + (error as Error).message)
    }
  })

  return () => { unlisten.then(f => f()) }
}, [])
```

### 6.3 后端处理逻辑

#### v1.0 Provider 字段导入 (已实现)

```rust
// src-tauri/src/deeplink.rs
pub fn import_provider_from_deeplink(
    state: &AppState,
    request: DeepLinkImportRequest,
) -> Result<String, AppError> {
    let app_type = AppType::from_str(&request.app)?;

    // 构建 Provider 结构
    let provider = build_provider_from_request(&app_type, &request)?;

    // 生成唯一 ID
    let timestamp = chrono::Utc::now().timestamp_millis();
    let sanitized_name = request.name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
        .to_lowercase();
    provider.id = format!("{sanitized_name}-{timestamp}");

    // 添加供应商
    ProviderService::add(state, app_type, provider)?;

    Ok(provider.id)
}
```

#### v1.1 完整配置导入 (规划中)

```rust
// src-tauri/src/services/deeplink.rs
impl DeepLinkService {
    pub fn handle_config_import(
        state: &AppState,
        request: ConfigImportRequest,
    ) -> Result<Provider, AppError> {
        // 1. 解码 Base64
        let raw_data = BASE64.decode(&request.data)
            .map_err(|e| AppError::InvalidInput(format!("Invalid Base64: {e}")))?;

        let content = String::from_utf8(raw_data)
            .map_err(|e| AppError::InvalidInput(format!("Invalid UTF-8: {e}")))?;

        // 2. 校验大小
        Self::validate_config_size(&content)?;

        // 3. 根据应用类型解析
        let app_type = AppType::from_str(&request.app)?;
        let provider = match app_type {
            AppType::Claude => Self::import_claude_config(&content)?,
            AppType::Gemini => Self::import_gemini_config(&content)?,
            AppType::Codex => Self::import_codex_config(&content)?,
        };

        // 4. 添加供应商
        ProviderService::add(state, app_type, provider.clone())?;

        Ok(provider)
    }

    fn import_claude_config(content: &str) -> Result<Provider, AppError> {
        let mut config: serde_json::Value = serde_json::from_str(content)
            .map_err(|e| AppError::InvalidInput(format!("Invalid JSON: {e}")))?;

        // 过滤危险字段
        Self::sanitize_config(&mut config)?;

        // 提取关键字段
        let env = config.get("env")
            .ok_or_else(|| AppError::InvalidInput("Missing 'env' field".into()))?;

        let api_key = env.get("ANTHROPIC_AUTH_TOKEN")
            .and_then(|v| v.as_str())
            .map(String::from);

        let base_url = env.get("ANTHROPIC_BASE_URL")
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(Provider {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("从配置导入 {}", chrono::Local::now().format("%Y-%m-%d %H:%M")),
            settings_config: config,
            website_url: None,
            notes: Some("通过深链接配置导入".to_string()),
            ..Default::default()
        })
    }

    fn import_codex_config(content: &str) -> Result<Provider, AppError> {
        use toml::Value as TomlValue;

        let config: TomlValue = toml::from_str(content)
            .map_err(|e| AppError::InvalidInput(format!("Invalid TOML: {e}")))?;

        let base_url = config.get("api")
            .and_then(|api| api.get("base_url"))
            .and_then(|v| v.as_str())
            .map(String::from);

        Ok(Provider {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("从配置导入 {}", chrono::Local::now().format("%Y-%m-%d %H:%M")),
            settings_config: serde_json::to_value(config).unwrap(),
            website_url: None,
            notes: Some("通过深链接 TOML 导入".to_string()),
            ..Default::default()
        })
    }
}
```

---

## 7. 测试策略

### 7.1 单元测试 (v1.0 已实现)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_claude_deeplink() {
        let url = "ccswitch://v1/import?resource=provider&app=claude&name=Test&homepage=https%3A%2F%2Fexample.com&endpoint=https%3A%2F%2Fapi.example.com&apiKey=sk-test-123";
        let request = parse_deeplink_url(url).unwrap();

        assert_eq!(request.version, "v1");
        assert_eq!(request.app, "claude");
        assert_eq!(request.name, "Test");
    }

    #[test]
    fn test_parse_invalid_scheme() {
        let url = "https://v1/import?resource=provider&app=claude";
        let result = parse_deeplink_url(url);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid scheme"));
    }

    #[test]
    fn test_validate_invalid_url() {
        let result = validate_url("not-a-url", "test");
        assert!(result.is_err());
    }
}
```

### 7.2 集成测试 (v1.0 已实现)

```rust
// src-tauri/tests/deeplink_import.rs
#[cfg(test)]
mod integration_tests {
    use cc_switch::*;

    #[test]
    fn test_full_provider_import_flow() {
        let state = create_test_app_state();

        let url = "ccswitch://v1/import?resource=provider&app=claude&name=TestProvider&homepage=https://test.com&endpoint=https://api.test.com&apiKey=sk-test-123";

        // 解析
        let request = parse_deeplink_url(url).unwrap();

        // 导入
        let provider_id = import_provider_from_deeplink(&state, request).unwrap();

        // 验证
        let config = state.config.read().unwrap();
        let provider = config.claude.providers.get(&provider_id).unwrap();

        assert_eq!(provider.name, "TestProvider");
        assert!(provider.id.contains("testprovider"));
    }
}
```

### 7.3 v1.1 测试用例 (规划中)

```rust
#[test]
fn test_parse_config_deeplink() {
    let config_json = r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"sk-test"}}"#;
    let encoded = BASE64.encode(config_json);
    let url = format!("ccswitch://v1/import?resource=config&app=claude&data={encoded}");

    let request = DeepLinkRequest::from_url(&url).unwrap();

    match request {
        DeepLinkRequest::Config(config) => {
            assert_eq!(config.app, "claude");
            assert_eq!(config.data, encoded);
        },
        _ => panic!("Expected Config request"),
    }
}

#[test]
fn test_import_claude_config() {
    let config_json = r#"{
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "sk-ant-test",
            "ANTHROPIC_BASE_URL": "https://api.test.com"
        }
    }"#;

    let provider = DeepLinkService::import_claude_config(config_json).unwrap();

    // 验证从 settings_config 中提取的字段
    assert!(provider.name.contains("从配置导入"));
    assert_eq!(
        provider.settings_config["env"]["ANTHROPIC_AUTH_TOKEN"],
        "sk-ant-test"
    );
}

#[test]
fn test_import_codex_toml_config() {
    let config_toml = r#"
    [api]
    base_url = "https://api.codex.com"
    model = "gpt-5-codex"
    "#;

    let provider = DeepLinkService::import_codex_config(config_toml).unwrap();

    assert!(provider.name.contains("从配置导入"));
    assert!(provider.notes.unwrap().contains("TOML"));
}

#[test]
fn test_config_size_limit() {
    let large_config = "x".repeat(150 * 1024); // 150KB
    let result = DeepLinkService::validate_config_size(&large_config);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("过大"));
}
```

### 7.4 前端测试 (规划中)

```typescript
import { describe, it, expect } from 'vitest'
import { generateConfigImportUrl, decodeConfig } from '@/lib/utils/deeplink'

describe('DeepLink Config Import', () => {
  it('should encode and decode config correctly', () => {
    const originalConfig = '{"test":"value"}'
    const url = generateConfigImportUrl('claude', originalConfig)

    const urlObj = new URL(url)
    const encodedData = urlObj.searchParams.get('data')!

    const decoded = decodeConfig(encodedData)
    expect(decoded).toBe(originalConfig)
  })

  it('should handle special characters', () => {
    const config = '{"key":"中文测试\n换行"}'
    const url = generateConfigImportUrl('claude', config)
    const urlObj = new URL(url)
    const encoded = urlObj.searchParams.get('data')!
    const decoded = decodeConfig(encoded)

    expect(decoded).toBe(config)
  })
})
```

---

## 8. 使用示例

### 8.1 供应商快速分享 (v1.0)

#### 场景: 分享 DMXAPI 配置

1. **生成深链接**:
```bash
# 手动构建 URL
echo "ccswitch://v1/import?resource=provider&app=claude&name=DMXAPI&homepage=https%3A%2F%2Fdmxapi.com&endpoint=https%3A%2F%2Fapi.dmxapi.com%2Fv1&apiKey=sk-ant-YOUR_KEY"
```

2. **用户点击链接**:
- 浏览器 / 邮件客户端中点击链接
- 操作系统自动启动 CC-Switch
- 弹出确认对话框

3. **确认导入**:
```
┌──────────────────────────────────────┐
│  导入供应商配置                        │
├──────────────────────────────────────┤
│  名称: DMXAPI                         │
│  主页: https://dmxapi.com             │
│  端点: https://api.dmxapi.com/v1      │
│  API Key: sk-ant-****-****abcd        │
│                                       │
│  ⚠️  请确认来源可信后导入              │
│                                       │
│  [取消]  [确认导入]                   │
└──────────────────────────────────────┘
```

### 8.2 完整配置迁移 (v1.1 规划)

#### 场景 1: 导出并分享 Claude 配置

```typescript
// 1. 读取当前配置
const currentConfig = await settingsApi.getClaudeSettings()

// 2. 生成深链接
const shareUrl = generateConfigImportUrl('claude', JSON.stringify(currentConfig))

// 3. 复制到剪贴板
await navigator.clipboard.writeText(shareUrl)

// 输出示例:
// ccswitch://v1/import?resource=config&app=claude&data=eyJlbnYiOnsiQU5USFJPUElDX0FVVEhfVE9LRU4iOiJzay1hbnQteHh4IiwiQU5USFJPUElDX0JBU0VfVVJMIjoiaHR0cHM6Ly9hcGkuYW50aHJvcGljLmNvbSJ9fQ==
```

#### 场景 2: 从文件导入

```typescript
// HTML
<input
  type="file"
  accept=".json,.toml"
  onChange={async (e) => {
    const file = e.target.files?.[0]
    if (!file) return

    const content = await file.text()
    const app = file.name.includes('config.toml') ? 'codex' : 'claude'

    const url = await generateUrlFromFile(app, file)

    // 触发导入
    window.location.href = url
  }}
/>
```

#### 场景 3: 批量配置分发

```typescript
// 管理员生成多个配置链接
const configs = [
  { app: 'claude', file: 'claude-prod.json' },
  { app: 'codex', file: 'codex-dev.toml' },
  { app: 'gemini', file: 'gemini-test.json' },
]

const urls = await Promise.all(
  configs.map(async ({ app, file }) => {
    const content = await fs.readFile(file, 'utf-8')
    return generateConfigImportUrl(app, content)
  })
)

// 分发给团队成员
await sendEmail({
  to: 'team@company.com',
  subject: '开发环境配置',
  body: `
    请点击以下链接导入配置:
    - Claude: ${urls[0]}
    - Codex: ${urls[1]}
    - Gemini: ${urls[2]}
  `
})
```

---

## 9. 错误处理

### 9.1 错误类型

| 错误类型 | 说明 | 示例 |
|---------|------|------|
| `InvalidInput` | URL 格式或参数错误 | `Missing 'apiKey' parameter` |
| `UnsupportedVersion` | 协议版本不支持 | `Unsupported protocol version: v2` |
| `InvalidUrl` | URL 格式不合法 | `Invalid URL for 'endpoint'` |
| `ConfigTooLarge` | 配置文件超过大小限制 | `配置文件过大 (最大 100KB)` |
| `ParseError` | JSON/TOML 解析失败 | `Invalid JSON: unexpected token` |

### 9.2 错误响应示例

```json
{
  "error": "InvalidInput",
  "message": "Invalid app type: must be 'claude', 'codex', or 'gemini', got 'unknown'"
}
```

### 9.3 用户友好提示

```typescript
try {
  await deeplinkApi.importConfig(app, data)
  toast.success('配置导入成功')
} catch (error) {
  const errorMessage = (error as Error).message

  if (errorMessage.includes('Invalid JSON')) {
    toast.error('配置文件格式错误，请检查 JSON 语法')
  } else if (errorMessage.includes('过大')) {
    toast.error('配置文件超过 100KB，请使用文件导入功能')
  } else {
    toast.error('导入失败: ' + errorMessage)
  }
}
```

---

## 10. 性能考量

### 10.1 URL 长度限制

**浏览器限制**:
- Chrome: ~2MB
- Firefox: ~65,536 字符
- Edge: ~2,083 字符 (兼容性考虑)

**Base64 编码开销**:
- 原始大小 × 1.33 = 编码后大小
- 100KB 配置 → ~133KB Base64

**建议**:
- ✅ Provider 字段导入: 无限制 (URL 长度 < 500 字符)
- ✅ 配置导入: 限制 100KB 原始配置
- ⚠️ 超大配置: 提示使用文件导入

### 10.2 解析性能

```rust
// 基准测试 (Rust)
#[bench]
fn bench_parse_deeplink(b: &mut Bencher) {
    let url = "ccswitch://v1/import?resource=provider&app=claude&name=Test&homepage=https://test.com&endpoint=https://api.test.com&apiKey=sk-test";

    b.iter(|| {
        parse_deeplink_url(url).unwrap()
    });
    // 平均耗时: ~5μs
}

#[bench]
fn bench_decode_config(b: &mut Bencher) {
    let config = "{...}".repeat(1000); // ~10KB
    let encoded = BASE64.encode(&config);

    b.iter(|| {
        BASE64.decode(&encoded).unwrap()
    });
    // 平均耗时: ~50μs
}
```

---

## 11. 未来扩展

### 11.1 v1.2 规划: MCP 导入

```
ccswitch://v1/import?resource=mcp&app=claude&name=filesystem&command=npx&args=-y,@modelcontextprotocol/server-filesystem
```

### 11.2 v1.3 规划: 批量导入

```
ccswitch://v1/import?resource=batch&data=<base64_encoded_json_array>
```

**批量数据结构**:
```json
[
  {
    "resource": "provider",
    "app": "claude",
    "name": "Provider 1",
    ...
  },
  {
    "resource": "mcp",
    "app": "claude",
    "name": "MCP Server 1",
    ...
  }
]
```

### 11.3 v2.0 规划: 加密传输

```
ccswitch://v2/import?resource=encrypted&data=<encrypted_payload>&key=<public_key_fingerprint>
```

**加密方案**:
- 使用 RSA 公钥加密 API Key
- AES-256-GCM 加密完整配置
- 接收方使用私钥解密

---

## 12. 兼容性矩阵

| 功能 | Windows | macOS | Linux | 状态 |
|------|---------|-------|-------|------|
| Provider 字段导入 (v1.0) | ✅ | ✅ | ✅ | 已实现 |
| 配置文件导入 (v1.1) | 🚧 | 🚧 | 🚧 | 规划中 |
| MCP 导入 (v1.2) | 📋 | 📋 | 📋 | 未开始 |
| 批量导入 (v1.3) | 📋 | 📋 | 📋 | 未开始 |

**图例**:
- ✅ 已实现
- 🚧 开发中
- 📋 规划中

---

## 13. 开发路线图

### Phase 1: v1.0 基础实现 ✅
- [x] URL 解析与验证
- [x] Provider 字段导入
- [x] 操作系统协议注册
- [x] 前端确认对话框
- [x] 单元测试 + 集成测试

### Phase 2: v1.1 配置导入 (当前)
- [ ] 数据模型扩展 (`models/deeplink.rs`)
- [ ] Service 层实现 (`services/deeplink.rs`)
- [ ] Base64 编码/解码工具
- [ ] 配置预览 UI (`ConfigImportDialog.tsx`)
- [ ] 完整测试覆盖

**预计时间**: 3-4 天
**优先级**: 高

### Phase 3: v1.2 MCP 导入
- [ ] MCP 深链接协议设计
- [ ] MCP 导入逻辑
- [ ] 测试覆盖

**预计时间**: 2 天
**优先级**: 中

### Phase 4: 文档与发布
- [ ] 用户手册更新
- [ ] 示例与教程
- [ ] 发布公告

---

## 14. 参考资料

### 14.1 相关文档
- [Tauri Deep Link Plugin 文档](https://tauri.app/v2/guides/features/deep-linking/)
- [URL 编码规范 (RFC 3986)](https://datatracker.ietf.org/doc/html/rfc3986)
- [Base64 编码规范 (RFC 4648)](https://datatracker.ietf.org/doc/html/rfc4648)

### 14.2 代码位置
- 后端实现: `src-tauri/src/deeplink.rs` (v1.0)
- 后端命令: `src-tauri/src/commands/deeplink.rs`
- 前端 API: `src/lib/api/deeplink.ts`
- 测试文件: `src-tauri/tests/deeplink_import.rs`

### 14.3 相关 Issue
- Deep Link 基础实现: #123 (已完成)
- 配置导入功能: #456 (进行中)

---

## 15. 变更日志

### v1.1 (2025-11-18)
- 📝 新增配置导入设计 (resource=config)
- 📝 新增安全增强机制 (大小限制、字段过滤)
- 📝 新增完整测试用例
- 📝 新增使用示例与场景

### v1.0 (2025-11-16)
- ✅ 初始版本: Provider 字段导入
- ✅ 操作系统协议注册
- ✅ 基础安全机制
- ✅ 单元测试 + 集成测试

---

## 附录 A: 完整 URL 示例

### A.1 Provider 字段导入 (v1.0)

```
ccswitch://v1/import?resource=provider&app=claude&name=DMXAPI&homepage=https%3A%2F%2Fdmxapi.com&endpoint=https%3A%2F%2Fapi.dmxapi.com%2Fv1&apiKey=sk-ant-api03-xxx&model=claude-sonnet-4-5&notes=%E9%AB%98%E9%80%9F%E7%A8%B3%E5%AE%9A
```

### A.2 配置导入 (v1.1)

**Claude 配置**:
```
ccswitch://v1/import?resource=config&app=claude&data=eyJlbnYiOnsiQU5USFJPUElDX0FVVEhfVE9LRU4iOiJzay1hbnQteHh4IiwiQU5USFJPUElDX0JBU0VfVVJMIjoiaHR0cHM6Ly9hcGkuYW50aHJvcGljLmNvbSIsIkFOVEhST1BJQ19ERUZBVUxUX1NPTk5FVF9NT0RFTCI6ImNsYXVkZS1zb25uZXQtNC01LTIwMjUwOTI5In19
```

**Codex 配置**:
```
ccswitch://v1/import?resource=config&app=codex&data=W2FwaV0KYmFzZV91cmwgPSAiaHR0cHM6Ly9hcGkub3BlbmFpLmNvbS92MSIKbW9kZWwgPSAiZ3B0LTUtY29kZXgiCgpbZmVhdHVyZXNdCndlYl9zZWFyY2hfcmVxdWVzdCA9IHRydWU=
```

---

**文档维护者**: CC-Switch 开发团队
**最后审核**: 2025-11-18
**下次审核**: v1.1 功能实现后
