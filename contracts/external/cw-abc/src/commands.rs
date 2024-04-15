use cosmwasm_std::{
    ensure, to_json_binary, Addr, BankMsg, Coin, CosmosMsg, Decimal as StdDecimal, DepsMut, Env,
    MessageInfo, QuerierWrapper, Response, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw_tokenfactory_issuer::msg::ExecuteMsg as IssuerExecuteMsg;
use cw_utils::must_pay;
use std::ops::Deref;

use crate::abc::{CommonsPhase, CurveType};
use crate::msg::{DonationPool, HatcherAllowlistEntry, UpdatePhaseConfigMsg};
use crate::state::{
    hatcher_allowlist, HatcherAllowlistConfigType, CURVE_STATE, CURVE_TYPE, DONATIONS,
    FUNDING_POOL_FORWARDING, HATCHERS, IS_PAUSED, MAX_SUPPLY, PHASE, PHASE_CONFIG, SUPPLY_DENOM,
    TOKEN_ISSUER_CONTRACT,
};
use crate::ContractError;

pub fn buy(deps: DepsMut, _env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let curve_type = CURVE_TYPE.load(deps.storage)?;
    let curve_fn = curve_type.to_curve_fn();

    let mut curve_state = CURVE_STATE.load(deps.storage)?;

    let payment = must_pay(&info, &curve_state.reserve_denom)?;

    // Load the phase config and phase
    let phase_config = PHASE_CONFIG.load(deps.storage)?;
    let mut phase = PHASE.load(deps.storage)?;

    let (reserved, funded) = match &phase {
        CommonsPhase::Hatch => {
            let hatch_config = phase_config.hatch;
            // Check that the potential hatcher is allowlisted
            assert_allowlisted(deps.querier, deps.storage, &info.sender)?;

            // Update hatcher contribution
            let contribution = update_hatcher_contributions(deps.storage, &info.sender, payment)?;

            // Check contribution is within limits
            if contribution < hatch_config.contribution_limits.min
                || contribution > hatch_config.contribution_limits.max
            {
                return Err(ContractError::ContributionLimit {
                    min: hatch_config.contribution_limits.min,
                    max: hatch_config.contribution_limits.max,
                });
            }

            // Check if the initial_raise max has been met
            if curve_state.reserve + payment >= hatch_config.initial_raise.max {
                // Transition to the Open phase
                phase = CommonsPhase::Open;

                // Can clean up state here
                hatcher_allowlist().clear(deps.storage);

                PHASE.save(deps.storage, &phase)?;
            }

            calculate_reserved_and_funded(payment, hatch_config.entry_fee)
        }
        CommonsPhase::Open => calculate_reserved_and_funded(payment, phase_config.open.entry_fee),
        CommonsPhase::Closed => {
            return Err(ContractError::CommonsClosed {});
        }
    };

    // Calculate how many tokens can be purchased with this and mint them
    let curve = curve_fn(curve_state.clone().decimals);
    curve_state.reserve += reserved;

    // Calculate the supply based on the reserve
    let new_supply = curve.supply(curve_state.reserve);
    let minted = new_supply
        .checked_sub(curve_state.supply)
        .map_err(StdError::overflow)?;

    // Check that the minted amount has not exceeded the max supply (if configured)
    if let Some(max_supply) = MAX_SUPPLY.may_load(deps.storage)? {
        if new_supply > max_supply {
            return Err(ContractError::CannotExceedMaxSupply { max: max_supply });
        }
    }

    // Mint tokens for sender by calling mint on the cw-tokenfactory-issuer contract
    let issuer_addr = TOKEN_ISSUER_CONTRACT.load(deps.storage)?;
    let mut msgs: Vec<CosmosMsg> = vec![CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: issuer_addr.to_string(),
        msg: to_json_binary(&IssuerExecuteMsg::Mint {
            to_address: info.sender.to_string(),
            amount: minted,
        })?,
        funds: vec![],
    })];

    // Send funding to fee recipient
    if funded > Uint128::zero() {
        if let Some(funding_pool_forwarding) = FUNDING_POOL_FORWARDING.may_load(deps.storage)? {
            msgs.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: funding_pool_forwarding.to_string(),
                amount: vec![Coin {
                    amount: funded,
                    denom: curve_state.reserve_denom.clone(),
                }],
            }))
        } else {
            curve_state.funding += funded;
        }
    };

    // Save the new curve state
    curve_state.supply = new_supply;
    CURVE_STATE.save(deps.storage, &curve_state)?;

    Ok(Response::new()
        .add_messages(msgs)
        .add_attribute("action", "buy")
        .add_attribute("from", info.sender)
        .add_attribute("reserved", reserved)
        .add_attribute("funded", funded)
        .add_attribute("supply", minted))
}

/// Return the reserved and funded amounts based on the payment and the allocation ratio
fn calculate_reserved_and_funded(
    payment: Uint128,
    allocation_ratio: StdDecimal,
) -> (Uint128, Uint128) {
    let funded = payment * allocation_ratio;
    let reserved = payment.checked_sub(funded).unwrap(); // Since allocation_ratio is < 1, this subtraction is safe
    (reserved, funded)
}

/// Add the hatcher's contribution to the total contributions
fn update_hatcher_contributions(
    storage: &mut dyn Storage,
    hatcher: &Addr,
    contribution: Uint128,
) -> StdResult<Uint128> {
    HATCHERS.update(storage, hatcher, |amount| -> StdResult<_> {
        match amount {
            Some(mut amount) => {
                amount += contribution;
                Ok(amount)
            }
            None => Ok(contribution),
        }
    })
}

/// Sell tokens on the bonding curve
pub fn sell(deps: DepsMut, _env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let curve_type = CURVE_TYPE.load(deps.storage)?;
    let curve_fn = curve_type.to_curve_fn();

    let supply_denom = SUPPLY_DENOM.load(deps.storage)?;
    let burn_amount = must_pay(&info, &supply_denom)?;

    let issuer_addr = TOKEN_ISSUER_CONTRACT.load(deps.storage)?;

    // Burn the sent supply tokens
    let burn_msgs: Vec<CosmosMsg> = vec![
        // Send tokens to the issuer contract to be burned
        CosmosMsg::Bank(BankMsg::Send {
            to_address: issuer_addr.to_string().clone(),
            amount: vec![Coin {
                amount: burn_amount,
                denom: supply_denom,
            }],
        }),
        // Execute burn on the cw-tokenfactory-issuer contract
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: issuer_addr.to_string(),
            msg: to_json_binary(&IssuerExecuteMsg::Burn {
                from_address: issuer_addr.to_string(),
                amount: burn_amount,
            })?,
            funds: vec![],
        }),
    ];

    let mut curve_state = CURVE_STATE.load(deps.storage)?;
    let curve = curve_fn(curve_state.clone().decimals);

    // Reduce the supply by the amount burned
    curve_state.supply = curve_state
        .supply
        .checked_sub(burn_amount)
        .map_err(StdError::overflow)?;

    // Calculate the new reserve based on the new supply
    let new_reserve = curve.reserve(curve_state.supply);

    // Calculate how many reserve tokens to release based on the sell amount
    let released_reserve = curve_state
        .reserve
        .checked_sub(new_reserve)
        .map_err(StdError::overflow)?;

    // Calculate the exit tax
    let taxed_amount = calculate_exit_fee(deps.storage, released_reserve)?;

    // Update the curve state
    curve_state.reserve = new_reserve;

    // Calculate the amount of tokens to send to the sender
    // Subtract the taxed amount from the released amount
    let released = released_reserve
        .checked_sub(taxed_amount)
        .map_err(StdError::overflow)?;

    // Now send the tokens to the sender and any fees to the DAO
    let mut send_msgs: Vec<CosmosMsg> = vec![CosmosMsg::Bank(BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: vec![Coin {
            amount: released,
            denom: curve_state.reserve_denom.clone(),
        }],
    })];

    // Send exit fees to the to the fee recipient
    if taxed_amount > Uint128::zero() {
        if let Some(funding_pool_forwarding) = FUNDING_POOL_FORWARDING.may_load(deps.storage)? {
            send_msgs.push(CosmosMsg::Bank(BankMsg::Send {
                to_address: funding_pool_forwarding.to_string(),
                amount: vec![Coin {
                    amount: taxed_amount,
                    denom: curve_state.reserve_denom.clone(),
                }],
            }))
        } else {
            curve_state.funding += taxed_amount;
        }
    }

    // Save the curve state
    CURVE_STATE.save(deps.storage, &curve_state)?;

    Ok(Response::new()
        .add_messages(burn_msgs)
        .add_messages(send_msgs)
        .add_attribute("action", "burn")
        .add_attribute("from", info.sender)
        .add_attribute("amount", burn_amount)
        .add_attribute("burned", released_reserve)
        .add_attribute("funded", taxed_amount))
}

/// Calculate the exit taxation for the sell amount based on the phase
fn calculate_exit_fee(
    storage: &dyn Storage,
    sell_amount: Uint128,
) -> Result<Uint128, ContractError> {
    // Load the phase config and phase
    let phase = PHASE.load(storage)?;
    let phase_config = PHASE_CONFIG.load(storage)?;

    // Calculate the exit tax based on the phase
    let exit_fee = match &phase {
        CommonsPhase::Hatch => phase_config.hatch.exit_fee,
        CommonsPhase::Open => phase_config.open.exit_fee,
        CommonsPhase::Closed => return Ok(Uint128::zero()),
    };

    // Ensure the exit fee is not greater than 100%
    ensure!(
        exit_fee <= StdDecimal::one(),
        ContractError::InvalidExitFee {}
    );

    // This won't ever overflow because it's checked
    let taxed_amount = sell_amount * exit_fee;
    Ok(taxed_amount)
}

/// Transitions the bonding curve to a closed phase where only sells are allowed
pub fn close(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    PHASE.save(deps.storage, &CommonsPhase::Closed)?;

    Ok(Response::new().add_attribute("action", "close"))
}

/// Send a donation to the funding pool
pub fn donate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    pool: DonationPool,
) -> Result<Response, ContractError> {
    let mut curve_state = CURVE_STATE.load(deps.storage)?;

    let payment = must_pay(&info, &curve_state.reserve_denom)?;

    let mut msgs = vec![];
    match pool {
        DonationPool::Funding {} => {
            if let Some(funding_pool_forwarding) = FUNDING_POOL_FORWARDING.may_load(deps.storage)? {
                msgs.push(CosmosMsg::Bank(BankMsg::Send {
                    to_address: funding_pool_forwarding.to_string(),
                    amount: info.funds,
                }));
            } else {
                curve_state.funding += payment;

                CURVE_STATE.save(deps.storage, &curve_state)?;
            }
        }
        DonationPool::Reserve {} => {
            curve_state.reserve += payment;

            CURVE_STATE.save(deps.storage, &curve_state)?;
        }
    };

    // No minting of tokens is necessary, the supply stays the same
    let total_donation =
        DONATIONS.update(deps.storage, &info.sender, |maybe_amount| -> StdResult<_> {
            if let Some(amount) = maybe_amount {
                Ok(amount.checked_add(payment)?)
            } else {
                Ok(payment)
            }
        })?;

    Ok(Response::new()
        .add_attribute("action", "donate")
        .add_attribute("donor", info.sender)
        .add_attribute("amount", payment)
        .add_attribute("pool", pool.to_string())
        .add_attribute("total_donation", total_donation)
        .add_messages(msgs))
}

/// Withdraw funds from the funding pool (only callable by owner)
pub fn withdraw(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    // Validate ownership
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    let mut curve_state = CURVE_STATE.load(deps.storage)?;

    // Get amount to withdraw
    let amount = amount.unwrap_or(curve_state.funding);

    // Construct the withdraw message
    let msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: vec![Coin {
            denom: curve_state.reserve_denom.clone(),
            amount,
        }],
    });

    // Update the curve state
    curve_state.funding = curve_state
        .funding
        .checked_sub(amount)
        .map_err(StdError::overflow)?;
    CURVE_STATE.save(deps.storage, &curve_state)?;

    Ok(Response::new()
        .add_attribute("action", "withdraw")
        .add_attribute("withdrawer", info.sender)
        .add_attribute("amount", amount)
        .add_message(msg))
}

/// Updates the funding pool forwarding (only callable by owner)
pub fn update_funding_pool_forwarding(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    address: Option<String>,
) -> Result<Response, ContractError> {
    // Validate ownership
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    // Update the funding pool forwarding
    match &address {
        Some(address) => {
            FUNDING_POOL_FORWARDING.save(deps.storage, &deps.api.addr_validate(address)?)?;
        }
        None => FUNDING_POOL_FORWARDING.remove(deps.storage),
    };

    Ok(Response::new()
        .add_attribute("action", "update_funding_pool_forwarding")
        .add_attribute("address", address.unwrap_or_default()))
}

/// Check if the sender is allowlisted for the hatch phase
fn assert_allowlisted(
    querier: QuerierWrapper,
    storage: &dyn Storage,
    hatcher: &Addr,
) -> Result<(), ContractError> {
    if !hatcher_allowlist().is_empty(storage) {
        ensure!(
            hatcher_allowlist().has(storage, hatcher)
                || is_allowlisted_through_daos(querier, storage, hatcher),
            ContractError::SenderNotAllowlisted {
                sender: hatcher.to_string(),
            }
        );
    }

    Ok(())
}

fn is_allowlisted_through_daos(
    querier: QuerierWrapper,
    storage: &dyn Storage,
    hatcher: &Addr,
) -> bool {
    for (dao, _) in hatcher_allowlist()
        .idx
        .config_type
        .prefix(HatcherAllowlistConfigType::DAO {}.to_string())
        .range(storage, None, None, cosmwasm_std::Order::Ascending)
        .flatten()
    {
        let voting_power_response_result: StdResult<
            dao_interface::voting::VotingPowerAtHeightResponse,
        > = querier.query_wasm_smart(
            dao,
            &dao_interface::msg::QueryMsg::VotingPowerAtHeight {
                address: hatcher.to_string(),
                height: None,
            },
        );

        if let Ok(voting_power_response) = voting_power_response_result {
            if voting_power_response.power > Uint128::zero() {
                return true;
            }
        }
    }

    false
}

/// Set the maximum supply (only callable by owner)
/// If `max_supply` is set to None there will be no limit.`
pub fn update_max_supply(
    deps: DepsMut,
    info: MessageInfo,
    max_supply: Option<Uint128>,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    match max_supply {
        Some(max) => MAX_SUPPLY.save(deps.storage, &max)?,
        None => MAX_SUPPLY.remove(deps.storage),
    }

    Ok(Response::new()
        .add_attribute("action", "update_max_supply")
        .add_attribute("value", max_supply.unwrap_or(Uint128::MAX).to_string()))
}

/// Toggles the paused state (only callable by owner)
pub fn toggle_pause(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    let is_paused =
        IS_PAUSED.update(deps.storage, |is_paused| -> StdResult<_> { Ok(!is_paused) })?;

    Ok(Response::new()
        .add_attribute("action", "toggle_pause")
        .add_attribute("is_paused", is_paused.to_string()))
}

/// Add and remove addresses from the hatcher allowlist (only callable by owner)
pub fn update_hatch_allowlist(
    deps: DepsMut,
    info: MessageInfo,
    to_add: Vec<HatcherAllowlistEntry<String>>,
    to_remove: Vec<String>,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    let list = hatcher_allowlist();

    // Add addresses to the allowlist
    for allow in to_add {
        let addr = deps.api.addr_validate(allow.addr.as_str())?;

        if !list.has(deps.storage, &addr) {
            list.save(deps.storage, &addr, &allow.config)?;
        }
    }

    // Remove addresses from the allowlist
    for deny in to_remove {
        let addr = deps.api.addr_validate(deny.as_str())?;

        list.remove(deps.storage, &addr)?;
    }

    Ok(Response::new().add_attributes(vec![("action", "update_hatch_allowlist")]))
}

/// Update the configuration of a particular phase (only callable by owner)
pub fn update_phase_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    update_phase_config_msg: UpdatePhaseConfigMsg,
) -> Result<Response, ContractError> {
    // Assert that the sender is the contract owner
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    // Load phase and phase config
    let phase = PHASE.load(deps.storage)?;

    // Load the current phase config
    let mut phase_config = PHASE_CONFIG.load(deps.storage)?;

    match update_phase_config_msg {
        UpdatePhaseConfigMsg::Hatch {
            exit_fee,
            initial_raise,
            entry_fee,
            contribution_limits,
        } => {
            // Check we are in the hatch phase
            phase.expect_hatch()?;

            // Update the hatch config if new values are provided
            if let Some(contribution_limits) = contribution_limits {
                phase_config.hatch.contribution_limits = contribution_limits;
            }
            if let Some(exit_fee) = exit_fee {
                phase_config.hatch.exit_fee = exit_fee;
            }
            if let Some(initial_raise) = initial_raise {
                phase_config.hatch.initial_raise = initial_raise;
            }
            if let Some(entry_fee) = entry_fee {
                phase_config.hatch.entry_fee = entry_fee;
            }

            // Validate config
            phase_config.hatch.validate()?;
            PHASE_CONFIG.save(deps.storage, &phase_config)?;

            Ok(Response::new().add_attribute("action", "update_hatch_phase_config"))
        }
        UpdatePhaseConfigMsg::Open {
            exit_fee,
            entry_fee,
        } => {
            // Check we are in the open phase
            phase.expect_open()?;

            // Update the hatch config if new values are provided
            if let Some(entry_fee) = entry_fee {
                phase_config.open.entry_fee = entry_fee;
            }
            if let Some(exit_fee) = exit_fee {
                phase_config.hatch.exit_fee = exit_fee;
            }

            // Validate config
            phase_config.open.validate()?;
            PHASE_CONFIG.save(deps.storage, &phase_config)?;

            Ok(Response::new().add_attribute("action", "update_open_phase_config"))
        }
        // TODO what should the closed phase configuration be, is there one?
        _ => todo!(),
    }
}

/// Update the bonding curve. (only callable by owner)
/// NOTE: this changes the pricing. Use with caution.
/// TODO: what other limitations do we want to put on this?
pub fn update_curve(
    deps: DepsMut,
    info: MessageInfo,
    curve_type: CurveType,
) -> Result<Response, ContractError> {
    cw_ownable::assert_owner(deps.storage, &info.sender)?;

    CURVE_TYPE.save(deps.storage, &curve_type)?;

    Ok(Response::new().add_attribute("action", "close"))
}

/// Update the ownership of the contract
pub fn update_ownership(
    deps: DepsMut,
    env: &Env,
    info: &MessageInfo,
    action: cw_ownable::Action,
) -> Result<Response, ContractError> {
    let ownership = cw_ownable::update_ownership(
        DepsMut {
            storage: deps.storage,
            api: deps.api,
            querier: QuerierWrapper::new(deps.querier.deref()),
        },
        &env.block,
        &info.sender,
        action,
    )?;

    Ok(Response::default().add_attributes(ownership.into_attributes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::prelude::*;
    use cosmwasm_std::testing::*;

    mod donate {
        use super::*;
        use crate::abc::CurveType;
        use crate::testing::{mock_init, TEST_CREATOR};
        use cosmwasm_std::coin;
        use cw_utils::PaymentError;

        const TEST_DONOR: &str = "donor";

        fn exec_donate(deps: DepsMut, donation_amount: u128) -> Result<Response, ContractError> {
            donate(
                deps,
                mock_env(),
                mock_info(TEST_DONOR, &[coin(donation_amount, TEST_RESERVE_DENOM)]),
                DonationPool::Funding {},
            )
        }

        #[test]
        fn should_fail_with_no_funds() -> Result<(), ContractError> {
            let mut deps = mock_dependencies();
            let curve_type = CurveType::Linear {
                slope: Uint128::new(1),
                scale: 1,
            };
            let init_msg = default_instantiate_msg(2, 8, curve_type);
            mock_init(deps.as_mut(), init_msg)?;

            let res = exec_donate(deps.as_mut(), 0);
            assert_that!(res)
                .is_err()
                .is_equal_to(ContractError::Payment(PaymentError::NoFunds {}));

            Ok(())
        }

        #[test]
        fn should_fail_with_incorrect_denom() -> Result<(), ContractError> {
            let mut deps = mock_dependencies();
            let curve_type = CurveType::Linear {
                slope: Uint128::new(1),
                scale: 1,
            };
            let init_msg = default_instantiate_msg(2, 8, curve_type);
            mock_init(deps.as_mut(), init_msg)?;

            let res = donate(
                deps.as_mut(),
                mock_env(),
                mock_info(TEST_DONOR, &[coin(1, "fake")]),
                DonationPool::Funding {},
            );
            assert_that!(res)
                .is_err()
                .is_equal_to(ContractError::Payment(PaymentError::MissingDenom(
                    TEST_RESERVE_DENOM.to_string(),
                )));

            Ok(())
        }

        #[test]
        fn test_donation() -> Result<(), ContractError> {
            let mut deps = mock_dependencies();
            // this matches `linear_curve` test case from curves.rs
            let curve_type = CurveType::SquareRoot {
                slope: Uint128::new(1),
                scale: 1,
            };
            let init_msg = default_instantiate_msg(2, 8, curve_type);
            mock_init(deps.as_mut(), init_msg)?;

            let donation_amount = 5;
            let _res = exec_donate(deps.as_mut(), donation_amount)?;

            // check that the curve's funding has been increased while supply and reserve have not
            let curve_state = CURVE_STATE.load(&deps.storage)?;
            assert_that!(curve_state.funding).is_equal_to(Uint128::new(donation_amount));

            // check that the donor is in the donations map
            let donation = DONATIONS.load(&deps.storage, &Addr::unchecked(TEST_DONOR))?;
            assert_that!(donation).is_equal_to(Uint128::new(donation_amount));

            // check that the owner can withdraw
            withdraw(
                deps.as_mut(),
                mock_env(),
                mock_info(TEST_CREATOR, &[]),
                None,
            )?;

            // check that a random can't withdraw
            let res = withdraw(deps.as_mut(), mock_env(), mock_info("random", &[]), None);
            assert_that!(res)
                .is_err()
                .is_equal_to(ContractError::Ownership(
                    cw_ownable::OwnershipError::NotOwner,
                ));

            // execute donation to the reserve pool
            donate(
                deps.as_mut(),
                mock_env(),
                mock_info(TEST_DONOR, &[coin(donation_amount, TEST_RESERVE_DENOM)]),
                DonationPool::Reserve {},
            )?;

            // check that the curve's reserve has been increased while supply and reserve have not
            let curve_state = CURVE_STATE.load(&deps.storage)?;
            assert_that!(curve_state.reserve).is_equal_to(Uint128::new(donation_amount));

            // check that the donor is in the donations map
            let donation = DONATIONS.load(&deps.storage, &Addr::unchecked(TEST_DONOR))?;
            assert_that!(donation).is_equal_to(Uint128::new(donation_amount * 2));

            Ok(())
        }

        #[test]
        fn should_send_to_funding_pool_forwarding() -> Result<(), ContractError> {
            let mut deps = mock_dependencies();
            // this matches `linear_curve` test case from curves.rs
            let curve_type = CurveType::SquareRoot {
                slope: Uint128::new(1),
                scale: 1,
            };
            let mut init_msg = default_instantiate_msg(2, 8, curve_type);
            init_msg.funding_pool_forwarding = Some(TEST_CREATOR.to_string());
            mock_init(deps.as_mut(), init_msg)?;

            let donation_amount = 5;
            let _res = exec_donate(deps.as_mut(), donation_amount)?;

            // Check that the funding pool did not increase, because it was sent to the funding pool forwarding
            // NOTE: the balance cannot be checked with mock_dependencies
            let curve_state = CURVE_STATE.load(&deps.storage)?;
            assert_that!(curve_state.funding).is_equal_to(Uint128::zero());

            // check that the donor is in the donations map
            let donation = DONATIONS.load(&deps.storage, &Addr::unchecked(TEST_DONOR))?;
            assert_that!(donation).is_equal_to(Uint128::new(donation_amount));

            Ok(())
        }

        #[test]
        fn test_pause() -> Result<(), ContractError> {
            let mut deps = mock_dependencies();
            // this matches `linear_curve` test case from curves.rs
            let curve_type = CurveType::SquareRoot {
                slope: Uint128::new(1),
                scale: 1,
            };
            let init_msg = default_instantiate_msg(2, 8, curve_type);
            mock_init(deps.as_mut(), init_msg)?;

            // Ensure not paused on instantiate
            assert!(!IS_PAUSED.load(&deps.storage)?);

            // Ensure random cannot pause
            let res = toggle_pause(deps.as_mut(), mock_info("random", &[]));
            assert_that!(res)
                .is_err()
                .is_equal_to(ContractError::Ownership(
                    cw_ownable::OwnershipError::NotOwner,
                ));

            // Ensure paused after toggling
            toggle_pause(deps.as_mut(), mock_info(TEST_CREATOR, &[]))?;
            assert!(IS_PAUSED.load(&deps.storage)?);

            // Ensure random cannot do anything
            let res = crate::contract::execute(
                deps.as_mut(),
                mock_env(),
                mock_info("random", &[]),
                crate::msg::ExecuteMsg::TogglePause {},
            );
            assert_that!(res)
                .is_err()
                .is_equal_to(ContractError::Paused {});

            // Ensure unpaused after toggling
            toggle_pause(deps.as_mut(), mock_info(TEST_CREATOR, &[]))?;
            assert!(!IS_PAUSED.load(&deps.storage)?);

            Ok(())
        }
    }
}
