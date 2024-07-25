use anyhow::Result;
use cosmwasm_std::{coin, coins, to_json_binary, Decimal, Uint128, Uint64};
use cw_abc::msg::{ExecuteMsgFns, QueryMsgFns};
use cw_abc::msg::{HatcherAllowlistConfigMsg, HatcherAllowlistEntryMsg, InstantiateMsg};
use cw_abc::{
    abc::{
        ClosedConfig, CommonsPhase, CommonsPhaseConfig, CurveType, HatchConfig, MinMax, OpenConfig,
        ReserveToken, SupplyToken,
    },
    state::HatcherAllowlistConfigType,
};
use cw_orch::prelude::*;
use cw_orch_osmosis_test_tube::OsmosisTestTube;
use dao_cw_orch::{
    CwAbc, DaoDaoCore, DaoExternalTokenfactoryIssuer, DaoProposalSudo, DaoVotingTokenStaked,
};
use dao_interface::msg::QueryMsgFns as DaoQueryMsgFns;
use dao_interface::state::{Admin, ModuleInstantiateInfo};
use dao_interface::token::{DenomUnit, InitialBalance, NewDenomMetadata, NewTokenInfo};
use dao_voting_token_staked::msg::TokenInfo;
use dao_voting_token_staked::msg::{ExecuteMsgFns as _, QueryMsgFns as _};
use osmosis_test_tube::{Account, SigningAccount};
use speculoos::assert_that;
use speculoos::string::StrAssertions;
use std::rc::Rc;

const TEST_RESERVE_DENOM: &str = "uosmo";

struct TestAccounts {
    creator: Rc<SigningAccount>,
    buyer: Rc<SigningAccount>,
    donor: Rc<SigningAccount>,
}

fn setup() -> Result<(OsmosisTestTube, CwAbc<OsmosisTestTube>, u64, TestAccounts)> {
    let mut chain = OsmosisTestTube::new(coins(1_000_000_000_000, TEST_RESERVE_DENOM));

    let creator = chain.init_account(coins(1_000_000_000_000, TEST_RESERVE_DENOM))?;
    let buyer = chain.init_account(coins(1_000_000_000_000, TEST_RESERVE_DENOM))?;
    let donor = chain.init_account(coins(1_000_000_000_000, TEST_RESERVE_DENOM))?;

    let accounts = TestAccounts {
        creator,
        buyer,
        donor,
    };

    // Upload and instantiate cw-tokenfactory-issuer
    let token_factory_issuer =
        DaoExternalTokenfactoryIssuer::new("token_factory_issuer", chain.clone());
    token_factory_issuer.upload()?;

    // Upload ABC contract
    let abc = CwAbc::new("cw-abc", chain.clone());
    abc.upload()?;

    Ok((chain, abc, token_factory_issuer.code_id()?, accounts))
}

fn default_instantiate(
    token_issuer_code_id: u64,
    curve_type: CurveType,
    funding_pool_forwarding: Option<String>,
) -> InstantiateMsg {
    InstantiateMsg {
        token_issuer_code_id,
        funding_pool_forwarding,
        supply: SupplyToken {
            subdenom: "abc".to_string(),
            metadata: None,
            decimals: 6,
            max_supply: None,
        },
        reserve: ReserveToken {
            denom: TEST_RESERVE_DENOM.to_string(),
            decimals: 6,
        },
        curve_type,
        phase_config: CommonsPhaseConfig {
            hatch: HatchConfig {
                contribution_limits: MinMax {
                    min: Uint128::new(100),
                    max: Uint128::new(1000000),
                },
                initial_raise: MinMax {
                    min: Uint128::new(1000),
                    max: Uint128::new(10000),
                },
                entry_fee: Decimal::percent(10),
            },
            open: OpenConfig {
                entry_fee: Decimal::percent(5),
                exit_fee: Decimal::percent(5),
            },
            closed: ClosedConfig {},
        },
        hatcher_allowlist: None,
    }
}

#[test]
fn donate_should_fail_with_no_funds() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    let result = abc.call_as(&accounts.donor).donate(&[]);
    assert_that!(result.unwrap_err().to_string()).contains("No funds sent");

    Ok(())
}

#[test]
fn donate_should_fail_with_incorrect_denom() -> Result<()> {
    let (mut chain, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    chain.add_balance(accounts.donor.address(), coins(100000u128, "fake"))?;

    let result = abc.call_as(&accounts.donor).donate(&[coin(1, "fake")]);
    assert_that!(result.unwrap_err().to_string()).contains("Must send reserve token");

    Ok(())
}

#[test]
fn donate_should_forward_to_funding_pool() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::SquareRoot {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(
        token_issuer_code_id,
        curve_type,
        Some(accounts.creator.as_ref().address()),
    );
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    let donation_amount = 5u128;
    abc.call_as(&accounts.donor)
        .donate(&[coin(donation_amount, TEST_RESERVE_DENOM)])?;

    let curve_state = abc.curve_info()?;
    assert_eq!(curve_state.funding, Uint128::zero());

    let donation_response = abc.donations(None, None)?;
    assert!(donation_response.donations.len() == 1);

    Ok(())
}

#[test]
fn test_donate_and_withdraw() -> Result<()> {
    let (mut chain, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::SquareRoot {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    let donation_amount = 5u128;
    abc.call_as(&accounts.donor)
        .donate(&[coin(donation_amount, TEST_RESERVE_DENOM)])?;

    let curve_state = abc.curve_info()?;
    assert_eq!(curve_state.funding, Uint128::new(donation_amount));

    // Random user can't withdraw
    let random = chain.init_account(coins(1_000_000, TEST_RESERVE_DENOM))?;
    let result = abc.call_as(&random).withdraw(None);
    assert_that!(result.unwrap_err().to_string()).contains("not the contract's current owner");

    // Creator can withdraw
    abc.call_as(&accounts.creator).withdraw(None)?;

    let curve_state = abc.curve_info()?;
    assert_eq!(curve_state.funding, Uint128::zero());

    Ok(())
}

#[test]
fn test_pause() -> Result<()> {
    let (mut chain, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::SquareRoot {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    assert!(!abc.is_paused()?);

    // Random user can't pause
    let random = chain.init_account(coins(1_000_000, TEST_RESERVE_DENOM))?;
    let result = abc.call_as(&random).toggle_pause();
    assert_that!(result.unwrap_err().to_string()).contains("not the contract's current owner");

    // Creator can pause
    abc.call_as(&accounts.creator).toggle_pause()?;
    assert!(abc.is_paused()?);

    // Can't execute when paused
    let result = abc
        .call_as(&accounts.buyer)
        .buy(&[coin(100, TEST_RESERVE_DENOM)]);
    assert_that!(result.unwrap_err().to_string()).contains("Contract is paused");

    // Creator can unpause
    abc.call_as(&accounts.creator).toggle_pause()?;
    assert!(!abc.is_paused()?);

    Ok(())
}

#[test]
fn test_buy_in_hatch_phase() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    let buy_amount = 1000u128;
    abc.call_as(&accounts.buyer)
        .buy(&[coin(buy_amount, TEST_RESERVE_DENOM)])?;

    let curve_info = abc.curve_info()?;
    assert!(curve_info.supply > Uint128::zero());
    // Buy amount - entry fee
    assert_eq!(curve_info.reserve, Uint128::new(buy_amount - 100u128));

    let phase = abc.phase()?;
    assert_eq!(phase, CommonsPhase::Hatch);

    Ok(())
}

#[test]
fn test_transition_to_open_phase() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    let buy_amount = 1000000u128; // Max raise amount
    abc.call_as(&accounts.buyer)
        .buy(&[coin(buy_amount, TEST_RESERVE_DENOM)])?;

    let phase = abc.phase()?;
    assert_eq!(phase, CommonsPhase::Open);

    let supply_denom = abc.supply_denom()?;

    // Now try to sell
    let sell_amount = Uint128::new(1000);
    abc.call_as(&accounts.buyer)
        .sell(&[coin(sell_amount.u128(), supply_denom)])?;

    Ok(())
}

#[test]
fn test_sell_in_hatch_phase() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    let buy_amount = 1000u128;
    abc.call_as(&accounts.buyer)
        .buy(&[coin(buy_amount, TEST_RESERVE_DENOM)])?;

    let supply_denom = abc.supply_denom()?;

    let sell_amount = Uint128::new(100);
    let result = abc
        .call_as(&accounts.buyer)
        .sell(&[coin(sell_amount.u128(), supply_denom)]);
    assert_that!(result.unwrap_err().to_string()).contains("commons is locked");

    Ok(())
}

#[test]
fn test_update_curve_parameters() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    let new_curve_type = CurveType::SquareRoot {
        slope: Uint128::new(2),
        scale: 1,
    };
    abc.call_as(&accounts.creator)
        .update_curve(new_curve_type.clone())?;

    let updated_curve_type = abc.curve_type()?;
    assert_eq!(updated_curve_type, new_curve_type);

    Ok(())
}

#[test]
fn test_query_functions() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);
    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    let curve_info = abc.curve_info()?;
    assert_eq!(curve_info.reserve, Uint128::zero());
    assert_eq!(curve_info.supply, Uint128::zero());

    let phase = abc.phase()?;
    assert_eq!(phase, CommonsPhase::Hatch);

    let curve_type = abc.curve_type()?;
    assert_eq!(
        curve_type,
        CurveType::Linear {
            slope: Uint128::new(1),
            scale: 1
        }
    );

    Ok(())
}

#[test]
fn test_contribution_limits() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let mut msg = default_instantiate(token_issuer_code_id, curve_type, None);
    msg.phase_config.hatch.contribution_limits = MinMax {
        min: Uint128::new(100),
        max: Uint128::new(1000),
    };

    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    // Test minimum contribution limit
    let result = abc
        .call_as(&accounts.buyer)
        .buy(&[coin(99, TEST_RESERVE_DENOM)]);
    assert_that!(result.unwrap_err().to_string())
        .contains("Contribution must be less than or equal to");

    // Test maximum contribution limit
    let result = abc
        .call_as(&accounts.buyer)
        .buy(&[coin(1001, TEST_RESERVE_DENOM)]);
    assert_that!(result.unwrap_err().to_string())
        .contains("Contribution must be less than or equal to");

    // Test valid contribution
    let result = abc
        .call_as(&accounts.buyer)
        .buy(&[coin(500, TEST_RESERVE_DENOM)]);
    assert!(result.is_ok());

    Ok(())
}

#[test]
fn test_max_supply() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Constant {
        scale: 0,
        value: Uint128::new(1),
    };
    let mut msg = default_instantiate(token_issuer_code_id, curve_type, None);
    msg.supply.max_supply = Some(Uint128::new(10000));
    msg.phase_config.hatch.entry_fee = Decimal::zero();

    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    // Buy tokens up to max supply
    abc.call_as(&accounts.buyer)
        .buy(&[coin(10000, TEST_RESERVE_DENOM)])?;

    // Attempt to buy more tokens
    let result = abc
        .call_as(&accounts.buyer)
        .buy(&[coin(100, TEST_RESERVE_DENOM)]);
    assert_that!(result.unwrap_err().to_string()).contains("Cannot mint more tokens");

    // Verify max supply
    let curve_info = abc.curve_info()?;
    assert_eq!(curve_info.supply, Uint128::new(10000));

    Ok(())
}

#[test]
fn test_closing_curve() -> Result<()> {
    let (_, abc, token_issuer_code_id, accounts) = setup()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);

    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    // Buy tokens to transition to open phase
    abc.call_as(&accounts.buyer)
        .buy(&[coin(1000000, TEST_RESERVE_DENOM)])?;

    // Close the curve
    abc.call_as(&accounts.creator).close()?;

    // Verify curve is closed
    assert_eq!(abc.phase()?, CommonsPhase::Closed);

    // Attempt to buy tokens (should fail)
    let result = abc
        .call_as(&accounts.buyer)
        .buy(&[coin(500, TEST_RESERVE_DENOM)]);
    assert_that!(result.unwrap_err().to_string())
        .contains("commons is closed to new contributions");

    // Sell tokens (should succeed with no exit fee)
    let curve_denom = abc.supply_denom()?;
    let result = abc
        .call_as(&accounts.buyer)
        .sell(&[coin(1000, &curve_denom)]);
    assert!(result.is_ok());

    Ok(())
}

#[test]
fn test_dao_hatcher_functionality() -> Result<()> {
    let (mut chain, abc, token_issuer_code_id, accounts) = setup()?;
    let dao_core = DaoDaoCore::new("dao_dao", chain.clone());
    dao_core.upload()?;
    let dao_proposal_sudo = DaoProposalSudo::new("dao_proposal_sudo", chain.clone());
    dao_proposal_sudo.upload()?;
    let dao_voting_token = DaoVotingTokenStaked::new("dao_voting_token", chain.clone());
    dao_voting_token.upload()?;

    // Setup multiple DAOs
    let dao_addrs = (0..6)
        .map(|_| {
            setup_dao(
                &dao_core,
                &dao_voting_token,
                &dao_proposal_sudo,
                token_issuer_code_id,
                &accounts.creator,
                vec![&accounts.buyer],
            )
        })
        .collect::<Result<Vec<_>>>()?;
    chain.next_block()?;

    let curve_type = CurveType::Linear {
        slope: Uint128::new(1),
        scale: 1,
    };
    let msg = default_instantiate(token_issuer_code_id, curve_type, None);

    abc.call_as(&accounts.creator)
        .instantiate(&msg, None, None)?;

    // Add DAOs to allowlist with different priorities and contribution limits
    for (i, dao_addr) in dao_addrs.iter().enumerate().take(5) {
        abc.call_as(&accounts.creator).update_hatch_allowlist(
            vec![HatcherAllowlistEntryMsg {
                addr: dao_addr.to_string(),
                config: HatcherAllowlistConfigMsg {
                    config_type: HatcherAllowlistConfigType::DAO {
                        priority: Some(Uint64::MAX - Uint64::new(i as u64)),
                    },
                    contribution_limits_override: Some(MinMax {
                        min: Uint128::one(),
                        max: Uint128::from(10u128) * Uint128::from(i as u128 + 1u128),
                    }),
                },
            }],
            vec![],
        )?;
    }

    // Add a DAO tied for the highest priority
    abc.call_as(&accounts.creator).update_hatch_allowlist(
        vec![HatcherAllowlistEntryMsg {
            addr: dao_addrs[5].to_string(),
            config: HatcherAllowlistConfigMsg {
                config_type: HatcherAllowlistConfigType::DAO {
                    priority: Some(Uint64::MAX - Uint64::new(4)),
                },
                contribution_limits_override: Some(MinMax {
                    min: Uint128::one(),
                    max: Uint128::from(1000u128),
                }),
            },
        }],
        vec![],
    )?;

    // Add a DAO without priority
    let dao_no_priority = setup_dao(
        &dao_core,
        &dao_voting_token,
        &dao_proposal_sudo,
        token_issuer_code_id,
        &accounts.creator,
        vec![&accounts.donor],
    )?;
    abc.call_as(&accounts.creator).update_hatch_allowlist(
        vec![HatcherAllowlistEntryMsg {
            addr: dao_no_priority.to_string(),
            config: HatcherAllowlistConfigMsg {
                config_type: HatcherAllowlistConfigType::DAO { priority: None },
                contribution_limits_override: Some(MinMax {
                    min: Uint128::one(),
                    max: Uint128::from(100u128),
                }),
            },
        }],
        vec![],
    )?;
    chain.next_block()?;

    // Check contribution limit (should be 50, the highest priority DAO's limit)
    let err = abc
        .call_as(&accounts.buyer)
        .buy(&[coin(51, TEST_RESERVE_DENOM)])
        .unwrap_err();
    assert_that!(err.to_string()).contains("Contribution must be less than or equal to");

    // Remove the highest priority DAO
    abc.call_as(&accounts.creator)
        .update_hatch_allowlist(vec![], vec![dao_addrs[4].to_string()])?;

    // Check new contribution limit (should be 1000, the next highest priority DAO's limit)
    abc.call_as(&accounts.buyer)
        .buy(&[coin(1000, TEST_RESERVE_DENOM)])?;

    // Add an individual address to the allowlist
    let individual = chain.init_account(coins(1_000_000_000_000, TEST_RESERVE_DENOM))?;

    abc.call_as(&accounts.creator).update_hatch_allowlist(
        vec![HatcherAllowlistEntryMsg {
            addr: individual.address(),
            config: HatcherAllowlistConfigMsg {
                config_type: HatcherAllowlistConfigType::Address {},
                contribution_limits_override: Some(MinMax {
                    min: Uint128::one(),
                    max: Uint128::from(2000u128),
                }),
            },
        }],
        vec![],
    )?;
    chain.next_block()?;

    // Check that the individual address limit takes precedence
    abc.call_as(&individual)
        .buy(&[coin(2000, TEST_RESERVE_DENOM)])?;

    // Non-DAO member cannot buy tokens
    let result = abc
        .call_as(&accounts.creator)
        .buy(&[coin(50, TEST_RESERVE_DENOM)]);
    assert_that!(result.unwrap_err().to_string()).contains("not in the hatcher allowlist");

    // DAO member without priority can buy tokens within their limit
    abc.call_as(&accounts.donor)
        .buy(&[coin(100, TEST_RESERVE_DENOM)])?;

    Ok(())
}

#[test]
pub fn arena() -> Result<()> {
    let (mut chain, abc, token_issuer_code_id, accounts) = setup()?;
    let dao = chain.init_account(coins(1_000_000_000_000, TEST_RESERVE_DENOM))?;
    let buyer = chain.init_account(coins(1_000_000_000_000_000, TEST_RESERVE_DENOM))?;

    let msg = InstantiateMsg {
        token_issuer_code_id,
        funding_pool_forwarding: None,
        supply: SupplyToken {
            subdenom: "uarena".to_string(),
            metadata: Some(NewDenomMetadata {
                name: "Arena Token".to_string(),
                description: "The governance token of the Arena DAO".to_string(),
                symbol: "ARENA".to_string(),
                display: "arena".to_string(),
                additional_denom_units: Some(vec![DenomUnit {
                    denom: "arena".to_string(),
                    exponent: 6,
                    aliases: vec![],
                }]),
            }),
            decimals: 6,
            max_supply: Some(Uint128::new(1_000_000_000_000)),
        },
        reserve: ReserveToken {
            denom: TEST_RESERVE_DENOM.to_string(),
            decimals: 6,
        },
        curve_type: CurveType::SquareRoot {
            slope: Uint128::new(3),
            scale: 7,
        },
        phase_config: CommonsPhaseConfig {
            hatch: HatchConfig {
                contribution_limits: MinMax {
                    min: Uint128::new(100_000),
                    max: Uint128::new(1_000_000),
                },
                initial_raise: MinMax {
                    min: Uint128::zero(),
                    max: Uint128::new(92_951_601),
                },
                entry_fee: Decimal::zero(),
            },
            open: OpenConfig {
                entry_fee: Decimal::from_atomics(999965u128, 6)?,
                exit_fee: Decimal::zero(),
            },
            closed: ClosedConfig {},
        },
        hatcher_allowlist: Some(vec![
            HatcherAllowlistEntryMsg {
                addr: accounts.creator.address(),
                config: HatcherAllowlistConfigMsg {
                    config_type: HatcherAllowlistConfigType::Address {},
                    contribution_limits_override: Some(MinMax {
                        min: Uint128::zero(),
                        max: Uint128::new(17_888_544),
                    }),
                },
            },
            HatcherAllowlistEntryMsg {
                addr: dao.address(),
                config: HatcherAllowlistConfigMsg {
                    config_type: HatcherAllowlistConfigType::Address {},
                    contribution_limits_override: Some(MinMax {
                        min: Uint128::zero(),
                        max: Uint128::new(75_063_057),
                    }),
                },
            },
        ]),
    };

    abc.call_as(&dao).instantiate(&msg, None, None)?;

    // Founder allocation
    abc.call_as(&accounts.creator)
        .buy(&coins(17_888_544, TEST_RESERVE_DENOM))?;

    // Get denom
    let supply_denom = abc.supply_denom()?;

    // Check balance
    let balance = chain.query_balance(&accounts.creator.address(), &supply_denom)?;
    assert_eq!(balance, Uint128::new(200_000_001_000));

    // Arena Gladiators + Treasury
    abc.call_as(&dao)
        .buy(&coins(75_063_057, TEST_RESERVE_DENOM))?;

    // Check we're in the open phase now
    let phase_config = abc.phase_config()?;
    assert_eq!(phase_config.phase, CommonsPhase::Open);

    // Check the current supply
    let curve_info = abc.curve_info()?;
    assert_eq!(curve_info.supply, Uint128::new(600_000_002_000));

    // User buys the rest
    abc.call_as(&buyer)
        .buy(&coins(3_058_525_685_714, TEST_RESERVE_DENOM))?;

    // Check the supply
    let curve_info = abc.curve_info()?;
    assert_eq!(curve_info.supply, Uint128::new(1_000_000_000_000));

    // Check the funding pool
    assert_eq!(curve_info.funding, Uint128::new(3_058_418_637_315));

    // Max supply reached
    let result = abc.call_as(&buyer).buy(&coins(1, TEST_RESERVE_DENOM));
    assert_that!(result.unwrap_err().to_string()).contains("Cannot mint more tokens");

    // Sell some into the curve
    abc.call_as(&dao).sell(&coins(1_000_000, supply_denom.clone()))?;

    Ok(())
}

// Helper function to setup a DAO
fn setup_dao(
    dao_core: &DaoDaoCore<OsmosisTestTube>,
    dao_voting_token: &DaoVotingTokenStaked<OsmosisTestTube>,
    dao_proposal_sudo: &DaoProposalSudo<OsmosisTestTube>,
    token_issuer_code_id: u64,
    admin: &Rc<SigningAccount>,
    members: Vec<&Rc<SigningAccount>>,
) -> Result<Addr> {
    dao_core.call_as(admin).instantiate(
        &dao_interface::msg::InstantiateMsg {
            admin: None,
            name: "DAO DAO".to_string(),
            description: "A DAO that makes DAO tooling".to_string(),
            image_url: None,
            automatically_add_cw20s: false,
            automatically_add_cw721s: false,
            voting_module_instantiate_info: ModuleInstantiateInfo {
                code_id: dao_voting_token.code_id()?,
                msg: to_json_binary(&dao_voting_token_staked::msg::InstantiateMsg {
                    token_info: TokenInfo::New(NewTokenInfo {
                        token_issuer_code_id,
                        subdenom: "cat".to_string(),
                        metadata: Some(NewDenomMetadata {
                            description: "Awesome token, get it meow!".to_string(),
                            additional_denom_units: Some(vec![DenomUnit {
                                denom: "cat".to_string(),
                                exponent: 6,
                                aliases: vec![],
                            }]),
                            display: "cat".to_string(),
                            name: "Cat Token".to_string(),
                            symbol: "CAT".to_string(),
                        }),
                        initial_balances: members
                            .iter()
                            .map(|x| InitialBalance {
                                address: x.address(),
                                amount: Uint128::new(100),
                            })
                            .collect(),
                        initial_dao_balance: None,
                    }),
                    unstaking_duration: None,
                    active_threshold: None,
                })
                .unwrap(),
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "DAO DAO Voting Module".to_string(),
            },
            proposal_modules_instantiate_info: vec![ModuleInstantiateInfo {
                code_id: dao_proposal_sudo.code_id()?,
                msg: to_json_binary(&dao_proposal_sudo::msg::InstantiateMsg {
                    root: admin.address(),
                })?,
                admin: Some(Admin::CoreModule {}),
                funds: vec![],
                label: "DAO Proposal Sudo".to_string(),
            }],
            initial_items: None,
            dao_uri: None,
        },
        None,
        None,
    )?;

    let voting_module = dao_core.voting_module()?;

    dao_voting_token.set_address(&Addr::unchecked(voting_module));
    let denom = dao_voting_token.denom()?.denom;

    for caller in members.iter() {
        dao_voting_token
            .call_as(caller)
            .stake(&coins(100, denom.clone()))?;
    }

    Ok(dao_core.address()?)
}
