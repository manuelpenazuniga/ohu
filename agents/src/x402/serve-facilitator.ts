import "dotenv/config";
import { loadX402Config } from "./config.js";
import { buildFacilitatorApp } from "./facilitator.js";

async function main(): Promise<void> {
  const cfg = loadX402Config();
  const { app } = await buildFacilitatorApp(cfg);
  app.listen(cfg.facilitatorPort, () => {
    console.log(`🚀 Facilitator local (Rail B fallback) en http://localhost:${cfg.facilitatorPort}`);
    console.log(`   Firmando deploys contra ${cfg.facilitatorRpcUrl} (${cfg.chainID}).`);
    console.log(`   Settle = transfer_with_authorization (CEP-18). NO toca el OhuVault.`);
  });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});