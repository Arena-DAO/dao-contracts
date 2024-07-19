use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult, Uint128};
use rust_decimal::Decimal;

use crate::{
    utils::{cube_root, decimal_to_std, square_root},
    Curve, DecimalPlaces,
};

/// spot_price is slope * (supply)^0.5
#[derive(Debug)]
pub struct SquareRoot {
    pub slope: Decimal,
    pub normalize: DecimalPlaces,
}

impl SquareRoot {
    pub fn new(slope: Decimal, normalize: DecimalPlaces) -> Self {
        Self { slope, normalize }
    }
}

impl Curve for SquareRoot {
    fn spot_price(&self, supply: Uint128) -> StdResult<StdDecimal> {
        // f(x) = self.slope * supply^0.5
        let square = self.normalize.from_supply(supply)?;
        let root = square_root(square)?;
        root.checked_mul(self.slope)
            .ok_or_else(|| StdError::generic_err("Overflow in spot price calculation"))
            .and_then(|result| {
                decimal_to_std(result)
                    .map_err(|_| StdError::generic_err("Failed to convert Decimal to StdDecimal"))
            })
    }

    fn reserve(&self, supply: Uint128) -> StdResult<Uint128> {
        // f(x) = self.slope * supply * supply^0.5 / 1.5
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

    fn supply(&self, reserve: Uint128) -> StdResult<Uint128> {
        // f(x) = (1.5 * reserve / self.slope) ^ (2/3)
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
