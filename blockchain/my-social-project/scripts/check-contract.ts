import hre from "hardhat";
import { encodePacked, keccak256 } from "viem";

async function main() {
  console.log("🛠  Начинаем проверку контракта...");

  // Получаем инструменты напрямую из HRE
  const viem = (hre as any).viem;
  const [owner, user, signerAccount] = await viem.getWalletClients();
  const publicClient = await viem.getPublicClient();

  console.log("📡 Деплоим контракт...");
  const profileNFT = await viem.deployContract("ProfileNFT", [
    signerAccount.account.address,
  ]);
  console.log("✅ Контракт задеплоен по адресу:", profileNFT.address);

  const nickname = "Oleg_Web3_Kyiv";
  const isSoulbound = false;
  const deadline = BigInt(Math.floor(Date.now() / 1000) + 3600);
  const chainId = BigInt(hre.network.config.chainId || 31337);

  const nonce = await profileNFT.read.nonces([user.account.address]);

  // Генерируем данные и подпись
  const messageHash = keccak256(
    encodePacked(
      ["address", "string", "bool", "uint256", "uint256", "uint256"],
      [user.account.address, nickname, isSoulbound, deadline, chainId, nonce]
    )
  );

  console.log("✍️  Подписываем сообщение...");
  const signature = await signerAccount.signMessage({
    message: { raw: messageHash },
  });

  const profileNFTAsUser = await viem.getContractAt(
    "ProfileNFT",
    profileNFT.address,
    { client: { wallet: user } }
  );

  console.log("🚀 Отправляем транзакцию createProfile...");
  const hash = await profileNFTAsUser.write.createProfile([
    nickname,
    isSoulbound,
    deadline,
    signature,
  ]);

  await publicClient.waitForTransactionReceipt({ hash });
  console.log("🎊 Транзакция подтверждена!");

  const balance = await profileNFT.read.balanceOf([user.account.address]);
  console.log("📊 Баланс пользователя:", balance.toString());

  if (balance === 1n) {
    console.log("🏆 ТЕСТ ПРОЙДЕН УСПЕШНО!");
  } else {
    console.error("❌ ОШИБКА: Баланс не изменился.");
  }
}

main().catch((error) => {
  console.error("💥 Произошла ошибка:");
  console.error(error);
  process.exit(1);
});
