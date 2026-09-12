use core::mem::MaybeUninit;

use crate::{
    errors::CoreError,
    fixed::{Fix, Percentage, FIX_SCALE},
    utils::{read_i64, read_percentage, read_u128},
};
use fix::aliases::si::{Centi, Unit};
use solana_math::{
    impls::safe_fix_math::{SafeFixAddSub, SafeFixMulDiv},
    SafeMath,
};
use solmath::{exp_fixed_i, fp_div, fp_mul, fp_mul_i};

pub const MAX_POINT_CURVE_POINTS: usize = 3;

fn percentage_to_fix(value: Percentage) -> Fix {
    Centi::new(u128::from(value.bits)).convert()
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CurvePoint {
    pub utilization: Percentage,
    pub junior_share: Percentage,
}

const _: () = assert!(core::mem::size_of::<CurvePoint>() == 4);

impl CurvePoint {
    pub const fn new(utilization: Percentage, junior_share: Percentage) -> Self {
        Self {
            utilization,
            junior_share,
        }
    }

    const fn is_null(self) -> bool {
        self.utilization.bits == 0 && self.junior_share.bits == 0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointCurve {
    pub points: [CurvePoint; MAX_POINT_CURVE_POINTS],
}

impl PointCurve {
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut active = 0;
        let mut previous: Option<CurvePoint> = None;
        let mut found_null = false;

        for point in self.points {
            // null points indicate the end of the curve
            if point.is_null() {
                found_null = true;
                continue;
            }
            // null point cannot be followed by a non-null point
            if found_null
                || point.utilization.bits == 0
                || !is_percentage_fraction(point.utilization)
                || !is_percentage_fraction(point.junior_share)
            {
                return Err(CoreError::InvalidPointCurve);
            }
            if let Some(previous) = previous {
                // each following point must increase utilization and not decrease junior share
                if point.utilization.bits <= previous.utilization.bits
                    || point.junior_share.bits < previous.junior_share.bits
                {
                    return Err(CoreError::InvalidPointCurve);
                }
            }
            previous = Some(point);
            active += 1;
        }

        // at least two points are required
        if active < 2 {
            Err(CoreError::InvalidPointCurve)
        } else {
            Ok(())
        }
    }

    /// J(U) = J0 + (J1 - J0) × (U - U0) / (U1 - U0)
    ///
    /// where:
    /// - J(U) is the junior share at utilization U
    /// - J0 is the junior share at utilization U0
    /// - J1 is the junior share at utilization U1
    /// - U is the utilization
    /// - U0 is the utilization of the first point
    /// - U1 is the utilization of the last point
    pub fn quote_junior_share(&self, utilization: Fix) -> Result<Fix, CoreError> {
        self.validate()?;
        let utilization = Fix::new(utilization.bits.min(FIX_SCALE));
        let active = self
            .points
            .iter()
            .take_while(|point| !point.is_null())
            .count();
        let first = self.points[0];
        let last = self.points[active - 1];
        if utilization.bits <= percentage_to_fix(first.utilization).bits {
            return Ok(percentage_to_fix(first.junior_share));
        }
        if utilization.bits >= percentage_to_fix(last.utilization).bits {
            return Ok(percentage_to_fix(last.junior_share));
        }
        for pair in self.points[..active].windows(2) {
            let lower = pair[0];
            let upper = pair[1];
            let lower_utilization = percentage_to_fix(lower.utilization);
            let upper_utilization = percentage_to_fix(upper.utilization);
            if utilization.bits <= upper_utilization.bits {
                let progress = fp_div(
                    utilization.bits - lower_utilization.bits,
                    upper_utilization.bits - lower_utilization.bits,
                )?;
                return percentage_to_fix(lower.junior_share)
                    .safe_add(Fix::new(fp_mul(
                        percentage_to_fix(upper.junior_share).bits
                            - percentage_to_fix(lower.junior_share).bits,
                        progress,
                    )?))
                    .map_err(Into::into);
            }
        }
        Ok(percentage_to_fix(last.junior_share))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UtilizationGuidedCurve {
    pub target_utilization: Percentage,
    pub initial_junior_share_at_target: Percentage,
    pub min_junior_share_at_target: Percentage,
    pub max_target_shift_speed: Fix,
    pub zero_utilization_junior_share_discount: Percentage,
    pub full_utilization_junior_share_premium: Percentage,
    pub current_junior_share_at_target: Percentage,
    pub last_target_shift_ts: i64,
}

impl UtilizationGuidedCurve {
    fn validate_config(&self) -> Result<(), CoreError> {
        if self.target_utilization.bits == 0
            || self.target_utilization.bits >= 100
            || !is_percentage_fraction(self.initial_junior_share_at_target)
            || !is_percentage_fraction(self.min_junior_share_at_target)
            || self.min_junior_share_at_target.bits > self.initial_junior_share_at_target.bits
            || !is_percentage_fraction(self.zero_utilization_junior_share_discount)
            || !is_percentage_fraction(self.full_utilization_junior_share_premium)
            || self.zero_utilization_junior_share_discount.bits
                > self.full_utilization_junior_share_premium.bits
        {
            return Err(CoreError::InvalidUtilizationGuidedCurve);
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        self.validate_config()?;
        if !is_percentage_fraction(self.current_junior_share_at_target)
            || self.current_junior_share_at_target.bits < self.min_junior_share_at_target.bits
        {
            return Err(CoreError::InvalidUtilizationGuidedCurve);
        }
        Ok(())
    }

    pub fn initialize(&mut self, now_ts: i64) -> Result<(), CoreError> {
        self.validate_config()?;
        self.current_junior_share_at_target = self.initial_junior_share_at_target;
        self.last_target_shift_ts = now_ts;
        Ok(())
    }

    pub fn quote_junior_share(&self, utilization: Fix) -> Result<Fix, CoreError> {
        self.validate()?;
        self.guided_quote(
            percentage_to_fix(self.current_junior_share_at_target),
            utilization,
        )
    }

    /// J(U) = clamp(T + d(U) * A, 0, 1)
    ///
    /// where:
    /// - T is the target share
    /// - d(U) is the distance from the target utilization
    /// - A is the amplitude of the adjustment
    /// - J(U) is the junior share at utilization U
    fn guided_quote(&self, target_share: Fix, utilization: Fix) -> Result<Fix, CoreError> {
        let distance =
            Self::signed_distance(utilization, percentage_to_fix(self.target_utilization))?;
        let amplitude = if distance < 0 {
            i128::try_from(percentage_to_fix(self.zero_utilization_junior_share_discount).bits)
                .map_err(|_| CoreError::ArithmeticOverflow)?
        } else {
            i128::try_from(percentage_to_fix(self.full_utilization_junior_share_premium).bits)
                .map_err(|_| CoreError::ArithmeticOverflow)?
        };
        let adjustment = fp_mul_i(distance, amplitude)?;
        let quote = i128::try_from(target_share.bits)
            .map_err(|_| CoreError::ArithmeticOverflow)?
            .safe_add(adjustment)?;
        Ok(Fix::new(quote.clamp(0, FIX_SCALE as i128) as u128))
    }

    /// if U <= U_target, d(U) = (U - U_target) / U_target
    ///
    /// if U > U_target, d(U) = (U - U_target) / (1 - U_target)
    ///
    /// d(U) is negative below target, positive above target
    fn signed_distance(utilization: Fix, target_utilization: Fix) -> Result<i128, CoreError> {
        let utilization = utilization.bits.min(FIX_SCALE);
        if utilization <= target_utilization.bits {
            Ok(-i128::try_from(fp_div(
                target_utilization.bits - utilization,
                target_utilization.bits,
            )?)
            .map_err(|_| CoreError::ArithmeticOverflow)?)
        } else {
            Ok(i128::try_from(fp_div(
                utilization - target_utilization.bits,
                FIX_SCALE - target_utilization.bits,
            )?)
            .map_err(|_| CoreError::ArithmeticOverflow)?)
        }
    }

    fn clamp_target(current: Fix, exponential: Fix, minimum: Fix) -> Result<Fix, CoreError> {
        Ok(Fix::new(
            fp_mul(current.bits, exponential.bits)?.clamp(minimum.bits, FIX_SCALE),
        ))
    }

    /// T_next = clamp(T_current * e^(S * Δt), T_min, 1)
    ///
    /// T_midpoint = clamp(T_current * e^(S * Δt / 2), T_min, 1)
    ///
    /// T_avg = (T_current + 4 * T_midpoint + T_next) / 6
    ///
    /// where:
    /// - T_next is the next target share
    /// - T_midpoint is the midpoint target share
    /// - T_avg is the average target share
    /// - T_current is the current target share
    /// - S is max target shift speed * signed distance from target utilization
    /// - Δt is the time elapsed
    /// - T_min is the minimum target share
    /// - 1 is the maximum target share
    pub fn advance_and_quote(
        &mut self,
        utilization: Fix,
        now_ts: i64,
        is_normal_state: bool,
    ) -> Result<Fix, CoreError> {
        self.validate()?;
        if now_ts < self.last_target_shift_ts {
            return Err(CoreError::InvalidTimestamp);
        }
        let elapsed = now_ts - self.last_target_shift_ts;
        let current = percentage_to_fix(self.current_junior_share_at_target);

        let average = if is_normal_state && elapsed > 0 {
            let distance =
                Self::signed_distance(utilization, percentage_to_fix(self.target_utilization))?;
            let shift = fp_mul_i(
                i128::try_from(self.max_target_shift_speed.bits)
                    .map_err(|_| CoreError::ArithmeticOverflow)?,
                distance,
            )?
            .safe_mul(i128::from(elapsed))?;
            let minimum = percentage_to_fix(self.min_junior_share_at_target);

            let next = Self::clamp_target(
                current,
                Fix::new(
                    u128::try_from(exp_fixed_i(shift)?)
                        .map_err(|_| CoreError::ArithmeticOverflow)?,
                ),
                minimum,
            )?;

            let midpoint = Self::clamp_target(
                current,
                Fix::new(
                    u128::try_from(exp_fixed_i(shift / 2)?)
                        .map_err(|_| CoreError::ArithmeticOverflow)?,
                ),
                minimum,
            )?;

            let sum = current
                .safe_add(midpoint.safe_mul(Unit::new(4))?.convert())?
                .safe_add(next)?
                .safe_div(Unit::new(6))?
                .convert();
            let next: Centi<u128> = next.convert();
            self.current_junior_share_at_target = u16::try_from(next.bits)
                .map(Percentage::new)
                .map_err(|_| CoreError::ArithmeticOverflow)?;
            self.last_target_shift_ts = now_ts;
            sum
        } else {
            current
        };

        self.guided_quote(average, utilization)
    }

    pub fn current_junior_share_at_target(&self) -> Result<Fix, CoreError> {
        Ok(percentage_to_fix(self.current_junior_share_at_target))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicLeverage {
    pub target_junior_ratio: Percentage,
    pub base_multiplier: Percentage,
    pub max_multiplier: Percentage,
}

impl DynamicLeverage {
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.target_junior_ratio.bits == 0
            || !is_percentage_fraction(self.target_junior_ratio)
            || self.base_multiplier.bits < 100
            || self.max_multiplier.bits < self.base_multiplier.bits
            || self.max_multiplier.bits > 10_000
        {
            return Err(CoreError::InvalidDynamicLeverage);
        }
        Ok(())
    }

    /// if `0 <= J < J_target`:
    ///
    /// `M(J) = B + (M_max - B) * (J_target - J) / J_target`
    ///
    /// if `J >= J_target`, `M(J) = B`.
    ///
    /// `M(J) = M_max - (M_max - B) * J / J_target`
    ///
    /// where:
    ///
    /// - `J` is the junior ratio
    /// - `J_target` is the target junior ratio
    /// - `B` is the base multiplier
    /// - `M_max` is the maximum multiplier
    /// - `M(J)` is the dynamic multiplier
    pub fn dynamic_multiplier(&self, junior_ratio: Fix) -> Result<Fix, CoreError> {
        self.validate()?;
        let target_junior_ratio = percentage_to_fix(self.target_junior_ratio);
        let base_multiplier = percentage_to_fix(self.base_multiplier);
        let max_multiplier = percentage_to_fix(self.max_multiplier);
        if junior_ratio.bits >= target_junior_ratio.bits {
            return Ok(base_multiplier);
        }
        let distance = target_junior_ratio.bits - junior_ratio.bits;
        let extra = fp_mul(max_multiplier.bits - base_multiplier.bits, distance)?;
        base_multiplier
            .safe_add(Fix::new(fp_div(extra, target_junior_ratio.bits)?))
            .map_err(Into::into)
    }

    /// J(E, R) = E * M(J) / (E + R)
    ///
    /// where:
    ///
    /// - `J` is the junior ratio
    /// - `E` is the junior effective NAV
    /// - `R` is the senior raw NAV
    /// - `M(J)` is the dynamic multiplier
    /// - `J(E, R)` is the junior share
    pub fn quote_junior_share(
        &self,
        junior_effective_nav: Fix,
        senior_raw_nav: Fix,
    ) -> Result<Fix, CoreError> {
        self.validate()?;
        let total = junior_effective_nav.safe_add(senior_raw_nav)?;
        if total.bits == 0 {
            return Err(CoreError::InvalidDynamicLeverage);
        }
        let ratio = Fix::new(fp_div(junior_effective_nav.bits, total.bits)?);
        let multiplier = self.dynamic_multiplier(ratio)?;
        let junior_weight = fp_mul(junior_effective_nav.bits, multiplier.bits)?;
        let denominator = Fix::new(junior_weight).safe_add(senior_raw_nav)?;
        if denominator.bits == 0 {
            return Err(CoreError::InvalidDynamicLeverage);
        }
        Ok(Fix::new(fp_div(junior_weight, denominator.bits)?))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Subsidy {
    pub senior_yield_to_junior_share: Percentage,
}

impl Subsidy {
    pub fn validate(&self) -> Result<(), CoreError> {
        if is_percentage_fraction(self.senior_yield_to_junior_share) {
            Ok(())
        } else {
            Err(CoreError::InvalidSubsidy)
        }
    }

    /// J = S
    ///
    /// where:
    ///
    /// - `J` is the junior share
    /// - `S` is the senior yield to junior share
    pub fn quote_junior_share(&self) -> Result<Fix, CoreError> {
        self.validate()?;
        Ok(percentage_to_fix(self.senior_yield_to_junior_share))
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrancheModel {
    PointCurve(PointCurve) = 0,
    UtilizationGuidedCurve(UtilizationGuidedCurve) = 1,
    DynamicLeverage(DynamicLeverage) = 2,
    Subsidy(Subsidy) = 3,
}

impl TrancheModel {
    pub fn encode(self) -> Result<TrancheModelBytes, CoreError> {
        self.validate()?;
        let mut encoded = TrancheModelBytes::uninit();

        match self {
            Self::PointCurve(PointCurve { points }) => {
                let count = points
                    .iter()
                    .take_while(|p| p.utilization.bits != 0 || p.junior_share.bits != 0)
                    .count();
                if !(2..=3).contains(&count) {
                    return Err(CoreError::InvalidTrancheModel);
                }
                encoded.write_byte(0, 0);
                let active_points = &points[..count];
                let len = core::mem::size_of_val(active_points);
                encoded.copy_curve_points(active_points, 1);
                encoded.len = 1 + len;
            }
            Self::UtilizationGuidedCurve(model) => {
                encoded.write_byte(0, 1);
                let tail = UtilizationGuidedCurveTail {
                    target: model.target_utilization.bits,
                    initial: model.initial_junior_share_at_target.bits,
                    minimum: model.min_junior_share_at_target.bits,
                    speed: model.max_target_shift_speed.bits,
                    discount: model.zero_utilization_junior_share_discount.bits,
                    premium: model.full_utilization_junior_share_premium.bits,
                    current: model.current_junior_share_at_target.bits,
                    timestamp: model.last_target_shift_ts,
                };
                encoded.copy_from(1, &tail);
                encoded.len = 37;
            }
            Self::DynamicLeverage(model) => {
                encoded.write_byte(0, 2);
                let tail = DynamicLeverageTail {
                    target: model.target_junior_ratio.bits,
                    base: model.base_multiplier.bits,
                    maximum: model.max_multiplier.bits,
                };
                encoded.copy_from(1, &tail);
                encoded.len = 7;
            }
            Self::Subsidy(model) => {
                encoded.write_byte(0, 3);
                encoded.copy_from(1, &model.senior_yield_to_junior_share.bits);
                encoded.len = 3;
            }
        }

        Ok(encoded)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CoreError> {
        let kind = *bytes.first().ok_or(CoreError::InvalidTrancheModel)?;
        let percentage =
            |offset| read_percentage(bytes, offset).ok_or(CoreError::InvalidTrancheModel);

        // decodes to valid TrancheModel as long there are enough bytes
        let tranche_model = match kind {
            0 if bytes.len() >= 9 => {
                let mut points = [CurvePoint::default(); MAX_POINT_CURVE_POINTS];
                for (i, point) in points.iter_mut().take((bytes.len() - 1) / 4).enumerate() {
                    let offset = 1 + i * 4;
                    *point = CurvePoint::new(percentage(offset)?, percentage(offset + 2)?);
                }
                Self::PointCurve(PointCurve { points })
            }
            1 if bytes.len() >= 37 => Self::UtilizationGuidedCurve(UtilizationGuidedCurve {
                target_utilization: percentage(1)?,
                initial_junior_share_at_target: percentage(3)?,
                min_junior_share_at_target: percentage(5)?,
                max_target_shift_speed: Fix::new(
                    read_u128(bytes, 7).ok_or(CoreError::InvalidTrancheModel)?,
                ),
                zero_utilization_junior_share_discount: percentage(23)?,
                full_utilization_junior_share_premium: percentage(25)?,
                current_junior_share_at_target: percentage(27)?,
                last_target_shift_ts: read_i64(bytes, 29).ok_or(CoreError::InvalidTrancheModel)?,
            }),
            2 if bytes.len() >= 7 => Self::DynamicLeverage(DynamicLeverage {
                target_junior_ratio: percentage(1)?,
                base_multiplier: percentage(3)?,
                max_multiplier: percentage(5)?,
            }),
            3 if bytes.len() >= 3 => Self::Subsidy(Subsidy {
                senior_yield_to_junior_share: percentage(1)?,
            }),
            _ => return Err(CoreError::InvalidTrancheModel),
        };

        tranche_model.validate().map(|_| tranche_model)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        match self {
            Self::PointCurve(model) => model.validate(),
            Self::UtilizationGuidedCurve(model) => model.validate(),
            Self::DynamicLeverage(model) => model.validate(),
            Self::Subsidy(model) => model.validate(),
        }
    }

    /// Non-guided models have no mutable model state.
    pub fn initialize(&mut self, now_ts: i64) -> Result<(), CoreError> {
        match self {
            Self::UtilizationGuidedCurve(model) => model.initialize(now_ts),
            _ => self.validate(),
        }
    }

    pub fn junior_share(
        &self,
        utilization: Fix,
        junior_effective_nav: Fix,
        senior_raw_nav: Fix,
    ) -> Result<Fix, CoreError> {
        self.validate()?;
        match self {
            Self::PointCurve(model) => model.quote_junior_share(utilization),
            Self::UtilizationGuidedCurve(model) => model.quote_junior_share(utilization),
            Self::DynamicLeverage(model) => {
                model.quote_junior_share(junior_effective_nav, senior_raw_nav)
            }
            Self::Subsidy(model) => model.quote_junior_share(),
        }
    }

    pub fn dynamic_multiplier(&self, junior_ratio: Fix) -> Result<Fix, CoreError> {
        match self {
            Self::DynamicLeverage(model) => model.dynamic_multiplier(junior_ratio),
            _ => Err(CoreError::UnsupportedOperation),
        }
    }

    pub fn advance_and_quote(
        &mut self,
        utilization: Fix,
        now_ts: i64,
        is_normal_state: bool,
    ) -> Result<Fix, CoreError> {
        match self {
            Self::UtilizationGuidedCurve(model) => {
                model.advance_and_quote(utilization, now_ts, is_normal_state)
            }
            _ => Err(CoreError::UnsupportedOperation),
        }
    }

    pub fn current_junior_share_at_target(&self) -> Result<Fix, CoreError> {
        match self {
            Self::UtilizationGuidedCurve(model) => model.current_junior_share_at_target(),
            _ => Err(CoreError::UnsupportedOperation),
        }
    }

    pub fn last_target_shift_ts(&self) -> Result<i64, CoreError> {
        match self {
            Self::UtilizationGuidedCurve(model) => Ok(model.last_target_shift_ts),
            _ => Err(CoreError::UnsupportedOperation),
        }
    }
}

pub const MAX_TRANCHE_MODEL_BYTES: usize = 37;

#[derive(Clone, Copy)]
pub struct TrancheModelBytes {
    bytes: MaybeUninit<[u8; MAX_TRANCHE_MODEL_BYTES]>,
    len: usize,
}

impl TrancheModelBytes {
    const fn uninit() -> Self {
        Self {
            bytes: MaybeUninit::uninit(),
            len: 0,
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.bytes.as_ptr().cast::<u8>(), self.len) }
    }

    fn write_byte(&mut self, offset: usize, value: u8) {
        assert!(offset < MAX_TRANCHE_MODEL_BYTES);
        unsafe {
            self.bytes
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset)
                .write(value)
        }
    }

    fn copy_from<T>(&mut self, offset: usize, value: &T) {
        let len = core::mem::size_of::<T>();
        assert!(offset <= MAX_TRANCHE_MODEL_BYTES - len);
        unsafe {
            core::ptr::copy_nonoverlapping(
                (value as *const T).cast::<u8>(),
                self.bytes.as_mut_ptr().cast::<u8>().add(offset),
                len,
            )
        }
    }

    fn copy_curve_points(&mut self, points: &[CurvePoint], offset: usize) {
        let len = core::mem::size_of_val(points);
        assert!(offset <= MAX_TRANCHE_MODEL_BYTES - len);
        // SAFETY: the bounds assertion reserves exactly `len` output bytes.
        // `CurvePoint` is `repr(C)` with a checked four-byte layout, so every
        // source byte in the active point slice is initialized.
        unsafe {
            core::ptr::copy_nonoverlapping(
                points.as_ptr().cast::<u8>(),
                self.bytes.as_mut_ptr().cast::<u8>().add(offset),
                len,
            )
        }
    }
}

#[repr(C, packed)]
struct UtilizationGuidedCurveTail {
    target: u16,
    initial: u16,
    minimum: u16,
    speed: u128,
    discount: u16,
    premium: u16,
    current: u16,
    timestamp: i64,
}

#[repr(C, packed)]
struct DynamicLeverageTail {
    target: u16,
    base: u16,
    maximum: u16,
}

const fn is_percentage_fraction(value: Percentage) -> bool {
    value.bits <= 100
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{
        read_i64, read_percentage, read_u128, read_u16, read_u32, read_u64, read_u8,
    };

    #[test]
    fn byte_readers_decode_little_endian_values() {
        assert_eq!(read_u8(&[0x12], 0), Some(0x12));
        assert_eq!(read_u16(&[0, 0x34, 0x12], 1), Some(0x1234));
        assert_eq!(read_u32(&[0, 0x78, 0x56, 0x34, 0x12], 1), Some(0x12345678));
        assert_eq!(read_u64(&[0xff; 9], 1), Some(u64::MAX));
        assert_eq!(read_u128(&[0; 17], 1), Some(0));
        assert_eq!(read_i64(&[0xff; 9], 1), Some(-1));
        assert_eq!(read_percentage(&[0, 20, 0], 1), Some(Percentage::new(20)));
    }

    #[test]
    fn tranche_model_bytes_copies_curve_point_slice() {
        let points = [
            CurvePoint::new(Percentage::new(20), Percentage::new(30)),
            CurvePoint::new(Percentage::new(40), Percentage::new(50)),
        ];
        let mut encoded = TrancheModelBytes::uninit();

        encoded.copy_curve_points(&points, 0);
        encoded.len = core::mem::size_of_val(&points);

        assert_eq!(encoded.as_slice(), &[20, 0, 30, 0, 40, 0, 50, 0]);
    }

    #[test]
    fn tranche_model_encodes_and_decodes_itself() {
        let model = TrancheModel::Subsidy(Subsidy {
            senior_yield_to_junior_share: percentage(20),
        });

        let encoded = model.encode().unwrap();

        assert_eq!(TrancheModel::decode(encoded.as_slice()), Ok(model));
    }

    #[test]
    fn percentage_converts_to_calculation_precision() {
        let percentage = Percentage::new(10_000);
        let fixed = percentage_to_fix(percentage);

        assert_eq!(fixed, Fix::new(100 * FIX_SCALE));
    }

    fn fix(value: u128) -> Fix {
        Fix::new(value * FIX_SCALE / 100)
    }

    fn percentage(value: u16) -> Percentage {
        Percentage::new(value)
    }

    #[test]
    fn tranche_model_wraps_concrete_model_types() {
        let model = TrancheModel::Subsidy(Subsidy {
            senior_yield_to_junior_share: percentage(20),
        });

        assert_eq!(
            model
                .junior_share(Fix::default(), Fix::default(), Fix::default())
                .unwrap(),
            fix(20)
        );
    }

    #[test]
    fn point_curve_interpolates_and_clamps() {
        let model = TrancheModel::PointCurve(PointCurve {
            points: [
                CurvePoint::new(percentage(50), percentage(20)),
                CurvePoint::new(percentage(90), percentage(45)),
                CurvePoint::new(percentage(100), percentage(70)),
            ],
        });

        assert_eq!(
            model
                .junior_share(fix(20), Fix::default(), Fix::default())
                .unwrap(),
            fix(20)
        );
        assert_eq!(
            model
                .junior_share(fix(70), Fix::default(), Fix::default())
                .unwrap(),
            Fix::new(FIX_SCALE * 325 / 1_000)
        );
        assert_eq!(
            model
                .junior_share(fix(120), Fix::default(), Fix::default())
                .unwrap(),
            fix(70)
        );
    }

    #[test]
    fn point_curve_rejects_non_trailing_null_points() {
        let model = TrancheModel::PointCurve(PointCurve {
            points: [
                CurvePoint::new(percentage(50), percentage(20)),
                CurvePoint::default(),
                CurvePoint::new(percentage(100), percentage(70)),
            ],
        });

        assert_eq!(model.validate(), Err(CoreError::InvalidPointCurve));
    }

    #[test]
    fn dynamic_leverage_is_base_at_target_and_capped_below_it() {
        let model = TrancheModel::DynamicLeverage(DynamicLeverage {
            target_junior_ratio: percentage(50),
            base_multiplier: percentage(200),
            max_multiplier: percentage(400),
        });

        assert_eq!(
            model.dynamic_multiplier(fix(50)).unwrap(),
            Fix::new(2 * FIX_SCALE)
        );
        assert_eq!(
            model.dynamic_multiplier(fix(0)).unwrap(),
            Fix::new(4 * FIX_SCALE)
        );
        assert_eq!(
            model.dynamic_multiplier(fix(75)).unwrap(),
            Fix::new(2 * FIX_SCALE)
        );
    }

    #[test]
    fn guided_curve_advances_target_and_uses_simpson_average() {
        let mut model = TrancheModel::UtilizationGuidedCurve(UtilizationGuidedCurve {
            target_utilization: percentage(90),
            initial_junior_share_at_target: percentage(40),
            min_junior_share_at_target: percentage(10),
            max_target_shift_speed: Fix::new(10_000_000_000),
            zero_utilization_junior_share_discount: percentage(10),
            full_utilization_junior_share_premium: percentage(10),
            current_junior_share_at_target: percentage(40),
            last_target_shift_ts: 0,
        });

        let quote = model.advance_and_quote(fix(100), 10, true).unwrap();

        assert!(quote > fix(40));
        assert!(model.current_junior_share_at_target().unwrap() > fix(40));
        assert_eq!(
            model.advance_and_quote(fix(100), 10, true).unwrap(),
            model
                .junior_share(fix(100), Fix::default(), Fix::default())
                .unwrap()
        );
    }

    #[test]
    fn guided_curve_applies_discount_below_target_utilization() {
        let model = UtilizationGuidedCurve {
            target_utilization: percentage(90),
            initial_junior_share_at_target: percentage(40),
            min_junior_share_at_target: percentage(10),
            max_target_shift_speed: Fix::default(),
            zero_utilization_junior_share_discount: percentage(10),
            full_utilization_junior_share_premium: percentage(10),
            current_junior_share_at_target: percentage(40),
            last_target_shift_ts: 0,
        };

        assert_eq!(model.quote_junior_share(fix(0)).unwrap(), fix(30));
    }

    #[test]
    fn guided_curve_requires_discount_not_to_exceed_premium() {
        let model = UtilizationGuidedCurve {
            target_utilization: percentage(90),
            initial_junior_share_at_target: percentage(40),
            min_junior_share_at_target: percentage(10),
            max_target_shift_speed: Fix::default(),
            zero_utilization_junior_share_discount: percentage(11),
            full_utilization_junior_share_premium: percentage(10),
            current_junior_share_at_target: percentage(40),
            last_target_shift_ts: 0,
        };

        assert_eq!(
            model.validate(),
            Err(CoreError::InvalidUtilizationGuidedCurve)
        );
    }

    #[test]
    fn subsidy_returns_configured_share() {
        let model = TrancheModel::Subsidy(Subsidy {
            senior_yield_to_junior_share: percentage(20),
        });

        assert_eq!(
            model
                .junior_share(fix(0), Fix::default(), Fix::default())
                .unwrap(),
            fix(20)
        );
    }

    #[test]
    fn initialization_sets_guided_curve_state_from_immutable_configuration() {
        let mut model = TrancheModel::UtilizationGuidedCurve(UtilizationGuidedCurve {
            target_utilization: percentage(90),
            initial_junior_share_at_target: percentage(40),
            min_junior_share_at_target: percentage(10),
            max_target_shift_speed: Fix::new(10_000_000_000),
            zero_utilization_junior_share_discount: percentage(10),
            full_utilization_junior_share_premium: percentage(10),
            current_junior_share_at_target: Percentage::default(),
            last_target_shift_ts: 0,
        });

        model.initialize(123).unwrap();

        assert_eq!(model.current_junior_share_at_target().unwrap(), fix(40));
        assert_eq!(model.last_target_shift_ts().unwrap(), 123);
    }
}
