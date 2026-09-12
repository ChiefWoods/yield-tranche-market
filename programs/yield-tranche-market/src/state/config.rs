use quasar_lang::prelude::*;

#[account(discriminator = 1)]
#[seeds(b"config")]
pub struct Config {
    pub admin: Address,
}
