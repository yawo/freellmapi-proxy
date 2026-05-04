use super::base::Provider;
use crate::models::openai::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, Platform, RoutedVia};
use async_trait::async_trait;
use futures::stream::{BoxStream, StreamExt};
use reqwest::Client;
use reqwest_eventsource::{Event, EventSource};
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

pub struct OpenAICompatProvider {
    platform: Platform,
    name: String,
    base_url: String,
    extra_headers: HashMap<String, String>,
    validate_url: Option<String>,
    client: Client,
}

impl OpenAICompatProvider {
    pub fn new(
        platform: Platform,
        name: String,
        base_url: String,
        extra_headers: Option<HashMap<String, String>>,
        validate_url: Option<String>,
        timeout_ms: Option<u64>,
    ) -> Self {
        let timeout_ms = timeout_ms.unwrap_or(15000);
        Self {
            platform,
            name,
            base_url,
            extra_headers: extra_headers.unwrap_or_default(),
            validate_url,
            client: Client::builder()
                .timeout(Duration::from_millis(timeout_ms))
                .build()
                .unwrap(),
        }
    }

    fn normalize_choices(data: ChatCompletionResponse) -> ChatCompletionResponse {
        // Similar to the JS logic for reasoning models & mistral array contents.
        data
    }
}

#[async_trait]
impl Provider for OpenAICompatProvider {
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
        let url = format!("{}/chat/completions", self.base_url);
        
        let mut request_body = req.clone();
        request_body.model = Some(model_id.to_string());
        request_body.stream = Some(false);

        let mut builder = self.client.post(&url)
            .bearer_auth(api_key)
            .json(&request_body);

        for (k, v) in &self.extra_headers {
            builder = builder.header(k, v);
        }

        let res = builder.send().await?;
        let status = res.status();

        if !status.is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(format!("{} API error {}: {}", self.name, status, text).into());
        }

        let mut data: ChatCompletionResponse = res.json().await?;
        data = Self::normalize_choices(data);
        data._routed_via = Some(RoutedVia {
            platform: self.platform.clone(),
            model: model_id.to_string(),
        });

        Ok(data)
    }

    async fn stream_chat_completion(
        &self,
        api_key: &str,
        req: &ChatCompletionRequest,
        model_id: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk, Box<dyn Error + Send + Sync>>>, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/chat/completions", self.base_url);

        let mut request_body = req.clone();
        request_body.model = Some(model_id.to_string());
        request_body.stream = Some(true);

        let mut builder = self.client.post(&url)
            .bearer_auth(api_key)
            .json(&request_body);

        for (k, v) in &self.extra_headers {
            builder = builder.header(k, v);
        }

        // We use reqwest_eventsource to handle the SSE stream.
        let mut es = EventSource::new(builder).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        let stream = async_stream::stream! {
            while let Some(event_res) = es.next().await {
                match event_res {
                    Ok(event) => match event {
                        Event::Open => {},
                        Event::Message(msg) => {
                            if msg.data == "[DONE]" {
                                break;
                            }
                            match serde_json::from_str::<ChatCompletionChunk>(&msg.data) {
                                Ok(chunk) => yield Ok(chunk),
                                Err(_) => {} // Skip malformed chunks like Node.js does
                            }
                        }
                    },
                    Err(e) => {
                        // Return error and abort
                        yield Err(Box::new(e) as Box<dyn Error + Send + Sync>);
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn validate_key(&self, api_key: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let default_url = format!("{}/models", self.base_url);
        let url = self.validate_url.as_deref().unwrap_or(&default_url);
        
        let mut builder = self.client.get(url).bearer_auth(api_key);
        for (k, v) in &self.extra_headers {
            builder = builder.header(k, v);
        }

        // We expect errors from fetch to bubble up.
        // We only return false if it's explicitly 401 or 403.
        let res = builder.send().await?;
        let status = res.status();
        Ok(status != reqwest::StatusCode::UNAUTHORIZED && status != reqwest::StatusCode::FORBIDDEN)
    }
}
