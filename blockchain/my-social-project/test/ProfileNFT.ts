import { expect } from "chai";

import { loadFixture } from "@nomicfoundation/hardhat-toolbox-viem";
import { ProfileNFT } from "../typechain-types";

describe("ProfileNFT", () => {
  let nft: ProfileNFT;
  let owner: SignerWithAddress;
  let signer: SignerWithAddress;
  let user1: SignerWithAddress;
  let user2: SignerWithAddress;

  const BASE_URI = "https://api.example.com/v1/";

  beforeEach(async () => {
    [owner, signer, user1, user2] = await ethers.getSigners();
    const Factory = await ethers.getContractFactory("ProfileNFT");
    nft = await Factory.deploy(signer.address, BASE_URI);
    await nft.waitForDeployment();
  });

  // --- Helpers ---

  function normalize(nickname: string): string {
    return nickname.toLowerCase();
  }

  async function buildSignature(
      user: string,
      nickname: string,
      soulbound: boolean,
      deadline: number,
      nonce: bigint
  ): Promise<string> {
    const normalized = normalize(nickname);
    const domain = {
      name: "Web3 Profile",
      version: "1",
      chainId: (await ethers.provider.getNetwork()).chainId,
      verifyingContract: await nft.getAddress(),
    };
    const types = {
      CreateProfile: [
        { name: "user",      type: "address" },
        { name: "nickname",  type: "string"  },
        { name: "soulbound", type: "bool"    },
        { name: "deadline",  type: "uint256" },
        { name: "nonce",     type: "uint256" },
      ],
    };
    const value = { user, nickname: normalized, soulbound, deadline, nonce };
    return signer.signTypedData(domain, types, value);
  }

  async function mintProfile(
      user: SignerWithAddress,
      nickname: string,
      soulbound = false
  ) {
    const deadline = Math.floor(Date.now() / 1000) + 1800;
    const nonce = await nft.nonces(user.address);
    const sig = await buildSignature(user.address, nickname, soulbound, deadline, nonce);
    return nft.connect(user).createProfile(nickname, soulbound, deadline, sig);
  }

  // --- Deploy ---

  describe("Deploy", () => {
    it("sets owner correctly", async () => {
      expect(await nft.owner()).to.equal(owner.address);
    });

    it("sets initial signer", async () => {
      expect(await nft.authorizedSigners(signer.address)).to.be.true;
    });

    it("reverts if initialSigner is zero address", async () => {
      const Factory = await ethers.getContractFactory("ProfileNFT");
      await expect(
          Factory.deploy(ethers.ZeroAddress, BASE_URI)
      ).to.be.revertedWithCustomError(nft, "ZeroAddress");
    });
  });

  // --- Minting ---

  describe("createProfile", () => {
    it("mints profile successfully", async () => {
      await mintProfile(user1, "alice");

      const profile = await nft.getProfile(user1.address);
      expect(profile.exists).to.be.true;
      expect(profile.tokenId).to.equal(1n);
      expect(profile.nickname).to.equal("alice");
    });

    it("normalizes nickname to lowercase", async () => {
      await mintProfile(user1, "Alice");

      const profile = await nft.getProfile(user1.address);
      expect(profile.nickname).to.equal("alice");
    });

    it("emits ProfileCreated event", async () => {
      const deadline = Math.floor(Date.now() / 1000) + 1800;
      const nonce = await nft.nonces(user1.address);
      const sig = await buildSignature(user1.address, "alice", false, deadline, nonce);

      await expect(nft.connect(user1).createProfile("alice", false, deadline, sig))
          .to.emit(nft, "ProfileCreated")
          .withArgs(user1.address, 1n, "alice", await ethers.provider.getBlock("latest").then(b => b!.timestamp + 1), false);
    });

    it("increments nonce after mint", async () => {
      expect(await nft.nonces(user1.address)).to.equal(0n);
      await mintProfile(user1, "alice");
      expect(await nft.nonces(user1.address)).to.equal(1n);
    });

    it("reverts if profile already exists", async () => {
      await mintProfile(user1, "alice");
      await expect(mintProfile(user1, "bob"))
          .to.be.revertedWithCustomError(nft, "ProfileAlreadyExists");
    });

    it("reverts if nickname taken (case-insensitive)", async () => {
      await mintProfile(user1, "alice");
      await expect(mintProfile(user2, "ALICE"))
          .to.be.revertedWithCustomError(nft, "NicknameTaken");
    });

    it("reverts if nickname too short", async () => {
      await expect(mintProfile(user1, "ab"))
          .to.be.revertedWithCustomError(nft, "InvalidNickname");
    });

    it("reverts if nickname too long", async () => {
      await expect(mintProfile(user1, "aaaaabbbbbcccccddddde"))
          .to.be.revertedWithCustomError(nft, "InvalidNickname");
    });

    it("reverts if signature expired", async () => {
      const deadline = Math.floor(Date.now() / 1000) - 10;
      const nonce = await nft.nonces(user1.address);
      const sig = await buildSignature(user1.address, "alice", false, deadline, nonce);
      await expect(
          nft.connect(user1).createProfile("alice", false, deadline, sig)
      ).to.be.revertedWithCustomError(nft, "SignatureExpired");
    });

    it("reverts if deadline too far", async () => {
      const deadline = Math.floor(Date.now() / 1000) + 7200;
      const nonce = await nft.nonces(user1.address);
      const sig = await buildSignature(user1.address, "alice", false, deadline, nonce);
      await expect(
          nft.connect(user1).createProfile("alice", false, deadline, sig)
      ).to.be.revertedWithCustomError(nft, "DeadlineTooFar");
    });

    it("reverts if signature from unauthorized signer", async () => {
      const deadline = Math.floor(Date.now() / 1000) + 1800;
      const nonce = await nft.nonces(user1.address);

      const domain = {
        name: "Web3 Profile",
        version: "1",
        chainId: (await ethers.provider.getNetwork()).chainId,
        verifyingContract: await nft.getAddress(),
      };
      const types = {
        CreateProfile: [
          { name: "user",      type: "address" },
          { name: "nickname",  type: "string"  },
          { name: "soulbound", type: "bool"    },
          { name: "deadline",  type: "uint256" },
          { name: "nonce",     type: "uint256" },
        ],
      };
      // подписываем рандомным кошельком (не signer)
      const badSig = await user2.signTypedData(domain, types, {
        user: user1.address, nickname: "alice", soulbound: false, deadline, nonce,
      });

      await expect(
          nft.connect(user1).createProfile("alice", false, deadline, badSig)
      ).to.be.revertedWithCustomError(nft, "InvalidSignature");
    });

    it("reverts replay attack (reused signature)", async () => {
      const deadline = Math.floor(Date.now() / 1000) + 1800;
      const nonce = await nft.nonces(user1.address);
      const sig = await buildSignature(user1.address, "alice", false, deadline, nonce);

      await nft.connect(user1).createProfile("alice", false, deadline, sig);

      const tokenId = await nft.userTokenId(user1.address);
      await nft.connect(user1).burn(tokenId);

      await expect(
          nft.connect(user1).createProfile("alice", false, deadline, sig)
      ).to.be.revertedWithCustomError(nft, "InvalidSignature");
    });

    it("reverts if paused", async () => {
      await nft.pause();
      await expect(mintProfile(user1, "alice"))
          .to.be.revertedWithCustomError(nft, "EnforcedPause");
    });

    it("reverts if user is blacklisted", async () => {
      await nft.blacklist(user1.address);
      await expect(mintProfile(user1, "alice"))
          .to.be.revertedWithCustomError(nft, "UserBlacklisted");
    });
  });

  // --- Soulbound ---

  describe("Soulbound", () => {
    it("blocks transfer if soulbound", async () => {
      await mintProfile(user1, "alice", true);
      const tokenId = await nft.userTokenId(user1.address);

      await expect(
          nft.connect(user1).transferFrom(user1.address, user2.address, tokenId)
      ).to.be.revertedWithCustomError(nft, "Soulbound");
    });

    it("allows transfer if not soulbound", async () => {
      await mintProfile(user1, "alice", false);
      const tokenId = await nft.userTokenId(user1.address);

      await nft.connect(user1).transferFrom(user1.address, user2.address, tokenId);

      expect(await nft.ownerOf(tokenId)).to.equal(user2.address);
      expect(await nft.userTokenId(user2.address)).to.equal(tokenId);
      expect(await nft.userTokenId(user1.address)).to.equal(0n);
    });

    it("globalSoulboundMode blocks transfer even for non-soulbound tokens", async () => {
      await mintProfile(user1, "alice", false);
      const tokenId = await nft.userTokenId(user1.address);

      await nft.setGlobalSoulboundMode(true);

      await expect(
          nft.connect(user1).transferFrom(user1.address, user2.address, tokenId)
      ).to.be.revertedWithCustomError(nft, "Soulbound");
    });

    it("receiver cannot receive if already has profile", async () => {
      await mintProfile(user1, "alice", false);
      await mintProfile(user2, "bob", false);

      const tokenId = await nft.userTokenId(user1.address);
      await expect(
          nft.connect(user1).transferFrom(user1.address, user2.address, tokenId)
      ).to.be.revertedWithCustomError(nft, "ProfileAlreadyExists");
    });
  });

  // --- Burn ---

  describe("Burn", () => {
    it("releases nickname on burn", async () => {
      await mintProfile(user1, "alice");
      const tokenId = await nft.userTokenId(user1.address);

      await nft.connect(user1).burn(tokenId);

      expect(await nft.isNicknameTaken("alice")).to.be.false;
      expect(await nft.userTokenId(user1.address)).to.equal(0n);
    });

    it("released nickname can be reused", async () => {
      await mintProfile(user1, "alice");
      const tokenId = await nft.userTokenId(user1.address);
      await nft.connect(user1).burn(tokenId);

      await mintProfile(user2, "alice");
      const profile = await nft.getProfile(user2.address);
      expect(profile.exists).to.be.true;
      expect(profile.nickname).to.equal("alice");
    });

    it("emits NicknameReleased on burn", async () => {
      await mintProfile(user1, "alice");
      const tokenId = await nft.userTokenId(user1.address);

      await expect(nft.connect(user1).burn(tokenId))
          .to.emit(nft, "NicknameReleased")
          .withArgs("alice", tokenId);
    });
  });

  // --- Admin ---

  describe("Admin", () => {
    it("blacklist revokes existing profile", async () => {
      await mintProfile(user1, "alice");
      const tokenId = await nft.userTokenId(user1.address);

      await nft.blacklist(user1.address);

      await expect(nft.ownerOf(tokenId)).to.be.reverted;
      expect(await nft.userTokenId(user1.address)).to.equal(0n);
    });

    it("blacklisted user cannot mint", async () => {
      await nft.blacklist(user1.address);
      await expect(mintProfile(user1, "alice"))
          .to.be.revertedWithCustomError(nft, "UserBlacklisted");
    });

    it("unblacklist allows mint again", async () => {
      await nft.blacklist(user1.address);
      await nft.unblacklist(user1.address);
      await expect(mintProfile(user1, "alice")).to.not.be.reverted;
    });

    it("revokeProfile burns token with reason", async () => {
      await mintProfile(user1, "alice");
      const tokenId = await nft.userTokenId(user1.address);

      await expect(nft.revokeProfile(tokenId, "tos_violation"))
          .to.emit(nft, "ProfileRevoked")
          .withArgs(tokenId, user1.address, "tos_violation");

      await expect(nft.ownerOf(tokenId)).to.be.reverted;
    });

    it("addSigner / removeSigner", async () => {
      await nft.addSigner(user2.address);
      expect(await nft.authorizedSigners(user2.address)).to.be.true;

      await nft.removeSigner(user2.address);
      expect(await nft.authorizedSigners(user2.address)).to.be.false;
    });

    it("reverts addSigner if already authorized", async () => {
      await expect(nft.addSigner(signer.address))
          .to.be.revertedWithCustomError(nft, "AlreadyAuthorized");
    });

    it("setMaxDeadlineWindow updates window", async () => {
      await nft.setMaxDeadlineWindow(2 * 3600);
      expect(await nft.maxDeadlineWindow()).to.equal(2 * 3600);
    });

    it("non-owner cannot call admin functions", async () => {
      await expect(nft.connect(user1).addSigner(user2.address))
          .to.be.revertedWithCustomError(nft, "OwnableUnauthorizedAccount");
      await expect(nft.connect(user1).pause())
          .to.be.revertedWithCustomError(nft, "OwnableUnauthorizedAccount");
    });
  });

  // --- Views ---

  describe("Views", () => {
    it("hasProfile returns false before mint", async () => {
      expect(await nft.hasProfile(user1.address)).to.be.false;
    });

    it("hasProfile returns true after mint", async () => {
      await mintProfile(user1, "alice");
      expect(await nft.hasProfile(user1.address)).to.be.true;
    });

    it("isNicknameTaken is case-insensitive", async () => {
      await mintProfile(user1, "alice");
      expect(await nft.isNicknameTaken("ALICE")).to.be.true;
      expect(await nft.isNicknameTaken("Alice")).to.be.true;
      expect(await nft.isNicknameTaken("alice")).to.be.true;
    });

    it("tokenURI returns correct URL", async () => {
      await mintProfile(user1, "alice");
      const tokenId = await nft.userTokenId(user1.address);
      expect(await nft.tokenURI(tokenId)).to.equal(`${BASE_URI}1`);
    });

    it("getProfile returns false for non-existent profile", async () => {
      const profile = await nft.getProfile(user1.address);
      expect(profile.exists).to.be.false;
    });
  });
});