use crate::crypto::decrypt;
use crate::db::{api_keys, fallback_config, models};
use crate::models::openai::Platform;
use crate::providers::{get_provider, base::Provider};
use crate::services::ratelimit::{can_make_request, can_use_tokens, is_on_cooldown, Limits};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder, QueryFilter, ColumnTrait};
use std::collections::HashSet;
use std::time::{Duration, Instant};

// Round-robin index per platform
// Key: "{platform}:{model_id}"
static ROUND_ROBIN: Lazy<DashMap<String, usize>> = Lazy::new(|| DashMap::new());

// Rate limit penalties
#[derive(Clone, Debug)]
struct PenaltyEntry {
    count: i32,
    last_hit: Instant,
    penalty: i32,
}
static PENALTIES: Lazy<DashMap<i32, PenaltyEntry>> = Lazy::new(|| DashMap::new());

const PENALTY_PER_429: i32 = 3;
const MAX_PENALTY: i32 = 10;
const DECAY_INTERVAL: Duration = Duration::from_secs(2 * 60);
const DECAY_AMOUNT: i32 = 1;

pub fn record_rate_limit_hit(model_db_id: i32) {
    let now = Instant::now();
    let mut entry = PENALTIES.entry(model_db_id).or_insert(PenaltyEntry {
        count: 0,
        last_hit: now,
        penalty: 0,
    });
    
    entry.count += 1;
    entry.last_hit = now;
    entry.penalty = std::cmp::min(entry.penalty + PENALTY_PER_429, MAX_PENALTY);
}

pub fn record_success(model_db_id: i32) {
    if let Some(mut entry) = PENALTIES.get_mut(&model_db_id) {
        entry.penalty = std::cmp::max(0, entry.penalty - 1);
        if entry.penalty == 0 {
            drop(entry);
            PENALTIES.remove(&model_db_id);
        }
    }
}

fn get_penalty(model_db_id: i32) -> i32 {
    let mut to_remove = false;
    let penalty = if let Some(mut entry) = PENALTIES.get_mut(&model_db_id) {
        let now = Instant::now();
        let elapsed = now.duration_since(entry.last_hit);
        let decay_steps = (elapsed.as_millis() / DECAY_INTERVAL.as_millis()) as i32;
        
        if decay_steps > 0 {
            entry.penalty = std::cmp::max(0, entry.penalty - (decay_steps * DECAY_AMOUNT));
            entry.last_hit = now;
            if entry.penalty == 0 {
                to_remove = true;
            }
        }
        entry.penalty
    } else {
        0
    };

    if to_remove {
        PENALTIES.remove(&model_db_id);
    }
    
    penalty
}

pub struct RouteResult {
    pub provider: Box<dyn Provider>,
    pub model_id: String,
    pub model_db_id: i32,
    pub api_key: String,
    pub key_id: i32,
    pub platform: String,
    pub display_name: String,
}

pub async fn route_request(
    db: &DatabaseConnection,
    estimated_tokens: i32,
    skip_keys: Option<&HashSet<String>>,
    preferred_model_db_id: Option<i32>,
) -> Result<RouteResult, String> {
    
    let fallback_entries = fallback_config::Entity::find()
        .order_by_asc(fallback_config::Column::Priority)
        .all(db)
        .await
        .map_err(|e| format!("DB Error: {}", e))?;

    let mut sorted_chain: Vec<_> = fallback_entries.into_iter().map(|entry| {
        let penalty = get_penalty(entry.model_db_id);
        (entry.priority + penalty, entry)
    }).collect();

    sorted_chain.sort_by_key(|k| k.0);

    if let Some(preferred) = preferred_model_db_id {
        if let Some(idx) = sorted_chain.iter().position(|(_, e)| e.model_db_id == preferred) {
            let item = sorted_chain.remove(idx);
            sorted_chain.insert(0, item);
        }
    }

    let default_skip = HashSet::new();
    let skips = skip_keys.unwrap_or(&default_skip);

    for (_, entry) in sorted_chain {
        if entry.enabled == 0 {
            continue;
        }

        let model = models::Entity::find_by_id(entry.model_db_id)
            .filter(models::Column::Enabled.eq(1))
            .one(db)
            .await
            .map_err(|e| format!("DB Error: {}", e))?;

        let model = match model {
            Some(m) => m,
            None => continue,
        };

        // Parse Platform enum
        // Convert string to JSON string to parse it using serde
        let platform_json = format!("\"{}\"", model.platform);
        let platform_enum: Platform = match serde_json::from_str(&platform_json) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let provider = match get_provider(&platform_enum, model.base_url.clone(), model.validate_url.clone()) {
            Some(p) => p,
            None => continue,
        };

        let keys = api_keys::Entity::find()
            .filter(api_keys::Column::Platform.eq(&model.platform))
            .filter(api_keys::Column::Enabled.eq(1))
            .filter(api_keys::Column::Status.ne("invalid"))
            .all(db)
            .await
            .map_err(|e| format!("DB Error: {}", e))?;

        if keys.is_empty() {
            continue;
        }

        let rr_key = format!("{}:{}", model.platform, model.model_id);
        let mut idx = ROUND_ROBIN.get(&rr_key).map(|v| *v).unwrap_or(0);

        for _ in 0..keys.len() {
            let key = &keys[idx % keys.len()];
            idx += 1;

            let skip_id = format!("{}:{}:{}", model.platform, model.model_id, key.id);
            if skips.contains(&skip_id) {
                continue;
            }

            if is_on_cooldown(&model.platform, &model.model_id, key.id) {
                continue;
            }

            let limits = Limits {
                rpm: model.rpm_limit,
                rpd: model.rpd_limit,
                tpm: model.tpm_limit,
                tpd: model.tpd_limit,
            };

            if !can_make_request(&model.platform, &model.model_id, key.id, &limits) {
                continue;
            }
            if !can_use_tokens(&model.platform, &model.model_id, key.id, estimated_tokens, &limits) {
                continue;
            }

            ROUND_ROBIN.insert(rr_key.clone(), idx);

            let decrypted_key = decrypt(&key.encrypted_key, &key.iv, &key.auth_tag);

            return Ok(RouteResult {
                provider,
                model_id: model.model_id,
                model_db_id: model.id,
                api_key: decrypted_key,
                key_id: key.id,
                platform: model.platform,
                display_name: model.display_name,
            });
        }
        ROUND_ROBIN.insert(rr_key.clone(), idx);
    }

    Err("All models exhausted. Add more API keys or wait for rate limits to reset.".to_string())
}
