//! AskUserQuestionTool — ask the user a question.
//! Corresponds to TS `src/tools/AskUserQuestionTool/`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    tool::{Tool, ToolMeta},
    types::{ToolContext, ToolError, ToolOutput},
};

pub struct AskUserQuestionTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptionInput {
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    preview: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuestionInput {
    text: String,
    #[serde(default)]
    options: Vec<OptionInput>,
    #[serde(default)]
    multi_select: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    questions: Vec<QuestionInput>,
}

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "AskUserQuestion",
            description: "Ask the user questions to gather information or clarify instructions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "questions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "text": { "type": "string" },
                                "options": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "label": { "type": "string" },
                                            "description": { "type": "string" },
                                            "preview": { "type": "string" },
                                            "value": { "type": "string" }
                                        },
                                        "required": ["label"]
                                    }
                                },
                                "multiSelect": { "type": "boolean" }
                            },
                            "required": ["text"]
                        }
                    }
                },
                "required": ["questions"]
            }),
        }
    }

    async fn call(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let inp: Input =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        // In a real TUI/CLI, this would wait for user input.
        Ok(ToolOutput::text(format!(
            "Asked {} question(s)",
            inp.questions.len()
        )))
    }
}
