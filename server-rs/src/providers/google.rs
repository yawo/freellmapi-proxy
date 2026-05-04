use super::base::Provider;
use crate::models::openai::{
    ChatCompletionChunk, ChatCompletionChunkChoice, ChatCompletionChunkDelta, ChatCompletionRequest, 
    ChatCompletionResponse, ChatCompletionChoice, ChatMessage, Platform, RoutedVia, TokenUsage
};
use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

pub struct GoogleProvider {
    platform: Platform,
    name: String,
    client: Client,
}

impl GoogleProvider {
    pub fn new() -> Self {
        Self {
            platform: Platform::Google,
            name: "Google AI Studio".into(),
            client: Client::builder()
                .timeout(Duration::from_millis(15000))
                .build()
                .unwrap(),
        }
    }
}

// ---- Google API Models ----

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_response: Option<GeminiFunctionResponse>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GeminiFunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    name: String,
    args: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GeminiFunctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    name: String,
    response: Value,
}

#[derive(Serialize, Deserialize, Debug)]
struct GeminiContent {
    role: String, // "user" or "model"
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    // tools and toolConfig skipped for brevity in this initial implementation
}

#[derive(Serialize, Deserialize, Debug)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: Option<GeminiCandidateContent>,
    finish_reason: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct GeminiCandidateContent {
    parts: Option<Vec<GeminiPart>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GeminiUsage {
    prompt_token_count: Option<i32>,
    candidates_token_count: Option<i32>,
    total_token_count: Option<i32>,
}

fn to_gemini_contents(messages: &[ChatMessage]) -> (Vec<GeminiContent>, Option<GeminiSystemInstruction>) {
    let mut contents = Vec::new();
    let mut system_parts = Vec::new();

    for m in messages {
        if m.role == "system" {
            if let Some(Value::String(s)) = &m.content {
                system_parts.push(GeminiPart {
                    text: Some(s.clone()),
                    function_call: None,
                    function_response: None,
                });
            }
        } else if m.role == "assistant" {
            let mut parts = Vec::new();
            if let Some(Value::String(s)) = &m.content {
                parts.push(GeminiPart {
                    text: Some(s.clone()),
                    function_call: None,
                    function_response: None,
                });
            }
            if !parts.is_empty() {
                contents.push(GeminiContent {
                    role: "model".into(),
                    parts,
                });
            }
        } else if m.role == "user" {
            if let Some(Value::String(s)) = &m.content {
                contents.push(GeminiContent {
                    role: "user".into(),
                    parts: vec![GeminiPart {
                        text: Some(s.clone()),
                        function_call: None,
                        function_response: None,
                    }],
                });
            }
        }
    }

    let sys_instr = if system_parts.is_empty() {
        None
    } else {
        Some(GeminiSystemInstruction { parts: system_parts })
    };

    (contents, sys_instr)
}

fn to_gemini_finish_reason(reason: Option<&str>) -> String {
    let r = reason.unwrap_or("").to_uppercase();
    if r == "MAX_TOKENS" {
        "length".into()
    } else if r == "SAFETY" || r == "RECITATION" || r == "BLOCKLIST" {
        "content_filter".into()
    } else {
        "stop".into()
    }
}

fn extract_text(parts: &Option<Vec<GeminiPart>>) -> Option<String> {
    if let Some(p) = parts {
        let text: String = p.iter().filter_map(|x| x.text.clone()).collect();
        if text.is_empty() { None } else { Some(text) }
    } else {
        None
    }
}

fn make_id() -> String {
    format!("chatcmpl-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis())
}

#[async_trait]
impl Provider for GoogleProvider {
    fn platform(&self) -> Platform {
        self.platform.clone()
    }

    fn name(&self) -> &str {
        &self.name
    }

    async fn chat_completion(
        &self,
        api_key: &str,
        req: &ChatCompletionRequest,
        model_id: &str,
    ) -> Result<ChatCompletionResponse, Box<dyn Error + Send + Sync>> {
        let (contents, system_instruction) = to_gemini_contents(&req.messages);

        let body = GeminiRequest {
            contents,
            system_instruction,
            generation_config: Some(GeminiGenerationConfig {
                temperature: req.temperature,
                max_output_tokens: req.max_tokens,
                top_p: req.top_p,
            }),
        };

        let url = format!("{}/models/{}:generateContent?key={}", API_BASE, model_id, api_key);
        let res = self.client.post(&url).json(&body).send().await?;
        let status = res.status();

        if !status.is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(format!("Google API error {}: {}", status, text).into());
        }

        let data: GeminiResponse = res.json().await?;
        
        let candidate = data.candidates.as_ref().and_then(|c| c.first());
        let parts = candidate.and_then(|c| c.content.as_ref()).and_then(|c| c.parts.as_ref());
        let text = extract_text(&parts.cloned());

        let usage = TokenUsage {
            prompt_tokens: data.usage_metadata.as_ref().and_then(|u| u.prompt_token_count).unwrap_or(0),
            completion_tokens: data.usage_metadata.as_ref().and_then(|u| u.candidates_token_count).unwrap_or(0),
            total_tokens: data.usage_metadata.as_ref().and_then(|u| u.total_token_count).unwrap_or(0),
        };

        Ok(ChatCompletionResponse {
            id: make_id(),
            object: "chat.completion".into(),
            created: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
            model: model_id.to_string(),
            choices: vec![ChatCompletionChoice {
                index: 0,
                message: ChatMessage {
                    role: "assistant".into(),
                    content: text.map(Value::String),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                },
                finish_reason: Some(to_gemini_finish_reason(candidate.and_then(|c| c.finish_reason.as_deref()))),
            }],
            usage,
            _routed_via: Some(RoutedVia {
                platform: self.platform.clone(),
                model: model_id.to_string(),
            }),
        })
    }

    async fn stream_chat_completion(
        &self,
        api_key: &str,
        req: &ChatCompletionRequest,
        model_id: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk, Box<dyn Error + Send + Sync>>>, Box<dyn Error + Send + Sync>> {
        let (contents, system_instruction) = to_gemini_contents(&req.messages);

        let body = GeminiRequest {
            contents,
            system_instruction,
            generation_config: Some(GeminiGenerationConfig {
                temperature: req.temperature,
                max_output_tokens: req.max_tokens,
                top_p: req.top_p,
            }),
        };

        let url = format!("{}/models/{}:streamGenerateContent?alt=sse&key={}", API_BASE, model_id, api_key);
        let builder = self.client.post(&url).json(&body);
        let mut es = EventSource::new(builder).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        let model_id_str = model_id.to_string();
        
        let stream = async_stream::stream! {
            let id = make_id();
            let mut emitted_finish = false;

            while let Some(event_res) = es.next().await {
                match event_res {
                    Ok(event) => match event {
                        Event::Open => {},
                        Event::Message(msg) => {
                            if msg.data == "[DONE]" {
                                if !emitted_finish {
                                    emitted_finish = true;
                                    let chunk = ChatCompletionChunk {
                                        id: id.clone(),
                                        object: "chat.completion.chunk".into(),
                                        created: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
                                        model: model_id_str.clone(),
                                        choices: vec![ChatCompletionChunkChoice {
                                            index: 0,
                                            delta: ChatCompletionChunkDelta { role: None, content: None, tool_calls: None },
                                            finish_reason: Some("stop".into()),
                                        }],
                                    };
                                    yield Ok(chunk);
                                }
                                break;
                            }
                            
                            if let Ok(gemini_res) = serde_json::from_str::<GeminiResponse>(&msg.data) {
                                let candidate = gemini_res.candidates.as_ref().and_then(|c| c.first());
                                let parts = candidate.and_then(|c| c.content.as_ref()).and_then(|c| c.parts.as_ref());
                                let text = extract_text(&parts.cloned());

                                if let Some(t) = text {
                                    if !t.is_empty() {
                                        let chunk = ChatCompletionChunk {
                                            id: id.clone(),
                                            object: "chat.completion.chunk".into(),
                                            created: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
                                            model: model_id_str.clone(),
                                            choices: vec![ChatCompletionChunkChoice {
                                                index: 0,
                                                delta: ChatCompletionChunkDelta {
                                                    role: Some("assistant".into()),
                                                    content: Some(t),
                                                    tool_calls: None,
                                                },
                                                finish_reason: None,
                                            }],
                                        };
                                        yield Ok(chunk);
                                    }
                                }

                                if let Some(reason) = candidate.and_then(|c| c.finish_reason.as_ref()) {
                                    if !emitted_finish {
                                        emitted_finish = true;
                                        let chunk = ChatCompletionChunk {
                                            id: id.clone(),
                                            object: "chat.completion.chunk".into(),
                                            created: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
                                            model: model_id_str.clone(),
                                            choices: vec![ChatCompletionChunkChoice {
                                                index: 0,
                                                delta: ChatCompletionChunkDelta { role: None, content: None, tool_calls: None },
                                                finish_reason: Some(to_gemini_finish_reason(Some(reason))),
                                            }],
                                        };
                                        yield Ok(chunk);
                                        break;
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => {
                        yield Err(Box::new(e) as Box<dyn Error + Send + Sync>);
                        break;
                    }
                }
            }

            if !emitted_finish {
                yield Ok(ChatCompletionChunk {
                    id,
                    object: "chat.completion.chunk".into(),
                    created: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
                    model: model_id_str,
                    choices: vec![ChatCompletionChunkChoice {
                        index: 0,
                        delta: ChatCompletionChunkDelta { role: None, content: None, tool_calls: None },
                        finish_reason: Some("stop".into()),
                    }],
                });
            }
        };

        Ok(Box::pin(stream))
    }

    async fn validate_key(&self, api_key: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/models?key={}", API_BASE, api_key);
        let res = self.client.get(&url).send().await?;
        let status = res.status();
        Ok(status != reqwest::StatusCode::UNAUTHORIZED && status != reqwest::StatusCode::FORBIDDEN)
    }
}
