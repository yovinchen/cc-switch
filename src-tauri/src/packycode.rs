use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackyCodeEndpoint {
    pub name: String,
    pub url: String,
    pub latency: Option<u128>, // 延迟（毫秒）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackyCodeService {
    pub name: String,
    pub endpoints: Vec<PackyCodeEndpoint>,
}

/// 获取所有 PackyCode 服务配置
pub fn get_packycode_services() -> Vec<PackyCodeService> {
    vec![
        PackyCodeService {
            name: "滴滴车".to_string(),
            endpoints: vec![
                PackyCodeEndpoint {
                    name: "默认节点".to_string(),
                    url: "https://share-api.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "香港CN2".to_string(),
                    url: "https://share-api-hk-cn2.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "香港G口".to_string(),
                    url: "https://share-api-hk-g.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "美国CN2".to_string(),
                    url: "https://share-api-us-cn2.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "Cloudflare Pro".to_string(),
                    url: "https://share-api-cf-pro.packycode.com".to_string(),
                    latency: None,
                },
            ],
        },
        PackyCodeService {
            name: "公交车".to_string(),
            endpoints: vec![
                PackyCodeEndpoint {
                    name: "默认节点".to_string(),
                    url: "https://api.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "香港CN2".to_string(),
                    url: "https://api-hk-cn2.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "香港G口".to_string(),
                    url: "https://api-hk-g.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "美国CN2".to_string(),
                    url: "https://api-us-cn2.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "Cloudflare Pro".to_string(),
                    url: "https://api-cf-pro.packycode.com".to_string(),
                    latency: None,
                },
            ],
        },
        PackyCodeService {
            name: "Codex".to_string(),
            endpoints: vec![
                PackyCodeEndpoint {
                    name: "默认节点".to_string(),
                    url: "https://codex-api.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "香港CN2".to_string(),
                    url: "https://codex-api-hk-cn2.packycode.com".to_string(),
                    latency: None,
                },
                PackyCodeEndpoint {
                    name: "香港CDN".to_string(),
                    url: "https://codex-api-hk-cdn.packycode.com".to_string(),
                    latency: None,
                },
            ],
        },
    ]
}

/// 测试单个端点的延迟（使用 HTTP HEAD 请求）
pub async fn test_endpoint_latency(url: &str) -> Result<u128, String> {
    let start = Instant::now();
    
    // 创建 HTTP 客户端，设置超时时间
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;
    
    // 发送 HEAD 请求测试连通性
    match client.head(url).send().await {
        Ok(_response) => {
            let elapsed = start.elapsed().as_millis();
            Ok(elapsed)
        }
        Err(e) => {
            if e.is_timeout() {
                Err("请求超时".to_string())
            } else if e.is_connect() {
                Err("连接失败".to_string())
            } else {
                Err(format!("请求失败: {}", e))
            }
        }
    }
}

/// 为指定服务选择最佳端点
pub async fn select_best_endpoint(service_name: &str) -> Result<PackyCodeEndpoint, String> {
    let services = get_packycode_services();
    
    let service = services
        .into_iter()
        .find(|s| s.name == service_name)
        .ok_or_else(|| format!("未找到服务: {}", service_name))?;
    
    let mut best_endpoint: Option<PackyCodeEndpoint> = None;
    let mut min_latency = u128::MAX;
    
    for mut endpoint in service.endpoints {
        match test_endpoint_latency(&endpoint.url).await {
            Ok(latency) => {
                endpoint.latency = Some(latency);
                if latency < min_latency {
                    min_latency = latency;
                    best_endpoint = Some(endpoint);
                }
            }
            Err(_) => continue,
        }
    }
    
    best_endpoint.ok_or_else(|| "所有端点都不可用".to_string())
}