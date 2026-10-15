import assert from "node:assert/strict";
import { afterEach, test } from "node:test";
import {
  CERBERUS_PROPOSAL_AGENT as agent,
  CERBERUS_PROPOSAL_CHANNEL as channel,
  inspectCerberusProposalUi,
  registerCerberusProposalUiDiagnostic,
  resetCerberusProposalUiDiagnostic,
} from "./cerberusProposalUiDiagnostic.ts";
import { cerberusProposalUiSnapshotSchema } from "./ownerTestHookProtocol.ts";

const expected = {
  ownerPubkey: "a".repeat(64),
  relayUrl: "wss://example.test",
};
const state = (changes = {}) => ({
  ...expected,
  managedAgents: undefined,
  relayAgents: undefined,
  channels: undefined,
  pending: false,
  sourceAgentPubkey: null,
  requestChannelId: undefined,
  seenCount: 0,
  bufferedCount: 0,
  ...changes,
});
afterEach(resetCerberusProposalUiDiagnostic);

test("unknown cache stays unknown; output contains only bounded closed fields", () => {
  registerCerberusProposalUiDiagnostic(() => state());
  const result = inspectCerberusProposalUi(expected);
  assert.deepEqual(result, {
    hookMounted: true,
    knownAgentCached: null,
    locallyManagedCached: null,
    relayOwnedCached: null,
    channelCached: null,
    ownerMemberCached: null,
    agentMemberCached: null,
    dialogRequested: false,
    pendingSameTarget: false,
    managementSeenCount: 0,
    managementBufferedCount: 0,
  });
  assert.equal(
    cerberusProposalUiSnapshotSchema.safeParse(result).success,
    true,
  );
});

test("fixed target cache facts do not infer absence from unavailable data", () => {
  for (const local of [undefined, [], [{ pubkey: agent.toUpperCase() }]]) {
    for (const remote of [
      undefined,
      [],
      [{ pubkey: agent, ownerPubkey: null }],
      [{ pubkey: agent, ownerPubkey: expected.ownerPubkey }],
      [{ pubkey: agent, ownerPubkey: "b".repeat(64) }],
    ]) {
      registerCerberusProposalUiDiagnostic(() =>
        state({ managedAgents: local, relayAgents: remote }),
      );
      const result = inspectCerberusProposalUi(expected);
      assert.equal(
        result.locallyManagedCached,
        local ? local.length > 0 : null,
      );
      assert.equal(
        result.knownAgentCached,
        local?.length || remote?.length ? true : local && remote ? false : null,
      );
      assert.equal(
        result.relayOwnedCached,
        remote?.[0]?.ownerPubkey
          ? remote[0].ownerPubkey === expected.ownerPubkey
          : null,
      );
    }
  }
});

test("channel membership, requested target and capped counts describe current refs only", () => {
  for (const isMember of [true, false]) {
    for (const memberPubkeys of [[], [agent.toUpperCase()]]) {
      for (const requestChannelId of [undefined, channel, "other-channel"]) {
        registerCerberusProposalUiDiagnostic(() =>
          state({
            channels: [{ id: channel, isMember, memberPubkeys }],
            pending: true,
            sourceAgentPubkey: agent,
            requestChannelId,
            seenCount: 10000,
            bufferedCount: 101,
          }),
        );
        const result = inspectCerberusProposalUi(expected);
        assert.equal(result.channelCached, true);
        assert.equal(result.ownerMemberCached, isMember);
        assert.equal(result.agentMemberCached, memberPubkeys.length > 0);
        assert.equal(result.dialogRequested, true);
        assert.equal(
          result.pendingSameTarget,
          requestChannelId === undefined ? null : requestChannelId === channel,
        );
        assert.equal(result.managementSeenCount, 100);
        assert.equal(result.managementBufferedCount, 100);
        assert.equal(
          cerberusProposalUiSnapshotSchema.safeParse(result).success,
          true,
        );
      }
    }
  }
  registerCerberusProposalUiDiagnostic(() => state({ channels: [] }));
  assert.equal(inspectCerberusProposalUi(expected).channelCached, false);
  assert.equal(inspectCerberusProposalUi(expected).ownerMemberCached, null);
  registerCerberusProposalUiDiagnostic(() =>
    state({ pending: true, requestChannelId: channel }),
  );
  assert.equal(inspectCerberusProposalUi(expected).pendingSameTarget, null);
});

test("unknown/wrong scope, unmounted, cleanup and community reset fail closed", () => {
  assert.equal(inspectCerberusProposalUi(expected), null);
  for (const change of [
    { ownerPubkey: undefined },
    { relayUrl: undefined },
    { ownerPubkey: "b".repeat(64) },
    { relayUrl: "wss://foreign.test" },
  ]) {
    registerCerberusProposalUiDiagnostic(() => state(change));
    assert.equal(inspectCerberusProposalUi(expected), null);
  }
  const disposeOld = registerCerberusProposalUiDiagnostic(() => state());
  const disposeNew = registerCerberusProposalUiDiagnostic(() => state());
  disposeOld();
  assert.equal(inspectCerberusProposalUi(expected).hookMounted, true);
  disposeNew();
  assert.equal(inspectCerberusProposalUi(expected), null);
  registerCerberusProposalUiDiagnostic(() => state());
  resetCerberusProposalUiDiagnostic();
  assert.equal(inspectCerberusProposalUi(expected), null);
});
