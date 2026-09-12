use quasar_lang::prelude::*;

use crate::state::Config;

#[derive(Accounts)]
pub struct CreateConfig {
    #[account(mut)]
    pub admin: Signer,
    #[account(
        init,
        payer = admin,
        address = Config::seeds()
    )]
    pub config: Account<Config>,
    pub system_program: Program<SystemProgram>,
}

impl CreateConfig {
    pub fn handler(&mut self) -> Result<(), ProgramError> {
        self.config.admin = *self.admin.address();
        Ok(())
    }
}
