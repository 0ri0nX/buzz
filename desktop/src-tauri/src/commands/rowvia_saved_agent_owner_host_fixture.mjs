// Inert owned pipe fixture. Never imports Main or invokes a real authority.
import { closeSync, readFileSync, readSync, writeSync } from "node:fs";
import { spawn } from "node:child_process";

const startup = JSON.parse(readFileSync(3, "utf8"));
closeSync(3);
function callback(id, kind, ack) {
  writeSync(
    1,
    `${JSON.stringify({ version: 1, id, kind, ...(ack ? { ack } : {}) })}\n`,
  );
  const byte = Buffer.alloc(1);
  let response = "";
  while (!response.endsWith("\n")) {
    if (readSync(0, byte, 0, 1, null) !== 1)
      throw new Error("fixture_callback_eof");
    response += byte.toString("utf8");
  }
  if (!["scope", "ack"].includes(JSON.parse(response).kind))
    throw new Error("fixture_callback_refused");
}
if (startup.kind !== "preflight") {
  callback(1, "capture");
  callback(
    2,
    startup.fixture === "retain" ? "retain" : "revalidate",
    startup.ack,
  );
}
let descendant;
if (startup.fixture === "descendant") {
  // Deliberately outlives the leader with every inherited pipe closed. It stays
  // in Native's original process group; EOF/leader exit cannot settle it.
  const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
    detached: false,
    stdio: "ignore",
    env: {},
  });
  descendant = child.pid;
  child.unref();
}
writeSync(
  4,
  `${JSON.stringify({ version: 1, leader: process.pid, ...(descendant ? { descendant } : {}) })}\n`,
);
closeSync(4);
