pub use std::{collections::HashMap, time::Duration};

pub use chrono::{NaiveDateTime as DateTime, TimeDelta, TimeZone, Utc};
pub use dashmap::DashMap;
pub use sea_orm::{
  ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection,
  EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
  TransactionTrait,
};
pub use sea_orm_migration::MigratorTrait;
pub use tracing::{error, info, warn};

pub use crate::error::{Error, Promo, Result};
