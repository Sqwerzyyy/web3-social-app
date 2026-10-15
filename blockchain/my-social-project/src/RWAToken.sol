// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IAccessControl} from "@openzeppelin/contracts/access/IAccessControl.sol";
import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IERC20Metadata} from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import {Initializable} from "@openzeppelin/contracts/proxy/utils/Initializable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts/proxy/utils/UUPSUpgradeable.sol";
import {ComplianceModule} from "./ComplianceModule.sol";

contract RWAToken is Initializable, IERC20, IERC20Metadata, AccessControl, UUPSUpgradeable {
    bytes32 public constant MINTER_ROLE = keccak256("MINTER_ROLE");

    string private _name;
    string private _symbol;
    uint8 private _decimals;
    uint256 private _totalSupply;
    ComplianceModule public complianceModule;

    mapping(address account => uint256 balance) private _balances;
    mapping(address owner => mapping(address spender => uint256 allowance)) private _allowances;

    event ComplianceModuleUpdated(address indexed compliance);

    error ZeroAddress();
    error InvalidComplianceModule();
    error InsufficientBalance(address account, uint256 balance, uint256 needed);
    error InsufficientAllowance(address spender, uint256 allowance, uint256 needed);
    error ComplianceDenied();

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    /// @notice Initializes token metadata, roles, and compliance.
    /// @param tokenName ERC-20 name.
    /// @param tokenSymbol ERC-20 symbol.
    /// @param admin Default admin.
    /// @param minter Initial minter.
    /// @param compliance Compliance module address.
    function initialize(
        string calldata tokenName,
        string calldata tokenSymbol,
        address admin,
        address minter,
        address compliance
    ) external initializer {
        if (admin == address(0) || minter == address(0) || compliance == address(0)) {
            revert ZeroAddress();
        }
        if (compliance.code.length == 0) revert InvalidComplianceModule();

        _name = tokenName;
        _symbol = tokenSymbol;
        _decimals = 18;
        complianceModule = ComplianceModule(compliance);

        _grantRole(DEFAULT_ADMIN_ROLE, admin);
        _grantRole(MINTER_ROLE, minter);

        emit ComplianceModuleUpdated(compliance);
    }

    /// @notice Returns the token name.
    function name() external view returns (string memory) {
        return _name;
    }

    /// @notice Returns the token symbol.
    function symbol() external view returns (string memory) {
        return _symbol;
    }

    /// @notice Returns token decimals.
    function decimals() external view returns (uint8) {
        return _decimals;
    }

    /// @notice Returns total token supply.
    function totalSupply() external view returns (uint256) {
        return _totalSupply;
    }

    /// @notice Returns an account balance.
    /// @param account Wallet address.
    function balanceOf(address account) external view returns (uint256) {
        return _balances[account];
    }

    /// @notice Transfers tokens.
    /// @param to Destination address.
    /// @param amount Token amount.
    function transfer(address to, uint256 amount) external returns (bool) {
        address owner = msg.sender;
        _transfer(owner, to, amount);
        return true;
    }

    /// @notice Returns spender allowance.
    /// @param owner Token owner.
    /// @param spender Spender address.
    function allowance(address owner, address spender) external view returns (uint256) {
        return _allowances[owner][spender];
    }

    /// @notice Approves spender allowance.
    /// @param spender Spender address.
    /// @param amount Token amount.
    function approve(address spender, uint256 amount) external returns (bool) {
        _approve(msg.sender, spender, amount);
        return true;
    }

    /// @notice Transfers tokens using allowance.
    /// @param from Source address.
    /// @param to Destination address.
    /// @param amount Token amount.
    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        address spender = msg.sender;
        _spendAllowance(from, spender, amount);
        _transfer(from, to, amount);
        return true;
    }

    /// @notice Updates the compliance module.
    /// @param compliance Compliance module address.
    function setComplianceModule(address compliance) external onlyRole(DEFAULT_ADMIN_ROLE) {
        if (compliance == address(0)) revert ZeroAddress();
        if (compliance.code.length == 0) revert InvalidComplianceModule();

        complianceModule = ComplianceModule(compliance);

        emit ComplianceModuleUpdated(compliance);
    }

    /// @notice Mints tokens to a verified address.
    /// @param to Recipient address.
    /// @param amount Token amount.
    function mint(address to, uint256 amount) external onlyRole(MINTER_ROLE) {
        if (to == address(0)) revert ZeroAddress();

        _update(address(0), to, amount);
    }

    /// @notice Burns tokens from a verified address.
    /// @param from Source address.
    /// @param amount Token amount.
    function burn(address from, uint256 amount) external onlyRole(MINTER_ROLE) {
        if (from == address(0)) revert ZeroAddress();

        _update(from, address(0), amount);
    }

    function supportsInterface(bytes4 interfaceId) public view override(AccessControl) returns (bool) {
        return interfaceId == type(IAccessControl).interfaceId || super.supportsInterface(interfaceId);
    }

    function _transfer(address from, address to, uint256 amount) internal {
        if (from == address(0) || to == address(0)) revert ZeroAddress();

        _update(from, to, amount);
    }

    function _approve(address owner, address spender, uint256 amount) internal {
        if (owner == address(0) || spender == address(0)) revert ZeroAddress();

        _allowances[owner][spender] = amount;

        emit Approval(owner, spender, amount);
    }

    function _spendAllowance(address owner, address spender, uint256 amount) internal {
        uint256 currentAllowance = _allowances[owner][spender];
        if (currentAllowance != type(uint256).max) {
            if (currentAllowance < amount) revert InsufficientAllowance(spender, currentAllowance, amount);
            unchecked {
                _allowances[owner][spender] = currentAllowance - amount;
            }
        }
    }

    function _update(address from, address to, uint256 amount) internal virtual {
        if (!complianceModule.canTransfer(from, to, amount)) revert ComplianceDenied();

        if (from == address(0)) {
            _totalSupply += amount;
        } else {
            uint256 fromBalance = _balances[from];
            if (fromBalance < amount) revert InsufficientBalance(from, fromBalance, amount);
            unchecked {
                _balances[from] = fromBalance - amount;
            }
        }

        if (to == address(0)) {
            unchecked {
                _totalSupply -= amount;
            }
        } else {
            unchecked {
                _balances[to] += amount;
            }
        }

        emit Transfer(from, to, amount);
    }

    function _authorizeUpgrade(address) internal override onlyRole(DEFAULT_ADMIN_ROLE) {}
}
