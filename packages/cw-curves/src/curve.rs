use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult, Uint128};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use std::fmt::Debug;

/*
This defines the curves we are using.

I am struggling on what type to use for the math. Tokens are often stored as Uint128,
but they may have 6 or 9 digits. When using constant or linear functions, this doesn't matter
much, but for non-linear functions a lot more. Also, supply and reserve most likely have different
decimals... either we leave it for the callers to normalize and accept a `Decimal` input,
or we pass in `Uint128` as well as the decimal places for supply and reserve.

After working the first route and realizing that `Decimal` is not all that great to work with
when you want to do more complex math than add and multiply `Uint128`, I decided to go the second
route. That made the signatures quite complex and my final idea was to pass in `supply_decimal`
and `reserve_decimal` in the curve constructors.
*/

/// Defines the interface for bonding curves
pub trait Curve: Debug {
    /// Returns the spot price given the supply.
    /// `f(x)` from the README
    fn spot_price(&self, supply: Uint128) -> StdResult<StdDecimal>;

    /// Returns the total price paid up to purchase supply tokens (integral)
    /// `F(x)` from the README
    fn reserve(&self, supply: Uint128) -> StdResult<Uint128>;

    /// Inverse of reserve. Returns how many tokens would be issued
    /// with a total paid amount of reserve.
    /// `F^-1(x)` from the README
    fn supply(&self, reserve: Uint128) -> StdResult<Uint128>;
}

/// DecimalPlaces for normalizing between supply and reserve tokens
#[cw_serde]
#[derive(Copy)]
pub struct DecimalPlaces {
    /// Number of decimal places for the supply token (this is what was passed in cw20-base instantiate
    pub supply: u32,
    /// Number of decimal places for the reserve token (eg. 6 for uatom, 9 for nstep, 18 for wei)
    pub reserve: u32,
}

impl DecimalPlaces {
    pub fn new(supply: u8, reserve: u8) -> Self {
        DecimalPlaces {
            supply: supply as u32,
            reserve: reserve as u32,
        }
    }

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

    pub fn from_supply(&self, supply: Uint128) -> StdResult<Decimal> {
        Decimal::from_u128(supply.u128())
            .ok_or_else(|| StdError::generic_err("Failed to convert supply to Decimal"))?
            .checked_div(
                Decimal::from_u128(10u128.pow(self.supply))
                    .ok_or_else(|| StdError::generic_err("Failed to create decimal divisor"))?,
            )
            .ok_or_else(|| StdError::generic_err("Division by zero in from_supply"))
    }

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
