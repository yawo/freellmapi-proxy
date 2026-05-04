use sea_orm::{DatabaseConnection, EntityTrait, ActiveModelTrait, ActiveValue, ColumnTrait, QueryFilter};
use std::time::Duration;
use tokio::time::sleep;
use crate::db::{api_keys, models};
use crate::providers::get_provider;
use crate::models::openai::Platform;
use crate::crypto::decrypt;

pub async fn run_health_checks(db: DatabaseConnection) {
    loop {
        // Sleep for 5 minutes between runs
        sleep(Duration::from_secs(300)).await;

        if let Ok(keys) = api_keys::Entity::find().all(&db).await {
            for key in keys {
                if key.enabled == 0 {
                    continue;
                }

                // Find a model that uses this platform to get its base_url
                // Since base_url is on the model level now, we pick the first one for this platform
                let model = models::Entity::find()
                    .filter(models::Column::Platform.eq(&key.platform))
                    .one(&db)
                    .await
                    .ok()
                    .flatten();

                let (base_url, validate_url) = if let Some(m) = model {
                    (m.base_url, m.validate_url)
                } else {
                    (None, None)
                };

                // Parse platform
                let platform_json = format!("\"{}\"", key.platform);
                let platform_enum: Platform = match serde_json::from_str(&platform_json) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let provider = match get_provider(&platform_enum, base_url, validate_url) {
                    Some(p) => p,
                    None => continue,
                };

                let decrypted_key = decrypt(&key.encrypted_key, &key.iv, &key.auth_tag);
                
                let is_valid = match provider.validate_key(&decrypted_key).await {
                    Ok(v) => v,
                    Err(_) => {
                        // Transport errors are marked as "error" without disabling
                        let mut active: api_keys::ActiveModel = key.into();
                        active.status = ActiveValue::Set("error".into());
                        let _ = active.update(&db).await;
                        continue;
                    }
                };

                let mut active: api_keys::ActiveModel = key.into();
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                active.last_checked_at = ActiveValue::Set(Some(format!("{}", now)));

                if is_valid {
                    // Just reset status to healthy if it was invalid
                    if active.status.as_ref() == "invalid" {
                        active.status = ActiveValue::Set("healthy".into());
                    } else if active.status.as_ref() != "healthy" && active.status.as_ref() != "rate_limited" {
                         active.status = ActiveValue::Set("healthy".into());
                    }
                } else {
                    active.status = ActiveValue::Set("invalid".into());
                }

                let _ = active.update(&db).await;
            }
        }
    }
}
