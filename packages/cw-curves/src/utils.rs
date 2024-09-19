use cosmwasm_std::{Decimal as StdDecimal, StdError, StdResult};
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

/// Calculates the square root of a `Decimal` value with maximum precision using the Newton-Raphson method.
///
/// This function computes the square root of a non-negative `Decimal` value to a high degree of accuracy,
/// limited only by the precision of the `Decimal` type (which supports up to 28 decimal places).
/// It uses the Newton-Raphson iterative method to refine an initial guess until the result converges
/// within a predefined tolerance or until a maximum number of iterations is reached to prevent infinite loops.
/// If the maximum number of iterations is reached without convergence, the function returns the best approximation obtained.
///
/// # Arguments
///
/// * `value` - The non-negative `Decimal` value for which to calculate the square root.
///
/// # Returns
///
/// * `StdResult<Decimal>` - The square root of the input `value` on success, or an error if the input is invalid.
///
/// # Errors
///
/// * Returns an error if the input `value` is negative, as the square root of a negative number is undefined in real numbers.
///
/// # Examples
///
/// ```rust
/// use cosmwasm_std::StdResult;
/// use rust_decimal::Decimal;
///
/// let value = Decimal::new(16, 0); // Represents 16.0
/// let sqrt_value = square_root(value)?;
/// assert_eq!(sqrt_value, Decimal::new(4, 0)); // sqrt(16) = 4
/// ```
///
/// # Implementation Details
///
/// * The function uses the Newton-Raphson method for finding successively better approximations to the square root.
/// * It starts with an initial guess of `value / 2` and iteratively refines it.
/// * The iteration continues until the difference between successive guesses is less than a small epsilon value,
///   set to match the maximum precision of the `Decimal` type, or until the maximum number of iterations is reached.
/// * A maximum iteration limit (set to 100 iterations) is used to prevent infinite loops in cases where convergence is not achieved.
/// * If the method fails to converge within the maximum iterations, the function returns the best approximation obtained so far.
/// * Calculations are performed directly on `Decimal` values to maintain maximum precision.
///
/// # Notes
///
/// * The convergence criterion (`epsilon`) is set to `1e-28`, which is the smallest number representable by `Decimal`.
/// * The maximum number of iterations is set to 100, which is sufficient for convergence in typical cases.
/// * By returning the best approximation when the maximum iterations are reached, the function avoids errors while still providing a usable result.
pub fn square_root(value: Decimal) -> StdResult<Decimal> {
    if value.is_sign_negative() {
        return Err(StdError::generic_err("square_root: Negative input"));
    }
    if value.is_zero() {
        return Ok(Decimal::ZERO);
    }

    let two = Decimal::from(2);
    let mut last_guess = Decimal::ZERO;
    let mut guess = value / two;

    // Set convergence criteria based on Decimal's maximum precision (28 decimal places)
    let epsilon = Decimal::new(1, 28); // 1e-28

    let max_iterations = 100;
    let mut iterations = 0;

    while (guess - last_guess).abs() > epsilon && iterations < max_iterations {
        last_guess = guess;
        guess = (guess + value / guess) / two;

        iterations += 1;
    }

    // Return the best approximation found
    Ok(guess)
}

/// Calculates the cube root of a `Decimal` value with maximum precision using the Newton-Raphson method.
///
/// This function computes the cube root of a `Decimal` value to a high degree of accuracy,
/// limited only by the precision of the `Decimal` type (which supports up to 28 decimal places).
/// It uses the Newton-Raphson iterative method to refine an initial guess until the result converges
/// within a predefined tolerance or until a maximum number of iterations is reached to prevent infinite loops.
/// If the maximum number of iterations is reached without convergence, the function returns the best approximation obtained so far.
/// The function supports both positive and negative inputs, as the cube root of a negative number is negative.
///
/// # Arguments
///
/// * `value` - The `Decimal` value for which to calculate the cube root.
///
/// # Returns
///
/// * `StdResult<Decimal>` - The cube root of the input `value` on success, or an error if the input is invalid.
///
/// # Examples
///
/// ```rust
/// use cosmwasm_std::StdResult;
/// use rust_decimal::Decimal;
///
/// let value = Decimal::new(27, 0); // Represents 27.0
/// let cbrt_value = cube_root(value)?;
/// assert_eq!(cbrt_value, Decimal::new(3, 0)); // cbrt(27) = 3
///
/// let negative_value = Decimal::new(-8, 0); // Represents -8.0
/// let cbrt_negative = cube_root(negative_value)?;
/// assert_eq!(cbrt_negative, Decimal::new(-2, 0)); // cbrt(-8) = -2
/// ```
///
/// # Implementation Details
///
/// * The function uses the Newton-Raphson method for finding successively better approximations to the cube root.
/// * It starts with an initial guess of `value / 3` and iteratively refines it.
/// * The iteration continues until the difference between successive guesses is less than a small epsilon value,
///   set to match the maximum precision of the `Decimal` type, or until the maximum number of iterations is reached.
/// * A maximum iteration limit (set to 100 iterations) is used to prevent infinite loops in cases where convergence is not achieved.
/// * If the method fails to converge within the maximum iterations, the function returns the best approximation obtained so far.
/// * Negative inputs are handled correctly, as the cube root of a negative number is negative.
/// * Calculations are performed directly on `Decimal` values to maintain maximum precision.
///
/// # Notes
///
/// * The convergence criterion (`epsilon`) is set to `1e-28`, which is the smallest number representable by `Decimal`.
/// * The maximum number of iterations is set to 100, which is sufficient for convergence in typical cases.
/// * By returning the best approximation when the maximum iterations are reached, the function avoids errors while still providing a usable result.
pub fn cube_root(value: Decimal) -> StdResult<Decimal> {
    if value.is_zero() {
        return Ok(Decimal::ZERO);
    }

    let three = Decimal::from(3);
    let two = Decimal::from(2);
    let mut last_guess = Decimal::ZERO;
    let mut guess = value / three;

    // Set convergence criteria based on Decimal's maximum precision (28 decimal places)
    let epsilon = Decimal::new(1, 28); // 1e-28

    let max_iterations = 100;
    let mut iterations = 0;

    // Handle negative inputs by operating on the absolute value and restoring the sign at the end
    let is_negative = value.is_sign_negative();
    let value_abs = if is_negative { -value } else { value };

    while (guess - last_guess).abs() > epsilon && iterations < max_iterations {
        last_guess = guess;
        guess = (two * guess + value_abs / (guess * guess)) / three;

        iterations += 1;
    }

    if is_negative {
        Ok(-guess)
    } else {
        Ok(guess)
    }
}
