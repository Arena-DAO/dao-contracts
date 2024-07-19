use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult};
use integer_cbrt::IntegerCubeRoot;
use integer_sqrt::IntegerSquareRoot;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use std::str::FromStr;

/// decimal returns an object = num * 10 ^ -scale
/// We use this function in contract.rs rather than call the crate constructor
/// itself, in case we want to swap out the implementation, we can do it only in this file.
pub fn decimal<T: Into<u128>>(num: T, scale: u32) -> StdResult<Decimal> {
    Decimal::try_from_i128_with_scale(num.into() as i128, scale)
        .map_err(|_| StdError::generic_err("Failed to create Decimal"))
}

/// StdDecimal stores as a u128 with 18 decimal points of precision
pub fn decimal_to_std(x: Decimal) -> StdResult<StdDecimal> {
    // this seems straight-forward (if inefficient), converting via string representation
    // TODO: execute errors better? Result?
    StdDecimal::from_str(&x.to_string())
        .map_err(|_| StdError::generic_err("Failed to convert Decimal to StdDecimal"))

    // // maybe a better approach doing math, not sure about rounding
    //
    // // try to preserve decimal points, max 9
    // let digits = min(x.scale(), 9);
    // let multiplier = 10u128.pow(digits);
    //
    // // we multiply up before we round off to u128,
    // // let StdDecimal do its best to keep these decimal places
    // let nominator = (x * decimal(multiplier, 0)).to_u128().unwrap();
    // StdDecimal::from_ratio(nominator, multiplier)
}

/// Calculates the square root of a Decimal
pub(crate) fn square_root(square: Decimal) -> StdResult<Decimal> {
    // must be even
    // TODO: this can overflow easily at 18... what is a good value?
    const EXTRA_DIGITS: u32 = 12;

    // we multiply by 10^18, turn to int, take square root, then divide by 10^9 as we convert back to decimal

    let multiplier = Decimal::from_u128(10u128.saturating_pow(EXTRA_DIGITS))
        .ok_or_else(|| StdError::generic_err("Failed to create multiplier"))?;

    let extended = square
        .checked_mul(multiplier)
        .ok_or_else(|| StdError::generic_err("Overflow in square root calculation"))?;

    let extended = extended.floor().to_u128().unwrap();

    // take square root, and build a decimal again
    let root = extended.integer_sqrt();
    decimal(root, EXTRA_DIGITS / 2)
}

/// Calculates the cube root of a Decimal
pub(crate) fn cube_root(cube: Decimal) -> StdResult<Decimal> {
    // must be multiple of 3
    // TODO: what is a good value?
    const EXTRA_DIGITS: u32 = 9;

    // we multiply by 10^9, turn to int, take cube root, then divide by 10^3 as we convert back to decimal

    let multiplier = Decimal::from_u128(10u128.saturating_pow(EXTRA_DIGITS))
        .ok_or_else(|| StdError::generic_err("Failed to create multiplier"))?;

    let extended = cube
        .checked_mul(multiplier)
        .ok_or_else(|| StdError::generic_err("Overflow in cube root calculation"))?;

    let extended_u128 = extended
        .floor()
        .to_u128()
        .ok_or_else(|| StdError::generic_err("Failed to convert to u128"))?;

    let root = extended_u128.integer_cbrt();
    decimal(root, EXTRA_DIGITS / 3)
}
