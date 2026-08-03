// SPDX-License-Identifier: MIT
pragma solidity 0.8.15;

import { CommonTest } from "test/setup/CommonTest.sol";
import { OperatorFeeVault } from "src/L2/OperatorFeeVault.sol";
import { FeeVault } from "src/universal/FeeVault.sol";
import { Predeploys } from "src/libraries/Predeploys.sol";

contract OperatorFeeVault_Constructor_Test is CommonTest {
    OperatorFeeVault vault;

    function setUp() public override {
        super.setUp();
        vault = new OperatorFeeVault();
    }

    function test_constructor_operatorFeeVault_succeeds() external view {
        assertEq(vault.version(), "1.0.0");
        assertEq(vault.RECIPIENT(), Predeploys.BASE_FEE_VAULT);
        assertEq(vault.MIN_WITHDRAWAL_AMOUNT(), 0);
        assertEq(uint8(vault.WITHDRAWAL_NETWORK()), uint8(FeeVault.WithdrawalNetwork.L2));
        assertEq(vault.recipient(), Predeploys.BASE_FEE_VAULT);
        assertEq(vault.minWithdrawalAmount(), 0);
        assertEq(uint8(vault.withdrawalNetwork()), uint8(FeeVault.WithdrawalNetwork.L2));
    }
}
