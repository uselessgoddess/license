//! Entity prelude for convenient imports

pub use super::build::{ActiveModel as BuildActiveModel, Entity as Build, Model as BuildModel};
pub use super::claimed_promo::{
  ActiveModel as ClaimedPromoActiveModel, Entity as ClaimedPromo, Model as ClaimedPromoModel,
};
pub use super::license::{
  ActiveModel as LicenseActiveModel, Entity as License, LicenseType, Model as LicenseModel,
};
pub use super::user::{ActiveModel as UserActiveModel, Entity as User, Model as UserModel};
pub use super::user_stats::{
  ActiveModel as UserStatsActiveModel, Entity as UserStats, Model as UserStatsModel,
};
