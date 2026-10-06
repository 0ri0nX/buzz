import { invokeTauri } from "@/shared/api/tauri";

/** Closed checkpoints, not readiness authority; never attach request or identity data. */
export const OWNER_TEST_PHASES = [
  "bootstrap_started",
  "bootstrap_completed",
  "bootstrap_failed",
  "registration_started",
  "registration_ready",
  "registration_failed",
  "request_received",
  "identity_started",
  "identity_ready",
  "identity_error",
  "relay_started",
  "relay_ready",
  "relay_error",
  "reply_started",
  "reply_accepted",
  "reply_failed",
] as const;

export type OwnerTestPhase = (typeof OWNER_TEST_PHASES)[number];
export type OwnerTestPhaseReporter = (phase: OwnerTestPhase) => void;

/** Diagnostic IPC is best effort and cannot delay or replace business IPC. */
export function createOwnerTestPhaseReporter(
  enabled: boolean,
  invoke: typeof invokeTauri = invokeTauri,
): OwnerTestPhaseReporter {
  return (phase) => {
    if (!enabled || !OWNER_TEST_PHASES.includes(phase)) return;
    try {
      // Only this diagnostic failure is absorbed; never print its error value.
      void invoke("rowvia_owner_test_phase", { phase }).catch(() => {});
    } catch {
      // An unavailable diagnostic transport must also leave business IPC alone.
    }
  };
}

export const reportOwnerTestPhase = createOwnerTestPhaseReporter(
  import.meta.env?.VITE_ROWVIA_OWNER_TEST_HOOK === "1",
);

/** Observe startup before its first await and preserve its original failure. */
export async function runOwnerTestBootstrap(
  bootstrap: () => Promise<void>,
  reportPhase: OwnerTestPhaseReporter = reportOwnerTestPhase,
): Promise<void> {
  reportPhase("bootstrap_started");
  try {
    await bootstrap();
    reportPhase("bootstrap_completed");
  } catch (error) {
    reportPhase("bootstrap_failed");
    throw error;
  }
}
