use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult, Uint128};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use std::fmt::Debug;

/// Defines the interface for bonding curves
pub trait Curve: Debug {
    /// Returns the spot price given the supply.
    ///
    /// This corresponds to `f(x)` in the mathematical representation.
    ///
    /// # Arguments
    ///
    /// * `supply` - The current token supply
    ///
    /// # Returns
    ///
    /// * `StdResult<StdDecimal>` - The spot price as a StdDecimal, or an error if the calculation fails
    fn spot_price(&self, supply: Uint128) -> StdResult<StdDecimal>;

    /// Returns the total price paid up to purchase the given supply of tokens.
    ///
    /// This corresponds to the integral `F(x)` in the mathematical representation.
    ///
    /// # Arguments
    ///
    /// * `supply` - The token supply to calculate the reserve for
    ///
    /// # Returns
    ///
    /// * `StdResult<Uint128>` - The total reserve amount, or an error if the calculation fails
    fn reserve(&self, supply: Uint128) -> StdResult<Uint128>;

    /// Inverse of reserve. Returns how many tokens would be issued for a given reserve amount.
    ///
    /// This corresponds to `F^-1(x)` in the mathematical representation.
    ///
    /// # Arguments
    ///
    /// * `reserve` - The reserve amount to calculate the supply for
    ///
    /// # Returns
    ///
    /// * `StdResult<Uint128>` - The token supply, or an error if the calculation fails
    fn supply(&self, reserve: Uint128) -> StdResult<Uint128>;
}

/// DecimalPlaces for normalizing between supply and reserve tokens
#[cw_serde]
#[derive(Copy)]
pub struct DecimalPlaces {
    /// Number of decimal places for the supply token
    pub supply: u32,
    /// Number of decimal places for the reserve token (e.g., 6 for uatom, 9 for nstep, 18 for wei)
    pub reserve: u32,
}

impl DecimalPlaces {
    /// Creates a new DecimalPlaces instance
    ///
    /// # Arguments
    ///
    /// * `supply` - Number of decimal places for the supply token
    /// * `reserve` - Number of decimal places for the reserve token
    pub fn new(supply: u8, reserve: u8) -> Self {
        DecimalPlaces {
            supply: supply as u32,
            reserve: reserve as u32,
        }
    }

    /// Converts a reserve amount from Decimal to Uint128
    pub fn to_reserve(&self, reserve: Decimal) -> StdResult<Uint128> {
        let factor = Decimal::from_u128(10u128.pow(self.reserve))
            .ok_or_else(|| StdError::generic_err("Failed to create decimal factor"))?;
        reserve
            .checked_mul(factor)
            .ok_or_else(|| StdError::generic_err("Overflow in reserve calculation"))?
            .floor()
            .to_u128()
            .ok_or_else(|| StdError::generic_err("Overflow in to_reserve conversion"))
            .map(Uint128::from)
    }

    /// Converts a supply amount from Decimal to Uint128
    pub fn to_supply(&self, supply: Decimal) -> StdResult<Uint128> {
        let factor = Decimal::from_u128(10u128.pow(self.supply))
            .ok_or_else(|| StdError::generic_err("Failed to create decimal factor"))?;
        supply
            .checked_mul(factor)
            .ok_or_else(|| StdError::generic_err("Overflow in supply calculation"))?
            .floor()
            .to_u128()
            .ok_or_else(|| StdError::generic_err("Overflow in to_supply conversion"))
            .map(Uint128::from)
    }

    /// Converts a supply amount from Uint128 to Decimal
    pub fn from_supply(&self, supply: Uint128) -> StdResult<Decimal> {
        Decimal::from_u128(supply.u128())
            .ok_or_else(|| StdError::generic_err("Failed to convert supply to Decimal"))?
            .checked_div(
                Decimal::from_u128(10u128.pow(self.supply))
                    .ok_or_else(|| StdError::generic_err("Failed to create decimal divisor"))?,
            )
            .ok_or_else(|| StdError::generic_err("Division by zero in from_supply"))
    }

    /// Converts a reserve amount from Uint128 to Decimal
    pub fn from_reserve(&self, reserve: Uint128) -> StdResult<Decimal> {
        Decimal::from_u128(reserve.u128())
            .ok_or_else(|| StdError::generic_err("Failed to convert reserve to Decimal"))?
            .checked_div(
                Decimal::from_u128(10u128.pow(self.reserve))
                    .ok_or_else(|| StdError::generic_err("Failed to create decimal divisor"))?,
            )
            .ok_or_else(|| StdError::generic_err("Division by zero in from_reserve"))
    }
}
