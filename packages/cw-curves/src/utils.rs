use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult};
use integer_cbrt::IntegerCubeRoot;
use integer_sqrt::IntegerSquareRoot;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Creates a Decimal representation of a number with a specified scale.
///
/// This function returns a Decimal object equal to num * 10^(-scale).
/// It's used in contract.rs instead of directly calling the crate constructor,
/// allowing for easier swapping of the implementation if needed in the future.
///
/// # Arguments
///
/// * `num` - The number to convert to Decimal.
/// * `scale` - The number of decimal places to shift the number.
///
/// # Returns
///
/// * `StdResult<Decimal>` - The resulting Decimal on success, or an error if the conversion fails.
pub fn decimal<T: Into<u128>>(num: T, scale: u32) -> StdResult<Decimal> {
    Decimal::try_from_i128_with_scale(num.into() as i128, scale)
        .map_err(|_| StdError::generic_err("Failed to create Decimal"))
}

/// Converts a rust_decimal::Decimal to cosmwasm_std::Decimal (StdDecimal).
///
/// StdDecimal stores values as a u128 with 18 decimal points of precision.
/// This function uses string conversion, which is straightforward but potentially inefficient.
///
/// # Arguments
///
/// * `x` - The rust_decimal::Decimal to convert.
///
/// # Returns
///
/// * `StdResult<StdDecimal>` - The resulting StdDecimal on success, or an error if the conversion fails.
///
/// # Note
///
/// An alternative approach using mathematical operations is commented out below.
/// This approach attempts to preserve decimal points (up to 9) and might handle rounding differently.
/// Further investigation may be needed to determine the most appropriate method for specific use cases.
pub fn decimal_to_std(x: Decimal) -> StdResult<StdDecimal> {
    StdDecimal::from_str(&x.to_string())
        .map_err(|_| StdError::generic_err("Failed to convert Decimal to StdDecimal"))

    // Alternative approach (commented out):
    //
    // let digits = min(x.scale(), 9);
    // let multiplier = 10u128.pow(digits);
    //
    // let nominator = (x * decimal(multiplier, 0)?).to_u128()
    //     .ok_or_else(|| StdError::generic_err("Overflow in conversion to u128"))?;
    // StdDecimal::from_ratio(nominator, multiplier)
}

/// Calculates the square root of a Decimal
pub(crate) fn square_root(square: Decimal) -> StdResult<Decimal> {
    const PRECISION_FACTOR: u32 = 12;
    let multiplier = Decimal::from_u128(10u128.saturating_pow(PRECISION_FACTOR))
        .ok_or_else(|| StdError::generic_err("Failed to create precision multiplier"))?;

    let scaled_square = square
        .checked_mul(multiplier)
        .ok_or_else(|| StdError::generic_err("Overflow in square root calculation"))?;

    let integer_square = scaled_square
        .floor()
        .to_u128()
        .ok_or_else(|| StdError::generic_err("Failed to convert to u128"))?;

    let integer_root = integer_square.integer_sqrt();
    decimal(integer_root, PRECISION_FACTOR / 2)
}

/// Calculates the cube root of a Decimal
pub(crate) fn cube_root(cube: Decimal) -> StdResult<Decimal> {
    const PRECISION_FACTOR: u32 = 9;
    let multiplier = Decimal::from_u128(10u128.saturating_pow(PRECISION_FACTOR))
        .ok_or_else(|| StdError::generic_err("Failed to create precision multiplier"))?;

    let scaled_cube = cube
        .checked_mul(multiplier)
        .ok_or_else(|| StdError::generic_err("Overflow in cube root calculation"))?;

    let integer_cube = scaled_cube
        .floor()
        .to_u128()
        .ok_or_else(|| StdError::generic_err("Failed to convert to u128"))?;

    let integer_root = integer_cube.integer_cbrt();
    decimal(integer_root, PRECISION_FACTOR / 3)
}
