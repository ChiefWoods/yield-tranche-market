use fix::aliases::si::{Centi, Pico};

// matches solmath::SCALE
pub type Fix = Pico<u128>;
// for storing fields on-chain without taking up too much space
pub type StoredFix = Pico<u64>;
pub type Percentage = Centi<u16>;

pub const FIX_SCALE: u128 = solmath::SCALE;
