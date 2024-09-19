use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult};
use integer_cbrt::IntegerCubeRoot;
use integer_sqrt::IntegerSquareRoot;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;

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
    let num_u128: u128 = num.into();
    let num_i128 = i128::try_from(num_u128)
        .map_err(|_| StdError::generic_err("decimal: Overflow when converting num to i128"))?;
    Decimal::try_from_i128_with_scale(num_i128, scale)
        .map_err(|_| StdError::generic_err("decimal: Failed to create Decimal with scale"))
}

/// Creates a `Decimal` representation of a number with a specified scale.
///
/// This function returns a `Decimal` object equal to `num * 10^(-scale)`.
/// It safely converts a numeric input into a `Decimal`, ensuring that
/// overflows are handled and appropriate errors are returned.
///
/// # Arguments
///
/// * `num` - The number to convert to `Decimal`.
/// * `scale` - The number of decimal places to shift the number.
///
/// # Returns
///
/// * `StdResult<Decimal>` - The resulting `Decimal` on success, or an error if the conversion fails.
///
/// # Errors
///
/// * Returns an error if the conversion from `u128` to `i128` overflows.
/// * Returns an error if creating the `Decimal` with the specified scale fails.
///
/// # Examples
///
/// ```
/// let value = decimal(1000u64, 2)?;
/// assert_eq!(value.to_string(), "10.00");
/// ```
pub fn decimal_to_std(x: Decimal) -> StdResult<StdDecimal> {
    if x.is_sign_negative() {
        return Err(StdError::generic_err(
            "decimal_to_std: Negative values are not supported",
        ));
    }

    // CosmWasm StdDecimal uses a fixed 18 decimal places
    const STD_DECIMAL_SCALE: u32 = 18;
    let x_scale = x.scale();

    // Calculate the difference in scales
    let scale_diff = STD_DECIMAL_SCALE as i32 - x_scale as i32;

    // Adjust the mantissa to match the scale of StdDecimal
    let adjusted_mantissa = if scale_diff >= 0 {
        x.mantissa()
            .checked_mul(10i128.pow(scale_diff as u32))
            .ok_or_else(|| {
                StdError::generic_err("decimal_to_std: Mantissa multiplication overflow")
            })?
    } else {
        x.mantissa()
            .checked_div(10i128.pow((-scale_diff) as u32))
            .ok_or_else(|| StdError::generic_err("decimal_to_std: Mantissa division underflow"))?
    };

    // Ensure the adjusted mantissa fits into u128
    let nominator_u128 = u128::try_from(adjusted_mantissa)
        .map_err(|_| StdError::generic_err("decimal_to_std: Mantissa conversion to u128 failed"))?;

    // Create StdDecimal with the adjusted mantissa and standard scale
    StdDecimal::from_atomics(nominator_u128, STD_DECIMAL_SCALE)
        .map_err(|_| StdError::generic_err("decimal_to_std: Failed to create StdDecimal"))
}

/// Calculates the square root of a Decimal with high precision.
///
/// This function scales the input `square` to an integer by multiplying it with a precision multiplier,
/// computes the integer square root, and then scales the result back down.
///
/// # Arguments
///
/// * `square` - The Decimal value for which to compute the square root.
///
/// # Returns
///
/// * `StdResult<Decimal>` - The square root of `square`, or an error if the calculation fails.
pub(crate) fn square_root(square: Decimal) -> StdResult<Decimal> {
    // Ensure the input is non-negative
    if square.is_sign_negative() {
        return Err(StdError::generic_err("square_root: Negative input"));
    }

    // Handle zero input
    if square.is_zero() {
        return Ok(Decimal::ZERO);
    }

    // Define the precision factor (must be even for square root)
    const PRECISION_FACTOR: u32 = 12;

    // Compute the multiplier as 10^PRECISION_FACTOR
    let multiplier = 10u128
        .checked_pow(PRECISION_FACTOR)
        .ok_or_else(|| StdError::generic_err("square_root: Exponentiation overflow"))?;

    // Convert multiplier to Decimal
    let multiplier_decimal = Decimal::from_u128(multiplier)
        .ok_or_else(|| StdError::generic_err("square_root: Multiplier conversion failed"))?;

    // Scale up the input to an integer representation
    let scaled_square = square
        .checked_mul(multiplier_decimal)
        .ok_or_else(|| StdError::generic_err("square_root: Overflow during scaling"))?;

    // Convert scaled value to u128
    let integer_square = scaled_square
        .floor()
        .to_u128()
        .ok_or_else(|| StdError::generic_err("square_root: Conversion to u128 failed"))?;

    // Compute the integer square root
    let integer_root = integer_square.integer_sqrt();

    // Calculate the precision for the result
    let result_precision = PRECISION_FACTOR / 2;

    // Convert the integer root back to Decimal
    decimal(integer_root, result_precision)
}

/// Calculates the cube root of a Decimal with high precision.
///
/// This function scales the input `cube` to an integer by multiplying it with a precision multiplier,
/// computes the integer cube root, and then scales the result back down.
///
/// # Arguments
///
/// * `cube` - The Decimal value for which to compute the cube root.
///
/// # Returns
///
/// * `StdResult<Decimal>` - The cube root of `cube`, or an error if the calculation fails.
pub(crate) fn cube_root(cube: Decimal) -> StdResult<Decimal> {
    // Handle zero input
    if cube.is_zero() {
        return Ok(Decimal::ZERO);
    }

    // Determine if the input is negative
    let is_negative = cube.is_sign_negative();

    // Use the absolute value for computation
    let cube_abs = cube.abs();

    // Define the precision factor (must be divisible by 3 for cube root)
    const PRECISION_FACTOR: u32 = 9;

    // Compute the multiplier as 10^PRECISION_FACTOR
    let multiplier = 10u128
        .checked_pow(PRECISION_FACTOR)
        .ok_or_else(|| StdError::generic_err("cube_root: Exponentiation overflow"))?;

    // Convert multiplier to Decimal
    let multiplier_decimal = Decimal::from_u128(multiplier)
        .ok_or_else(|| StdError::generic_err("cube_root: Multiplier conversion failed"))?;

    // Scale up the input to an integer representation
    let scaled_cube = cube_abs
        .checked_mul(multiplier_decimal)
        .ok_or_else(|| StdError::generic_err("cube_root: Overflow during scaling"))?;

    // Convert scaled value to u128
    let integer_cube = scaled_cube
        .floor()
        .to_u128()
        .ok_or_else(|| StdError::generic_err("cube_root: Conversion to u128 failed"))?;

    // Compute the integer cube root
    let integer_root = integer_cube.integer_cbrt();

    // Calculate the precision for the result
    let result_precision = PRECISION_FACTOR / 3;

    // Convert the integer root back to Decimal
    let root_decimal = decimal(integer_root, result_precision)?;

    // Apply the sign back
    if is_negative {
        Ok(-root_decimal)
    } else {
        Ok(root_decimal)
    }
}
