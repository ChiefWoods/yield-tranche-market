use quasar_lang::prelude::*;
use solana_math::SafeMathError;
use solmath::SolMathError;
use yield_tranche_market_core::errors::CoreError;

#[error_code]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum YieldTrancheMarketError {
    /// Source account list has an unexpected length.
    InvalidAccountCount,
    /// Source account is not owned by its expected program.
    InvalidAccountOwner,
    /// Source account does not match its expected PDA or associated address.
    InvalidAccountAddress,
    /// Source account data is malformed or has an unexpected layout.
    InvalidAccountData,
    /// Instruction data is truncated or has an unexpected layout.
    InvalidInstructionData,
    /// Admin does not match that of the config.
    InvalidAdmin,
    /// Underlying mint does not match that of the market.
    InvalidUnderlyingMint,
    /// Source backing or receipt-token supply is zero.
    InvalidSupply,
    /// Tranche-model configuration or model-specific state is invalid.
    InvalidTrancheModel,
    /// A checked arithmetic operation overflowed.
    ArithmeticOverflow,
    /// The clock sysvar could not be read.
    CannotReadSysvar,
    /// Source must be between 0 and 1.
    InvalidSource,
    /// Market coverage configuration is invalid.
    InvalidCoverageConfiguration,
    /// Amount must be greater than zero.
    InvalidAmount,
    /// Tranche effective NAV must be greater than zero when supply is outstanding.
    TrancheHasNoEffectiveNav,
    /// Withdrawal exceeds this tranche's raw or effective NAV.
    WithdrawalExceedsNav,
    /// Market coverage is less than the configured minimum.
    InsufficientCoverage,
    /// Amount out is less than the minimum allowed.
    SlippageExceeded,
    /// Senior mint does not match that of the market.
    InvalidSeniorMint,
    /// Junior mint does not match that of the market.
    InvalidJuniorMint,
    /// Tranche mint supply must be greater than zero.
    TrancheMintHasNoSupply,
    /// Exchange rate cannot be zero.
    InvalidExchangeRate,
    /// Market vault does not have enough balance for withdrawals.
    InsufficientVaultBalance,
    /// Market has not been refreshed within the permitted slot tolerance.
    MarketStale,
}

impl From<SafeMathError> for YieldTrancheMarketError {
    fn from(_error: SafeMathError) -> Self {
        Self::ArithmeticOverflow
    }
}

impl From<CoreError> for YieldTrancheMarketError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::ArithmeticOverflow => Self::ArithmeticOverflow,
            CoreError::InvalidTrancheModel
            | CoreError::InvalidPointCurve
            | CoreError::InvalidUtilizationGuidedCurve
            | CoreError::InvalidDynamicLeverage
            | CoreError::InvalidSubsidy
            | CoreError::InvalidTimestamp
            | CoreError::UnsupportedOperation => Self::InvalidTrancheModel,
        }
    }
}

impl From<SolMathError> for YieldTrancheMarketError {
    fn from(_error: SolMathError) -> Self {
        Self::ArithmeticOverflow
    }
}
