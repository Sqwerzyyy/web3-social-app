// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";
import {Initializable} from "@openzeppelin/contracts/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts/proxy/utils/UUPSUpgradeable.sol";

contract IdentityRegistry is Initializable, AccessControl, UUPSUpgradeable {
    enum KYCStatus {
        None,
        Verified,
        Suspended
    }

    struct Identity {
        uint256 userId;
        KYCStatus status;
        bool exists;
    }

    bytes32 public constant KYC_PROVIDER_ROLE = keccak256("KYC_PROVIDER_ROLE");

    mapping(address account => Identity identity) private _identities;

    event IdentityUpdated(address indexed account, uint256 indexed userId, KYCStatus status);
    event IdentityRemoved(address indexed account, uint256 indexed userId);

    error ZeroAddress();
    error InvalidKYCStatus();
    error IdentityNotFound(address account);

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes registry roles.
    /// @param admin Default admin.
    /// @param kycProvider Initial KYC provider.
    function initialize(address admin, address kycProvider) external initializer {
        if (admin == address(0) || kycProvider == address(0)) revert ZeroAddress();

        _grantRole(DEFAULT_ADMIN_ROLE, admin);
        _grantRole(KYC_PROVIDER_ROLE, kycProvider);
    }

    /// @notice Creates or updates an address identity.
    /// @param account Wallet address.
    /// @param userId Internal user identifier.
    /// @param status KYC status.
    function setIdentity(address account, uint256 userId, KYCStatus status) external onlyRole(KYC_PROVIDER_ROLE) {
        if (account == address(0)) revert ZeroAddress();
        if (status == KYCStatus.None) revert InvalidKYCStatus();

        _identities[account] = Identity({userId: userId, status: status, exists: true});

        emit IdentityUpdated(account, userId, status);
    }

    /// @notice Updates an existing address KYC status.
    /// @param account Wallet address.
    /// @param status KYC status.
    function setKYCStatus(address account, KYCStatus status) external onlyRole(KYC_PROVIDER_ROLE) {
        if (account == address(0)) revert ZeroAddress();
        if (status == KYCStatus.None) revert InvalidKYCStatus();

        Identity storage identity = _identities[account];
        if (!identity.exists) revert IdentityNotFound(account);

        identity.status = status;

        emit IdentityUpdated(account, identity.userId, status);
    }

    /// @notice Removes an address identity.
    /// @param account Wallet address.
    function removeIdentity(address account) external onlyRole(KYC_PROVIDER_ROLE) {
        if (account == address(0)) revert ZeroAddress();

        uint256 userId = _identities[account].userId;
        delete _identities[account];

        emit IdentityRemoved(account, userId);
    }

    /// @notice Returns identity data for an address.
    /// @param account Wallet address.
    function identityOf(address account) external view returns (uint256 userId, KYCStatus status, bool exists) {
        Identity storage identity = _identities[account];
        return (identity.userId, identity.status, identity.exists);
    }

    /// @notice Returns an address user identifier.
    /// @param account Wallet address.
    function userIdOf(address account) external view returns (uint256) {
        return _identities[account].userId;
    }

    /// @notice Returns an address KYC status.
    /// @param account Wallet address.
    function kycStatusOf(address account) external view returns (KYCStatus) {
        return _identities[account].status;
    }

    /// @notice Returns true when an address is currently verified.
    /// @param account Wallet address.
    function isVerified(address account) public view returns (bool) {
        Identity storage identity = _identities[account];
        return identity.exists && identity.status == KYCStatus.Verified;
    }

    function _authorizeUpgrade(address) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
