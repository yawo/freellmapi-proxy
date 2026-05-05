
use axum::{
    extract::{State, Path, Query},
    http::StatusCode,
    routing::{get, post, put, delete},
    Json, Router,
};
use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait, QueryOrder, QuerySelect, QueryFilter, ColumnTrait};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rand::Rng;
use rand::distributions::Alphanumeric;

use crate::db::{api_keys, fallback_config, models, settings, requests};
use crate::crypto::{encrypt, mask_key};
use crate::proxy::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyDto {
    pub id: i32,
    pub platform: String,
    pub label: String,
    pub masked_key: String,
    pub status: String,
    pub enabled: bool,
    pub created_at: String,
    pub last_checked_at: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateKeyDto {
    pub platform: String,
    pub key: String,
    pub label: Option<String>,
}

pub async fn list_keys(State(state): State<AppState>) -> Result<Json<Vec<ApiKeyDto>>, (StatusCode, String)> {
    let keys = api_keys::Entity::find()
        .order_by_desc(api_keys::Column::Id)
        .all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;

    let dtos = keys.into_iter().map(|k| ApiKeyDto {
        id: k.id,
        platform: k.platform.clone(),
        label: k.label.clone(),
        masked_key: mask_key(&crate::crypto::decrypt(&k.encrypted_key, &k.iv, &k.auth_tag)),
        status: k.status.clone(),
        enabled: k.enabled == 1,
        created_at: k.created_at.clone(),
        last_checked_at: k.last_checked_at.clone(),
    }).collect();

    Ok(Json(dtos))
}

pub async fn create_key(State(state): State<AppState>, Json(payload): Json<CreateKeyDto>) -> Result<Json<ApiKeyDto>, (StatusCode, String)> {
    if payload.key.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Key is required".into()));
    }

    let encrypted = encrypt(&payload.key);
    
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let now_str = format!("{}", now); // Ideally ISO8601, simplified for now

    let new_key = api_keys::ActiveModel {
        platform: ActiveValue::Set(payload.platform.clone()),
        label: ActiveValue::Set(payload.label.unwrap_or_default()),
        encrypted_key: ActiveValue::Set(encrypted.encrypted),
        iv: ActiveValue::Set(encrypted.iv),
        auth_tag: ActiveValue::Set(encrypted.auth_tag),
        status: ActiveValue::Set("unknown".into()),
        enabled: ActiveValue::Set(1),
        created_at: ActiveValue::Set(now_str),
        ..Default::default()
    };

    let inserted = new_key.insert(&state.db).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;

    Ok(Json(ApiKeyDto {
        id: inserted.id,
        platform: inserted.platform.clone(),
        label: inserted.label.clone(),
        masked_key: mask_key(&payload.key),
        status: inserted.status.clone(),
        enabled: inserted.enabled == 1,
        created_at: inserted.created_at.clone(),
        last_checked_at: inserted.last_checked_at.clone(),
    }))
}

pub async fn toggle_key(State(state): State<AppState>, Path(id): Path<i32>) -> Result<Json<bool>, (StatusCode, String)> {
    let key = api_keys::Entity::find_by_id(id).one(&state.db).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;
        
    if let Some(key) = key {
        let mut active: api_keys::ActiveModel = key.into();
        active.enabled = ActiveValue::Set(if active.enabled.unwrap() == 1 { 0 } else { 1 });
        active.update(&state.db).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;
        Ok(Json(true))
    } else {
        Err((StatusCode::NOT_FOUND, "Key not found".into()))
    }
}

pub async fn delete_key(State(state): State<AppState>, Path(id): Path<i32>) -> Result<Json<bool>, (StatusCode, String)> {
    let res = api_keys::Entity::delete_by_id(id).exec(&state.db).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;
    
    if res.rows_affected > 0 {
        Ok(Json(true))
    } else {
        Err((StatusCode::NOT_FOUND, "Key not found".into()))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDto {
    pub id: i32,
    pub platform: String,
    pub model_id: String,
    pub display_name: String,
    pub intelligence_rank: i32,
    pub speed_rank: i32,
    pub size_label: String,
    pub rpm_limit: Option<i32>,
    pub rpd_limit: Option<i32>,
    pub tpm_limit: Option<i32>,
    pub tpd_limit: Option<i32>,
    pub monthly_token_budget: String,
    pub context_window: Option<i32>,
    pub enabled: bool,
    pub base_url: Option<String>,
    pub validate_url: Option<String>,
}

pub async fn list_models(State(state): State<AppState>) -> Result<Json<Vec<ModelDto>>, (StatusCode, String)> {
    let models_from_db = models::Entity::find()
        .all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;

    let dtos = models_from_db.into_iter().map(|m| ModelDto {
        id: m.id,
        platform: m.platform,
        model_id: m.model_id,
        display_name: m.display_name,
        intelligence_rank: m.intelligence_rank,
        speed_rank: m.speed_rank,
        size_label: m.size_label,
        rpm_limit: m.rpm_limit,
        rpd_limit: m.rpd_limit,
        tpm_limit: m.tpm_limit,
        tpd_limit: m.tpd_limit,
        monthly_token_budget: m.monthly_token_budget,
        context_window: m.context_window,
        enabled: m.enabled == 1,
        base_url: m.base_url,
        validate_url: m.validate_url,
    }).collect();

    Ok(Json(dtos))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateModelDto {
    pub platform: String,
    pub model_id: String,
    pub display_name: String,
    pub base_url: Option<String>,
    pub validate_url: Option<String>,
    pub intelligence_rank: Option<i32>,
    pub speed_rank: Option<i32>,
}

pub async fn create_model(State(state): State<AppState>, Json(payload): Json<CreateModelDto>) -> Result<Json<models::Model>, (StatusCode, String)> {
    let new_model = models::ActiveModel {
        platform: ActiveValue::Set(payload.platform),
        model_id: ActiveValue::Set(payload.model_id),
        display_name: ActiveValue::Set(payload.display_name),
        base_url: ActiveValue::Set(payload.base_url),
        validate_url: ActiveValue::Set(payload.validate_url),
        intelligence_rank: ActiveValue::Set(payload.intelligence_rank.unwrap_or(10)),
        speed_rank: ActiveValue::Set(payload.speed_rank.unwrap_or(10)),
        size_label: ActiveValue::Set("Custom".into()),
        monthly_token_budget: ActiveValue::Set("unknown".into()),
        enabled: ActiveValue::Set(1),
        ..Default::default()
    };

    let inserted = new_model.insert(&state.db).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;

    Ok(Json(inserted))
}

pub async fn delete_model(State(state): State<AppState>, Path(id): Path<i32>) -> Result<Json<bool>, (StatusCode, String)> {
    let res = models::Entity::delete_by_id(id).exec(&state.db).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;
    
    if res.rows_affected > 0 {
        Ok(Json(true))
    } else {
        Err((StatusCode::NOT_FOUND, "Model not found".into()))
    }
}

pub async fn list_fallback_chain(State(state): State<AppState>) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let fallbacks = fallback_config::Entity::find()
        .order_by_asc(fallback_config::Column::Priority)
        .all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;
        
    let all_models = models::Entity::find()
        .all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;
        
    let all_keys = api_keys::Entity::find()
        .all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;
        
    let mut res = Vec::new();
    for f in fallbacks {
        if let Some(m) = all_models.iter().find(|x| x.id == f.model_db_id) {
            let key_count = all_keys.iter().filter(|k| k.platform == m.platform && k.enabled == 1).count();
            
            res.push(serde_json::json!({
                "modelDbId": m.id,
                "platform": m.platform,
                "modelId": m.model_id,
                "displayName": m.display_name,
                "intelligenceRank": m.intelligence_rank,
                "speedRank": m.speed_rank,
                "sizeLabel": m.size_label,
                "rpmLimit": m.rpm_limit,
                "rpdLimit": m.rpd_limit,
                "monthlyTokenBudget": m.monthly_token_budget,
                "priority": f.priority,
                "enabled": f.enabled == 1,
                "keyCount": key_count,
                "penalty": 0, // Simplified for now
                "rateLimitHits": 0,
            }));
        }
    }
    
    Ok(Json(res))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub api_key: String,
}

pub async fn get_api_key(State(state): State<AppState>) -> Result<Json<SettingsDto>, (StatusCode, String)> {
    let k = settings::Entity::find_by_id("unified_api_key").one(&state.db).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;
        
    Ok(Json(SettingsDto {
        api_key: k.map(|x| x.value).unwrap_or_default(),
    }))
}

pub async fn regenerate_api_key(State(state): State<AppState>) -> Result<Json<SettingsDto>, (StatusCode, String)> {
    let new_key = format!("freellmapi-{}", rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect::<String>());

    let setting = settings::ActiveModel {
        key: ActiveValue::Set("unified_api_key".to_string()),
        value: ActiveValue::Set(new_key.clone()),
    };

    settings::Entity::update(setting).exec(&state.db).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;

    Ok(Json(SettingsDto {
        api_key: new_key,
    }))
}


#[derive(Deserialize)]
pub struct AnalyticsQuery {
    #[serde(default)]
    pub days: u32,
}

pub async fn get_analytics(State(state): State<AppState>, Query(q): Query<AnalyticsQuery>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let _days = if q.days == 0 { 7 } else { q.days };
    
    let reqs = requests::Entity::find()
        .order_by_desc(requests::Column::Id)
        .limit(1000)
        .all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB Error: {}", e)))?;

    let total_requests = reqs.len() as u32;
    let mut success_count = 0;
    let mut total_input_tokens = 0;
    let mut total_output_tokens = 0;
    let mut latency_sum = 0;

    let mut platform_stats: HashMap<String, serde_json::Value> = HashMap::new();

    for r in &reqs {
        if r.status == "success" {
            success_count += 1;
        }
        total_input_tokens += r.input_tokens;
        total_output_tokens += r.output_tokens;
        latency_sum += r.latency_ms;

        let p_stat = platform_stats.entry(r.platform.clone()).or_insert(serde_json::json!({
            "platform": r.platform.clone(),
            "requests": 0,
            "successRate": 0,
            "avgLatencyMs": 0,
            "totalInputTokens": 0,
            "totalOutputTokens": 0,
        }));
        p_stat["requests"] = serde_json::Value::Number((p_stat["requests"].as_i64().unwrap_or(0) + 1).into());
    }

    let avg_latency = if total_requests > 0 { latency_sum / total_requests as i32 } else { 0 };
    let success_rate = if total_requests > 0 { (success_count as f64 / total_requests as f64) * 100.0 } else { 0.0 };

    Ok(Json(serde_json::json!({
        "summary": {
            "totalRequests": total_requests,
            "successRate": success_rate as u32,
            "totalInputTokens": total_input_tokens,
            "totalOutputTokens": total_output_tokens,
            "avgLatencyMs": avg_latency,
            "estimatedCostSavings": 0,
        },
        "platforms": platform_stats.values().collect::<Vec<_>>(),
        "timeline": []
    })))
}

pub async fn health_check() -> (StatusCode, &'static str) {
    (StatusCode::OK, "OK")
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/keys", get(list_keys).post(create_key))
        .route("/keys/{id}/toggle", put(toggle_key))
        .route("/keys/{id}", delete(delete_key))
        .route("/models", get(list_models).post(create_model))
        .route("/models/{id}", delete(delete_model))
        .route("/fallback", get(list_fallback_chain))
        .route("/settings/api-key", get(get_api_key))
        .route("/settings/api-key/regenerate", post(regenerate_api_key))
        .route("/analytics", get(get_analytics))
        .route("/health", get(health_check))
}
