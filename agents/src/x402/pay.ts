import "dotenv/config";
import { payForReputation } from "./client.js";

async function main(): Promise<void> {
  const serverUrl = process.env["SERVER_URL"];
  const endpointPath = process.env["ENDPOINT_PATH"] || "/reputation/acme-farm";
  const clientPemPath = process.env["CLIENT_PRIVATE_KEY_PATH"];
  const clientKeyAlgo = (process.env["CLIENT_KEY_ALGO"] || "ed25519") as "ed25519" | "secp256k1";
  const preferNetwork = process.env["CLIENT_PREFER_NETWORK"] || "casper:";

  if (!serverUrl) {
    console.error("❌ SERVER_URL es requerido");
    process.exit(1);
  }
  if (!clientPemPath) {
    console.error("❌ CLIENT_PRIVATE_KEY_PATH es requerido (PEM Ed25519 del pagador)");
    process.exit(1);
  }

  const result = await payForReputation({
    serverUrl,
    endpoint: endpointPath,
    clientPemPath,
    clientKeyAlgo,
    preferNetwork,
  });

  console.log(`HTTP ${result.status}`);
  console.log(JSON.stringify(result.body, null, 2));
  if (result.settle) {
    console.log("💰 Settlement on-chain:");
    console.log(JSON.stringify(result.settle, null, 2));
  }
}

main().catch((err) => {
  console.error(err?.response?.data?.error ?? err);
  process.exit(1);
});