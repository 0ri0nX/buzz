import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import {
  canonical21Fields,
  canonicalRegistration,
} from "@/shared/api/rowviaSavedAgentContract";
import type { useSavedAgentRegistration } from "../useSavedAgentRegistration";

type Props = Readonly<{
  registration: ReturnType<typeof useSavedAgentRegistration>;
}>;
const statusText = {
  loading: "Loading registration review…",
  pending: "Pending owner approval",
  unavailable: "Saved Codex registration is unavailable.",
  authorizing: "Authorizing registration…",
  authorized: "Authorized. Registration has not completed.",
  expired: "Authorization expired. Registration has not completed.",
  revoked: "Authorization revoked. Registration has not completed.",
  unknown:
    "Outcome unknown. Do not repeat approval or Save. Reconcile the Native operation before continuing.",
  "registered-stopped": "Registered · Stopped",
  starting: "Starting…",
  started: "Start request completed.",
} as const;

/** Exact public review, with explicit approval and a separate exact-key Start. */
export function SavedAgentRegistrationReviewDialog({ registration }: Props) {
  const { state } = registration;
  const review = state?.review;
  const fields = review
    ? canonicalRegistration(review.canonical21, review)
    : null;
  return (
    <>
      {!state && registration.pending.length > 0 ? (
        <div className="fixed bottom-5 right-5 z-40 flex flex-col gap-2">
          {registration.pending.map((item) => (
            <Button
              key={item.operationId}
              type="button"
              variant="outline"
              onClick={() => void registration.open(item.operationId)}
            >
              Review saved Codex: {item.name}
            </Button>
          ))}
        </div>
      ) : null}
      <Dialog
        open={state !== null}
        onOpenChange={(open) => {
          if (!open) registration.dismiss();
        }}
      >
        <DialogContent className="max-h-[90vh] max-w-2xl overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Saved Codex registration</DialogTitle>
            <DialogDescription>
              Inspect the exact public registration commitment before approving.
              Registration keeps this agent stopped.
            </DialogDescription>
          </DialogHeader>
          <p role="status">{state ? statusText[state.phase] : ""}</p>
          {state?.error ? <p role="alert">{state.error}</p> : null}
          {review && fields ? (
            <>
              <dl className="space-y-2 text-sm">
                {[
                  ["Display name", review.name],
                  ["Agent instructions", review.systemPrompt],
                  ["Owner", review.ownerPubkey],
                  ["Community", review.relayUrl],
                  ["Channel", review.channelId],
                  ["Recipe", review.recipeId],
                  ["Model", review.model],
                  ["Recipe SHA-256", review.recipeSha256],
                  ["Canonical registration SHA-256", review.canonical21Sha256],
                ].map(([label, value]) => (
                  <div key={label}>
                    <dt className="font-medium">{label}</dt>
                    <dd className="whitespace-pre-wrap break-all">{value}</dd>
                  </div>
                ))}
                {canonical21Fields.map((field) => (
                  <div key={field}>
                    <dt className="font-medium">{field}</dt>
                    <dd className="whitespace-pre-wrap break-all">
                      {Array.isArray(fields[field])
                        ? fields[field].join(", ")
                        : String(fields[field])}
                    </dd>
                  </div>
                ))}
              </dl>
              <details>
                <summary className="cursor-pointer text-sm">
                  Original canonical registration JSON
                </summary>
                <pre className="whitespace-pre-wrap break-all text-xs">
                  {review.canonical21}
                </pre>
              </details>
            </>
          ) : null}
          <div className="flex flex-wrap justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={registration.dismiss}
            >
              Cancel
            </Button>
            {state?.phase === "pending" ? (
              <Button type="button" onClick={() => void registration.approve()}>
                Approve registration
              </Button>
            ) : null}
            {state?.phase === "registered-stopped" ? (
              <Button type="button" onClick={() => void registration.start()}>
                Start
              </Button>
            ) : null}
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
