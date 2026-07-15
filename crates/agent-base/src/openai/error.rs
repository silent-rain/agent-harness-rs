//! OpenAI 错误响应数据模型
//!
//! 对齐 OpenAI Chat Completions API 错误响应格式，便于客户端 SDK 直接消费。
//!
//! 提供的主要内容：
//! - [`OpenAIError`]: 顶层错误响应（含 `error` 字段）
//! - [`ErrorBody`]: 错误主体（type / code / message / param）

use serde::{Deserialize, Serialize};

/// 顶层错误响应
///
/// 序列化后形如：
/// ```jsonc
/// { "error": { "type": "...", "code": "...", "message": "..." } }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAIError {
    /// 错误主体
    pub error: ErrorBody,
}

/// 错误主体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorBody {
    /// 错误类型（如 `"agent_run_failed"` / `"invalid_request_error"`）
    #[serde(rename = "type")]
    pub error_type: String,
    /// 错误码（业务侧细分，如 `"no_final_answer"`）
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub code: Option<String>,
    /// 人类可读错误消息
    pub message: String,
    /// 出错的请求参数名（OpenAI 校验失败时使用，nucleus 一般为 None）
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub param: Option<String>,
}

impl OpenAIError {
    /// 构造一个 nucleus agent 运行失败的错误
    ///
    /// # Parameters
    ///
    /// * `code` - 业务错误码（如 `"no_final_answer"`）
    /// * `message` - 人类可读错误消息
    pub fn agent_run_failed(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ErrorBody {
                error_type: "agent_run_failed".to_string(),
                code: Some(code.into()),
                message: message.into(),
                param: None,
            },
        }
    }

    /// 构造一个无效请求错误（对齐 OpenAI `invalid_request_error`）
    ///
    /// # Parameters
    ///
    /// * `message` - 人类可读错误消息
    /// * `param` - 出错的参数名（可选）
    pub fn invalid_request(message: impl Into<String>, param: Option<String>) -> Self {
        Self {
            error: ErrorBody {
                error_type: "invalid_request_error".to_string(),
                code: None,
                message: message.into(),
                param,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 错误响应序列化结构应严格对齐 OpenAI 标准
    #[test]
    fn test_error_serialize_layout() {
        let err = OpenAIError::agent_run_failed("no_final_answer", "Agent did not finalize");
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["error"]["type"].as_str(), Some("agent_run_failed"));
        assert_eq!(json["error"]["code"].as_str(), Some("no_final_answer"));
        assert_eq!(
            json["error"]["message"].as_str(),
            Some("Agent did not finalize")
        );
        assert!(json["error"].get("param").is_none());
    }

    /// invalid_request_error 中 param 字段应被携带
    #[test]
    fn test_invalid_request_with_param() {
        let err = OpenAIError::invalid_request(
            "messages must not be empty",
            Some("messages".to_string()),
        );
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["error"]["param"].as_str(), Some("messages"));
    }
}
