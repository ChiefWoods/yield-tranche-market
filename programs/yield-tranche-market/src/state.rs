use quasar_lang::prelude::*;

#[account(discriminator = 1)]
pub struct MyAccount {
    pub authority: Address,
    pub value: u64,
}
