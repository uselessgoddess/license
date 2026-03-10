pub mod balance;
pub mod build;
pub mod cryptobot;
pub mod license;
pub mod payment;
pub mod referral;
pub mod stats;
pub mod steam;
#[cfg(test)]
pub mod test_utils;
pub mod user;

pub use balance::Balance;
pub use build::Build;
pub use license::License;
pub use payment::Payment;
pub use referral::Referral;
pub use stats::Stats;
pub use steam::Steam;
pub use user::User;

#[derive(Clone)]
pub enum Op {
  Add,
  Sub,
  Set,
}

use std::ops::{Add, Sub};

impl Op {
  #[allow(unused)]
  pub fn apply<T: Add<Output = T> + Sub<Output = T>>(
    self,
    prev: T,
    next: T,
  ) -> T {
    match self {
      Op::Add => prev + next,
      Op::Sub => prev - next,
      Op::Set => next,
    }
  }
}
