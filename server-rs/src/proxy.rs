use crate::db::{settings, requests};
use crate::models::openai::ChatCompletionRequest;
use crate::services::ratelimit::{record_request, record_tokens};
use crate::services::router::{route_request, RouteResult, record_rate_limit_hit, record_success};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{sse::Event as SseEvent, IntoResponse, Sse, Response},
    Json,
};
use sea_orm::{DatabaseConnection, EntityTrait, ActiveValue, ActiveModelTrait};
use std::collections::HashSet;
use tokio_stream::StreamExt;
use std::time::Instant;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
}

pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChatCompletionRequest>,
) -> Result<Response, (StatusCode, String)> {
    
    // Check auth
    if let Some(auth) = headers.get("authorization") {
        let token = auth.to_str().unwrap_or("").replace("Bearer ", "");
        
        let unified_key = settings::Entity::find_by_id("unified_api_key")
            .one(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;
            
        if let Some(setting) = unified_key {
            if token != setting.value {
                return Err((StatusCode::UNAUTHORIZED, "Invalid API key".into()));
            }
        } else {
             return Err((StatusCode::INTERNAL_SERVER_ERROR, "Unified API key not initialized".into()));
        }
    } else {
        return Err((StatusCode::UNAUTHORIZED, "Missing Authorization header".into()));
    }

    let is_stream = payload.stream.unwrap_or(false);
    
    // Fallback loop
    let mut skip_keys = HashSet::new();
    let mut attempts = 0;
    const MAX_ATTEMPTS: i32 = 20;

    let start_time = Instant::now();

    while attempts < MAX_ATTEMPTS {
        attempts += 1;
        
        // Rough prompt token estimation based on content length
        let estimated_prompt_tokens = payload.messages.iter().fold(0, |acc, m| {
            if let Some(serde_json::Value::String(s)) = &m.content {
                acc + (s.len() as i32 / 4)
            } else {
                acc
            }
        });
        let estimated_tokens = estimated_prompt_tokens + 500; // Simplified estimation for request checking
        
        let route = match route_request(&state.db, estimated_tokens, Some(&skip_keys), None).await {
            Ok(r) => r,
            Err(e) => return Err((StatusCode::TOO_MANY_REQUESTS, e)),
        };

        let RouteResult { provider, model_id, model_db_id, api_key, key_id, platform, display_name: _ } = route;

        if is_stream {
            match provider.stream_chat_completion(&api_key, &payload, &model_id).await {
                Ok(mut stream) => {
                    record_success(model_db_id);
                    record_request(&platform, &model_id, key_id);
                    
                    let p_clone = platform.clone();
                    let m_clone = model_id.clone();
                    let db_clone = state.db.clone();

                    let sse_stream = async_stream::stream! {
                        let mut tokens_yielded = 0;
                        while let Some(chunk_res) = stream.next().await {
                            match chunk_res {
                                Ok(chunk) => {
                                    if let Some(choices) = chunk.choices.first() {
                                        if let Some(content) = &choices.delta.content {
                                            tokens_yielded += content.len() as i32 / 4;
                                        }
                                    }
                                    let json = serde_json::to_string(&chunk).unwrap_or_default();
                                    yield Ok::<_, axum::Error>(SseEvent::default().data(json));
                                }
                                Err(e) => {
                                    yield Ok::<_, axum::Error>(SseEvent::default().data(format!("{{\"error\": \"{}\"}}", e)));
                                    break;
                                }
                            }
                        }
                        
                        record_tokens(&p_clone, &m_clone, key_id, estimated_prompt_tokens + tokens_yielded);
                        
                        let latency_ms = start_time.elapsed().as_millis() as i32;
                        tokio::spawn(async move {
                            let log = requests::ActiveModel {
                                platform: ActiveValue::Set(p_clone),
                                model_id: ActiveValue::Set(m_clone),
                                status: ActiveValue::Set("success".into()),
                                input_tokens: ActiveValue::Set(estimated_prompt_tokens),
                                output_tokens: ActiveValue::Set(tokens_yielded),
                                latency_ms: ActiveValue::Set(latency_ms),
                                error: ActiveValue::Set(None),
                                ..Default::default()
                            };
                            let _ = log.insert(&db_clone).await;
                        });

                        yield Ok::<_, axum::Error>(SseEvent::default().data("[DONE]"));
                    };

                    let mut sse = Sse::new(sse_stream).into_response();
                    sse.headers_mut().insert("X-Routed-Via", format!("{}/{}", platform, model_id).parse().unwrap());
                    sse.headers_mut().insert("X-Fallback-Attempts", attempts.to_string().parse().unwrap());
                    
                    return Ok(sse);
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    
                    let latency_ms = start_time.elapsed().as_millis() as i32;
                    let p_clone = platform.clone();
                    let m_clone = model_id.clone();
                    let e_clone = e.to_string();
                    let db_clone = state.db.clone();
                    
                    tokio::spawn(async move {
                        let log = requests::ActiveModel {
                            platform: ActiveValue::Set(p_clone),
                            model_id: ActiveValue::Set(m_clone),
                            status: ActiveValue::Set("error".into()),
                            input_tokens: ActiveValue::Set(estimated_prompt_tokens),
                            output_tokens: ActiveValue::Set(0),
                            latency_ms: ActiveValue::Set(latency_ms),
                            error: ActiveValue::Set(Some(e_clone)),
                            ..Default::default()
                        };
                        let _ = log.insert(&db_clone).await;
                    });

                    if err_str.contains("429") || err_str.contains("50") || err_str.contains("timeout") {
                        record_rate_limit_hit(model_db_id);
                        crate::services::ratelimit::set_cooldown(&platform, &model_id, key_id, 60_000);
                        skip_keys.insert(format!("{}:{}:{}", platform, model_id, key_id));
                        continue; // try next fallback
                    }
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
                }
            }
        } else {
            match provider.chat_completion(&api_key, &payload, &model_id).await {
                Ok(resp) => {
                    record_success(model_db_id);
                    record_request(&platform, &model_id, key_id);
                    record_tokens(&platform, &model_id, key_id, resp.usage.total_tokens);
                    
                    let latency_ms = start_time.elapsed().as_millis() as i32;
                    let p_clone = platform.clone();
                    let m_clone = model_id.clone();
                    let db_clone = state.db.clone();
                    let in_tok = resp.usage.prompt_tokens;
                    let out_tok = resp.usage.completion_tokens;
                    
                    tokio::spawn(async move {
                        let log = requests::ActiveModel {
                            platform: ActiveValue::Set(p_clone),
                            model_id: ActiveValue::Set(m_clone),
                            status: ActiveValue::Set("success".into()),
                            input_tokens: ActiveValue::Set(in_tok),
                            output_tokens: ActiveValue::Set(out_tok),
                            latency_ms: ActiveValue::Set(latency_ms),
                            error: ActiveValue::Set(None),
                            ..Default::default()
                        };
                        let _ = log.insert(&db_clone).await;
                    });

                    let mut res = Json(resp).into_response();
                    res.headers_mut().insert("X-Routed-Via", format!("{}/{}", platform, model_id).parse().unwrap());
                    res.headers_mut().insert("X-Fallback-Attempts", attempts.to_string().parse().unwrap());
                    
                    return Ok(res);
                }
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    
                    let latency_ms = start_time.elapsed().as_millis() as i32;
                    let p_clone = platform.clone();
                    let m_clone = model_id.clone();
                    let e_clone = e.to_string();
                    let db_clone = state.db.clone();
                    
                    tokio::spawn(async move {
                        let log = requests::ActiveModel {
                            platform: ActiveValue::Set(p_clone),
                            model_id: ActiveValue::Set(m_clone),
                            status: ActiveValue::Set("error".into()),
                            input_tokens: ActiveValue::Set(estimated_prompt_tokens),
                            output_tokens: ActiveValue::Set(0),
                            latency_ms: ActiveValue::Set(latency_ms),
                            error: ActiveValue::Set(Some(e_clone)),
                            ..Default::default()
                        };
                        let _ = log.insert(&db_clone).await;
                    });

                    if err_str.contains("429") || err_str.contains("50") || err_str.contains("timeout") {
                        record_rate_limit_hit(model_db_id);
                        crate::services::ratelimit::set_cooldown(&platform, &model_id, key_id, 60_000);
                        skip_keys.insert(format!("{}:{}:{}", platform, model_id, key_id));
                        continue; // try next fallback
                    }
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
                }
            }
        }
    }

    Err((StatusCode::TOO_MANY_REQUESTS, "Fallback chain exhausted".into()))
}
