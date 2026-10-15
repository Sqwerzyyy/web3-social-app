// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "сontracts/ProfileNFT.sol";

contract ProfileNFTTest is Test {
    ProfileNFT public nft;

    uint256 internal profileSignerKey = 0xA11CE;
    uint256 internal kycSignerKey     = 0xB0B;

    address internal profileSigner;
    address internal kycSigner;
    address internal user  = address(0x1234);
    address internal user2 = address(0x5678);

    bytes32 internal constant VALID_NICK = bytes32("alice");

    function setUp() public {
        profileSigner = vm.addr(profileSignerKey);
        kycSigner     = vm.addr(kycSignerKey);
        nft = new ProfileNFT(profileSigner, kycSigner, "https://example.com/");
    }

    function _signProfile(
        address _user,
        bytes32 _nick,
        bool _soulbound,
        uint256 _deadline,
        uint256 _nonce,
        uint256 _signerKey
    ) internal view returns (bytes memory) {
        bytes32 structHash = keccak256(abi.encode(
            nft.CREATE_PROFILE_TYPEHASH(),
            _user,
            _nick,
            _soulbound,
            _deadline,
            _nonce
        ));
        bytes32 digest = MessageHashUtils.toTypedDataHash(nft.domainSeparator(), structHash);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(_signerKey, digest);
        return abi.encodePacked(r, s, v);
    }

    function _signKYC(
        address _user,
        uint8   _level,
        bytes32 _docHash,
        uint64  _expiresAt,
        uint256 _deadline,
        uint256 _nonce,
        uint256 _signerKey
    ) internal view returns (bytes memory) {
        bytes32 structHash = keccak256(abi.encode(
            nft.SET_KYC_TYPEHASH(),
            _user, _level, _docHash, _expiresAt, _deadline, _nonce
        ));
        bytes32 digest = MessageHashUtils.toTypedDataHash(nft.domainSeparator(), structHash);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(_signerKey, digest);
        return abi.encodePacked(r, s, v);
    }

    function _mintProfile(address _user, bytes32 _nick) internal {
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signProfile(_user, _nick, false, deadline, nft.profileNonces(_user), profileSignerKey);
        vm.prank(_user);
        nft.createProfile(_nick, false, deadline, sig);
    }

    function _grantKYC(address _user, uint8 _level) internal {
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signKYC(_user, _level, bytes32(0), 0, deadline, nft.kycNonces(_user), kycSignerKey);
        nft.setKYC(_user, _level, bytes32(0), 0, deadline, sig);
    }

    // ─── createProfile ──────────────────────────────────────────────────────────

    function test_createProfile_success() public {
        _mintProfile(user, VALID_NICK);
        assertTrue(nft.hasProfile(user));
        assertEq(nft.userTokenId(user), 1);
    }

    function test_createProfile_duplicateReverts() public {
        _mintProfile(user, VALID_NICK);
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signProfile(user, VALID_NICK, false, deadline, nft.profileNonces(user), profileSignerKey);
        vm.prank(user);
        vm.expectRevert(ProfileNFT.ProfileAlreadyExists.selector);
        nft.createProfile(VALID_NICK, false, deadline, sig);
    }

    function test_createProfile_nicknameTakenReverts() public {
        _mintProfile(user, VALID_NICK);
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signProfile(user2, VALID_NICK, false, deadline, nft.profileNonces(user2), profileSignerKey);
        vm.prank(user2);
        vm.expectRevert(ProfileNFT.NicknameTaken.selector);
        nft.createProfile(VALID_NICK, false, deadline, sig);
    }

    function test_createProfile_expiredDeadlineReverts() public {
        uint256 deadline = block.timestamp - 1;
        bytes memory sig = _signProfile(user, VALID_NICK, false, deadline, nft.profileNonces(user), profileSignerKey);
        vm.prank(user);
        vm.expectRevert(ProfileNFT.SignatureExpired.selector);
        nft.createProfile(VALID_NICK, false, deadline, sig);
    }

    function test_createProfile_deadlineTooFarReverts() public {
        uint256 deadline = block.timestamp + 2 hours;
        bytes memory sig = _signProfile(user, VALID_NICK, false, deadline, nft.profileNonces(user), profileSignerKey);
        vm.prank(user);
        vm.expectRevert(ProfileNFT.DeadlineTooFar.selector);
        nft.createProfile(VALID_NICK, false, deadline, sig);
    }

    function test_createProfile_wrongSignerReverts() public {
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signProfile(user, VALID_NICK, false, deadline, nft.profileNonces(user), kycSignerKey);
        vm.prank(user);
        vm.expectRevert(ProfileNFT.InvalidSignature.selector);
        nft.createProfile(VALID_NICK, false, deadline, sig);
    }

    function test_createProfile_blacklistedReverts() public {
        nft.blacklist(user);
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signProfile(user, VALID_NICK, false, deadline, nft.profileNonces(user), profileSignerKey);
        vm.prank(user);
        vm.expectRevert(ProfileNFT.UserBlacklisted.selector);
        nft.createProfile(VALID_NICK, false, deadline, sig);
    }

    // ─── KYC ────────────────────────────────────────────────────────────────────

    function test_setKYC_success() public {
        _mintProfile(user, VALID_NICK);
        _grantKYC(user, nft.KYC_LEVEL_BASIC());
        assertTrue(nft.isKYCVerified(user));
        assertEq(nft.getKYC(user).level, nft.KYC_LEVEL_BASIC());
    }

    function test_setKYC_expiredAtWriteTime() public {
        uint256 deadline   = block.timestamp + 30 minutes;
        uint64  expiresAt  = uint64(block.timestamp - 1);
        bytes memory sig   = _signKYC(user, nft.KYC_LEVEL_BASIC(), bytes32(0), expiresAt, deadline, nft.kycNonces(user), kycSignerKey);
        nft.setKYC(user, nft.KYC_LEVEL_BASIC(), bytes32(0), expiresAt, deadline, sig);
        assertFalse(nft.isKYCVerified(user));
    }

    function test_setKYC_expiresAfterTime() public {
        uint256 deadline  = block.timestamp + 30 minutes;
        uint64  expiresAt = uint64(block.timestamp + 1 days);
        bytes memory sig  = _signKYC(user, nft.KYC_LEVEL_BASIC(), bytes32(0), expiresAt, deadline, nft.kycNonces(user), kycSignerKey);
        nft.setKYC(user, nft.KYC_LEVEL_BASIC(), bytes32(0), expiresAt, deadline, sig);
        assertTrue(nft.isKYCVerified(user));
        vm.warp(block.timestamp + 2 days);
        assertFalse(nft.isKYCVerified(user));
    }

    // ─── revokeKYC invalidates pending signature ─────────────────────────────

    function test_revokeKYC_invalidatesPendingSignature() public {
        _mintProfile(user, VALID_NICK);

        uint256 deadline = block.timestamp + 30 minutes;
        uint256 nonceBefore = nft.kycNonces(user);
        bytes memory pendingSig = _signKYC(user, nft.KYC_LEVEL_BASIC(), bytes32(0), 0, deadline, nonceBefore, kycSignerKey);

        vm.prank(kycSigner);
        nft.revokeKYC(user, "test revoke");

        assertEq(nft.kycNonces(user), nonceBefore + 1);

        vm.expectRevert(ProfileNFT.InvalidSignature.selector);
        nft.setKYC(user, nft.KYC_LEVEL_BASIC(), bytes32(0), 0, deadline, pendingSig);
    }

    function test_revokeKYC_clearsRecord() public {
        _mintProfile(user, VALID_NICK);
        _grantKYC(user, nft.KYC_LEVEL_DOCUMENT());
        assertTrue(nft.isKYCVerified(user));

        vm.prank(kycSigner);
        nft.revokeKYC(user, "manual revoke");

        assertFalse(nft.isKYCVerified(user));
        assertEq(nft.getKYC(user).level, nft.KYC_LEVEL_NONE());
    }

    // ─── blacklist clears KYC ────────────────────────────────────────────────

    function test_blacklist_clearsKYC_withProfile() public {
        _mintProfile(user, VALID_NICK);
        _grantKYC(user, nft.KYC_LEVEL_INVESTOR());
        assertTrue(nft.isKYCVerified(user));

        nft.blacklist(user);

        assertFalse(nft.isKYCVerified(user));
        assertEq(nft.getKYC(user).level, nft.KYC_LEVEL_NONE());
        assertFalse(nft.hasProfile(user));
    }

    function test_blacklist_clearsKYC_withoutProfile() public {
        _grantKYC(user, nft.KYC_LEVEL_BASIC());
        assertTrue(nft.isKYCVerified(user));

        nft.blacklist(user);

        assertFalse(nft.isKYCVerified(user));
        assertEq(nft.getKYC(user).level, nft.KYC_LEVEL_NONE());
    }

    function test_blacklist_preventsNewProfile() public {
        nft.blacklist(user);
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signProfile(user, VALID_NICK, false, deadline, nft.profileNonces(user), profileSignerKey);
        vm.prank(user);
        vm.expectRevert(ProfileNFT.UserBlacklisted.selector);
        nft.createProfile(VALID_NICK, false, deadline, sig);
    }

    // ─── burn clears KYC ─────────────────────────────────────────────────────

    function test_burn_clearsKYC() public {
        _mintProfile(user, VALID_NICK);
        _grantKYC(user, nft.KYC_LEVEL_BASIC());
        assertTrue(nft.isKYCVerified(user));

        uint256 tokenId = nft.userTokenId(user);
        vm.prank(user);
        nft.burn(tokenId);

        assertFalse(nft.isKYCVerified(user));
        assertEq(nft.getKYC(user).level, nft.KYC_LEVEL_NONE());
        assertFalse(nft.hasProfile(user));
    }

    // ─── soulbound ───────────────────────────────────────────────────────────

    function test_soulbound_transferReverts() public {
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signProfile(user, VALID_NICK, true, deadline, nft.profileNonces(user), profileSignerKey);
        vm.prank(user);
        nft.createProfile(VALID_NICK, true, deadline, sig);

        uint256 tokenId = nft.userTokenId(user);
        vm.prank(user);
        vm.expectRevert(ProfileNFT.Soulbound.selector);
        nft.transferFrom(user, user2, tokenId);
    }

    function test_soulbound_userCannotBurn() public {
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signProfile(user, VALID_NICK, true, deadline, nft.profileNonces(user), profileSignerKey);
        vm.prank(user);
        nft.createProfile(VALID_NICK, true, deadline, sig);

        uint256 tokenId = nft.userTokenId(user);
        vm.prank(user);
        vm.expectRevert(ProfileNFT.Soulbound.selector);
        nft.burn(tokenId);
    }

    function test_soulbound_ownerCanBurn() public {
        uint256 deadline = block.timestamp + 30 minutes;
        bytes memory sig = _signProfile(user, VALID_NICK, true, deadline, nft.profileNonces(user), profileSignerKey);
        vm.prank(user);
        nft.createProfile(VALID_NICK, true, deadline, sig);

        uint256 tokenId = nft.userTokenId(user);
        nft.revokeProfile(tokenId, "admin revoke");
        assertFalse(nft.hasProfile(user));
    }

    // ─── nickname fuzz ───────────────────────────────────────────────────────

    function testFuzz_isValidNickname_allZeros() public view {
        assertFalse(_callIsValid(bytes32(0)));
    }

    function testFuzz_isValidNickname_allOxFF() public view {
        assertFalse(_callIsValid(bytes32(type(uint256).max)));
    }

    function testFuzz_isValidNickname_nullAtPosition0() public view {
        bytes32 n;
        assertFalse(_callIsValid(n));
    }

    function testFuzz_isValidNickname_nullAtPosition1() public view {
        bytes memory b = new bytes(32);
        b[0] = 0x61;
        assertFalse(_callIsValid(bytes32(abi.encodePacked(b))));
    }

    function testFuzz_isValidNickname_embeddedNull_position10() public view {
        bytes memory b = new bytes(32);
        for (uint i = 0; i < 20; i++) b[i] = 0x61;
        b[10] = 0x00;
        assertFalse(_callIsValid(bytes32(abi.encodePacked(b))));
    }

    function testFuzz_isValidNickname_exactly3chars() public view {
        bytes32 n = bytes32("abc");
        assertTrue(_callIsValid(n));
    }

    function testFuzz_isValidNickname_exactly20chars() public view {
        bytes32 n = bytes32("abcdefghij0123456789");
        assertTrue(_callIsValid(n));
    }

    function testFuzz_isValidNickname_21chars() public view {
        bytes32 n = bytes32("abcdefghij01234567890");
        assertFalse(_callIsValid(n));
    }

    function testFuzz_isValidNickname_2chars() public view {
        bytes32 n = bytes32("ab");
        assertFalse(_callIsValid(n));
    }

    function testFuzz_isValidNickname_uppercaseInvalid() public view {
        assertFalse(_callIsValid(bytes32("Alice")));
    }

    function testFuzz_isValidNickname_underscoreValid() public view {
        assertTrue(_callIsValid(bytes32("a_b_c")));
    }

    function testFuzz_isValidNickname_fuzz(bytes32 n) public view {
        bool result = _callIsValid(n);
        if (result) {
            uint256 len = 0;
            bool ended  = false;
            for (uint256 i = 0; i < 32; i++) {
                bytes1 c = n[i];
                if (c == 0x00) { ended = true; continue; }
                assertFalse(ended, "embedded null detected");
                bool ok = (c >= 0x30 && c <= 0x39) || (c >= 0x61 && c <= 0x7A) || c == 0x5F;
                assertTrue(ok, "invalid char in validated nickname");
                len++;
            }
            assertTrue(len >= 3 && len <= 20, "length out of bounds");
        }
    }

    // ─── deadline window ─────────────────────────────────────────────────────

    function test_setMaxDeadlineWindow_tooSmallReverts() public {
        vm.expectRevert(ProfileNFT.DeadlineWindowTooSmall.selector);
        nft.setMaxDeadlineWindow(1 minutes);
    }

    function test_setMaxDeadlineWindow_tooLargeReverts() public {
        vm.expectRevert(ProfileNFT.DeadlineWindowTooLarge.selector);
        nft.setMaxDeadlineWindow(8 days);
    }

    function test_setMaxDeadlineWindow_success() public {
        nft.setMaxDeadlineWindow(2 hours);
        assertEq(nft.maxDeadlineWindow(), 2 hours);
    }

    // ─── helpers ─────────────────────────────────────────────────────────────

    function _callIsValid(bytes32 n) internal view returns (bool ok) {
        (bool success, bytes memory data) = address(nft).staticcall(
            abi.encodeWithSignature("_isValidNickname_exposed(bytes32)", n)
        );
        if (!success || data.length == 0) return false;
        ok = abi.decode(data, (bool));
    }
}