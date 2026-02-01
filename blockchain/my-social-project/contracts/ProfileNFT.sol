// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import "@openzeppelin/contracts/token/ERC721/extensions/ERC721Burnable.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/utils/Pausable.sol";
import "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

contract ProfileNFT is ERC721, ERC721Burnable, Ownable, Pausable {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    uint256 private _nextTokenId = 1;

    mapping(address => bool) public authorizedSigners;
    mapping(address => uint256) public userTokenId;
    mapping(address => uint256) public nonces;
    mapping(string => bool) public takenNicknames;
    mapping(uint256 => string) public nicknames;
    mapping(string => uint256) public nicknameToTokenId;
    mapping(uint256 => uint256) public activationTime;
    mapping(uint256 => bool) public isSoulbound;

    bool public globalSoulboundMode;

    event ProfileCreated(
        address indexed user,
        uint256 indexed tokenId,
        string nickname,
        uint256 timestamp,
        bool soulbound
    );

    event SignerAdded(address indexed signer);
    event SignerRemoved(address indexed signer);
    event NicknameReleased(string indexed nickname, uint256 indexed tokenId);
    event SoulboundModeChanged(bool enabled);

    error ProfileAlreadyExists();
    error NicknameTaken();
    error InvalidSignature();
    error SignatureExpired();
    error InvalidNickname();
    error Soulbound();
    error UnauthorizedSigner();

    constructor(address initialSigner)
    ERC721("Web3 Profile", "W3P")
    Ownable(msg.sender)
    {
        require(initialSigner != address(0), "Invalid signer");
        authorizedSigners[initialSigner] = true;
        emit SignerAdded(initialSigner);
    }

    function addSigner(address signer) external onlyOwner {
        require(signer != address(0), "Invalid signer");
        require(!authorizedSigners[signer], "Already authorized");
        authorizedSigners[signer] = true;
        emit SignerAdded(signer);
    }

    function removeSigner(address signer) external onlyOwner {
        require(authorizedSigners[signer], "Not authorized");
        authorizedSigners[signer] = false;
        emit SignerRemoved(signer);
    }

    function createProfile(
        string calldata nickname,
        bool soulbound,
        uint256 deadline,
        bytes calldata signature
    ) external whenNotPaused {
        if (balanceOf(msg.sender) > 0) revert ProfileAlreadyExists();
        if (takenNicknames[nickname]) revert NicknameTaken();
        if (!_isValidNickname(nickname)) revert InvalidNickname();
        if (block.timestamp > deadline) revert SignatureExpired();

        bytes32 messageHash = keccak256(
            abi.encodePacked(
                msg.sender,
                nickname,
                soulbound,
                deadline,
                block.chainid,
                nonces[msg.sender]
            )
        );
        bytes32 ethSignedMessageHash = messageHash.toEthSignedMessageHash();
        address recoveredSigner = ethSignedMessageHash.recover(signature);

        if (!authorizedSigners[recoveredSigner]) revert InvalidSignature();

        nonces[msg.sender]++;
        takenNicknames[nickname] = true;

        uint256 tokenId = _nextTokenId++;
        nicknames[tokenId] = nickname;
        nicknameToTokenId[nickname] = tokenId;
        activationTime[tokenId] = block.timestamp;
        userTokenId[msg.sender] = tokenId;

        bool _isSoulbound = globalSoulboundMode || soulbound;
        isSoulbound[tokenId] = _isSoulbound;

        _safeMint(msg.sender, tokenId);

        emit ProfileCreated(msg.sender, tokenId, nickname, block.timestamp, _isSoulbound);
    }

    function getProfile(address user) external view returns (
        bool exists,
        uint256 tokenId,
        string memory nickname,
        uint256 activatedAt,
        bool soulbound
    ) {
        tokenId = userTokenId[user];

        if (tokenId == 0 || _ownerOf(tokenId) != user) {
            return (false, 0, "", 0, false);
        }

        nickname = nicknames[tokenId];
        activatedAt = activationTime[tokenId];
        soulbound = isSoulbound[tokenId];
        exists = true;
    }

    function isProfileActivated(address user) external view returns (bool) {
        return balanceOf(user) > 0;
    }

    function getMessageHash(
        address user,
        string calldata nickname,
        bool soulbound,
        uint256 deadline,
        uint256 nonce
    ) external view returns (bytes32) {
        return keccak256(
            abi.encodePacked(user, nickname, soulbound, deadline, block.chainid, nonce)
        );
    }

    function _update(
        address to,
        uint256 tokenId,
        address auth
    ) internal virtual override returns (address) {
        address from = _ownerOf(tokenId);

        if (from != address(0) && to != address(0)) {
            if (isSoulbound[tokenId]) revert Soulbound();
        }

        if (from != address(0)) {
            userTokenId[from] = 0;
        }

        if (to != address(0)) {
            if (balanceOf(to) > 0) revert ProfileAlreadyExists();
            userTokenId[to] = tokenId;
        }

        if (to == address(0)) {
            string memory nickname = nicknames[tokenId];
            delete takenNicknames[nickname];
            delete nicknameToTokenId[nickname];
            delete nicknames[tokenId];
            emit NicknameReleased(nickname, tokenId);
        }

        return super._update(to, tokenId, auth);
    }

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }

    function setGlobalSoulboundMode(bool enabled) external onlyOwner {
        globalSoulboundMode = enabled;
        emit SoulboundModeChanged(enabled);
    }

    function adminBurnProfile(uint256 tokenId) external onlyOwner {
        _burn(tokenId);
    }

    function _isValidNickname(string calldata nickname) internal pure returns (bool) {
        bytes memory b = bytes(nickname);

        if (b.length < 3 || b.length > 20) return false;

        for (uint256 i = 0; i < b.length; i++) {
            bytes1 char = b[i];
            if (!(
                (char >= 0x30 && char <= 0x39) ||
                (char >= 0x41 && char <= 0x5A) ||
                (char >= 0x61 && char <= 0x7A) ||
                (char == 0x5F)
            )) {
                return false;
            }
        }

        return true;
    }
}
