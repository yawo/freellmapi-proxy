pub mod base;
pub mod openai_compat;
pub mod google;

use base::Provider;
use openai_compat::OpenAICompatProvider;
use google::GoogleProvider;
use crate::models::openai::Platform;
use std::collections::HashMap;

pub fn get_provider(
    platform: &Platform,
    base_url_override: Option<String>,
    validate_url_override: Option<String>,
) -> Option<Box<dyn Provider>> {
    match platform {
        Platform::Google => Some(Box::new(GoogleProvider::new())),
        Platform::Groq => Some(Box::new(OpenAICompatProvider::new(
            Platform::Groq,
            "Groq".into(),
            base_url_override.unwrap_or_else(|| "https://api.groq.com/openai/v1".into()),
            None,
            validate_url_override,
            None,
        ))),
        Platform::Cerebras => Some(Box::new(OpenAICompatProvider::new(
            Platform::Cerebras,
            "Cerebras".into(),
            base_url_override.unwrap_or_else(|| "https://api.cerebras.ai/v1".into()),
            None,
            validate_url_override,
            None,
        ))),
        Platform::Mistral => Some(Box::new(OpenAICompatProvider::new(
            Platform::Mistral,
            "Mistral".into(),
            base_url_override.unwrap_or_else(|| "https://api.mistral.ai/v1".into()),
            None,
            validate_url_override,
            None,
        ))),
        Platform::Openrouter => {
            let mut headers = HashMap::new();
            headers.insert("HTTP-Referer".into(), "https://freellmapi.local".into());
            headers.insert("X-Title".into(), "FreeLLMAPI".into());
            Some(Box::new(OpenAICompatProvider::new(
                Platform::Openrouter,
                "OpenRouter".into(),
                base_url_override.unwrap_or_else(|| "https://openrouter.ai/api/v1".into()),
                Some(headers),
                validate_url_override.or_else(|| Some("https://openrouter.ai/api/v1/auth/key".into())),
                None,
            )))
        },
        Platform::Github => Some(Box::new(OpenAICompatProvider::new(
            Platform::Github,
            "GitHub Models".into(),
            base_url_override.unwrap_or_else(|| "https://models.inference.ai.azure.com".into()),
            None,
            validate_url_override,
            None,
        ))),
        Platform::Sambanova => Some(Box::new(OpenAICompatProvider::new(
            Platform::Sambanova,
            "SambaNova".into(),
            base_url_override.unwrap_or_else(|| "https://api.sambanova.ai/v1".into()),
            None,
            validate_url_override,
            None,
        ))),
        Platform::Cloudflare => Some(Box::new(OpenAICompatProvider::new(
            Platform::Cloudflare,
            "Cloudflare".into(),
            base_url_override.unwrap_or_else(|| "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1".into()),
            None,
            validate_url_override,
            None,
        ))),
        Platform::Custom(name) => {
            if let Some(url) = base_url_override {
                Some(Box::new(OpenAICompatProvider::new(
                    Platform::Custom(name.clone()),
                    name.clone(),
                    url,
                    None,
                    validate_url_override,
                    None,
                )))
            } else {
                None
            }
        },
        _ => {
            // Any other variants that might be added to Platform but not handled explicitly yet
            if let Some(url) = base_url_override {
                let name = platform.to_string();
                Some(Box::new(OpenAICompatProvider::new(
                    platform.clone(),
                    name,
                    url,
                    None,
                    validate_url_override,
                    None,
                )))
            } else {
                None
            }
        }
    }
}
