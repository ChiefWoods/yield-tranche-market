use quasar_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize {
    pub payer: Signer,
    pub system_program: Program<SystemProgram>,
}

impl Initialize {
    #[inline(always)]
    pub fn initialize(&self) -> Result<(), ProgramError> {
        Ok(())
    }
}
