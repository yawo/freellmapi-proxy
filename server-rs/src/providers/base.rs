use crate::models::openai::{ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, Platform};
use async_trait::async_trait;
use futures::stream::BoxStream;
use std::error::Error;

#[async_trait]
pub trait Provider: Send + Sync {
    fn platform(&self) -> Platform;
    fn name(&self) -> &str;

    async fn chat_completion(
        &self,
        api_key: &str,
        req: &ChatCompletionRequest,
        model_id: &str,
    ) -> Result<ChatCompletionResponse, Box<dyn Error + Send + Sync>>;

    async fn stream_chat_completion(
        &self,
        api_key: &str,
        req: &ChatCompletionRequest,
        model_id: &str,
    ) -> Result<BoxStream<'static, Result<ChatCompletionChunk, Box<dyn Error + Send + Sync>>>, Box<dyn Error + Send + Sync>>;

    async fn validate_key(&self, api_key: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;
}
