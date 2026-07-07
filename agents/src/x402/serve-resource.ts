import "dotenv/config";
import { loadX402Config } from "./config.js";
import { buildFacilitatorClient } from "./facilitator-client.js";
import { buildReputationApp } from "./reputation-server.js";
import { loadCsprCloudConfig } from "./reputation-source.js";

async function main(): Promise<void> {
  const cfg = loadX402Config();
  const facilitator = buildFacilitatorClient(cfg);
  const reputationSource = loadCsprCloudConfig();
  const app = buildReputationApp(cfg, facilitator, reputationSource);
  app.listen(cfg.resourcePort, () => {
    console.log(`🏛️  Ohu reputación (Rail B / x402) en http://localhost:${cfg.resourcePort}`);
    console.log(
      reputationSource
        ? `   Fuente: historial on-chain REAL vía CSPR.cloud (${reputationSource.apiUrl}).`
        : `   Fuente: SEED (fallback — configura CSPRCLOUD_API_KEY para datos reales).`,
    );
    console.log(
      `   NO es el rail de settlement de escrow. Escrow vive en OhuVault (Rail A).`,
    );
  });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});