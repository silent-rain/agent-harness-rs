//! DuckDuckGo 网络搜索工具
//!
//! 使用 DuckDuckGo 的免费 HTTP API，对 LLM 提供基础的实时检索能力。
//! 实现 `rig::Tool` trait，可直接注册到 `rig::ToolSet`。
//!
//! 当前为最小可用实现：调用 `https://api.duckduckgo.com/?q=...&format=json`，
//! 返回 `AbstractText` / `RelatedTopics` 摘要。任务 5 中会进一步丰富参数与结果结构。

use rig_core::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Error;

/// DuckDuckGo 网络搜索工具
///
/// 注意：此处使用的是 DuckDuckGo Instant Answer API，并非完整网页搜索；
/// 仅在 LLM 需要轻量摘要类信息时使用。
#[derive(Debug, Default, Clone, Copy)]
pub struct DuckDuckGoTool;

/// DuckDuckGo 搜索工具参数
#[derive(Debug, Deserialize)]
pub struct DuckDuckGoArgs {
    /// 查询关键字
    query: String,
    /// 返回的相关条目最大数量（默认 5，最大 20）
    #[serde(default)]
    max_results: Option<usize>,
}

/// DuckDuckGo 搜索结果条目
#[derive(Debug, Serialize)]
pub struct DuckDuckGoResultItem {
    /// 条目标题
    title: String,
    /// 摘要
    snippet: String,
    /// 跳转链接（可能为空）
    url: String,
}

/// DuckDuckGo 搜索工具输出
#[derive(Debug, Serialize)]
pub struct DuckDuckGoOutput {
    /// 直接摘要（来自 `AbstractText`，可能为空字符串）
    abstract_text: String,
    /// 相关条目列表
    results: Vec<DuckDuckGoResultItem>,
    /// 实际命中数量
    total: usize,
}

const DEFAULT_MAX_RESULTS: usize = 5;
const MAX_RESULTS_CAP: usize = 20;
const ENDPOINT: &str = "https://api.duckduckgo.com/";

impl Tool for DuckDuckGoTool {
    const NAME: &'static str = "duckduckgo_search";

    type Error = Error;
    type Args = DuckDuckGoArgs;
    type Output = DuckDuckGoOutput;

    fn description(&self) -> String {
        "使用 DuckDuckGo 进行轻量级网络搜索，返回 Instant Answer 摘要及相关链接条目。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "要搜索的查询关键字"
                },
                "max_results": {
                    "type": "integer",
                    "description": "返回的相关条目最大数量（默认 5，最大 20）",
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": ["query"]
        })
    }

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let limit = args
            .max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, MAX_RESULTS_CAP);

        let client = reqwest::Client::builder()
            .user_agent("nucleus-rs-agent/0.1 (+https://example.invalid)")
            .build()
            .map_err(|e| Error::ToolExecution {
                tool_name: Self::NAME.to_string(),
                message: format!("构建 HTTP 客户端失败: {}", e),
            })?;

        let endpoint_with_query = format!(
            "{}?q={}&format=json&no_html=1&no_redirect=1&skip_disambig=1",
            ENDPOINT,
            urlencoding_minimal(&args.query)
        );

        let response =
            client
                .get(endpoint_with_query)
                .send()
                .await
                .map_err(|e| Error::ToolExecution {
                    tool_name: Self::NAME.to_string(),
                    message: format!("DuckDuckGo 请求失败: {}", e),
                })?;

        if !response.status().is_success() {
            return Err(Error::ToolExecution {
                tool_name: Self::NAME.to_string(),
                message: format!("DuckDuckGo 返回非成功状态: {}", response.status()),
            });
        }

        let body: Value = response.json().await.map_err(|e| Error::ToolExecution {
            tool_name: Self::NAME.to_string(),
            message: format!("解析 DuckDuckGo 响应失败: {}", e),
        })?;

        let abstract_text = body
            .get("AbstractText")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let mut results: Vec<DuckDuckGoResultItem> = Vec::new();
        if let Some(items) = body.get("RelatedTopics").and_then(Value::as_array) {
            collect_topics(items, &mut results, limit);
        }

        Ok(DuckDuckGoOutput {
            abstract_text,
            total: results.len(),
            results,
        })
    }
}

/// 递归展开 `RelatedTopics`，DuckDuckGo 的分组项也用同样字段呈现。
fn collect_topics(items: &[Value], out: &mut Vec<DuckDuckGoResultItem>, limit: usize) {
    for item in items {
        if out.len() >= limit {
            return;
        }
        // 分组节点的形态：{ Name: "...", Topics: [...] }
        if let Some(sub_topics) = item.get("Topics").and_then(Value::as_array) {
            collect_topics(sub_topics, out, limit);
            continue;
        }
        let url = item
            .get("FirstURL")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let snippet = item
            .get("Text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if snippet.is_empty() && url.is_empty() {
            continue;
        }
        // 标题优先取摘要的第一句，否则取 URL
        let title = snippet
            .split(" - ")
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(url.as_str())
            .to_string();
        out.push(DuckDuckGoResultItem {
            title,
            snippet,
            url,
        });
    }
}

/// 最小 URL 查询参数编码：仅保留 RFC 3986 的 unreserved 字符，其他字节按 %HH 编码。
///
/// 仅用于 `q=` 等简单查询场景，避免再引入 `urlencoding` 依赖。
fn urlencoding_minimal(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        let b = *byte;
        let is_unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_topics_respects_limit_and_flattens_groups() {
        let payload = serde_json::json!([
            { "Text": "First - intro", "FirstURL": "https://example.com/1" },
            { "Name": "Group", "Topics": [
                { "Text": "Nested - desc", "FirstURL": "https://example.com/2" },
                { "Text": "Also nested - desc", "FirstURL": "https://example.com/3" }
            ]},
            { "Text": "Third - intro", "FirstURL": "https://example.com/4" }
        ]);
        let arr = payload.as_array().unwrap();
        let mut out = Vec::new();
        collect_topics(arr, &mut out, 3);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].title, "First");
        assert_eq!(out[1].url, "https://example.com/2");
    }
}
