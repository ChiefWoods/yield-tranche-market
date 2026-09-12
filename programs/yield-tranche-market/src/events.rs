use quasar_lang::prelude::*;

// no event for create_config

#[event(discriminator = 0)]
#[repr(C, packed)]
pub struct MarketCreated {
    pub market: Address,
    pub underlying_mint: Address,
    pub senior_mint: Address,
    pub junior_mint: Address,
    pub min_coverage: u16,
    pub junior_exposure_beta: u16,
    pub slot: u64,
}

#[event(discriminator = 1)]
#[repr(C, packed)]
pub struct MarketRefreshed {
    pub market: Address,
    pub exchange_rate: u128,
    pub slot: u64,
}

#[event(discriminator = 2)]
#[repr(C, packed)]
pub struct Deposited {
    pub market: Address,
    pub authority: Address,
    pub is_senior: bool,
    pub amount_in: u64,
    pub amount_out: u64,
    pub slot: u64,
}

#[event(discriminator = 3)]
#[repr(C, packed)]
pub struct Withdrawn {
    pub market: Address,
    pub authority: Address,
    pub is_senior: bool,
    pub amount_in: u64,
    pub amount_out: u64,
    pub slot: u64,
}
