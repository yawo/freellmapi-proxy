use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[sea_orm(table_name = "models")]
pub struct Model {
    #[sea_orm(primary_key)]
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
    pub enabled: i32,
    pub base_url: Option<String>,
    pub validate_url: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
