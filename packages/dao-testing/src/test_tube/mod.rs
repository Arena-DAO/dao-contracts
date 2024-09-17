// Ignore integration tests for code coverage since there will be problems with dynamic linking libosmosistesttube
// and also, tarpaulin will not be able read coverage out of wasm binary anyway
#![cfg(not(tarpaulin))]

// Integration tests using an actual chain binary, requires
// the "test-tube" feature to be enabled
// cargo test --features test-tube

pub mod cw4_group;
pub mod cw721_base;
pub mod cw_abc;
pub mod cw_admin_factory;
pub mod cw_tokenfactory_issuer;
pub mod dao_abc_factory;
pub mod dao_dao_core;
pub mod dao_proposal_single;
pub mod dao_test_custom_factory;
pub mod dao_voting_cw4;
pub mod dao_voting_token_staked;
