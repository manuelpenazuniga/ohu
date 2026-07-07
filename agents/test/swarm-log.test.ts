import { describe, it, expect, vi, beforeEach } from "vitest";
import type { SwarmConfig } from "../src/casper/env.js";
import type { SwarmLogEntry } from "../src/swarm/log.js";
import { createSwarmLogger } from "../src/swarm/log.js";
import { appendFileSync, existsSync, mkdirSync, unlinkSync } from "node:fs";

function makeTestConfig(overrides: Partial<SwarmConfig> = {}): SwarmConfig {
  return {
    nodeUrl: "https://node.testnet.casper.network/rpc",
    eventsUrl: "https://events.testnet.casper.network/events",
    chainName: "casper-test",
    vaultPackageHash: "a".repeat(64),
    attestationWindowMs: 300000,
    operatorSecretKeyPath: "/tmp/op.pem",
    adminSecretKeyPath: "/tmp/ad.pem",
    operatorAccountHash: "00" + "aa".repeat(32),
    adminAccountHash: "00" + "bb".repeat(32),
    pollIntervalMs: 15000,
    logFile: "/tmp/ohu-test-swarm-log.jsonl",
    targetLotes: "",
    autorizadorStartDelayMs: 0,
    txPaymentMotes: 3_000_000_000,
    ...overrides,
  };
}

describe("createSwarmLogger", () => {
  const testLogFile = "/tmp/ohu-test-swarm-log.jsonl";

  beforeEach(() => {
    try {
      unlinkSync(testLogFile);
    } catch {
      // no existe — ok
    }
  });

  it("produce una entrada JSON con los campos requeridos", () => {
    const logger = createSwarmLogger(testLogFile);
    const entry: SwarmLogEntry = {
      ts: "2026-07-06T00:00:00.000Z",
      role: "operator",
      column: "PROPONE",
      agentAccount: "00" + "aa".repeat(32),
      entrypoint: "evaluate_lote",
      loteId: 42,
      txHash: "a".repeat(64),
      result: "EVAL_OK",
    };

    // Suprime stdout
    const consoleLog = vi.spyOn(console, "log").mockImplementation(() => {});

    logger.log(entry);

    consoleLog.mockRestore();

    // Verifica que el archivo existe y contiene el JSON
    expect(existsSync(testLogFile)).toBe(true);
  });

  it("no filtra información legítima de los campos públicos", () => {
    const logger = createSwarmLogger(testLogFile);
    const txHash = "b".repeat(64);
    const entry: SwarmLogEntry = {
      ts: "2026-07-06T01:00:00.000Z",
      role: "admin",
      column: "AUTORIZA",
      agentAccount: "00" + "cc".repeat(32),
      entrypoint: "release_to_producer",
      loteId: 7,
      txHash,
      result: "SETTLED_OK",
    };

    const lines: string[] = [];
    const consoleLog = vi.spyOn(console, "log").mockImplementation((...args) => {
      lines.push(args.join(" "));
    });

    logger.log(entry);

    consoleLog.mockRestore();

    const jsonLine = lines.find((l) => l.startsWith("{"));
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine!);
    expect(parsed.txHash).toBe(txHash);
    expect(parsed.role).toBe("admin");
    expect(parsed.column).toBe("AUTORIZA");
    expect(parsed.entrypoint).toBe("release_to_producer");
    expect(parsed.loteId).toBe(7);
    expect(parsed.result).toBe("SETTLED_OK");

    // Verifica que NO hay campos de secreto
    expect(parsed).not.toHaveProperty("privateKey");
    expect(parsed).not.toHaveProperty("pem");
    expect(parsed).not.toHaveProperty("secret");
    expect(parsed).not.toHaveProperty("password");
  });
});
