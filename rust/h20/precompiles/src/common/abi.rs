//! ABI definition for the `IH20` interface.

use alloy_sol_types::sol;

sol! {
    #[derive(Debug, PartialEq, Eq)]
    interface IH20 {
        enum PausableFeature {
            /// Transfer operations.
            TRANSFER,
            /// Mint operations.
            MINT,
            /// Burn operations.
            BURN
        }

        // Errors

        /// ETH was attached to a call targeting a nonpayable token selector.
        error NonPayable();

        error AccessControlUnauthorizedAccount(address account, bytes32 neededRole);
        error Unauthorized();
        error ContractPaused(PausableFeature feature);
        error InsufficientAllowance(address spender, uint256 allowance, uint256 needed);
        error InsufficientBalance(address sender, uint256 balance, uint256 needed);
        error InvalidSender(address sender);
        error InvalidReceiver(address receiver);
        error InvalidApprover(address approver);
        error InvalidSpender(address spender);
        error InvalidAmount();
        error EmptyFeatureSet();
        error InvalidSupplyCap(uint256 currentSupply, uint256 proposedCap);
        error SupplyCapExceeded(uint256 cap, uint256 attempted);
        error PolicyForbids(bytes32 policyScope, uint64 policyId);
        error PolicyNotFound(uint64 policyId);
        error UnsupportedPolicyType(bytes32 policyScope);
        error AccountNotBlocked(address account);
        error ExpiredSignature(uint256 deadline);
        error InvalidSigner(address signer, address owner);
        error LastAdminCannotRenounce();
        error NotSoleAdmin();
        error AccessControlBadConfirmation();

        // Events
        event Transfer(address indexed from, address indexed to, uint256 amount);
        event Approval(address indexed owner, address indexed spender, uint256 amount);
        event Memo(address indexed caller, bytes32 indexed memo);
        event BurnedBlocked(address indexed caller, address indexed from, uint256 amount);
        event RoleGranted(bytes32 indexed role, address indexed account, address indexed sender);
        event RoleRevoked(bytes32 indexed role, address indexed account, address indexed sender);
        event RoleAdminChanged(bytes32 indexed role, bytes32 indexed previousAdminRole, bytes32 indexed newAdminRole);
        event LastAdminRenounced(address indexed previousAdmin);
        event Paused(address indexed updater, PausableFeature[] features);
        event Unpaused(address indexed updater, PausableFeature[] features);
        event PolicyUpdated(bytes32 indexed policyScope, uint64 oldPolicyId, uint64 newPolicyId);
        event SupplyCapUpdated(address indexed updater, uint256 oldSupplyCap, uint256 newSupplyCap);
        event ContractURIUpdated();
        event NameUpdated(address indexed updater, string newName);
        event SymbolUpdated(address indexed updater, string newSymbol);
        event EIP712DomainChanged();

        // Role identifiers
        function DEFAULT_ADMIN_ROLE() external view returns (bytes32);
        function MINT_ROLE() external view returns (bytes32);
        function BURN_ROLE() external view returns (bytes32);
        function BURN_BLOCKED_ROLE() external view returns (bytes32);
        function PAUSE_ROLE() external view returns (bytes32);
        function UNPAUSE_ROLE() external view returns (bytes32);
        function METADATA_ROLE() external view returns (bytes32);

        // Policy type identifiers
        function TRANSFER_SENDER_POLICY() external view returns (bytes32);
        function TRANSFER_RECEIVER_POLICY() external view returns (bytes32);
        function TRANSFER_EXECUTOR_POLICY() external view returns (bytes32);
        function MINT_RECEIVER_POLICY() external view returns (bytes32);

        // ERC-20
        function name() external view returns (string);
        function symbol() external view returns (string);
        function decimals() external view returns (uint8);
        function totalSupply() external view returns (uint256);
        function balanceOf(address account) external view returns (uint256);
        function allowance(address owner, address spender) external view returns (uint256);
        function transfer(address to, uint256 amount) external returns (bool);
        function transferFrom(address from, address to, uint256 amount) external returns (bool);
        function approve(address spender, uint256 amount) external returns (bool);

        // Metadata updates
        function updateName(string calldata newName) external;
        function updateSymbol(string calldata newSymbol) external;

        // Memo transfer variants
        function transferWithMemo(address to, uint256 amount, bytes32 memo) external returns (bool);
        function transferFromWithMemo(address from, address to, uint256 amount, bytes32 memo) external returns (bool);

        // Mint / burn
        function mint(address to, uint256 amount) external;
        function mintWithMemo(address to, uint256 amount, bytes32 memo) external;
        function burn(uint256 amount) external;
        function burnWithMemo(uint256 amount, bytes32 memo) external;
        function burnBlocked(address from, uint256 amount) external;

        // Roles
        function hasRole(bytes32 role, address account) external view returns (bool);
        function getRoleAdmin(bytes32 role) external view returns (bytes32);
        function grantRole(bytes32 role, address account) external;
        function revokeRole(bytes32 role, address account) external;
        function renounceRole(bytes32 role, address callerConfirmation) external;
        function renounceLastAdmin() external;
        function setRoleAdmin(bytes32 role, bytes32 newAdminRole) external;

        // Pause
        function pausedFeatures() external view returns (PausableFeature[] memory);
        function isPaused(PausableFeature feature) external view returns (bool);
        function pause(PausableFeature[] calldata features) external;
        function unpause(PausableFeature[] calldata features) external;

        // Policy
        function policyId(bytes32 policyScope) external view returns (uint64);
        function updatePolicy(bytes32 policyScope, uint64 newPolicyId) external;

        // Supply cap
        function supplyCap() external view returns (uint256);
        function updateSupplyCap(uint256 newSupplyCap) external;

        // Permit (EIP-2612 + ERC-5267)
        function DOMAIN_SEPARATOR() external view returns (bytes32);
        function nonces(address owner) external view returns (uint256);
        function permit(address owner, address spender, uint256 value, uint256 deadline, uint8 v, bytes32 r, bytes32 s) external;
        function eip712Domain() external view returns (bytes1 fields, string memory name, string memory version, uint256 chainId, address verifyingContract, bytes32 salt, uint256[] memory extensions);

        // Contract URI (ERC-7572)
        function contractURI() external view returns (string);
        function updateContractURI(string calldata newURI) external;
    }
}

impl IH20::IH20Calls {
    /// Returns the stable label for this decoded B-20 call.
    pub const fn as_label(&self) -> &'static str {
        match self {
            Self::name(_) => "precompile-h20-name",
            Self::symbol(_) => "precompile-h20-symbol",
            Self::decimals(_) => "precompile-h20-decimals",
            Self::totalSupply(_) => "precompile-h20-totalSupply",
            Self::balanceOf(_) => "precompile-h20-balanceOf",
            Self::allowance(_) => "precompile-h20-allowance",
            Self::supplyCap(_) => "precompile-h20-supplyCap",
            Self::nonces(_) => "precompile-h20-nonces",
            Self::contractURI(_) => "precompile-h20-contractURI",
            Self::DEFAULT_ADMIN_ROLE(_) => "precompile-h20-DEFAULT_ADMIN_ROLE",
            Self::MINT_ROLE(_) => "precompile-h20-MINT_ROLE",
            Self::BURN_ROLE(_) => "precompile-h20-BURN_ROLE",
            Self::BURN_BLOCKED_ROLE(_) => "precompile-h20-BURN_BLOCKED_ROLE",
            Self::PAUSE_ROLE(_) => "precompile-h20-PAUSE_ROLE",
            Self::UNPAUSE_ROLE(_) => "precompile-h20-UNPAUSE_ROLE",
            Self::METADATA_ROLE(_) => "precompile-h20-METADATA_ROLE",
            Self::TRANSFER_SENDER_POLICY(_) => "precompile-h20-TRANSFER_SENDER_POLICY",
            Self::TRANSFER_RECEIVER_POLICY(_) => "precompile-h20-TRANSFER_RECEIVER_POLICY",
            Self::TRANSFER_EXECUTOR_POLICY(_) => "precompile-h20-TRANSFER_EXECUTOR_POLICY",
            Self::MINT_RECEIVER_POLICY(_) => "precompile-h20-MINT_RECEIVER_POLICY",
            Self::hasRole(_) => "precompile-h20-hasRole",
            Self::getRoleAdmin(_) => "precompile-h20-getRoleAdmin",
            Self::pausedFeatures(_) => "precompile-h20-pausedFeatures",
            Self::policyId(_) => "precompile-h20-policyId",
            Self::isPaused(_) => "precompile-h20-isPaused",
            Self::DOMAIN_SEPARATOR(_) => "precompile-h20-DOMAIN_SEPARATOR",
            Self::eip712Domain(_) => "precompile-h20-eip712Domain",
            Self::transfer(_) => "precompile-h20-transfer",
            Self::transferFrom(_) => "precompile-h20-transferFrom",
            Self::approve(_) => "precompile-h20-approve",
            Self::transferWithMemo(_) => "precompile-h20-transferWithMemo",
            Self::transferFromWithMemo(_) => "precompile-h20-transferFromWithMemo",
            Self::mint(_) => "precompile-h20-mint",
            Self::mintWithMemo(_) => "precompile-h20-mintWithMemo",
            Self::burn(_) => "precompile-h20-burn",
            Self::burnWithMemo(_) => "precompile-h20-burnWithMemo",
            Self::burnBlocked(_) => "precompile-h20-burnBlocked",
            Self::pause(_) => "precompile-h20-pause",
            Self::unpause(_) => "precompile-h20-unpause",
            Self::updateSupplyCap(_) => "precompile-h20-updateSupplyCap",
            Self::updateName(_) => "precompile-h20-updateName",
            Self::updateSymbol(_) => "precompile-h20-updateSymbol",
            Self::updateContractURI(_) => "precompile-h20-updateContractURI",
            Self::grantRole(_) => "precompile-h20-grantRole",
            Self::revokeRole(_) => "precompile-h20-revokeRole",
            Self::renounceRole(_) => "precompile-h20-renounceRole",
            Self::renounceLastAdmin(_) => "precompile-h20-renounceLastAdmin",
            Self::setRoleAdmin(_) => "precompile-h20-setRoleAdmin",
            Self::updatePolicy(_) => "precompile-h20-updatePolicy",
            Self::permit(_) => "precompile-h20-permit",
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, U256};

    use crate::IH20;

    #[test]
    fn h20_call_labels_are_stable() {
        assert_eq!(
            IH20::IH20Calls::transfer(IH20::transferCall { to: Address::ZERO, amount: U256::ZERO })
                .as_label(),
            "precompile-h20-transfer"
        );
        assert_eq!(
            IH20::IH20Calls::updateSupplyCap(IH20::updateSupplyCapCall {
                newSupplyCap: U256::ZERO,
            })
            .as_label(),
            "precompile-h20-updateSupplyCap"
        );
    }
}
