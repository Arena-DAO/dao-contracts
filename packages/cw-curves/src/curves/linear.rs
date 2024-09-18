use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult, Uint128};
use rust_decimal::Decimal;

use crate::{
    utils::{decimal_to_std, square_root},
    Curve, DecimalPlaces,
};

/// Implements a Linear bonding curve where spot price is slope * supply
#[derive(Debug)]
pub struct Linear {
    /// The slope of the linear curve
    pub slope: Decimal,
    /// Decimal places for normalization between supply and reserve tokens
    pub normalize: DecimalPlaces,
}

impl Linear {
    /// Creates a new Linear curve instance
    ///
    /// # Arguments
    ///
    /// * `slope` - The slope of the curve
    /// * `normalize` - DecimalPlaces for normalization between supply and reserve tokens
    pub fn new(slope: Decimal, normalize: DecimalPlaces) -> Self {
        Self { slope, normalize }
    }
}

impl Curve for Linear {
    /// Calculates the spot price for a given supply
    ///
    /// The spot price is calculated as: f(x) = slope * supply
    fn spot_price(&self, supply: Uint128) -> StdResult<StdDecimal> {
        let out = self
            .normalize
            .from_supply(supply)?
            .checked_mul(self.slope)
            .ok_or_else(|| StdError::generic_err("Overflow in spot price calculation"))?;
        decimal_to_std(out)
    }

    /// Calculates the reserve for a given supply
    ///
    /// The reserve is calculated as: f(x) = (slope * supply * supply) / 2
    fn reserve(&self, supply: Uint128) -> StdResult<Uint128> {
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

    /// Calculates the supply for a given reserve
    ///
    /// The supply is calculated as: f(x) = sqrt(2 * reserve / slope)
    fn supply(&self, reserve: Uint128) -> StdResult<Uint128> {
        let doubled_reserve = reserve
            .checked_add(reserve)
            .map_err(|_| StdError::generic_err("Overflow in doubling reserve"))?;
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
