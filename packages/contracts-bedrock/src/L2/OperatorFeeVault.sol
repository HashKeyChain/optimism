// SPDX-License-Identifier: MIT
pragma solidity 0.8.15;

// Contracts
import { FeeVault } from "src/universal/FeeVault.sol";

// Libraries
import { Predeploys } from "src/libraries/Predeploys.sol";

// Interfaces
import { ISemver } from "src/universal/interfaces/ISemver.sol";

/// @custom:proxied true
/// @custom:predeploy 0x420000000000000000000000000000000000001B
/// @title OperatorFeeVault
/// @notice The OperatorFeeVault accumulates the operator portion of the transaction fees.
contract OperatorFeeVault is FeeVault, ISemver {
    /// @notice Semantic version.
    /// @custom:semver 1.0.0
    string public constant version = "1.0.0";

    /// @notice Constructs the OperatorFeeVault contract.
    /// Funds are withdrawn to the base fee vault on the L2 network.
    constructor() FeeVault(Predeploys.BASE_FEE_VAULT, 0, FeeVault.WithdrawalNetwork.L2) { }
}
