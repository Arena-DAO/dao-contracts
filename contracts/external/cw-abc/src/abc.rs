use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure, Decimal, StdResult, Uint128};
use cw_curves::{
    curves::{Constant, Linear, SquareRoot},
    utils::decimal,
    Curve, DecimalPlaces,
};
use dao_interface::token::NewDenomMetadata;

use crate::ContractError;

#[cw_serde]
pub struct SupplyToken {
    /// The denom to create for the supply token
    pub subdenom: String,
    /// Metadata for the supply token to create
    pub metadata: Option<NewDenomMetadata>,
    /// Number of decimal places for the supply token, needed for proper curve math.
    /// Default for token factory is 6
    pub decimals: u8,
    // Optional maximum supply
    pub max_supply: Option<Uint128>,
}

#[cw_serde]
pub struct ReserveToken {
    /// Reserve token denom (only support native for now)
    pub denom: String,
    /// Number of decimal places for the reserve token, needed for proper curve math.
    /// Same format as decimals above, eg. if it is uatom, where 1 unit is 10^-6 ATOM, use 6 here
    pub decimals: u8,
}

/// Struct for minimum and maximum values
#[cw_serde]
pub struct MinMax {
    pub min: Uint128,
    pub max: Uint128,
}

impl Copy for MinMax {}

#[cw_serde]
pub struct HatchConfig {
    /// The minimum and maximum contribution amounts (min, max) in the reserve token
    pub contribution_limits: MinMax,
    /// The initial raise range (min, max) in the reserve token
    pub initial_raise: MinMax,
    /// The initial allocation (θ), percentage of the initial raise allocated to the Funding Pool
    pub entry_fee: Decimal,
}

impl Copy for HatchConfig {}

impl HatchConfig {
    /// Validate the hatch config
    pub fn validate(&self) -> Result<(), ContractError> {
        ensure!(
            self.initial_raise.min < self.initial_raise.max,
            ContractError::HatchPhaseConfigError(
                "Initial raise minimum value must be less than maximum value.".to_string()
            )
        );

        ensure!(
            self.entry_fee <= Decimal::percent(100u64),
            ContractError::HatchPhaseConfigError(
                "Initial allocation percentage must be between 0 and 100.".to_string()
            )
        );

        Ok(())
    }
}

#[cw_serde]
pub struct OpenConfig {
    /// Percentage of capital put into the Reserve Pool during the Open phase
    /// when buying from the curve.
    pub entry_fee: Decimal,
    /// Exit taxation ratio
    pub exit_fee: Decimal,
}

impl OpenConfig {
    /// Validate the open config
    pub fn validate(&self) -> Result<(), ContractError> {
        ensure!(
            self.entry_fee <= Decimal::percent(100u64),
            ContractError::OpenPhaseConfigError(
                "Reserve percentage must be between 0 and 100.".to_string()
            )
        );

        ensure!(
            self.exit_fee <= Decimal::percent(100u64),
            ContractError::OpenPhaseConfigError(
                "Exit taxation percentage must be between 0 and 100.".to_string()
            )
        );

        Ok(())
    }
}

#[cw_serde]
pub struct ClosedConfig {}

impl ClosedConfig {
    /// Validate the closed config
    pub fn validate(&self) -> Result<(), ContractError> {
        Ok(())
    }
}

#[cw_serde]
pub struct CommonsPhaseConfig {
    /// The Hatch phase where initial contributors (Hatchers) participate in a hatch sale.
    pub hatch: HatchConfig,
    /// TODO Vest tokens after hatch phase
    /// The Vesting phase where tokens minted during the Hatch phase are locked (burning is disabled) to combat early speculation/arbitrage.
    /// pub vesting: VestingConfig,
    /// The Open phase where anyone can mint tokens by contributing the reserve token into the curve and becoming members of the Commons.
    pub open: OpenConfig,
    /// The Closed phase where the Commons is closed to new members.
    pub closed: ClosedConfig,
}

#[cw_serde]
pub enum CommonsPhase {
    Hatch,
    Open,
    Closed,
}

impl CommonsPhase {
    pub fn expect_hatch(&self) -> Result<(), ContractError> {
        ensure!(
            matches!(self, CommonsPhase::Hatch),
            ContractError::InvalidPhase {
                expected: "Hatch".to_string(),
                actual: format!("{:?}", self)
            }
        );
        Ok(())
    }

    pub fn expect_open(&self) -> Result<(), ContractError> {
        ensure!(
            matches!(self, CommonsPhase::Open),
            ContractError::InvalidPhase {
                expected: "Open".to_string(),
                actual: format!("{:?}", self)
            }
        );
        Ok(())
    }

    pub fn expect_closed(&self) -> Result<(), ContractError> {
        ensure!(
            matches!(self, CommonsPhase::Closed),
            ContractError::InvalidPhase {
                expected: "Closed".to_string(),
                actual: format!("{:?}", self)
            }
        );
        Ok(())
    }
}

impl CommonsPhaseConfig {
    /// Validate that the commons configuration is valid
    pub fn validate(&self) -> Result<(), ContractError> {
        self.hatch.validate()?;
        self.open.validate()?;
        self.closed.validate()?;

        Ok(())
    }
}

pub type CurveFn = Box<dyn Fn(DecimalPlaces) -> Box<dyn Curve>>;

// TODO Curve type validation?
// TODO add S-curve and taylor series
#[cw_serde]
pub enum CurveType {
    /// Constant always returns `value * 10^-scale` as spot price
    Constant { value: Uint128, scale: u32 },
    /// Linear returns `slope * 10^-scale * supply` as spot price
    Linear { slope: Uint128, scale: u32 },
    /// SquareRoot returns `slope * 10^-scale * supply^0.5` as spot price
    SquareRoot { slope: Uint128, scale: u32 },
}

impl CurveType {
    pub fn to_curve_fn(&self) -> StdResult<CurveFn> {
        match self.clone() {
            CurveType::Constant { value, scale } => {
                let value = decimal(value, scale)?;
                let calc =
                    move |places| -> Box<dyn Curve> { Box::new(Constant::new(value, places)) };
                Ok(Box::new(calc))
            }
            CurveType::Linear { slope, scale } => {
                let slope = decimal(slope, scale)?;
                let calc = move |places| -> Box<dyn Curve> { Box::new(Linear::new(slope, places)) };
                Ok(Box::new(calc))
            }
            CurveType::SquareRoot { slope, scale } => {
                let slope = decimal(slope, scale)?;
                let calc =
                    move |places| -> Box<dyn Curve> { Box::new(SquareRoot::new(slope, places)) };
                Ok(Box::new(calc))
            }
        }
    }
}
