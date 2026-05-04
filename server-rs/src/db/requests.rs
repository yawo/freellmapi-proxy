use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[sea_orm(table_name = "requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub platform: String,
    pub model_id: String,
    pub status: String,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub latency_ms: i32,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
