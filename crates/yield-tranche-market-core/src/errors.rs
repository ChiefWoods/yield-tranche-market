use solana_math::SafeMathError;
use solmath::SolMathError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreError {
    InvalidTrancheModel,
    InvalidPointCurve,
    InvalidUtilizationGuidedCurve,
    InvalidDynamicLeverage,
    InvalidSubsidy,
    InvalidTimestamp,
    UnsupportedOperation,
    ArithmeticOverflow,
}

impl From<SafeMathError> for CoreError {
    fn from(_: SafeMathError) -> Self {
        Self::ArithmeticOverflow
    }
}

impl From<SolMathError> for CoreError {
    fn from(_: SolMathError) -> Self {
        Self::ArithmeticOverflow
    }
}
