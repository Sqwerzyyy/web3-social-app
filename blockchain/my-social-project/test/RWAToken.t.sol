// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {IAccessControl} from "@openzeppelin/contracts/access/IAccessControl.sol";
import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";
import {IdentityRegistry} from "../src/IdentityRegistry.sol";
import {ComplianceModule} from "../src/ComplianceModule.sol";
import {RWAToken} from "../src/RWAToken.sol";

contract RWATokenV2 is RWAToken {
    function version() external pure returns (uint256) {
        return 2;
    }
}

contract RWATokenTest is Test {
    address private admin = address(0xA11CE);
    address private kycProvider = address(0xB0B);
    address private freezer = address(0xF233);
    address private minter = address(0x1117E2);
    address private alice = address(0xA71CE);
    address private bob = address(0xB0B01);
    address private spender = address(0x5EED);

    IdentityRegistry private registry;
    ComplianceModule private compliance;
    RWAToken private token;

    function setUp() public {
        IdentityRegistry registryImpl = new IdentityRegistry();
        registry = IdentityRegistry(
            address(
                new ERC1967Proxy(
                    address(registryImpl), abi.encodeCall(IdentityRegistry.initialize, (admin, kycProvider))
                )
            )
        );

        ComplianceModule complianceImpl = new ComplianceModule();
        compliance = ComplianceModule(
            address(
                new ERC1967Proxy(
                    address(complianceImpl),
                    abi.encodeCall(ComplianceModule.initialize, (admin, freezer, address(registry)))
                )
            )
        );

        RWAToken tokenImpl = new RWAToken();
        token = RWAToken(
            address(
                new ERC1967Proxy(
                    address(tokenImpl),
                    abi.encodeCall(RWAToken.initialize, ("RWA Hub Token", "RWA", admin, minter, address(compliance)))
                )
            )
        );
    }

    function testKycProviderCanManageIdentity() public {
        vm.prank(kycProvider);
        registry.setIdentity(alice, 1001, IdentityRegistry.KYCStatus.Verified);

        (uint256 userId, IdentityRegistry.KYCStatus status, bool exists) = registry.identityOf(alice);

        assertEq(userId, 1001);
        assertEq(uint256(status), uint256(IdentityRegistry.KYCStatus.Verified));
        assertTrue(exists);
        assertTrue(registry.isVerified(alice));

        vm.prank(kycProvider);
        registry.setKYCStatus(alice, IdentityRegistry.KYCStatus.Suspended);

        assertFalse(registry.isVerified(alice));

        vm.prank(kycProvider);
        registry.removeIdentity(alice);

        assertFalse(registry.isVerified(alice));
    }

    function testOnlyKycProviderCanUpdateIdentity() public {
        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, alice, registry.KYC_PROVIDER_ROLE()
            )
        );
        vm.prank(alice);
        registry.setIdentity(alice, 1001, IdentityRegistry.KYCStatus.Verified);
    }

    function testOnlyMinterCanMintAndBurnWithCompliance() public {
        _verify(alice, 1001);

        vm.expectRevert(
            abi.encodeWithSelector(IAccessControl.AccessControlUnauthorizedAccount.selector, alice, token.MINTER_ROLE())
        );
        vm.prank(alice);
        token.mint(alice, 100 ether);

        vm.prank(minter);
        token.mint(alice, 100 ether);

        assertEq(token.totalSupply(), 100 ether);
        assertEq(token.balanceOf(alice), 100 ether);

        vm.prank(minter);
        token.burn(alice, 40 ether);

        assertEq(token.totalSupply(), 60 ether);
        assertEq(token.balanceOf(alice), 60 ether);
    }

    function testMintRequiresRecipientVerification() public {
        vm.expectRevert(abi.encodeWithSelector(ComplianceModule.AddressNotVerified.selector, alice));
        vm.prank(minter);
        token.mint(alice, 100 ether);

        _verify(alice, 1001);

        vm.prank(minter);
        token.mint(alice, 100 ether);

        assertEq(token.balanceOf(alice), 100 ether);
    }

    function testTransferRequiresBothPartiesVerified() public {
        _verify(alice, 1001);

        vm.prank(minter);
        token.mint(alice, 100 ether);

        vm.expectRevert(abi.encodeWithSelector(ComplianceModule.AddressNotVerified.selector, bob));
        vm.prank(alice);
        token.transfer(bob, 10 ether);

        _verify(bob, 1002);

        vm.prank(alice);
        token.transfer(bob, 10 ether);

        assertEq(token.balanceOf(alice), 90 ether);
        assertEq(token.balanceOf(bob), 10 ether);
    }

    function testFrozenAddressBlocksMintTransferAndBurn() public {
        _verify(alice, 1001);
        _verify(bob, 1002);

        vm.prank(minter);
        token.mint(alice, 100 ether);

        vm.prank(freezer);
        compliance.freeze(bob);

        vm.expectRevert(abi.encodeWithSelector(ComplianceModule.AddressFrozen.selector, bob));
        vm.prank(alice);
        token.transfer(bob, 1 ether);

        vm.prank(freezer);
        compliance.freeze(alice);

        vm.expectRevert(abi.encodeWithSelector(ComplianceModule.AddressFrozen.selector, alice));
        vm.prank(minter);
        token.burn(alice, 1 ether);

        vm.prank(freezer);
        compliance.unfreeze(alice);

        vm.prank(minter);
        token.burn(alice, 1 ether);

        assertEq(token.balanceOf(alice), 99 ether);
    }

    function testTransferFromUsesAllowanceAndCompliance() public {
        _verify(alice, 1001);
        _verify(bob, 1002);

        vm.prank(minter);
        token.mint(alice, 100 ether);

        vm.prank(alice);
        token.approve(spender, 25 ether);

        vm.prank(spender);
        token.transferFrom(alice, bob, 20 ether);

        assertEq(token.allowance(alice, spender), 5 ether);
        assertEq(token.balanceOf(alice), 80 ether);
        assertEq(token.balanceOf(bob), 20 ether);

        vm.prank(freezer);
        compliance.freeze(bob);

        vm.expectRevert(abi.encodeWithSelector(ComplianceModule.AddressFrozen.selector, bob));
        vm.prank(spender);
        token.transferFrom(alice, bob, 1 ether);

        assertEq(token.allowance(alice, spender), 5 ether);
    }

    function testAdminCanUpgradeToken() public {
        RWATokenV2 tokenV2 = new RWATokenV2();

        vm.prank(admin);
        token.upgradeToAndCall(address(tokenV2), "");

        assertEq(RWATokenV2(address(token)).version(), 2);
    }

    function testOnlyAdminCanUpgradeToken() public {
        RWATokenV2 tokenV2 = new RWATokenV2();

        vm.expectRevert(
            abi.encodeWithSelector(
                IAccessControl.AccessControlUnauthorizedAccount.selector, alice, token.DEFAULT_ADMIN_ROLE()
            )
        );
        vm.prank(alice);
        token.upgradeToAndCall(address(tokenV2), "");
    }

    function _verify(address account, uint256 userId) private {
        vm.prank(kycProvider);
        registry.setIdentity(account, userId, IdentityRegistry.KYCStatus.Verified);
    }
}
