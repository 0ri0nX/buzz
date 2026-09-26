import * as React from "react";
import type { ConnectorDraftRequest } from "@/features/agents/agentManagement";
import {
  subscribeConnectorRequests,
  type ConnectorRequestEvidence,
} from "@/features/agents/observerRelayStore";
import { useCommunities } from "@/features/communities/useCommunities";
import { useIdentityQuery } from "@/shared/api/hooks";
import {
  approveRowviaManagementProposal,
  createRowviaManagementProposal,
  getRowviaManagementCandidates,
  getRowviaManagementOperation,
  loadRowviaManagementRecoveryState,
  rejectRowviaManagementLocal,
  retryRowviaManagementApproval,
  retryRowviaManagementProposal,
  type RowviaCandidates,
  type RowviaNativeReceipt,
  type RowviaOperation,
  type RowviaOperationRecovery,
  type RowviaUnresolvedProposal,
} from "@/shared/api/rowviaManagement";

const MAX_PENDING = 20;
type Phase =
  | "needs-selection"
  | "preparing"
  | "unresolved"
  | "retrying"
  | "review"
  | "submitting"
  | "retrying-approval"
  | "unconfirmed"
  | "stale"
  | "failed"
  | "policy-applied";

type Item = {
  id: string;
  request: ConnectorDraftRequest | null;
  evidence: ConnectorRequestEvidence | null;
  phase: Phase;
  selectionHandle: string | null;
  receipt: RowviaNativeReceipt | null;
  unresolved: RowviaUnresolvedProposal | null;
  operationId: string | null;
  operation: RowviaOperation | null;
  error: string | null;
};

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isStaleError(error: unknown): boolean {
  return /stale|expired|conflict|changed|unknown.*selection/i.test(
    errorMessage(error),
  );
}

/** Fill the visible window from durable receipts after each completed review. */
export function refillRowviaRecoveryQueue(
  previous: Item[],
  pending: {
    receipts: RowviaNativeReceipt[];
    unresolved: RowviaUnresolvedProposal[];
  },
  operations: RowviaOperationRecovery[],
  dismissed: Set<string>,
): { items: Item[]; overflow: boolean } {
  const known = new Set(
    previous
      .map(
        (item) =>
          item.receipt?.receipt_handle ??
          item.unresolved?.receipt_handle ??
          (item.id.startsWith("operation:")
            ? item.id.slice("operation:".length)
            : null),
      )
      .filter(Boolean),
  );
  const incoming: Item[] = pending.receipts
    .filter(
      (receipt) =>
        !known.has(receipt.receipt_handle) &&
        !dismissed.has(receipt.receipt_handle),
    )
    .map((receipt) => ({
      id: `receipt:${receipt.receipt_handle}`,
      request: null,
      evidence: null,
      phase: "review",
      selectionHandle: null,
      receipt,
      unresolved: null,
      operationId: receipt.operation_id,
      operation: null,
      error: null,
    }));
  const unresolved: Item[] = pending.unresolved
    .filter(
      (entry) =>
        !known.has(entry.receipt_handle) &&
        !dismissed.has(entry.receipt_handle),
    )
    .map((entry) => ({
      id: `unresolved:${entry.receipt_handle}`,
      request: null,
      evidence: null,
      phase: "unresolved",
      selectionHandle: null,
      receipt: null,
      unresolved: entry,
      operationId: null,
      operation: null,
      error: null,
    }));
  const approved: Item[] = operations
    .filter((entry) => entry.approval_intent && !entry.refused)
    .filter(
      (entry) =>
        !entry.last_status?.policy_applied &&
        entry.last_status?.state !== "failed" &&
        entry.last_status?.state !== "expired" &&
        !known.has(entry.receipt_handle) &&
        !dismissed.has(entry.receipt_handle),
    )
    .map((entry) => ({
      id: `operation:${entry.receipt_handle}`,
      request: null,
      evidence: null,
      phase: "unconfirmed",
      selectionHandle: null,
      receipt: null,
      unresolved: null,
      operationId: entry.operation_id,
      operation: entry.last_status,
      error: null,
    }));
  const recovered = [...incoming, ...unresolved, ...approved];
  return {
    items: [
      ...previous,
      ...recovered.slice(0, Math.max(0, MAX_PENDING - previous.length)),
    ],
    overflow: previous.length + recovered.length > MAX_PENDING,
  };
}

export function useRowviaManagement() {
  const { activeCommunity } = useCommunities();
  const identity = useIdentityQuery();
  const scope =
    activeCommunity && identity.data
      ? `${activeCommunity.id}:${activeCommunity.relayUrl}:${identity.data.pubkey}`
      : null;
  const scopeRef = React.useRef(scope);
  scopeRef.current = scope;
  const generation = React.useRef(0);
  const busy = React.useRef(false);
  const seen = React.useRef(new Set<string>());
  const dismissedReceipts = React.useRef(new Set<string>());
  const [items, setItems] = React.useState<Item[]>([]);
  const [open, setOpen] = React.useState(true);
  const [candidates, setCandidates] = React.useState<RowviaCandidates | null>(
    null,
  );
  const [candidateError, setCandidateError] = React.useState<string | null>(
    null,
  );
  const [candidateLoading, setCandidateLoading] = React.useState(false);
  const [overflow, setOverflow] = React.useState(false);
  const [recoveryError, setRecoveryError] = React.useState<string | null>(null);
  const [refillTick, setRefillTick] = React.useState(0);
  const current = items[0] ?? null;

  React.useEffect(() => {
    // Scope changes invalidate every in-flight native response and queued item.
    void scope;
    generation.current += 1;
    busy.current = false;
    seen.current.clear();
    dismissedReceipts.current.clear();
    setItems([]);
    setCandidates(null);
    setCandidateLoading(false);
    setCandidateError(null);
    setOverflow(false);
    setRecoveryError(null);
    setOpen(true);
  }, [scope]);

  React.useEffect(() => {
    if (!scope) return;
    void refillTick;
    let active = true;
    const recover = async () => {
      const { pending, operations, errors } =
        await loadRowviaManagementRecoveryState();
      if (!active || scopeRef.current !== scope) return;
      setRecoveryError(
        errors.length > 0 ? errors.map(errorMessage).join("; ") : null,
      );
      setItems((previous) => {
        const next = refillRowviaRecoveryQueue(
          previous,
          pending,
          operations,
          dismissedReceipts.current,
        );
        setOverflow(next.overflow);
        return next.items;
      });
    };
    void recover();
    window.addEventListener("online", recover);
    return () => {
      active = false;
      window.removeEventListener("online", recover);
    };
  }, [scope, refillTick]);

  React.useEffect(() => {
    if (!scope) return;
    const unsubscribe = subscribeConnectorRequests((request, evidence) => {
      if (scopeRef.current !== scope) return;
      const id = `${evidence.relayEvent.id}:${request.requestId}`;
      if (seen.current.has(id)) return;
      setItems((previous) => {
        if (previous.some((item) => item.id === id)) return previous;
        if (previous.length >= MAX_PENDING) {
          setOverflow(true);
          return previous;
        }
        seen.current.add(id);
        return [
          ...previous,
          {
            id,
            request,
            evidence,
            phase: "needs-selection",
            selectionHandle: null,
            receipt: null,
            unresolved: null,
            operationId: null,
            operation: null,
            error: null,
          },
        ];
      });
      setOpen(true);
    });
    return () => {
      unsubscribe();
    };
  }, [scope]);

  const updateCurrent = React.useCallback(
    (id: string, change: Partial<Item>) => {
      setItems((previous) =>
        previous.map((item) =>
          item.id === id ? { ...item, ...change } : item,
        ),
      );
    },
    [],
  );

  const refreshCandidates = React.useCallback(async () => {
    if (!scope || !current) return;
    const token = generation.current;
    setCandidateLoading(true);
    setCandidateError(null);
    try {
      const result = await getRowviaManagementCandidates();
      if (token !== generation.current || scopeRef.current !== scope) return;
      setCandidates(result);
    } catch (error) {
      if (token !== generation.current || scopeRef.current !== scope) return;
      setCandidateError(errorMessage(error));
    } finally {
      if (token === generation.current && scopeRef.current === scope)
        setCandidateLoading(false);
    }
  }, [scope, current]);

  React.useEffect(() => {
    if (
      current?.phase === "needs-selection" &&
      open &&
      !candidates &&
      !candidateLoading &&
      !candidateError
    )
      void refreshCandidates();
  }, [
    current?.phase,
    open,
    candidates,
    candidateLoading,
    candidateError,
    refreshCandidates,
  ]);

  const select = React.useCallback(
    (selectionHandle: string | null) => {
      if (!current || busy.current) return;
      generation.current += 1;
      setCandidateLoading(false);
      updateCurrent(current.id, {
        selectionHandle,
        receipt: null,
        unresolved: null,
        operationId: null,
        operation: null,
        phase: "needs-selection",
        error: null,
      });
    },
    [current, updateCurrent],
  );

  const prepare = React.useCallback(async () => {
    if (
      !current?.selectionHandle ||
      !current.request ||
      !current.evidence ||
      busy.current ||
      !scope
    )
      return;
    busy.current = true;
    const token = generation.current;
    const { id, selectionHandle, request, evidence } = current;
    updateCurrent(id, { phase: "preparing", error: null });
    try {
      const receipt = await createRowviaManagementProposal({
        selectionHandle,
        action: request.action,
        relayEvent: evidence.relayEvent,
      });
      if (token !== generation.current || scopeRef.current !== scope) return;
      updateCurrent(id, {
        receipt,
        operationId: receipt.operation_id,
        phase: "review",
      });
    } catch (error) {
      if (token !== generation.current || scopeRef.current !== scope) return;
      updateCurrent(id, {
        phase: isStaleError(error) ? "stale" : "failed",
        error: errorMessage(error),
      });
    } finally {
      if (token === generation.current && scopeRef.current === scope)
        busy.current = false;
    }
  }, [current, scope, updateCurrent]);

  const retryUnresolved = React.useCallback(async () => {
    if (
      !current?.unresolved ||
      current.phase !== "unresolved" ||
      busy.current ||
      !scope
    )
      return;
    busy.current = true;
    const token = generation.current;
    const { id, unresolved } = current;
    updateCurrent(id, { phase: "retrying", error: null });
    try {
      const receipt = await retryRowviaManagementProposal(
        unresolved.receipt_handle,
      );
      if (token !== generation.current || scopeRef.current !== scope) return;
      updateCurrent(id, {
        receipt,
        unresolved: null,
        operationId: receipt.operation_id,
        phase: "review",
      });
    } catch (error) {
      if (token !== generation.current || scopeRef.current !== scope) return;
      updateCurrent(id, { phase: "unresolved", error: errorMessage(error) });
    } finally {
      if (token === generation.current && scopeRef.current === scope)
        busy.current = false;
    }
  }, [current, scope, updateCurrent]);

  const approve = React.useCallback(async () => {
    if (
      !current?.receipt ||
      current.phase !== "review" ||
      busy.current ||
      !scope
    )
      return;
    busy.current = true;
    const token = generation.current;
    const { id, receipt } = current;
    updateCurrent(id, { phase: "submitting", error: null });
    try {
      const operation = await approveRowviaManagementProposal(
        receipt.receipt_handle,
      );
      if (token !== generation.current || scopeRef.current !== scope) return;
      updateCurrent(id, {
        operation,
        phase: operation.policy_applied ? "policy-applied" : "unconfirmed",
      });
    } catch (error) {
      if (token !== generation.current || scopeRef.current !== scope) return;
      // Native records the click before transport. A lost response is unknown,
      // so only operation status may settle it; never issue a second approval.
      updateCurrent(id, { phase: "unconfirmed", error: errorMessage(error) });
    } finally {
      if (token === generation.current && scopeRef.current === scope)
        busy.current = false;
    }
  }, [current, scope, updateCurrent]);

  const retryApproval = React.useCallback(async () => {
    if (current?.phase !== "unconfirmed" || busy.current || !scope) return;
    const receiptHandle =
      current.receipt?.receipt_handle ??
      (current.id.startsWith("operation:")
        ? current.id.slice("operation:".length)
        : null);
    if (!receiptHandle) return;
    busy.current = true;
    generation.current += 1;
    const token = generation.current;
    const id = current.id;
    updateCurrent(id, { phase: "retrying-approval", error: null });
    try {
      const operation = await retryRowviaManagementApproval(receiptHandle);
      if (token !== generation.current || scopeRef.current !== scope) return;
      updateCurrent(id, {
        operation,
        phase: operation.policy_applied
          ? "policy-applied"
          : operation.state === "failed"
            ? "failed"
            : operation.state === "expired"
              ? "stale"
              : "unconfirmed",
      });
    } catch (error) {
      if (token !== generation.current || scopeRef.current !== scope) return;
      updateCurrent(id, {
        phase: isStaleError(error) ? "stale" : "unconfirmed",
        error: errorMessage(error),
      });
    } finally {
      if (token === generation.current && scopeRef.current === scope)
        busy.current = false;
    }
  }, [current, scope, updateCurrent]);

  const pollingId = current?.id;
  const pollingOperationId = current?.operationId;
  const pollingPhase = current?.phase;
  React.useEffect(() => {
    if (
      !open ||
      !pollingId ||
      !pollingOperationId ||
      (pollingPhase !== "unconfirmed" && pollingPhase !== "submitting")
    )
      return;
    const id = pollingId;
    const operationId = pollingOperationId;
    const token = generation.current;
    let cancelled = false;
    let timeout: number | undefined;
    let delay = 2_000;
    const poll = async () => {
      try {
        const operation = await getRowviaManagementOperation(operationId);
        if (
          cancelled ||
          token !== generation.current ||
          scopeRef.current !== scope
        )
          return;
        if (operation.policy_applied) {
          updateCurrent(id, {
            operation,
            phase: "policy-applied",
            error: null,
          });
          return;
        }
        if (operation.state === "failed" || operation.state === "expired") {
          updateCurrent(id, {
            operation,
            phase: operation.state === "expired" ? "stale" : "failed",
            error: operation.error_code ?? operation.state,
          });
          return;
        }
        updateCurrent(id, { operation, phase: "unconfirmed" });
      } catch (error) {
        if (
          cancelled ||
          token !== generation.current ||
          scopeRef.current !== scope
        )
          return;
        updateCurrent(id, { phase: "unconfirmed", error: errorMessage(error) });
      }
      if (!cancelled) {
        timeout = window.setTimeout(poll, delay);
        delay = Math.min(delay * 2, 30_000);
      }
    };
    timeout = window.setTimeout(poll, delay);
    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  }, [pollingId, pollingOperationId, pollingPhase, open, scope, updateCurrent]);

  const finish = React.useCallback(() => {
    if (!current || busy.current) return;
    const receiptHandle =
      current.receipt?.receipt_handle ??
      current.unresolved?.receipt_handle ??
      (current.id.startsWith("operation:")
        ? current.id.slice("operation:".length)
        : null);
    if (receiptHandle) dismissedReceipts.current.add(receiptHandle);
    generation.current += 1;
    setItems((previous) => previous.slice(1));
    setRefillTick((value) => value + 1);
    setCandidates(null);
    setCandidateLoading(false);
    setCandidateError(null);
    setOpen(true);
  }, [current]);

  const reject = React.useCallback(async () => {
    if (!current || busy.current) return;
    if (!current.receipt) {
      finish();
      return;
    }
    busy.current = true;
    const token = generation.current;
    try {
      await rejectRowviaManagementLocal(current.receipt.receipt_handle);
      if (token === generation.current && scopeRef.current === scope) {
        busy.current = false;
        finish();
      }
    } catch (error) {
      if (token === generation.current && scopeRef.current === scope)
        updateCurrent(current.id, { error: errorMessage(error) });
    } finally {
      if (token === generation.current && scopeRef.current === scope)
        busy.current = false;
    }
  }, [current, finish, scope, updateCurrent]);

  return {
    current,
    pendingCount: Math.max(0, items.length - 1),
    open,
    setOpen,
    candidates,
    candidateError,
    candidateLoading,
    overflow,
    recoveryError,
    refreshCandidates,
    select,
    prepare,
    retryUnresolved,
    approve,
    retryApproval,
    finish,
    reject,
  };
}
