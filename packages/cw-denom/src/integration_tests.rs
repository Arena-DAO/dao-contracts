use cosmwasm_std::{coins, Addr, Uint128};
use cw_multi_test::{App, BankSudo, Executor};

use crate::CheckedDenom;

#[test]
fn test_native_denom_send() {
    let mut app = App::default();
    app.sudo(cw_multi_test::SudoMsg::Bank(BankSudo::Mint {
        to_address: "ekez".to_string(),
        amount: coins(10, "ujuno"),
    }))
    .unwrap();

    let denom = CheckedDenom::Native("ujuno".to_string());

    let start_balance = denom
        .query_balance(&app.wrap(), &Addr::unchecked("ekez"))
        .unwrap();
    let send_message = denom
        .get_transfer_to_message(&Addr::unchecked("dao"), Uint128::new(9))
        .unwrap();
    app.execute(Addr::unchecked("ekez"), send_message).unwrap();
    let end_balance = denom
        .query_balance(&app.wrap(), &Addr::unchecked("ekez"))
        .unwrap();

    assert_eq!(start_balance, Uint128::new(10));
    assert_eq!(end_balance, Uint128::new(1));

    let dao_balance = denom
        .query_balance(&app.wrap(), &Addr::unchecked("dao"))
        .unwrap();
    assert_eq!(dao_balance, Uint128::new(9))
}
