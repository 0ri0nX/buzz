import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useCommunities } from "@/features/communities/useCommunities";
import { useIdentityQuery } from "@/shared/api/hooks";
import {
  approveSavedAgentRegistration,
  listSavedAgentPending,
  reviewSavedAgentPreparation,
  saveCodexAgent,
  type SavedAgentPending,
  type SavedAgentReview,
} from "@/shared/api/rowviaSavedAgent";
import { startManagedAgent } from "@/shared/api/tauriManagedAgents";
import { addChannelMembers } from "@/shared/api/tauri";
import { managedAgentsQueryKey } from "./hooks";

type Phase =
  | "loading"
  | "pending"
  | "unavailable"
  | "authorizing"
  | "authorized"
  | "expired"
  | "revoked"
  | "unknown"
  | "registered-stopped"
  | "starting"
  | "started";
type ReviewState = Readonly<{
  phase: Phase;
  review?: SavedAgentReview;
  operationId?: string;
  error?: string;
}>;

/** Ephemeral owner view only. Native owns recovery; no browser approval store. */
export function useSavedAgentRegistration() {
  const queryClient = useQueryClient();
  const identity = useIdentityQuery();
  const { activeCommunity } = useCommunities();
  const owner = identity.data?.pubkey;
  const relay = activeCommunity?.relayUrl;
  const scope = `${owner ?? ""}\n${relay ?? ""}`;
  const scopeRef = React.useRef(scope);
  scopeRef.current = scope;
  const epoch = React.useRef(0);
  const busy = React.useRef(false);
  const attempted = React.useRef(new Set<string>());
  const saveAttempted = React.useRef(false);
  const savedOperation = React.useRef<string | null>(null);
  const pendingRead = React.useRef(0);
  const [state, setState] = React.useState<ReviewState | null>(null);
  const [pending, setPending] = React.useState<readonly SavedAgentPending[]>(
    [],
  );

  React.useEffect(() => {
    const token = ++epoch.current;
    const read = ++pendingRead.current;
    busy.current = false;
    attempted.current.clear();
    saveAttempted.current = false;
    savedOperation.current = null;
    setState(null);
    setPending([]);
    if (owner && relay) {
      void listSavedAgentPending()
        .then((items) => {
          if (
            epoch.current === token &&
            scopeRef.current === scope &&
            pendingRead.current === read
          )
            setPending(items);
        })
        .catch(() => {
          /* A missing installation never offers a generic fallback. */
        });
    }
    return () => {
      epoch.current++;
    };
  }, [owner, relay, scope]);

  function current(token: number, captured: string) {
    return epoch.current === token && scopeRef.current === captured;
  }

  async function open(operationId: string) {
    if (busy.current || !owner || !relay) return;
    busy.current = true;
    const token = epoch.current;
    const captured = scope;
    setState({ phase: "loading", operationId });
    try {
      const review = await reviewSavedAgentPreparation(operationId);
      if (current(token, captured)) {
        if (
          review.ownerPubkey !== owner ||
          review.relayUrl !== relay ||
          review.recipeId !== "codex-development-memory-git-read-v1"
        )
          throw new Error("scope");
        // A reviewed Native operation remains recoverable after closing its view.
        pendingRead.current++;
        setPending((items) => [
          ...items.filter((item) => item.operationId !== operationId),
          {
            version: 1,
            operationId,
            agentPubkey: review.agentPubkey,
            name: review.name,
            status: "pending",
          },
        ]);
        setState({
          phase: attempted.current.has(operationId) ? "unknown" : "pending",
          operationId,
          review,
        });
      }
    } catch {
      if (current(token, captured))
        setState({ phase: "unavailable", operationId });
    } finally {
      if (current(token, captured)) busy.current = false;
    }
  }

  async function save(
    displayName: string,
    systemPrompt: string,
    channelId: string,
  ): Promise<boolean> {
    if (busy.current || saveAttempted.current || !owner || !relay) return false;
    busy.current = true;
    saveAttempted.current = true;
    savedOperation.current = null;
    const token = epoch.current;
    const captured = scope;
    setState({ phase: "loading" });
    try {
      const result = await saveCodexAgent({
        displayName,
        systemPrompt,
        channelId,
        recipe: "codex-development-memory-git-read-v1",
      });
      if (!current(token, captured)) return false;
      savedOperation.current = result.operationId;
      const read = ++pendingRead.current;
      try {
        const items = await listSavedAgentPending();
        if (current(token, captured) && pendingRead.current === read)
          setPending(items);
      } catch {
        // A failed inventory read does not change the known Save outcome.
      }
      if (!current(token, captured)) return false;
      busy.current = false;
      await open(result.operationId);
      return true;
    } catch (error: unknown) {
      if (current(token, captured)) {
        if (
          error instanceof Error &&
          error.message === "saved_agent_owner_unavailable"
        )
          saveAttempted.current = false;
        setState({
          phase:
            error instanceof Error &&
            error.message === "saved_agent_owner_unavailable"
              ? "unavailable"
              : "unknown",
        });
      }
      return false;
    } finally {
      if (current(token, captured)) busy.current = false;
    }
  }

  async function approve() {
    const review = state?.review;
    if (
      busy.current ||
      state?.phase !== "pending" ||
      !review ||
      attempted.current.has(review.operationId) ||
      review.ownerPubkey !== owner ||
      review.relayUrl !== relay
    )
      return;
    busy.current = true;
    attempted.current.add(review.operationId);
    const token = epoch.current;
    const captured = scope;
    setState({ phase: "authorizing", review, operationId: review.operationId });
    try {
      const result = await approveSavedAgentRegistration(review);
      if (!current(token, captured)) return;
      if (result.kind === "authorization") {
        const phase =
          result.authorization.state === "authorized" &&
          Date.parse(result.authorization.expiresAt) <= Date.now()
            ? "expired"
            : result.authorization.state;
        setState({ phase, review, operationId: review.operationId });
      } else {
        // An unrelated completion must not release an unknown Save attempt.
        if (savedOperation.current === review.operationId) {
          saveAttempted.current = false;
          savedOperation.current = null;
        }
        pendingRead.current++;
        setPending((items) =>
          items.filter(
            (item) => item.operationId !== result.completion.operationId,
          ),
        );
        setState({
          phase: "registered-stopped",
          review,
          operationId: result.completion.operationId,
        });
        await queryClient.invalidateQueries({
          queryKey: managedAgentsQueryKey,
        });
      }
    } catch (error: unknown) {
      if (current(token, captured))
        setState({
          phase:
            error instanceof Error &&
            error.message === "saved_agent_owner_unavailable"
              ? "unavailable"
              : "unknown",
          review,
          operationId: review.operationId,
        });
    } finally {
      if (current(token, captured)) busy.current = false;
    }
  }

  async function start() {
    const review = state?.review;
    if (
      busy.current ||
      state?.phase !== "registered-stopped" ||
      !review ||
      review.ownerPubkey !== owner ||
      review.relayUrl !== relay
    )
      return;
    busy.current = true;
    const token = epoch.current;
    const captured = scope;
    setState({ ...state, phase: "starting", error: undefined });
    try {
      try {
        const membership = await addChannelMembers({
          channelId: review.channelId,
          pubkeys: [review.agentPubkey],
          role: "bot",
          expectedSignerPubkey: owner,
          expectedRelayUrl: relay,
        });
        if (
          !membership.added.includes(review.agentPubkey) ||
          membership.errors.some((item) => item.pubkey === review.agentPubkey)
        )
          throw new Error("channel_attachment_failed");
      } catch {
        if (current(token, captured))
          setState({
            ...state,
            phase: "registered-stopped",
            error:
              "Could not add this agent to the reviewed channel. The agent is still stopped. Try Start again.",
          });
        return;
      }
      if (!current(token, captured)) return;
      await startManagedAgent(review.agentPubkey, {
        expectedSignerPubkey: owner,
        expectedRelayUrl: relay,
      });
      if (current(token, captured)) {
        setState({ ...state, phase: "started", error: undefined });
        await queryClient.invalidateQueries({
          queryKey: managedAgentsQueryKey,
        });
      }
    } catch {
      if (current(token, captured))
        setState({ ...state, phase: "unknown", error: undefined });
    } finally {
      if (current(token, captured)) busy.current = false;
    }
  }

  function dismiss() {
    // Cancel only discards this view. In-flight outcomes remain Native-owned.
    epoch.current++;
    busy.current = false;
    setState(null);
  }
  return { state, pending, open, save, approve, start, dismiss };
}
