import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { RowviaConnectorProposalReview } from "./RowviaConnectorProposalReview";
import { useRowviaManagement } from "./useRowviaManagement";

/** Owner-facing review; only a native-journaled receipt reaches the verifier. */
export function RowviaManagementDialog() {
  const management = useRowviaManagement();
  const item = management.current;
  const status =
    item?.phase === "submitting"
      ? "loading"
      : item?.phase === "stale"
        ? "stale"
        : item?.phase === "review"
          ? "pending"
          : "error";

  return (
    <>
      {item && !management.open ? (
        <Button
          className="fixed bottom-5 right-5 z-40 shadow-lg"
          onClick={() => management.setOpen(true)}
          type="button"
        >
          Review Gmail access request
          {management.pendingCount > 0
            ? ` (${management.pendingCount + 1})`
            : ""}
        </Button>
      ) : null}
      <Dialog
        onOpenChange={(open) => management.setOpen(open)}
        open={Boolean(item && management.open)}
      >
        <DialogContent className="max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Cerberus Gmail access request</DialogTitle>
            <DialogDescription>
              {item?.unresolved
                ? "Recover the saved proposal request, then inspect the exact proposal before deciding."
                : "Choose an existing agent and Gmail connection, then inspect the exact proposal before deciding."}
            </DialogDescription>
          </DialogHeader>
          {management.pendingCount > 0 ? (
            <p role="status" className="text-sm text-muted-foreground">
              {management.pendingCount} more request
              {management.pendingCount === 1 ? "" : "s"} waiting.
            </p>
          ) : null}
          {management.overflow ? (
            <p role="alert" className="text-sm text-destructive">
              More requests arrived than can be shown at once. Ask Cerberus to
              resend after this queue is cleared.
            </p>
          ) : null}
          {management.recoveryError ? (
            <p role="alert" className="text-sm text-destructive">
              Could not recover saved reviews: {management.recoveryError}
            </p>
          ) : null}
          {item?.unresolved &&
          (item.phase === "unresolved" || item.phase === "retrying") ? (
            <section
              className="space-y-3 text-sm"
              aria-label="Recover proposal"
            >
              <p role="status">
                The saved{" "}
                {item.unresolved.action === "connector.revoke"
                  ? "revocation"
                  : "grant"}{" "}
                request has no confirmed proposal receipt. Retrying uses the
                same saved request ID and bytes; review the returned proposal
                before any approval.
              </p>
              {item.error ? (
                <p role="alert" className="text-destructive">
                  {item.error}
                </p>
              ) : null}
              <Button
                disabled={item.phase === "retrying"}
                onClick={() => void management.retryUnresolved()}
                type="button"
              >
                {item.phase === "retrying" ? "Retrying…" : "Retry proposal"}
              </Button>
            </section>
          ) : null}
          {item?.phase === "needs-selection" ||
          item?.phase === "preparing" ||
          (item?.phase === "failed" && !item.receipt) ? (
            <section className="space-y-4" aria-label="Choose access target">
              <p className="text-sm">
                Cerberus requested{" "}
                {item.request?.action === "connector.revoke"
                  ? "revoking"
                  : "granting"}{" "}
                Gmail read and search access for “
                {item.request?.request.targetName}”. This name is a hint.
                Confirm the exact agent and account below.
              </p>
              {item.request?.request.gmailLabels?.length ? (
                <p className="text-sm text-muted-foreground">
                  Requested account hint:{" "}
                  {item.request.request.gmailLabels.join(", ")}
                </p>
              ) : null}
              {management.candidateLoading ? (
                <p role="status">Loading eligible connections…</p>
              ) : null}
              {management.candidateError ? (
                <p role="alert" className="text-sm text-destructive">
                  Could not load eligible connections:{" "}
                  {management.candidateError}
                </p>
              ) : null}
              {management.candidates?.truncated ? (
                <p role="alert" className="text-sm text-destructive">
                  The connection list is incomplete. Refine setup and refresh
                  it.
                </p>
              ) : null}
              {management.candidates &&
              management.candidates.candidates.length === 0 ? (
                <p role="status">
                  No eligible existing Gmail connections were found.
                </p>
              ) : null}
              {management.candidates &&
              management.candidates.candidates.length > 0 ? (
                <label className="block space-y-2 text-sm font-medium">
                  <span>Agent and Gmail connection</span>
                  <select
                    className="w-full rounded-md border border-input bg-background px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    onChange={(event) =>
                      management.select(event.target.value || null)
                    }
                    value={item.selectionHandle ?? ""}
                  >
                    <option value="">Select a connection…</option>
                    {management.candidates.candidates.map((candidate) => (
                      <option
                        disabled={candidate.binding_status !== "active"}
                        key={candidate.selection_handle}
                        value={candidate.selection_handle}
                      >
                        {candidate.agent_name} — {candidate.gmail_label} (
                        {candidate.binding_status})
                      </option>
                    ))}
                  </select>
                </label>
              ) : null}
              {item.error ? (
                <p role="alert" className="text-sm text-destructive">
                  {item.error}
                </p>
              ) : null}
              <div className="flex flex-wrap justify-end gap-2">
                <Button
                  onClick={() => void management.refreshCandidates()}
                  type="button"
                  variant="outline"
                >
                  Refresh connections
                </Button>
                <Button
                  disabled={
                    !item.selectionHandle ||
                    item.phase === "preparing" ||
                    management.candidates?.truncated
                  }
                  onClick={() => void management.prepare()}
                  type="button"
                >
                  {item.phase === "preparing"
                    ? "Preparing…"
                    : "Review proposal"}
                </Button>
              </div>
            </section>
          ) : null}
          {item?.receipt &&
          (item.phase === "review" ||
            item.phase === "submitting" ||
            item.phase === "stale") ? (
            <RowviaConnectorProposalReview
              onApprove={() => void management.approve()}
              onReject={() => void management.reject()}
              proposal={item.receipt}
              status={status}
            />
          ) : null}
          {item &&
          (item.phase === "unconfirmed" ||
            item.phase === "retrying-approval" ||
            item.phase === "policy-applied" ||
            (item.phase === "failed" && Boolean(item.receipt)) ||
            item.phase === "stale") ? (
            <section aria-live="polite" className="space-y-3 text-sm">
              <p role="status">
                {item.phase === "policy-applied"
                  ? item.operation?.action === "connector.revoke"
                    ? "Access revoked. Runtime update may still be in progress."
                    : item.operation?.runtime_ready
                      ? "Access granted and runtime ready."
                      : "Access granted, runtime updating."
                  : item.phase === "unconfirmed"
                    ? "Decision unconfirmed. Check the operation, then explicitly retry the same approval if it is still pending."
                    : item.phase === "retrying-approval"
                      ? "Checking the operation and retrying the saved approval only if it is still pending…"
                      : item.phase === "stale"
                        ? "This proposal is stale or expired. Request a new proposal."
                        : "The operation failed. Check the status before requesting another change."}
              </p>
              {item.error ? (
                <p role="alert" className="text-destructive">
                  {item.error}
                </p>
              ) : null}
              {item.phase === "unconfirmed" ? (
                <Button
                  onClick={() => void management.retryApproval()}
                  type="button"
                >
                  Check status and retry approval
                </Button>
              ) : null}
              {item.phase !== "unconfirmed" &&
              item.phase !== "retrying-approval" ? (
                <Button
                  onClick={management.finish}
                  type="button"
                  variant="outline"
                >
                  Done
                </Button>
              ) : null}
            </section>
          ) : null}
        </DialogContent>
      </Dialog>
    </>
  );
}
