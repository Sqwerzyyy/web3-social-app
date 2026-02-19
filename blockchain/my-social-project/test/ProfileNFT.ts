import { expect } from "chai";
import hre from "hardhat";
import { encodePacked, keccak256 } from "viem";
import { loadFixture } from "@nomicfoundation/hardhat-network-helpers";

describe("ProfileNFT", function () {
  async function deployProfileNFTFixture() {
    const [owner, user, signerAccount] = await hre.viem.getWalletClients();
    const publicClient = await hre.viem.getPublicClient();

    const profileNFT = await hre.viem.deployContract("ProfileNFT", [
      signerAccount.account.address,
    ]);

    return { profileNFT, owner, user, signerAccount, publicClient };
  }

  it("Should create a profile with a valid signature and age", async function () {
    const { profileNFT, user, signerAccount, publicClient } = await loadFixture(deployProfileNFTFixture);

    const nickname = "Oleg_Web3";
    const age = 17n; 
    const soulbound = false;
    const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);
    const chainId = BigInt(hre.network.config.chainId || 31337);

    const nonce = await publicClient.readContract({
      address: profileNFT.address,
      abi: profileNFT.abi,
      functionName: "nonces",
      args: [user.account.address],
    }) as bigint;

    const messageHash = keccak256(
      encodePacked(
        ["address", "string", "uint256", "bool", "uint256", "uint256", "uint256"],
        [user.account.address, nickname, age, soulbound, deadline, chainId, nonce]
      )
    );

    const signature = await signerAccount.signMessage({
      message: { raw: messageHash },
    });

    const hash = await user.writeContract({
      address: profileNFT.address,
      abi: profileNFT.abi,
      functionName: "createProfile",
      args: [nickname, age, soulbound, deadline, signature],
    });

    await publicClient.waitForTransactionReceipt({ hash });

    const balance = await publicClient.readContract({
      address: profileNFT.address,
      abi: profileNFT.abi,
      functionName: "balanceOf",
      args: [user.account.address],
    });

    console.log("🚀 ПРОФИЛЬ СОЗДАН! Баланс:", balance.toString());
    expect(balance).to.equal(1n);
  });
});
