use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[sea_orm(table_name = "api_keys")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub platform: String,
    pub encrypted_key: String,
    pub iv: String,
    pub auth_tag: String,
    pub label: String,
    pub status: String, // 'healthy', 'rate_limited', 'invalid', 'error', 'unknown'
    pub enabled: i32,   // 0 or 1
    pub created_at: String,
    pub last_checked_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
