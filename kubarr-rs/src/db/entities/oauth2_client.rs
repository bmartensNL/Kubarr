use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "oauth2_clients")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub client_id: String,
    #[serde(skip_serializing)]
    pub client_secret_hash: String,
    pub name: String,
    pub redirect_uris: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::oauth2_authorization_code::Entity")]
    AuthorizationCodes,
    #[sea_orm(has_many = "super::oauth2_token::Entity")]
    Tokens,
}

impl Related<super::oauth2_authorization_code::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AuthorizationCodes.def()
    }
}

impl Related<super::oauth2_token::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Tokens.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
