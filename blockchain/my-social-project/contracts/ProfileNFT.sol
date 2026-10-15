// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import "@openzeppelin/contracts/token/ERC721/extensions/ERC721Burnable.sol";
import "@openzeppelin/contracts/access/Ownable2Step.sol";
import "@openzeppelin/contracts/utils/Pausable.sol";
import "@openzeppelin/contracts/utils/cryptography/EIP712.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";

/// @title ProfileNFT
/// @notice Soulbound-capable ERC721 profile token with EIP-712 signature minting and on-chain KYC levels.
/// @dev KYC is stored per address, not per tokenId. On transfer, the sender's KYC is deleted automatically.
/// @dev Soulbound tokens cannot be transferred; only owner or profileSigners can burn them.
/// @dev nickname must be right-padded with zero bytes and contain no embedded null bytes.
/// @dev Two independent signer roles exist: profileSigners for minting, kycSigners for KYC operations.
///      Compromise of one role does not affect the other.
/// @dev Operational note: removeProfileSigner does not check whether at least one signer remains active.
///      Removing the last profileSigner makes createProfile impossible until a new one is added.
///      Maintain at least two active profileSigners and two active kycSigners at all times.
contract ProfileNFT is ERC721, ERC721Burnable, Ownable2Step, Pausable, EIP712 {
    using ECDSA for bytes32;

    bytes32 public constant CREATE_PROFILE_TYPEHASH = keccak256(
        "CreateProfile(address user,bytes32 nickname,bool soulbound,uint256 deadline,uint256 nonce)"
    );
    bytes32 public constant SET_KYC_TYPEHASH = keccak256(
        "SetKYC(address user,uint8 level,bytes32 documentHash,uint64 expiresAt,uint256 deadline,uint256 nonce)"
    );

    uint256 public constant MIN_NICKNAME_LENGTH = 3;
    uint256 public constant MAX_NICKNAME_LENGTH = 20;
    uint256 public constant MIN_DEADLINE_WINDOW = 5 minutes;
    uint256 public constant MAX_DEADLINE_WINDOW = 7 days;
    uint8   public constant KYC_LEVEL_NONE      = 0;
    uint8   public constant KYC_LEVEL_BASIC     = 1;
    uint8   public constant KYC_LEVEL_DOCUMENT  = 2;
    uint8   public constant KYC_LEVEL_INVESTOR  = 3;

    uint256 private _nextTokenId = 1;
    uint256 public maxDeadlineWindow = 1 hours;
    bool    public globalSoulboundMode;
    string  private _baseTokenURI;

    /// @notice KYC record for an address.
    /// @dev verified is set false if level == NONE or expiresAt already passed at write time.
    struct KYCData {
        uint8   level;
        bool    verified;
        uint64  verifiedAt;
        uint64  expiresAt;
        bytes32 documentHash;
    }

    /// @notice On-chain metadata stored per tokenId.
    /// @dev nickname is bytes32 right-padded with zeros; no embedded null bytes allowed.
    struct ProfileMetadata {
        uint64  activationTime;
        bool    isSoulbound;
        bytes32 nickname;
    }

    mapping(address => KYCData)         private  _kyc;
    mapping(address => bool)            public   profileSigners;
    mapping(address => bool)            public   kycSigners;
    mapping(address => bool)            public   blacklisted;
    mapping(address => uint256)         public   profileNonces;
    mapping(address => uint256)         public   kycNonces;
    mapping(address => uint256)         public   userTokenId;
    mapping(bytes32 => uint256)         public   nicknameToTokenId;
    mapping(uint256 => ProfileMetadata) public   profileMetadata;

    event ProfileCreated(address indexed user, uint256 indexed tokenId, bytes32 nickname, uint256 timestamp, bool soulbound);
    event ProfileRevoked(uint256 indexed tokenId, address indexed user, string reason);
    event NicknameReleased(bytes32 indexed nickname, uint256 indexed tokenId);
    event ProfileSignerAdded(address indexed signer);
    event ProfileSignerRemoved(address indexed signer);
    event KYCSignerAdded(address indexed signer);
    event KYCSignerRemoved(address indexed signer);
    event Blacklisted(address indexed user);
    event Unblacklisted(address indexed user);
    event SoulboundModeChanged(bool enabled);
    event MaxDeadlineWindowUpdated(uint256 oldWindow, uint256 newWindow);
    event BaseURIUpdated(string oldBaseURI, string newBaseURI);
    event KYCUpdated(address indexed user, uint8 indexed level, bool verified, uint64 expiresAt, bytes32 documentHash, address indexed updatedBy);
    event KYCRevoked(address indexed user, address indexed revokedBy, string reason);

    error ProfileAlreadyExists();
    error NicknameTaken();
    error InvalidSignature();
    error SignatureExpired();
    error DeadlineTooFar();
    error InvalidNickname();
    error Soulbound();
    error UserBlacklisted();
    error ZeroAddress();
    error AlreadyAuthorized();
    error NotAuthorized();
    error InvalidKYCLevel();
    error DeadlineWindowTooSmall();
    error DeadlineWindowTooLarge();

    /// @param initialProfileSigner Initial signer authorized to sign createProfile payloads.
    /// @param initialKYCSigner     Initial signer authorized to sign setKYC payloads.
    /// @param baseTokenURI         Base URI for tokenURI resolution.
    constructor(address initialProfileSigner, address initialKYCSigner, string memory baseTokenURI)
        ERC721("Web3 Profile", "W3P")
        Ownable(msg.sender)
        EIP712("Web3 Profile", "1")
    {
        if (initialProfileSigner == address(0)) revert ZeroAddress();
        if (initialKYCSigner == address(0))     revert ZeroAddress();
        profileSigners[initialProfileSigner] = true;
        kycSigners[initialKYCSigner]         = true;
        _baseTokenURI = baseTokenURI;
        emit ProfileSignerAdded(initialProfileSigner);
        emit KYCSignerAdded(initialKYCSigner);
    }

    /// @notice Authorize a new address to sign createProfile payloads.
    function addProfileSigner(address signer) external onlyOwner {
        if (signer == address(0))   revert ZeroAddress();
        if (profileSigners[signer]) revert AlreadyAuthorized();
        profileSigners[signer] = true;
        emit ProfileSignerAdded(signer);
    }

    /// @notice Revoke createProfile signing authority from an address.
    /// @dev Does not enforce a minimum active signer count. See contract-level dev note.
    function removeProfileSigner(address signer) external onlyOwner {
        if (!profileSigners[signer]) revert NotAuthorized();
        profileSigners[signer] = false;
        emit ProfileSignerRemoved(signer);
    }

    /// @notice Authorize a new address to sign setKYC payloads and call revokeKYC.
    function addKYCSigner(address signer) external onlyOwner {
        if (signer == address(0)) revert ZeroAddress();
        if (kycSigners[signer])   revert AlreadyAuthorized();
        kycSigners[signer] = true;
        emit KYCSignerAdded(signer);
    }

    /// @notice Revoke KYC signing authority from an address.
    /// @dev Does not enforce a minimum active signer count. See contract-level dev note.
    function removeKYCSigner(address signer) external onlyOwner {
        if (!kycSigners[signer]) revert NotAuthorized();
        kycSigners[signer] = false;
        emit KYCSignerRemoved(signer);
    }

    /// @notice Mint a profile NFT for msg.sender using an EIP-712 signature from a profileSigner.
    /// @param nickname  bytes32 right-padded with zeros, no embedded null bytes, 3–20 lowercase/digit/underscore chars.
    /// @param soulbound If true, token is permanently non-transferable (also forced if globalSoulboundMode is on).
    /// @param deadline  Signature expiry timestamp; must be within maxDeadlineWindow from now.
    /// @param signature EIP-712 signature over CreateProfile struct by an authorized profileSigner.
    function createProfile(
        bytes32 nickname,
        bool    soulbound,
        uint256 deadline,
        bytes calldata signature
    ) external whenNotPaused {
        if (blacklisted[msg.sender])                        revert UserBlacklisted();
        if (userTokenId[msg.sender] != 0)                   revert ProfileAlreadyExists();
        if (!_isValidNickname(nickname))                    revert InvalidNickname();
        if (nicknameToTokenId[nickname] != 0)               revert NicknameTaken();
        if (block.timestamp > deadline)                     revert SignatureExpired();
        if (deadline > block.timestamp + maxDeadlineWindow) revert DeadlineTooFar();

        bytes32 structHash = keccak256(abi.encode(
            CREATE_PROFILE_TYPEHASH,
            msg.sender,
            nickname,
            soulbound,
            deadline,
            profileNonces[msg.sender]
        ));

        address recovered = _hashTypedDataV4(structHash).recover(signature);
        if (!profileSigners[recovered]) revert InvalidSignature();

        profileNonces[msg.sender]++;

        uint256 tokenId   = _nextTokenId++;
        bool _isSoulbound = globalSoulboundMode || soulbound;

        nicknameToTokenId[nickname] = tokenId;
        profileMetadata[tokenId] = ProfileMetadata({
            activationTime: uint64(block.timestamp),
            isSoulbound:    _isSoulbound,
            nickname:       nickname
        });

        _safeMint(msg.sender, tokenId);
        emit ProfileCreated(msg.sender, tokenId, nickname, block.timestamp, _isSoulbound);
    }

    /// @notice Set or update KYC data for a user via EIP-712 signature from a kycSigner.
    /// @dev verified is computed at write time: true iff level > NONE and not already expired.
    /// @param expiresAt Unix timestamp after which KYC is considered expired; 0 means no expiry.
    function setKYC(
        address user,
        uint8   level,
        bytes32 documentHash,
        uint64  expiresAt,
        uint256 deadline,
        bytes calldata signature
    ) external whenNotPaused {
        if (user == address(0))                             revert ZeroAddress();
        if (level > KYC_LEVEL_INVESTOR)                     revert InvalidKYCLevel();
        if (block.timestamp > deadline)                     revert SignatureExpired();
        if (deadline > block.timestamp + maxDeadlineWindow) revert DeadlineTooFar();

        bytes32 structHash = keccak256(abi.encode(
            SET_KYC_TYPEHASH,
            user, level, documentHash, expiresAt, deadline, kycNonces[user]
        ));

        address recovered = _hashTypedDataV4(structHash).recover(signature);
        if (!kycSigners[recovered]) revert InvalidSignature();

        kycNonces[user]++;

        bool isVerified = (level > KYC_LEVEL_NONE) &&
            (expiresAt == 0 || block.timestamp < expiresAt);

        _kyc[user] = KYCData({
            level:        level,
            verified:     isVerified,
            verifiedAt:   uint64(block.timestamp),
            expiresAt:    expiresAt,
            documentHash: documentHash
        });

        emit KYCUpdated(user, level, isVerified, expiresAt, documentHash, msg.sender);
    }

    /// @notice Immediately delete KYC record for a user. Callable by owner or any kycSigner.
    function revokeKYC(address user, string calldata reason) external {
        if (msg.sender != owner() && !kycSigners[msg.sender]) revert NotAuthorized();
        delete _kyc[user];
        emit KYCRevoked(user, msg.sender, reason);
    }

    /// @notice Blacklist a user and burn their profile token if one exists.
    function blacklist(address user) external onlyOwner {
        blacklisted[user] = true;
        emit Blacklisted(user);
        uint256 tokenId = userTokenId[user];
        if (tokenId != 0 && _ownerOf(tokenId) == user) {
            _burn(tokenId);
            emit ProfileRevoked(tokenId, user, "blacklisted");
        }
    }

    /// @notice Remove a user from the blacklist. Does not restore a previously burned token.
    function unblacklist(address user) external onlyOwner {
        blacklisted[user] = false;
        emit Unblacklisted(user);
    }

    /// @notice Burn any profile token by tokenId. Callable by owner only.
    function revokeProfile(uint256 tokenId, string calldata reason) external onlyOwner {
        address user = _requireOwned(tokenId);
        _burn(tokenId);
        emit ProfileRevoked(tokenId, user, reason);
    }

    /// @notice Token holder may burn their own token unless it is soulbound.
    /// @dev Soulbound tokens can only be burned by owner or a profileSigner.
    function burn(uint256 tokenId) public override {
        _requireOwned(tokenId);
        if (globalSoulboundMode || profileMetadata[tokenId].isSoulbound) {
            if (msg.sender != owner() && !profileSigners[msg.sender])
                revert Soulbound();
        }
        super.burn(tokenId);
    }

    function pause()   external onlyOwner { _pause(); }
    function unpause() external onlyOwner { _unpause(); }

    /// @notice Toggle soulbound enforcement for all tokens globally.
    function setGlobalSoulboundMode(bool enabled) external onlyOwner {
        globalSoulboundMode = enabled;
        emit SoulboundModeChanged(enabled);
    }

    /// @notice Update the maximum allowed deadline window for signatures.
    /// @dev Bounded between MIN_DEADLINE_WINDOW and MAX_DEADLINE_WINDOW to prevent
    ///      both DoS (window=0) and long-lived signature attacks (window=max).
    function setMaxDeadlineWindow(uint256 window) external onlyOwner {
        if (window < MIN_DEADLINE_WINDOW) revert DeadlineWindowTooSmall();
        if (window > MAX_DEADLINE_WINDOW) revert DeadlineWindowTooLarge();
        emit MaxDeadlineWindowUpdated(maxDeadlineWindow, window);
        maxDeadlineWindow = window;
    }

    function setBaseURI(string calldata newBaseURI) external onlyOwner {
        emit BaseURIUpdated(_baseTokenURI, newBaseURI);
        _baseTokenURI = newBaseURI;
    }

    /// @notice Returns raw KYC record. Check isKYCVerified for live validity.
    function getKYC(address user) external view returns (KYCData memory) {
        return _kyc[user];
    }

    /// @notice Returns true iff user has a non-expired, verified KYC record with level > NONE.
    function isKYCVerified(address user) public view returns (bool) {
        KYCData storage k = _kyc[user];
        if (k.level == KYC_LEVEL_NONE) return false;
        if (!k.verified) return false;
        if (k.expiresAt != 0 && block.timestamp >= k.expiresAt) return false;
        return true;
    }

    /// @notice Returns true iff user's KYC level is >= requiredLevel and currently valid.
    function meetsKYCLevel(address user, uint8 requiredLevel) public view returns (bool) {
        if (!isKYCVerified(user)) return false;
        return _kyc[user].level >= requiredLevel;
    }

    function isVerifiedInvestor(address user) external view returns (bool) {
        return meetsKYCLevel(user, KYC_LEVEL_INVESTOR);
    }

    /// @notice Returns full profile view for a user. exists=false if no active token found.
    function getProfile(address user) external view returns (
        bool    exists,
        uint256 tokenId,
        bytes32 nickname,
        uint256 activatedAt,
        bool    soulbound,
        bool    kycVerified,
        uint8   kycLevel
    ) {
        tokenId = userTokenId[user];
        if (tokenId == 0 || _ownerOf(tokenId) != user)
            return (false, 0, bytes32(0), 0, false, false, 0);

        ProfileMetadata memory meta = profileMetadata[tokenId];
        return (
            true,
            tokenId,
            meta.nickname,
            meta.activationTime,
            meta.isSoulbound,
            isKYCVerified(user),
            _kyc[user].level
        );
    }

    function hasProfile(address user) external view returns (bool) {
        uint256 tokenId = userTokenId[user];
        return tokenId != 0 && _ownerOf(tokenId) == user;
    }

    function isNicknameTaken(bytes32 nickname) external view returns (bool) {
        return nicknameToTokenId[nickname] != 0;
    }

    function domainSeparator() external view returns (bytes32) {
        return _domainSeparatorV4();
    }

    /// @notice Off-chain helper returns the raw struct hash for createProfile (without domain separator)
    /// @dev 
    function buildStructHash(
        address user,
        bytes32 nickname,
        bool    soulbound,
        uint256 deadline,
        uint256 nonce
    ) external pure returns (bytes32) {
        return keccak256(abi.encode(
            CREATE_PROFILE_TYPEHASH,
            user,
            nickname,
            soulbound,
            deadline,
            nonce
        ));
    }

    /// @notice fff-chain helper: returns the raw struct hash for setKYC (without domain separator)
    /// @dev always wrap with _hashTypedDataV4 before signing, never sign this value directly
    function buildKYCStructHash(
        address user,
        uint8   level,
        bytes32 documentHash,
        uint64  expiresAt,
        uint256 deadline,
        uint256 nonce
    ) external pure returns (bytes32) {
        return keccak256(abi.encode(
            SET_KYC_TYPEHASH,
            user, level, documentHash, expiresAt, deadline, nonce
        ));
    }

    /// @dev Core transfer/mint/burn hook.
    ///      Invariant: exactly one tokenId per address at all times (enforced via userTokenId).
    ///      On transfer (from != 0 && to != 0): KYC of sender is deleted to prevent KYC inheritance.
    ///      On burn (to == 0): nickname mapping and profileMetadata are cleaned up atomically.
    function _update(address to, uint256 tokenId, address auth) internal virtual override returns (address) {
        address from = _ownerOf(tokenId);

        if (blacklisted[from] || blacklisted[to]) revert UserBlacklisted();

        if (from != address(0) && to != address(0)) {
            if (globalSoulboundMode || profileMetadata[tokenId].isSoulbound) revert Soulbound();
            delete _kyc[from];
            emit KYCRevoked(from, address(this), "token transferred");
        }

        if (from != address(0)) userTokenId[from] = 0;

        if (to != address(0)) {
            if (userTokenId[to] != 0) revert ProfileAlreadyExists();
            userTokenId[to] = tokenId;
        }

        if (to == address(0)) {
            bytes32 nickname = profileMetadata[tokenId].nickname;
            delete nicknameToTokenId[nickname];
            delete profileMetadata[tokenId];
            emit NicknameReleased(nickname, tokenId);
        }

        return super._update(to, tokenId, auth);
    }

    function _baseURI() internal view override returns (string memory) {
        return _baseTokenURI;
    }

    /// @dev Validates a bytes32 nickname:
    ///      - Must be right-padded with zeros (no embedded null bytes).
    ///      - Length (non-null prefix) must be between MIN and MAX_NICKNAME_LENGTH.
    ///      - Allowed chars: lowercase a-z, digits 0-9, underscore.
    function _isValidNickname(bytes32 nickname) internal pure returns (bool) {
        uint256 len    = 0;
        bool    sawEnd = false;

        for (uint256 i = 0; i < 32; i++) {
            bytes1 c = nickname[i];

            if (c == 0x00) {
                sawEnd = true;
                continue;
            }

            if (sawEnd) return false;

            bool isDigit      = (c >= 0x30 && c <= 0x39);
            bool isLowercase  = (c >= 0x61 && c <= 0x7A);
            bool isUnderscore = (c == 0x5F);
            if (!isDigit && !isLowercase && !isUnderscore) return false;

            len++;
        }

        return len >= MIN_NICKNAME_LENGTH && len <= MAX_NICKNAME_LENGTH;
    }

    function supportsInterface(bytes4 interfaceId) public view override(ERC721) returns (bool) {
        return super.supportsInterface(interfaceId);
    }
}