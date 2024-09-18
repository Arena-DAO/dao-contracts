use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult, Uint128};
use rust_decimal::Decimal;

use crate::{utils::decimal_to_std, Curve, DecimalPlaces};

/// Implements a Constant bonding curve where spot price is always a constant value
#[derive(Debug)]
pub struct Constant {
    /// The constant value of the curve
    pub value: Decimal,
    /// Decimal places for normalization between supply and reserve tokens
    pub normalize: DecimalPlaces,
}

impl Constant {
    /// Creates a new Constant curve instance
    ///
    /// # Arguments
    ///
    /// * `value` - The constant value of the curve
    /// * `normalize` - DecimalPlaces for normalization between supply and reserve tokens
    pub fn new(value: Decimal, normalize: DecimalPlaces) -> Self {
        Self { value, normalize }
    }
}

impl Curve for Constant {
    /// Calculates the spot price, which is always the constant value
    ///
    /// Note: The value is normalized with the reserve decimal places
    /// (e.g., 0.1 value would return 100_000 if reserve was uatom)
    ///
    /// # Arguments
    ///
    /// * `_supply` - The current supply (unused in this implementation)
    ///
    /// # Returns
    ///
    /// * `StdResult<StdDecimal>` - The constant spot price as a StdDecimal
    fn spot_price(&self, _supply: Uint128) -> StdResult<StdDecimal> {
        // f(x) = self.value
        decimal_to_std(self.value)
    }

    /// Calculates the total number of reserve tokens needed to purchase a given number of supply tokens
    ///
    /// Note: Both supply and reserve are normalized internally
    ///
    /// # Arguments
    ///
    /// * `supply` - The amount of supply tokens
    ///
    /// # Returns
    ///
    /// * `StdResult<Uint128>` - The amount of reserve tokens needed
    fn reserve(&self, supply: Uint128) -> StdResult<Uint128> {
        // f(x) = supply * self.value
        let reserve = self
            .normalize
            .from_supply(supply)?
            .checked_mul(self.value)
            .ok_or_else(|| StdError::generic_err("Overflow in reserve calculation"))?;
        self.normalize.to_reserve(reserve)
    }

    /// Calculates the number of supply tokens that can be purchased with a given amount of reserve tokens
    ///
    /// # Arguments
    ///
    /// * `reserve` - The amount of reserve tokens
    ///
    /// # Returns
    ///
    /// * `StdResult<Uint128>` - The amount of supply tokens that can be purchased
    fn supply(&self, reserve: Uint128) -> StdResult<Uint128> {
        // f(x) = reserve / self.value
        let supply = self
            .normalize
            .from_reserve(reserve)?
            .checked_div(self.value)
            .ok_or_else(|| {
                StdError::generic_err("Division by zero or overflow in supply calculation")
            })?;
        self.normalize.to_supply(supply)
    }
}
