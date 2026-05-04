use dashmap::DashMap;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

const MINUTE: Duration = Duration::from_secs(60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

// Key format: "platform:modelId:keyId:type"
// type is rpm|rpd|tpm|tpd
static TIMESTAMPS: Lazy<DashMap<String, Vec<Instant>>> = Lazy::new(|| DashMap::new());

#[derive(Clone, Debug)]
struct TokenUsage {
    ts: Instant,
    tokens: i32,
}
static TOKEN_TIMESTAMPS: Lazy<DashMap<String, Vec<TokenUsage>>> = Lazy::new(|| DashMap::new());

// Cooldown: when a provider returns 429, block that model+key for a period
// key -> expiry Instant
static COOLDOWNS: Lazy<DashMap<String, Instant>> = Lazy::new(|| DashMap::new());

pub struct Limits {
    pub rpm: Option<i32>,
    pub rpd: Option<i32>,
    pub tpm: Option<i32>,
    pub tpd: Option<i32>,
}

fn prune_timestamps(key: &str, window: Duration, now: Instant) -> usize {
    let mut count = 0;
    if let Some(mut entry) = TIMESTAMPS.get_mut(key) {
        let cutoff = now.checked_sub(window).unwrap_or(now);
        entry.retain(|ts| *ts > cutoff);
        count = entry.len();
    }
    count
}

fn prune_token_timestamps(key: &str, window: Duration, now: Instant) -> i32 {
    let mut total_tokens = 0;
    if let Some(mut entry) = TOKEN_TIMESTAMPS.get_mut(key) {
        let cutoff = now.checked_sub(window).unwrap_or(now);
        entry.retain(|t| t.ts > cutoff);
        total_tokens = entry.iter().map(|t| t.tokens).sum();
    }
    total_tokens
}

pub fn can_make_request(platform: &str, model_id: &str, key_id: i32, limits: &Limits) -> bool {
    let now = Instant::now();

    if let Some(rpm_limit) = limits.rpm {
        let key = format!("{}:{}:{}:rpm", platform, model_id, key_id);
        if prune_timestamps(&key, MINUTE, now) >= rpm_limit as usize {
            return false;
        }
    }

    if let Some(rpd_limit) = limits.rpd {
        let key = format!("{}:{}:{}:rpd", platform, model_id, key_id);
        if prune_timestamps(&key, DAY, now) >= rpd_limit as usize {
            return false;
        }
    }

    true
}

pub fn can_use_tokens(platform: &str, model_id: &str, key_id: i32, estimated_tokens: i32, limits: &Limits) -> bool {
    let now = Instant::now();

    if let Some(tpm_limit) = limits.tpm {
        let key = format!("{}:{}:{}:tpm", platform, model_id, key_id);
        let used = prune_token_timestamps(&key, MINUTE, now);
        if used + estimated_tokens > tpm_limit {
            return false;
        }
    }

    if let Some(tpd_limit) = limits.tpd {
        let key = format!("{}:{}:{}:tpd", platform, model_id, key_id);
        let used = prune_token_timestamps(&key, DAY, now);
        if used + estimated_tokens > tpd_limit {
            return false;
        }
    }

    true
}

pub fn record_request(platform: &str, model_id: &str, key_id: i32) {
    let now = Instant::now();

    let rpm_key = format!("{}:{}:{}:rpm", platform, model_id, key_id);
    TIMESTAMPS.entry(rpm_key).or_default().push(now);

    let rpd_key = format!("{}:{}:{}:rpd", platform, model_id, key_id);
    TIMESTAMPS.entry(rpd_key).or_default().push(now);
}

pub fn record_tokens(platform: &str, model_id: &str, key_id: i32, tokens: i32) {
    let now = Instant::now();

    let tpm_key = format!("{}:{}:{}:tpm", platform, model_id, key_id);
    TOKEN_TIMESTAMPS.entry(tpm_key).or_default().push(TokenUsage { ts: now, tokens });

    let tpd_key = format!("{}:{}:{}:tpd", platform, model_id, key_id);
    TOKEN_TIMESTAMPS.entry(tpd_key).or_default().push(TokenUsage { ts: now, tokens });
}

pub fn set_cooldown(platform: &str, model_id: &str, key_id: i32, duration_ms: u64) {
    let key = format!("{}:{}:{}:cooldown", platform, model_id, key_id);
    let expiry = Instant::now() + Duration::from_millis(duration_ms);
    COOLDOWNS.insert(key, expiry);
}

pub fn is_on_cooldown(platform: &str, model_id: &str, key_id: i32) -> bool {
    let key = format!("{}:{}:{}:cooldown", platform, model_id, key_id);
    if let Some(expiry) = COOLDOWNS.get(&key) {
        if Instant::now() > *expiry {
            drop(expiry); // release lock
            COOLDOWNS.remove(&key);
            false
        } else {
            true
        }
    } else {
        false
    }
}
