use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult, Uint128};
use rust_decimal::Decimal;

use crate::{
    utils::{decimal_to_std, square_root},
    Curve, DecimalPlaces,
};

/// spot_price is slope * supply
#[derive(Debug)]
pub struct Linear {
    pub slope: Decimal,
    pub normalize: DecimalPlaces,
}

impl Linear {
    pub fn new(slope: Decimal, normalize: DecimalPlaces) -> Self {
        Self { slope, normalize }
    }
}

impl Curve for Linear {
    fn spot_price(&self, supply: Uint128) -> StdResult<StdDecimal> {
        // f(x) = supply * self.value
        let out = self
            .normalize
            .from_supply(supply)?
            .checked_mul(self.slope)
            .ok_or_else(|| StdError::generic_err("Overflow in spot price calculation"))?;
        decimal_to_std(out)
    }

    fn reserve(&self, supply: Uint128) -> StdResult<Uint128> {
        // f(x) = self.slope * supply * supply / 2
        let normalized = self.normalize.from_supply(supply)?;
        let square = normalized
            .checked_mul(normalized)
            .ok_or_else(|| StdError::generic_err("Overflow in supply squaring"))?;
        // Note: multiplying by 0.5 is much faster than dividing by 2
        let reserve = square
            .checked_mul(self.slope)
            .and_then(|r| r.checked_mul(Decimal::new(5, 1)))
            .ok_or_else(|| StdError::generic_err("Overflow in reserve calculation"))?;
        self.normalize.to_reserve(reserve)
    }

    fn supply(&self, reserve: Uint128) -> StdResult<Uint128> {
        // f(x) = (2 * reserve / self.slope) ^ 0.5
        let doubled_reserve = reserve.checked_add(reserve)?;
        let square = self
            .normalize
            .from_reserve(doubled_reserve)?
            .checked_div(self.slope)
            .ok_or_else(|| {
                StdError::generic_err("Division by zero or overflow in supply calculation")
            })?;
        let supply = square_root(square)?;
        self.normalize.to_supply(supply)
    }
}
