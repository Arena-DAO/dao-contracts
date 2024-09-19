use cosmwasm_std::{Decimal as StdDecimal, StdResult, Uint128};

use crate::{
    curves::{Constant, Linear, SquareRoot},
    utils::decimal,
    Curve, DecimalPlaces,
};

#[test]
fn constant_curve() -> StdResult<()> {
    // Set up normalization for 9 decimal places input, 6 decimal places output
    let normalize = DecimalPlaces::new(9, 6);
    // Create a constant curve with a value of 1.5
    let curve = Constant::new(decimal(15u128, 1)?, normalize);

    // Test spot price (should always be 1.5 regardless of supply)
    assert_eq!(
        StdDecimal::percent(150),
        curve.spot_price(Uint128::new(100))?
    );
    // Test reserve calculation (30 billion input should yield 45 million output)
    assert_eq!(
        Uint128::new(45_000_000),
        curve.reserve(Uint128::new(30_000_000_000))?
    );
    // Test supply calculation (36 million input should yield 24 billion output)
    assert_eq!(
        Uint128::new(24_000_000_000),
        curve.supply(Uint128::new(36_000_000))?
    );

    Ok(())
}

#[test]
fn linear_curve() -> StdResult<()> {
    // Set up normalization for 2 decimal places input, 8 decimal places output
    let normalize = DecimalPlaces::new(2, 8);
    // Create a linear curve with a slope of 0.1
    let curve = Linear::new(decimal(1u128, 1)?, normalize);

    // Test spot price (100 input should yield 0.1 output)
    assert_eq!(
        StdDecimal::permille(100),
        curve.spot_price(Uint128::new(100))?
    );
    // Test reserve calculation (1000 input should yield 500 million output)
    assert_eq!(
        Uint128::new(500_000_000),
        curve.reserve(Uint128::new(1000))?
    );
    // Test supply calculation (125 million input should yield 500 output)
    assert_eq!(Uint128::new(500), curve.supply(Uint128::new(125_000_000))?);

    Ok(())
}

#[test]
fn sqrt_curve() -> StdResult<()> {
    // Set up normalization for 6 decimal places input, 2 decimal places output
    let normalize = DecimalPlaces::new(6, 2);
    // Create a square root curve with a multiplier of 0.35
    let curve = SquareRoot::new(decimal(35u128, 2)?, normalize);

    // Test spot price (1 million input should yield 0.35 output)
    assert_eq!(
        StdDecimal::percent(35),
        curve.spot_price(Uint128::new(1_000_000))?
    );
    // Test reserve calculation (1 million input should yield 23 output)
    assert_eq!(Uint128::new(23), curve.reserve(Uint128::new(1_000_000))?);
    // Test supply calculation (23 input should yield 990,453 output)
    assert_eq!(Uint128::new(990_453), curve.supply(Uint128::new(23))?);

    Ok(())
}

#[test]
fn generic_curve_test() -> StdResult<()> {
    // Set up normalization for 6 decimal places input and output
    let normalize = DecimalPlaces::new(6, 6);
    // Create a vector of different curve types
    let curves: Vec<Box<dyn Curve>> = vec![
        Box::new(Constant::new(decimal(1u128, 0)?, normalize)),
        Box::new(Linear::new(decimal(1u128, 2)?, normalize)),
        Box::new(SquareRoot::new(decimal(1u128, 1)?, normalize)),
    ];

    for curve in curves.iter() {
        // Test with a supply of 1 million
        let supply = Uint128::new(1_000_000);
        // Calculate the reserve for this supply
        let reserve = curve.reserve(supply)?;
        // Calculate the supply from the reserve (should match original supply)
        let calculated_supply = curve.supply(reserve)?;

        // Calculate the percentage difference between original and calculated supply
        let diff =
            (calculated_supply.u128() as f64 - supply.u128() as f64).abs() / supply.u128() as f64;
        // Assert that the difference is within 1%
        assert!(
            diff <= 0.01,
            "Supply mismatch: original={}, calculated={}, difference={:.1}%",
            supply,
            calculated_supply,
            diff * 100.0
        );
    }

    Ok(())
}
