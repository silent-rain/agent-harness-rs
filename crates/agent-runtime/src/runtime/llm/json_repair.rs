//! Tool-call arguments JSON 修复
//!
//! LLM 在生成 `tool_calls.arguments` 时偶尔会输出非严格 JSON
//! （单引号、未引用 key、尾随逗号、Python 风格字面量、Markdown 代码围栏等）。
//! 本模块以"尽力而为"的原则对原始字符串做一次轻量修复，
//! 失败时直接返回原文，调用方可继续走严格解析。
//!
//! 提供的主要内容：
//! - [`repair_json_arguments`]: 对 arguments 字符串做一次尽力修复
//!
//! # 设计说明
//!
//! - 仅在常见已知问题上做替换，不引入完整的 JSON5 解析器，避免依赖膨胀
//! - 修复后会再次用 `serde_json::from_str::<Value>` 验证，验证失败则返回原文
//! - 函数无副作用、不 panic，可在任意线程安全调用

use serde_json::Value;

/// 对 LLM 输出的 tool_call `arguments` 字符串做一次尽力 JSON 修复。
///
/// 修复策略（按顺序尝试）：
///
/// 1. 已是合法 JSON → 原样返回
/// 2. 去除 Markdown 代码围栏（` ``` ` / ` ```json `）
/// 3. 替换 smart quotes（`“ ” ‘ ’`）为标准引号
/// 4. 替换 Python 字面量 `True` / `False` / `None` → `true` / `false` / `null`
/// 5. 删除对象 / 数组结尾多余的 `,`
/// 6. 仍非合法 JSON → 原样返回
///
/// # Parameters
///
/// * `raw` - 原始 arguments 字符串
///
/// # Returns
///
/// 修复后的字符串。若无法修复，则返回原文（调用方仍可走严格解析）。
pub fn repair_json_arguments(raw: &str) -> String {
    // 快路径：已经是合法 JSON，直接返回
    if serde_json::from_str::<Value>(raw).is_ok() {
        return raw.to_string();
    }

    let mut s = raw.trim().to_string();

    // 1) 去 Markdown 代码围栏
    s = strip_code_fences(&s);

    // 2) smart quotes → ASCII quotes
    s = s
        .replace(['\u{201c}', '\u{201d}'], "\"")
        .replace(['\u{2018}', '\u{2019}'], "'");

    // 3) Python 字面量
    s = replace_python_literals(&s);

    // 4) 尾随逗号
    s = remove_trailing_commas(&s);

    // 验证修复结果
    if serde_json::from_str::<Value>(&s).is_ok() {
        s
    } else {
        raw.to_string()
    }
}

/// 去除 Markdown 代码围栏。
///
/// 例如：` ```json\n{...}\n``` ` → `{...}`。
fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    // 跳过开头的 ``` 或 ```json / ```JSON 行
    let after_open = trimmed.trim_start_matches("```");
    let after_open = after_open
        .strip_prefix("json")
        .or_else(|| after_open.strip_prefix("JSON"))
        .unwrap_or(after_open);
    let after_open = after_open.trim_start_matches('\n');
    // 去掉结尾的 ```
    after_open
        .trim_end()
        .trim_end_matches("```")
        .trim()
        .to_string()
}

/// 替换字符串外部的 Python 字面量为 JSON 字面量。
///
/// 仅替换不在字符串内的 `True` / `False` / `None`，避免破坏含 Python
/// 字样的字符串字面量。
fn replace_python_literals(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut in_string = false;
    let mut string_quote = b'"';
    let mut prev_escape = false;

    while i < bytes.len() {
        let b = bytes[i];

        if in_string {
            out.push(b as char);
            if !prev_escape && b == string_quote {
                in_string = false;
            }
            prev_escape = !prev_escape && b == b'\\';
            i += 1;
            continue;
        }

        if b == b'"' || b == b'\'' {
            in_string = true;
            string_quote = b;
            prev_escape = false;
            out.push(b as char);
            i += 1;
            continue;
        }

        // 仅在词边界处替换关键字
        if is_word_boundary(s, i) {
            if matches_keyword(bytes, i, b"True") {
                out.push_str("true");
                i += 4;
                continue;
            }
            if matches_keyword(bytes, i, b"False") {
                out.push_str("false");
                i += 5;
                continue;
            }
            if matches_keyword(bytes, i, b"None") {
                out.push_str("null");
                i += 4;
                continue;
            }
        }

        out.push(b as char);
        i += 1;
    }

    out
}

/// 在词边界处匹配关键字。
fn matches_keyword(bytes: &[u8], pos: usize, kw: &[u8]) -> bool {
    if pos + kw.len() > bytes.len() {
        return false;
    }
    if &bytes[pos..pos + kw.len()] != kw {
        return false;
    }
    // 关键字之后必须是非字母数字
    let after = pos + kw.len();
    !(after < bytes.len() && (bytes[after].is_ascii_alphanumeric() || bytes[after] == b'_'))
}

/// 判断 `pos` 是否处于词边界（前一个字符非字母数字下划线）。
fn is_word_boundary(s: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let prev = s.as_bytes()[pos - 1];
    !(prev.is_ascii_alphanumeric() || prev == b'_')
}

/// 删除对象或数组结尾多余的逗号。
///
/// 例如：`{"a": 1,}` → `{"a": 1}`、`[1, 2,]` → `[1, 2]`。
fn remove_trailing_commas(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut in_string = false;
    let mut string_quote = b'"';
    let mut prev_escape = false;
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if in_string {
            out.push(b as char);
            if !prev_escape && b == string_quote {
                in_string = false;
            }
            prev_escape = !prev_escape && b == b'\\';
            i += 1;
            continue;
        }

        if b == b'"' || b == b'\'' {
            in_string = true;
            string_quote = b;
            prev_escape = false;
            out.push(b as char);
            i += 1;
            continue;
        }

        if b == b',' {
            // 向后查找下一个非空白字符
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'}' || bytes[j] == b']') {
                // 跳过此逗号
                i += 1;
                continue;
            }
        }

        out.push(b as char);
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_already_valid_json_unchanged() {
        let input = r#"{"a":1,"b":"x"}"#;
        assert_eq!(repair_json_arguments(input), input);
    }

    #[test]
    fn test_strip_code_fences() {
        let input = "```json\n{\"a\":1}\n```";
        let repaired = repair_json_arguments(input);
        assert!(serde_json::from_str::<Value>(&repaired).is_ok());
    }

    #[test]
    fn test_python_literals() {
        let input = r#"{"a": True, "b": False, "c": None}"#;
        let repaired = repair_json_arguments(input);
        assert_eq!(repaired, r#"{"a": true, "b": false, "c": null}"#);
    }

    #[test]
    fn test_python_literals_inside_string_preserved() {
        // 修复后仍非合法 JSON（None 在字符串内不应替换为 null）
        let input = r#"{"msg": "True story", "ok": True}"#;
        let repaired = repair_json_arguments(input);
        let v: Value = serde_json::from_str(&repaired).expect("should parse");
        assert_eq!(v["msg"], "True story");
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn test_trailing_comma_object() {
        let input = r#"{"a": 1, "b": 2,}"#;
        let repaired = repair_json_arguments(input);
        let v: Value = serde_json::from_str(&repaired).expect("should parse");
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn test_trailing_comma_array() {
        let input = r#"{"arr": [1, 2, 3,]}"#;
        let repaired = repair_json_arguments(input);
        assert!(serde_json::from_str::<Value>(&repaired).is_ok());
    }

    #[test]
    fn test_smart_quotes() {
        let input = "{\u{201c}a\u{201d}: 1}";
        let repaired = repair_json_arguments(input);
        let v: Value = serde_json::from_str(&repaired).expect("should parse");
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn test_unfixable_returns_original() {
        let input = "this is not json at all <<>>";
        assert_eq!(repair_json_arguments(input), input);
    }
}
