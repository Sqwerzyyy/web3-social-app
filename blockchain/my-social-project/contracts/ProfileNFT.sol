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
    mapping(address => uint256) public nonces;
    mapping(string => bool) public takenNicknames;
    mapping(uint256 => string) public nicknames;
    mapping(address => uint256) public userTokenId;

    error AgeIsTooSmall();
    error ProfileAlreadyExists();
    error NicknameTaken();
    error InvalidSignature();
    error SignatureExpired();

    constructor(address initialSigner) ERC721("Web3 Profile", "W3P") Ownable(msg.sender) {
        authorizedSigners[initialSigner] = true;
    }

    function createProfile(
        string calldata nickname,
        uint256 age, // <--- Добавили параметр
        bool soulbound,
        uint256 deadline,
        bytes calldata signature
    ) external whenNotPaused {
        if (age < 16) revert AgeIsTooSmall(); // <--- Твоя логика!
        if (balanceOf(msg.sender) > 0) revert ProfileAlreadyExists();
        if (takenNicknames[nickname]) revert NicknameTaken();
        if (block.timestamp > deadline) revert SignatureExpired();

        bytes32 messageHash = keccak256(
            abi.encodePacked(msg.sender, nickname, age, soulbound, deadline, block.chainid, nonces[msg.sender])
        );
        bytes32 ethSignedMessageHash = messageHash.toEthSignedMessageHash();
        address recoveredSigner = ethSignedMessageHash.recover(signature);

        if (!authorizedSigners[recoveredSigner]) revert InvalidSignature();

        nonces[msg.sender]++;
        takenNicknames[nickname] = true;
        uint256 tokenId = _nextTokenId++;
        nicknames[tokenId] = nickname;
        userTokenId[msg.sender] = tokenId;

        _safeMint(msg.sender, tokenId);
    }
}
