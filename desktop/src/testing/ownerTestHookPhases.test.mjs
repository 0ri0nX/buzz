import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  OWNER_TEST_PHASES,
  createOwnerTestPhaseReporter,
  reportOwnerTestPhase,
  runOwnerTestBootstrap,
} from "./ownerTestHookPhases.ts";

test("ordinary builds and explicitly disabled reporting emit no diagnostic IPC", () => {
  const calls = [];
  globalThis.window = {
    __TAURI_INTERNALS__: {
      invoke: (...args) => {
        calls.push(args);
        return Promise.resolve();
      },
    },
  };
  const disabled = createOwnerTestPhaseReporter(false);
  for (const phase of OWNER_TEST_PHASES) {
    disabled(phase);
    reportOwnerTestPhase(phase);
  }
  assert.deepEqual(calls, []);
});

test("enabled reports send only closed phase arguments and match native literals", () => {
  const calls = [];
  const report = createOwnerTestPhaseReporter(true, async (command, args) => {
    calls.push({ command, args });
  });
  for (const phase of OWNER_TEST_PHASES) report(phase);
  for (const rejected of ["secret_value", "reply_accepted\nsecret_value", null])
    report(rejected);
  assert.deepEqual(
    calls,
    OWNER_TEST_PHASES.map((phase) => ({
      command: "rowvia_owner_test_phase",
      args: { phase },
    })),
  );
  const native = readFileSync(
    new URL("../../src-tauri/src/owner_test_hook.rs", import.meta.url),
    "utf8",
  );
  const literals = [...native.matchAll(/"(\w+)" => Ok\("(\w+)"\)/g)];
  assert.deepEqual(
    literals.map((match) => match[1]),
    OWNER_TEST_PHASES,
  );
  assert.ok(literals.every((match) => match[1] === match[2]));
  const registry = readFileSync(
    new URL("../../src-tauri/src/lib.rs", import.meta.url),
    "utf8",
  );
  assert.match(
    registry,
    /#\[cfg\(all\(unix, feature = "rowvia-owner-test-hook"\)\)\]\s+owner_test_hook::rowvia_owner_test_phase,/,
  );
});

test("diagnostic rejection, synchronous failure and stalls cannot reject or delay bootstrap", async (t) => {
  const failure = new Error("secret_value must not be printed");
  const printed = [];
  for (const method of ["log", "info", "warn", "error"])
    t.mock.method(console, method, (...args) => printed.push(args));
  for (const invoke of [
    () => Promise.reject(failure),
    () => {
      throw failure;
    },
    () => new Promise(() => {}),
  ]) {
    const report = createOwnerTestPhaseReporter(true, invoke);
    let completed = false;
    await runOwnerTestBootstrap(async () => {
      completed = true;
    }, report);
    assert.equal(completed, true);
    await assert.rejects(
      runOwnerTestBootstrap(async () => {
        throw failure;
      }, report),
      (error) => error === failure,
    );
  }
  await new Promise(setImmediate);
  assert.deepEqual(printed, []);
});

test("bootstrap markers surround startup without awaiting diagnostic acceptance", async () => {
  const calls = [];
  const startup = Promise.withResolvers();
  const report = createOwnerTestPhaseReporter(true, (command, args) => {
    calls.push({ command, args });
    return new Promise(() => {});
  });
  const pending = runOwnerTestBootstrap(async () => {
    calls.push("startup_entered");
    await startup.promise;
  }, report);
  assert.deepEqual(calls, [
    {
      command: "rowvia_owner_test_phase",
      args: { phase: "bootstrap_started" },
    },
    "startup_entered",
  ]);
  startup.resolve();
  await pending;
  assert.deepEqual(calls.at(-1), {
    command: "rowvia_owner_test_phase",
    args: { phase: "bootstrap_completed" },
  });
  calls.length = 0;
  const failure = new Error("startup failure");
  await assert.rejects(
    runOwnerTestBootstrap(async () => {
      throw failure;
    }, report),
    (error) => error === failure,
  );
  assert.deepEqual(
    calls.map((call) => call.args.phase),
    ["bootstrap_started", "bootstrap_failed"],
  );
});
