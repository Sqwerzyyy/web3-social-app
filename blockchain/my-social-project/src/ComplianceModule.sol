// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";
import {Initializable} from "@openzeppelin/contracts/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts/proxy/utils/UUPSUpgradeable.sol";
import {IdentityRegistry} from "./IdentityRegistry.sol";

contract ComplianceModule is Initializable, AccessControl, UUPSUpgradeable {
    bytes32 public constant FREEZER_ROLE = keccak256("FREEZER_ROLE");

    IdentityRegistry public identityRegistry;

    mapping(address account => bool frozen) private _frozen;

    event IdentityRegistryUpdated(address indexed registry);
    event Frozen(address indexed account);
    event Unfrozen(address indexed account);

    error ZeroAddress();
    error InvalidIdentityRegistry();
    error AddressFrozen(address account);
    error AddressNotVerified(address account);

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes compliance roles and registry.
    /// @param admin Default admin.
    /// @param freezer Initial freezer.
    /// @param registry Identity registry address.
    function initialize(address admin, address freezer, address registry) external initializer {
        if (admin == address(0) || freezer == address(0) || registry == address(0)) revert ZeroAddress();
        if (registry.code.length == 0) revert InvalidIdentityRegistry();

        identityRegistry = IdentityRegistry(registry);

        _grantRole(DEFAULT_ADMIN_ROLE, admin);
        _grantRole(FREEZER_ROLE, freezer);

        emit IdentityRegistryUpdated(registry);
    }

    /// @notice Updates the identity registry.
    /// @param registry Identity registry address.
    function setIdentityRegistry(address registry) external onlyRole(DEFAULT_ADMIN_ROLE) {
        if (registry == address(0)) revert ZeroAddress();
        if (registry.code.length == 0) revert InvalidIdentityRegistry();

        identityRegistry = IdentityRegistry(registry);

        emit IdentityRegistryUpdated(registry);
    }

    /// @notice Freezes an address.
    /// @param account Wallet address.
    function freeze(address account) external onlyRole(FREEZER_ROLE) {
        if (account == address(0)) revert ZeroAddress();

        _frozen[account] = true;

        emit Frozen(account);
    }

    /// @notice Unfreezes an address.
    /// @param account Wallet address.
    function unfreeze(address account) external onlyRole(FREEZER_ROLE) {
        if (account == address(0)) revert ZeroAddress();

        _frozen[account] = false;

        emit Unfrozen(account);
    }

    /// @notice Returns true when an address is frozen.
    /// @param account Wallet address.
    function isFrozen(address account) external view returns (bool) {
        return _frozen[account];
    }

    /// @notice Validates transfer, mint, or burn compliance.
    /// @param from Source address.
    /// @param to Destination address.
    function canTransfer(address from, address to, uint256) external view returns (bool) {
        _checkAccount(from);
        _checkAccount(to);
        return true;
    }

    function _checkAccount(address account) private view {
        if (account == address(0)) return;
        if (_frozen[account]) revert AddressFrozen(account);
        if (!identityRegistry.isVerified(account)) revert AddressNotVerified(account);
    }

    function _authorizeUpgrade(address) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
