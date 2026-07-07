// Datos REALES del enjambre de Ohu en Casper Testnet (lote 4, liquidado por los
// agentes sin intervención humana el 2026-07-07). Cada hash es una transacción
// verificada on-chain (error_message=None). Ver infra/deployments/testnet.md §P1-2.

export const NETWORK = "casper-test";
export const EXPLORER = "https://testnet.cspr.live/deploy/";
export const VAULT =
  "hash-94c4d7b466a035e0aac9bb60daeaa179432ad2df93de3dfe2759812676bf3b6c";

export const OPERATOR =
  "account-hash-9c28ba3e5c1154fa23085326c9e165de79a32a67b1145edce5e0a2b949f80186";
export const ADMIN =
  "account-hash-59d06759666ef90a065d023c4c2b6a77708c38945380a0b36380f07e71bd70b4";

export type StepKind = "setup" | "agent";
export type Column = "PROPONE" | "AUTORIZA";

export interface LoteStep {
  readonly n: number;
  readonly state: string;
  readonly entrypoint: string;
  readonly actor: string;
  readonly kind: StepKind;
  readonly tx: string;
  readonly column?: Column;
  readonly result?: string;
}

export interface Lote {
  readonly id: number;
  readonly funded: string;
  readonly bond: string;
  readonly premiumBps: number;
  readonly quorumFailBps: number;
  readonly steps: readonly LoteStep[];
}

export const LOTE: Lote = {
  id: 4,
  funded: "10 CSPR",
  bond: "10 CSPR",
  premiumBps: 50,
  quorumFailBps: 6000,
  steps: [
    { n: 1, state: "OPEN", entrypoint: "open_lote", actor: "admin", kind: "setup", tx: "66a2b8e5a945fb7c26628d203011528e409a8d743a16d67025524e37eaf9f03a" },
    { n: 2, state: "OPEN", entrypoint: "deposit_to_lote", actor: "buyer", kind: "setup", tx: "fc46859a4e8dcc50dd5baff7bb8034c3b2988bd5ffb7c7a91da0ef80e3f2e139" },
    { n: 3, state: "OPEN", entrypoint: "post_bond", actor: "producer", kind: "setup", tx: "700a14664c999789c0abbb2f0cfb9e0a3cf0f67a4da31b17abba9c3097bcd5bb" },
    { n: 4, state: "FUNDED", entrypoint: "lock_lote", actor: "admin", kind: "setup", tx: "0e102eab8504785fa6b9cde31d9e3adc53b89f8cf5a1f12aacb5c5f4a88902f4" },
    { n: 5, state: "EVAL_OK", entrypoint: "evaluate_lote", actor: "operator", kind: "agent", column: "PROPONE", result: "EVAL_OK", tx: "58d917305b1552dde941cab76c65ac7d635e55c288069ef6b4cc7ee9a7da21bc" },
    { n: 6, state: "SETTLED_OK", entrypoint: "release_to_producer", actor: "admin", kind: "agent", column: "AUTORIZA", result: "SETTLED_OK", tx: "c1f374a2de8704391edb47de27681eef4c66ceb7b81f6a1965c9a4a065af4c95" },
  ],
};

export const shortHash = (h: string, n = 10): string =>
  h.length > n * 2 ? `${h.slice(0, n)}…${h.slice(-4)}` : h;

export const explorerUrl = (tx: string): string => `${EXPLORER}${tx}`;
