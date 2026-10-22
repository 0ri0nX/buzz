// Inert owned test child. It never imports Main, Python or a production authority.
import { readFileSync, writeSync, closeSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const startup = JSON.parse(readFileSync(3, "utf8"));
closeSync(3);
const mode = startup.host.mode;
if (
  mode === "bootstrap-contract" &&
  (process.argv[1] !== fileURLToPath(import.meta.url) ||
    process.cwd() !== dirname(fileURLToPath(import.meta.url)))
) {
  throw new Error("synthetic_bootstrap_contract");
}
if (mode === "stall") {
  setInterval(() => {}, 1000);
} else {
  const ack = startup.execution.ack;
  const request = (id, kind, evidence) => {
    const line = Buffer.from(
      `${JSON.stringify({ version: 1, id, kind, ...(evidence ? { ack: evidence } : {}) })}\n`,
    );
    for (const byte of line) writeSync(1, Buffer.from([byte]));
    if (mode === "lost-response" && kind === "retain") {
      closeSync(0);
      return;
    }
    let response = "";
    const one = Buffer.alloc(1);
    while (!response.endsWith("\n")) {
      // Deliberate byte reads exercise the parent's full-frame delivery gate.
      const count = readSyncFixed(0, one, 0, 1, null);
      if (count !== 1) throw new Error("synthetic_callback_eof");
      response += one.toString("utf8");
    }
    if (!["scope", "ack"].includes(JSON.parse(response).kind))
      throw new Error("synthetic_callback_refused");
  };
  // Use the fixed import below, with no descriptor selector in startup.
  request(1, "capture");
  if (mode !== "no-ack") {
    request(2, "retain", ack);
    if (mode === "lost-response") process.exit(7);
    request(3, "revalidate", ack);
  }
  const canonical21 = "synthetic public canonical cohort";
  const result = {
    version: 1,
    operation_id: ack.operationId,
    agent_pubkey: ack.agentPubkey,
    recipe_sha256: ack.recipeSha256,
    canonical21,
    canonical21_sha256: createHash("sha256").update(canonical21).digest("hex"),
    preparation_retained: true,
    bindings_ready: false,
    runtime_ready: false,
  };
  if (mode === "changed-cohort") result.recipe_sha256 = "f".repeat(64);
  if (mode === "stderr-flood") writeSync(2, Buffer.alloc(65537, 120));
  const wireResult =
    mode === "sequence-result" ? Object.values(result) : result;
  let frame = `${JSON.stringify(wireResult)}\n`;
  if (mode === "trailing-result") frame += "{}\n";
  if (mode === "duplicate-result")
    frame = frame.replace('"version":1', '"version":1,"version":1');
  writeSync(4, frame);
  closeSync(4);
  closeSync(1);
  closeSync(2);
  if (mode === "exit-failure") process.exitCode = 8;
}

import { readSync as readSyncFixed } from "node:fs";
