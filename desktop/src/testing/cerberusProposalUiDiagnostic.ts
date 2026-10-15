import { normalizeRelayUrl } from "@/features/communities/relayProbe";
import type { Channel, ManagedAgent, RelayAgent } from "@/shared/api/types";
import type { CerberusProposalUiSnapshot } from "./ownerTestHookProtocol";

/** Fixed public target of the temporary, read-only owner diagnostic. */
export const CERBERUS_PROPOSAL_AGENT =
  "9d3972d809d53ce8a87003ca7f631009963d9414739188cea6209ced88365c87";
export const CERBERUS_PROPOSAL_CHANNEL = "fd0cc15d-c216-442f-bac2-837dd6fddbbd";

type CachedState = {
  ownerPubkey: string | undefined;
  relayUrl: string | undefined;
  managedAgents: readonly Pick<ManagedAgent, "pubkey">[] | undefined;
  relayAgents:
    | readonly Pick<RelayAgent, "pubkey" | "ownerPubkey">[]
    | undefined;
  channels:
    | readonly Pick<Channel, "id" | "isMember" | "memberPubkeys">[]
    | undefined;
  pending: boolean;
  sourceAgentPubkey: string | null;
  requestChannelId: string | undefined;
  seenCount: number;
  bufferedCount: number;
};

let registration: { read: () => CachedState } | null = null;

/** Register only from the opted-in hook; cleanup cannot clear a newer mount. */
export function registerCerberusProposalUiDiagnostic(read: () => CachedState) {
  const entry = { read };
  registration = entry;
  return () => {
    if (registration === entry) registration = null;
  };
}

/** Clear the getter before a community transition can expose previous refs. */
export function resetCerberusProposalUiDiagnostic() {
  registration = null;
}

/** Read current refs/cache only. Null means unmounted or unknown/mismatched scope. */
export function inspectCerberusProposalUi(expected: {
  ownerPubkey: string;
  relayUrl: string;
}): CerberusProposalUiSnapshot | null {
  if (!registration) return null;
  const state = registration.read();
  if (
    state.ownerPubkey !== expected.ownerPubkey ||
    !state.relayUrl ||
    normalizeRelayUrl(state.relayUrl) !== expected.relayUrl
  )
    return null;
  const local =
    state.managedAgents?.some(
      (agent) => agent.pubkey.toLowerCase() === CERBERUS_PROPOSAL_AGENT,
    ) ?? null;
  const remote = state.relayAgents?.find(
    (agent) => agent.pubkey.toLowerCase() === CERBERUS_PROPOSAL_AGENT,
  );
  const channel = state.channels?.find(
    (candidate) => candidate.id === CERBERUS_PROPOSAL_CHANNEL,
  );
  const bounded = (count: number) =>
    Math.min(100, Math.max(0, Math.floor(count)));
  return {
    hookMounted: true,
    knownAgentCached:
      local === true || remote !== undefined
        ? true
        : state.managedAgents && state.relayAgents
          ? false
          : null,
    locallyManagedCached: local,
    relayOwnedCached: remote?.ownerPubkey
      ? remote.ownerPubkey.toLowerCase() === expected.ownerPubkey
      : null,
    channelCached: state.channels ? channel !== undefined : null,
    ownerMemberCached: channel?.isMember ?? null,
    agentMemberCached:
      channel?.memberPubkeys.some(
        (pubkey) => pubkey.toLowerCase() === CERBERUS_PROPOSAL_AGENT,
      ) ?? null,
    dialogRequested: state.pending,
    pendingSameTarget: state.pending
      ? state.requestChannelId === undefined || state.sourceAgentPubkey === null
        ? null
        : state.sourceAgentPubkey?.toLowerCase() === CERBERUS_PROPOSAL_AGENT &&
          state.requestChannelId === CERBERUS_PROPOSAL_CHANNEL
      : false,
    managementSeenCount: bounded(state.seenCount),
    managementBufferedCount: bounded(state.bufferedCount),
  };
}
