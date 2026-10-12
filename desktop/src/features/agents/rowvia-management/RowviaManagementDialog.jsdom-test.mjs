import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { after, afterEach, test } from "node:test";
import React from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

// Use synthetic management and theme state; render the real dialog and controls.
let management;
const fixtureKey = "__BUZZ_TEST_ROWVIA_MANAGEMENT__";
const hooks = registerHooks({
  resolve(specifier, context, nextResolve) {
    if (
      specifier === "@/shared/theme/ThemeProvider" &&
      context.parentURL?.endsWith("/shared/ui/dialog.tsx")
    ) {
      return {
        shortCircuit: true,
        url: "data:text/javascript,export const useTheme = () => ({ isDark: false });",
      };
    }
    if (
      specifier === "./useRowviaManagement" &&
      context.parentURL?.endsWith("/RowviaManagementDialog.tsx")
    ) {
      return {
        shortCircuit: true,
        url: `data:text/javascript,${encodeURIComponent(
          `export const useRowviaManagement = () => globalThis.${fixtureKey}();`,
        )}`,
      };
    }
    return nextResolve(specifier, context);
  },
});
globalThis[fixtureKey] = () => management;
const { RowviaManagementDialog } = await import("./RowviaManagementDialog.tsx");
afterEach(cleanup);
after(() => {
  hooks.deregister();
  delete globalThis[fixtureKey];
});

function show(phase, operation = null) {
  const calls = [];
  management = {
    current: { phase, operation },
    open: true,
    pendingCount: 0,
    setOpen: () => {},
    finish: () => calls.push("finish"),
    retryApproval: () => calls.push("retryApproval"),
  };
  render(React.createElement(RowviaManagementDialog));
  return { calls, status: screen.getByRole("status") };
}

for (const runtimeReady of [false, undefined]) {
  test(`grant with runtime_ready=${runtimeReady} reports unverified readiness`, () => {
    const { calls, status } = show("policy-applied", {
      action: "connector.grant",
      runtime_ready: runtimeReady,
    });
    assert.equal(
      status.textContent,
      "Access granted. Runtime readiness is unverified.",
    );
    assert.equal(status.closest("section").getAttribute("aria-live"), "polite");
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    assert.deepEqual(calls, ["finish"]);
  });
}

test("verified runtime retains its existing success message", () => {
  const { status } = show("policy-applied", {
    action: "connector.grant",
    runtime_ready: true,
  });
  assert.equal(status.textContent, "Access granted and runtime ready.");
});

for (const runtimeReady of [false, true]) {
  test(`revocation confirms access revoked with runtime_ready=${runtimeReady}`, () => {
    const { status } = show("policy-applied", {
      action: "connector.revoke",
      runtime_ready: runtimeReady,
    });
    assert.equal(status.textContent, "Access revoked.");
  });
}

test("unconfirmed decisions retain the explicit status and retry action", () => {
  const { calls, status } = show("unconfirmed");
  assert.match(status.textContent, /^Decision unconfirmed\./);
  fireEvent.click(
    screen.getByRole("button", { name: "Check status and retry approval" }),
  );
  assert.deepEqual(calls, ["retryApproval"]);
  assert.equal(screen.queryByRole("button", { name: "Done" }), null);
});

test("an in-flight approval retry keeps its recovery message", () => {
  const { status } = show("retrying-approval");
  assert.match(status.textContent, /^Checking the operation and retrying/);
  assert.equal(
    screen.queryByRole("button", { name: "Check status and retry approval" }),
    null,
  );
  assert.equal(screen.queryByRole("button", { name: "Done" }), null);
});
