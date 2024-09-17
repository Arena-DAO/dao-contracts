use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult, Uint128};
use rust_decimal::Decimal;

use crate::{
    utils::{cube_root, decimal_to_std, square_root},
    Curve, DecimalPlaces,
};

/// Implements a Square Root bonding curve where spot price is slope * (supply)^0.5
#[derive(Debug)]
pub struct SquareRoot {
    /// The slope of the curve
    pub slope: Decimal,
    /// Decimal places for normalization between supply and reserve tokens
    pub normalize: DecimalPlaces,
}

impl SquareRoot {
    /// Creates a new SquareRoot curve instance
    ///
    /// # Arguments
    ///
    /// * `slope` - The slope of the curve
    /// * `normalize` - DecimalPlaces for normalization between supply and reserve tokens
    pub fn new(slope: Decimal, normalize: DecimalPlaces) -> Self {
        Self { slope, normalize }
    }
}

impl Curve for SquareRoot {
    /// Calculates the spot price for a given supply
    ///
    /// The spot price is calculated as: f(x) = slope * supply^0.5
    fn spot_price(&self, supply: Uint128) -> StdResult<StdDecimal> {
        let square = self.normalize.from_supply(supply)?;
        let root = square_root(square)?;
        root.checked_mul(self.slope)
            .ok_or_else(|| StdError::generic_err("Overflow in spot price calculation"))
            .and_then(decimal_to_std)
    }

    /// Calculates the reserve for a given supply
    ///
    /// The reserve is calculated as: f(x) = (slope * supply * supply^0.5) / 1.5
    fn reserve(&self, supply: Uint128) -> StdResult<Uint128> {
        let normalized = self.normalize.from_supply(supply)?;
        let root = square_root(normalized)?;
        normalized
            .checked_mul(root)
            .and_then(|r| r.checked_mul(self.slope))
            .and_then(|r| r.checked_div(Decimal::new(15, 1)))
            .ok_or_else(|| {
                StdError::generic_err("Overflow or division by zero in reserve calculation")
            })
            .and_then(|result| self.normalize.to_reserve(result))
    }

    /// Calculates the supply for a given reserve
    ///
    /// The supply is calculated as: f(x) = (1.5 * reserve / slope) ^ (2/3)
    fn supply(&self, reserve: Uint128) -> StdResult<Uint128> {
        let base = self
            .normalize
            .from_reserve(reserve)?
            .checked_mul(Decimal::new(15, 1))
            .and_then(|r| r.checked_div(self.slope))
            .ok_or_else(|| {
                StdError::generic_err("Overflow or division by zero in supply calculation")
            })?;

        let squared = base
            .checked_mul(base)
            .ok_or_else(|| StdError::generic_err("Overflow in supply calculation"))?;

        let supply = cube_root(squared)?;
        self.normalize.to_supply(supply)
    }
}
