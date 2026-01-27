import { loadFixture, time } from "@nomicfoundation/hardhat-toolbox/network-helpers";
import { anyValue } from "@nomicfoundation/hardhat-chai-matchers/withArgs";
import { expect } from "chai";
import { ethers } from "hardhat";

describe("ProfileNFT", function () {

    function generateRandomNickname() {
        const timestamp = Date.now();
        const random = Math.floor(Math.random() * 1000);
        return `User_${timestamp}_${random}`;
    }

    async function deployFixture() {
        const [owner, user1, user2] = await ethers.getSigners();
        const chainId = (await ethers.provider.getNetwork()).chainId;

        const ProfileNFT = await ethers.getContractFactory("ProfileNFT");
        const profileContract = await ProfileNFT.deploy(owner.address);

        return { profileContract, owner, user1, user2, chainId };
    }

    describe("Registration Flow", function () {
        it("Should allow user to mint profile with valid server signature", async function () {
            const { profileContract, owner, user1, chainId } = await loadFixture(deployFixture);

            const nickname = generateRandomNickname();
            const soulbound = false;
            const deadline = await time.latest() + 3600;
            const nonce = await profileContract.nonces(user1.address);

            const messageHash = ethers.solidityPackedKeccak256(
                ["address", "string", "bool", "uint256", "uint256", "uint256"],
                [user1.address, nickname, soulbound, deadline, chainId, nonce]
            );

            const messageBytes = ethers.getBytes(messageHash);
            const signature = await owner.signMessage(messageBytes);

            await expect(profileContract.connect(user1).createProfile(nickname, soulbound, deadline, signature))
                .to.emit(profileContract, "ProfileCreated")
                .withArgs(user1.address, 1n, nickname, anyValue, soulbound);

            expect(await profileContract.takenNicknames(nickname)).to.be.true;
            expect(await profileContract.balanceOf(user1.address)).to.equal(1n);
            expect(await profileContract.userTokenId(user1.address)).to.equal(1n);
        });

        it("Should FAIL if signature is wrong (Hacking attempt)", async function () {
            const { profileContract, owner, user1, user2, chainId } = await loadFixture(deployFixture);

            const nickname = generateRandomNickname();
            const deadline = await time.latest() + 3600;
            const nonce = await profileContract.nonces(user1.address);

            const messageHash = ethers.solidityPackedKeccak256(
                ["address", "string", "bool", "uint256", "uint256", "uint256"],
                [user2.address, nickname, false, deadline, chainId, nonce]
            );
            const signature = await owner.signMessage(ethers.getBytes(messageHash));

            await expect(
                profileContract.connect(user1).createProfile(nickname, false, deadline, signature)
            ).to.be.revertedWithCustomError(profileContract, "InvalidSignature");
        });

        it("Should FAIL if user tries to mint twice", async function () {
            const { profileContract, owner, user1, chainId } = await loadFixture(deployFixture);

            const nickname1 = generateRandomNickname();
            const deadline = await time.latest() + 3600;

            let nonce = await profileContract.nonces(user1.address);
            let messageHash = ethers.solidityPackedKeccak256(
                ["address", "string", "bool", "uint256", "uint256", "uint256"],
                [user1.address, nickname1, false, deadline, chainId, nonce]
            );
            let signature = await owner.signMessage(ethers.getBytes(messageHash));

            await profileContract.connect(user1).createProfile(nickname1, false, deadline, signature);

            const nickname2 = generateRandomNickname();
            nonce = await profileContract.nonces(user1.address);

            messageHash = ethers.solidityPackedKeccak256(
                ["address", "string", "bool", "uint256", "uint256", "uint256"],
                [user1.address, nickname2, false, deadline, chainId, nonce]
            );
            signature = await owner.signMessage(ethers.getBytes(messageHash));

            await expect(
                profileContract.connect(user1).createProfile(nickname2, false, deadline, signature)
            ).to.be.revertedWithCustomError(profileContract, "ProfileAlreadyExists");
        });

        it("Should FAIL if signature expired", async function () {
            const { profileContract, owner, user1, chainId } = await loadFixture(deployFixture);

            const nickname = generateRandomNickname();
            const deadline = await time.latest() - 10;
            const nonce = await profileContract.nonces(user1.address);

            const messageHash = ethers.solidityPackedKeccak256(
                ["address", "string", "bool", "uint256", "uint256", "uint256"],
                [user1.address, nickname, false, deadline, chainId, nonce]
            );
            const signature = await owner.signMessage(ethers.getBytes(messageHash));

            await expect(
                profileContract.connect(user1).createProfile(nickname, false, deadline, signature)
            ).to.be.revertedWithCustomError(profileContract, "SignatureExpired");
        });
    });
});