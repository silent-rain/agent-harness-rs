//! 知识库查询工具
//!
//! 查询指定的知识库，获取相关信息。
//! 实现 `rig::Tool` trait，可直接注册到 `rig::ToolSet`。

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};

use crate::{error::Error, knowledge::KnowledgeQueryResult, runtime::services::RuntimeServices};

/// 知识库查询工具
///
/// 查询指定的知识库，获取相关信息。
pub struct KnowledgeQueryTool {
    /// 运行时服务引用
    services: RuntimeServices,
}

/// 知识库查询工具参数
#[derive(Debug, Deserialize)]
pub struct KnowledgeQueryArgs {
    /// 知识库 ID 列表（空表示使用所有绑定知识库）
    kb_ids: Vec<i64>,
    /// 检索查询
    query: String,
    /// 最大返回数量
    top_k: Option<u32>,
}

/// 知识库查询工具输出
#[derive(Debug, Serialize)]
pub struct KnowledgeQueryOutput {
    /// 检索到的上下文文本
    context_text: String,
    /// 命中的分块数量
    hit_count: usize,
}

impl KnowledgeQueryTool {
    /// 创建新的知识库查询工具
    pub fn new(services: RuntimeServices) -> Self {
        Self { services }
    }
}

impl Tool for KnowledgeQueryTool {
    const NAME: &'static str = "query_knowledge";

    type Error = Error;
    type Args = KnowledgeQueryArgs;
    type Output = KnowledgeQueryOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "query_knowledge".to_string(),
            description: "查询指定的知识库，获取相关信息。".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "kb_ids": {
                        "type": "array",
                        "items": {
                            "type": "integer"
                        },
                        "description": "知识库 ID 列表（空表示使用所有绑定知识库）"
                    },
                    "query": {
                        "type": "string",
                        "description": "检索查询"
                    },
                    "top_k": {
                        "type": "integer",
                        "description": "最大返回数量（可选）"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // TODO: 从运行时上下文中获取 user_id
        let user_id = 0; // 临时值，需要从运行时上下文获取
        let top_k = args.top_k.unwrap_or(5) as usize;

        let result: KnowledgeQueryResult = self
            .services
            .knowledge
            .query(&args.kb_ids, &args.query, user_id, top_k)
            .await
            .map_err(|e| Error::ToolExecution {
                tool_name: "query_knowledge".to_string(),
                message: format!("知识库查询失败: {}", e),
            })?;

        Ok(KnowledgeQueryOutput {
            context_text: result.context_text,
            hit_count: result.hit_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::NoopKnowledgeRetriever;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_knowledge_query_tool() {
        let services = RuntimeServices {
            session: Arc::new(crate::session::InMemorySessionService::new()),
            knowledge: Arc::new(NoopKnowledgeRetriever),
        };

        let tool = KnowledgeQueryTool::new(services);
        let args = KnowledgeQueryArgs {
            kb_ids: vec![],
            query: "test query".to_string(),
            top_k: Some(3),
        };

        let result = tool.call(args).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(output.context_text, "knowledge retrieval disabled");
        assert_eq!(output.hit_count, 0);
    }
}
