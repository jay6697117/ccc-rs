/// Corresponds to Anthropic Messages API `role`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// Image source. Corresponds to Anthropic API image source variants.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

/// Corresponds to Anthropic Messages API `ContentBlock`.
/// Covers all variants used by Claude Code: text, tool_use, tool_result, thinking, image.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Vec<ContentBlock>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    Thinking {
        thinking: String,
        signature: String,
    },
    Image {
        source: ImageSource,
    },
}

/// A single message in a conversation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

/// Core tool trait. Every tool implementation in ccc-tools implements this.
/// Corresponds to the TS `Tool` abstract class / interface (src/Tool.ts).
pub trait ToolDef: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> serde_json::Value;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_block_text_serde() {
        let block = ContentBlock::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(
            json.contains("\"type\":\"text\""),
            "missing type field: {json}"
        );
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, back);
    }

    #[test]
    fn content_block_tool_use_serde() {
        let block = ContentBlock::ToolUse {
            id: "toolu_01".into(),
            name: "bash".into(),
            input: serde_json::json!({"command": "ls"}),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"type\":\"tool_use\""));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, back);
    }

    #[test]
    fn message_role_serde() {
        assert_eq!(serde_json::to_string(&Role::User).unwrap(), "\"user\"");
        assert_eq!(
            serde_json::to_string(&Role::Assistant).unwrap(),
            "\"assistant\""
        );
    }

    #[test]
    fn thinking_block_serde() {
        let block = ContentBlock::Thinking {
            thinking: "let me think".into(),
            signature: "sig123".into(),
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"type\":\"thinking\""));
        let back: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, back);
    }

    #[test]
    fn tool_result_optional_error_omitted() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "toolu_01".into(),
            content: vec![],
            is_error: None,
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(
            !json.contains("is_error"),
            "is_error should be omitted when None"
        );
    }
}
