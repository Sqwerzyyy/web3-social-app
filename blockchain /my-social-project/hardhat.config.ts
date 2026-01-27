import { HardhatUserConfig } from "hardhat/config";
import "@nomicfoundation/hardhat-toolbox"; // <-- Важно: без лишних слов типа "-mocha-ethers"

const config: HardhatUserConfig = {
  solidity: "0.8.20", // Версия должна совпадать с pragma в твоем контракте
};

export default config;
