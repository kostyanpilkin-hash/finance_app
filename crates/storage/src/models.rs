//! Typed rows shared across layers. `sync` fields: version/updated_at/updated_by_device/
//! deleted_at/seq/local_dirty exist on every syncable entity.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Entity {
    Users,
    Accounts,
    Categories,
    Transactions,
    Attachments,
    Goals,
    Reminders,
    ReminderEvents,
    ExchangeRates,
    SharedSettings,
    AuditLog,
}

impl Entity {
    pub fn table(self) -> &'static str {
        match self {
            Entity::Users => "users",
            Entity::Accounts => "accounts",
            Entity::Categories => "categories",
            Entity::Transactions => "transactions",
            Entity::Attachments => "attachments",
            Entity::Goals => "goals",
            Entity::Reminders => "reminders",
            Entity::ReminderEvents => "reminder_events",
            Entity::ExchangeRates => "exchange_rates",
            Entity::SharedSettings => "shared_settings",
            Entity::AuditLog => "audit_log",
        }
    }

    /// Whether `deleted_at` exists on the table (soft-delete entities).
    pub fn has_deleted_at(self) -> bool {
        !matches!(
            self,
            Entity::AuditLog | Entity::ExchangeRates | Entity::SharedSettings
        )
    }

    /// Whether the entity is replicated to clients via pull.
    pub fn replicated(self) -> bool {
        true
    }

    pub fn all() -> &'static [Entity] {
        &[
            Entity::Users,
            Entity::Accounts,
            Entity::Categories,
            Entity::Transactions,
            Entity::Attachments,
            Entity::Goals,
            Entity::Reminders,
            Entity::ReminderEvents,
            Entity::ExchangeRates,
            Entity::SharedSettings,
            Entity::AuditLog,
        ]
    }

    pub fn parse(s: &str) -> Option<Entity> {
        Entity::all().iter().copied().find(|e| e.table() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Upsert,
    Delete,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Desktop,
    Bot,
    Sync,
    System,
    Reminder,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Desktop => "desktop",
            Source::Bot => "bot",
            Source::Sync => "sync",
            Source::System => "system",
            Source::Reminder => "reminder",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub name: String,
    pub color: String,
    pub sort: i64,
    #[serde(flatten)]
    pub sync: SyncFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFields {
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub updated_by_device: Option<String>,
    pub deleted_at: Option<String>,
    pub seq: i64,
    #[serde(default)]
    pub local_dirty: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub kind: String, // card|cash|savings|other
    pub last4: Option<String>,
    pub currency: String,
    pub color: String,
    pub owner_user_id: Option<String>,
    pub include_in_total: i64,
    pub archived: i64,
    pub sort: i64,
    #[serde(flatten)]
    pub sync: SyncFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    pub key: Option<String>, // i18n key for seeded categories (categories.<key>)
    pub kind: String,        // expense|income
    pub icon: String,
    pub color: String,
    pub archived: i64,
    pub is_system: i64,
    pub sort: i64,
    #[serde(flatten)]
    pub sync: SyncFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub id: String,
    pub kind: String, // expense|income|transfer
    pub occurred_at: String,
    pub account_id: String,
    pub amount_minor: i64,
    pub category_id: Option<String>,
    pub to_account_id: Option<String>,
    pub to_amount_minor: Option<i64>,
    pub comment: String,
    pub created_by_user_id: Option<String>,
    /// Whose expense this is (kind=expense); NULL elsewhere. Analytics groups by it.
    pub spent_by_user_id: Option<String>,
    pub source: String,
    pub device_id: Option<String>,
    pub tg_message_id: Option<i64>,
    #[serde(flatten)]
    pub sync: SyncFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub transaction_id: String,
    pub sha256: String,
    pub file_name: String,
    pub mime: String,
    pub size: i64,
    #[serde(flatten)]
    pub sync: SyncFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub id: String,
    pub name: String,
    pub target_minor: i64,
    pub current_minor: i64,
    pub currency: String,
    pub deadline: Option<String>,
    pub color: String,
    pub icon: String,
    pub archived: i64,
    #[serde(flatten)]
    pub sync: SyncFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub name: String,
    pub amount_minor: Option<i64>,
    pub currency: String,
    pub account_id: Option<String>,
    pub category_id: Option<String>,
    pub kind: String,
    pub rule_type: String,
    pub rule_day: i64,
    pub rule_month: Option<i64>,
    pub next_due: String,
    pub active: i64,
    #[serde(flatten)]
    pub sync: SyncFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderEvent {
    pub id: String,
    pub reminder_id: String,
    pub due_date: String,
    pub status: String,
    pub transaction_id: Option<String>,
    #[serde(flatten)]
    pub sync: SyncFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeRate {
    pub date: String,
    pub currency: String,
    pub rate_byn_scaled: i64,
    pub scale: i64,
    #[serde(flatten)]
    pub sync: SyncFields,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: String,
    pub ts: String,
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub source: String,
    pub entity: String,
    pub entity_id: String,
    pub action: String,
    pub before_json: Option<String>,
    pub after_json: Option<String>,
    pub changes_json: Option<String>,
    #[serde(flatten)]
    pub sync: SyncFields,
}
