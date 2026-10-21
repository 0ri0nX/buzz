import { z } from "zod";
import { normalizeRelayUrl } from "@/features/communities/relayProbe";

/** Versioned, deliberately narrow temporary owner workflow protocol. */
export const OWNER_TEST_SCHEMA = "rowvia.buzz.owner-test/v1";
/** Public identities of the two existing pilot runtimes; test-only, not setup. */
export const OWNER_TEST_ENSURE_TARGET_PUBKEYS = [
  "9d3972d809d53ce8a87003ca7f631009963d9414739188cea6209ced88365c87",
  "fa17b69969fa097c40b105bfd85b383101b2e918827ce0f67958f7c7197da2dc",
] as const;
const pubkey = z.string().regex(/^[0-9a-f]{64}$/);
const eventId = z.string().regex(/^[0-9a-f]{64}$/);
const channelId = z.string().uuid();
const relayUrl = z
  .string()
  .max(2048)
  .url()
  .refine((url) => /^(?:https?|wss?):\/\//.test(url))
  .transform((url) => normalizeRelayUrl(url) ?? url);
const base = {
  schema: z.literal(OWNER_TEST_SCHEMA),
  requestId: z.string().uuid(),
  expected: z.strictObject({
    ownerPubkey: pubkey,
    architectPubkey: pubkey,
    relayUrl,
  }),
};

/** Public pairs from owner_test_saved_developer_core.rs and cerberusProposalUiDiagnostic.ts. */
export const OWNER_TEST_MESSAGE_TARGETS = {
  "31986c38-6676-4d6f-a4ba-ed833926d0d5": [
    "29cc1bb0ac338520c263dcfe654764224018a1ff5e5d3fc633be73f71f0c8c78",
    "01ab717b2ebeeef89c3abde0be5a55f6aa95fac66d89c96124083ad381857b4c",
  ],
  "fd0cc15d-c216-442f-bac2-837dd6fddbbd": [
    "9d3972d809d53ce8a87003ca7f631009963d9414739188cea6209ced88365c87",
  ],
} as const;

export const OWNER_TEST_CLAUDE_PUBKEY =
  "01ab717b2ebeeef89c3abde0be5a55f6aa95fac66d89c96124083ad381857b4c";
export const OWNER_TEST_CLAUDE_CHANNEL = "31986c38-6676-4d6f-a4ba-ed833926d0d5";

const pilotExpected = z.strictObject({
  ownerPubkey: z.literal(
    "89af092923ad49e3b9916b0c9f28c6b9237d8fcab4461cb7ddd12afe354f0d68",
  ),
  architectPubkey: z.literal(
    "05d1901b020205f04db1ee6c434d4cd69211af3dbc1d2a46e0e1e93f81f6dd20",
  ),
  relayUrl: relayUrl.refine((value) => value === "wss://buzz.rowvia.ai:8443"),
});
const pilotTargetFields = {
  channelId: z.enum([
    "31986c38-6676-4d6f-a4ba-ed833926d0d5",
    "fd0cc15d-c216-442f-bac2-837dd6fddbbd",
  ]),
  recipientPubkey: pubkey,
};

/** Rechecked immediately before sending, so time spent in the queue cannot authorize a late send. */
export function ownerTestMessageUnexpired(expiresAt: number): boolean {
  const now = Date.now();
  return expiresAt > now && expiresAt <= now + 120_000;
}

/** Strict validation rejects extra arguments and unrestricted operations. */
export const ownerTestRequestSchema = z.discriminatedUnion("operation", [
  z.strictObject({
    ...base,
    expected: pilotExpected,
    operation: z.literal("attach_saved_claude_developer"),
    arguments: z.strictObject({
      expiresAt: z
        .number()
        .int()
        .nonnegative()
        .refine(ownerTestMessageUnexpired),
    }),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("read_existing_test_channel"),
    expected: pilotExpected,
    arguments: z
      .strictObject({
        ...pilotTargetFields,
        limit: z.number().int().min(1).max(20),
        since: z.number().int().nonnegative().optional(),
      })
      .refine((value) =>
        (
          OWNER_TEST_MESSAGE_TARGETS[value.channelId] as readonly string[]
        ).includes(value.recipientPubkey),
      ),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("send_existing_test_message"),
    expected: pilotExpected,
    arguments: z
      .strictObject({
        ...pilotTargetFields,
        content: z
          .string()
          .max(1024)
          .startsWith("ROWVIA_E2E_")
          .refine((value) => new TextEncoder().encode(value).length <= 1024),
        // Absolute Unix milliseconds, checked again after the membership read.
        expiresAt: z
          .number()
          .int()
          .nonnegative()
          .refine(ownerTestMessageUnexpired),
      })
      .refine((value) =>
        (
          OWNER_TEST_MESSAGE_TARGETS[value.channelId] as readonly string[]
        ).includes(value.recipientPubkey),
      ),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("inspect_saved_connector"),
    arguments: z.strictObject({}),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("prepare_saved_connector"),
    arguments: z.strictObject({}),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("approve_saved_connector"),
    arguments: z.strictObject({ fingerprint: eventId }),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("inspect_saved_connector_operation"),
    arguments: z.strictObject({ fingerprint: eventId }),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("status"),
    arguments: z.strictObject({}),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("inspect_cerberus_proposal_ui"),
    arguments: z.strictObject({}),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("inspect_managed_agent_runtime"),
    arguments: z.strictObject({ pubkey, relayUrl }),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("ensure_managed_agent_runtime"),
    arguments: z.strictObject({
      pubkey: z.enum(OWNER_TEST_ENSURE_TARGET_PUBKEYS),
      relayUrl,
    }),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("create_private_stream"),
    arguments: z.strictObject({
      name: z
        .string()
        .min(12)
        .max(80)
        .regex(/^rowvia-e2e-[a-zA-Z0-9_-]+$/),
    }),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("add_architect"),
    arguments: z.strictObject({ channelId }),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("send_message"),
    arguments: z
      .strictObject({
        channelId,
        content: z
          .string()
          .min(1)
          .max(4096)
          .refine((value) => value.includes("ROWVIA_E2E_")),
        parentEventId: eventId.optional(),
        rootEventId: eventId.optional(),
      })
      .refine(
        (value) =>
          value.rootEventId === undefined || value.parentEventId !== undefined,
      ),
  }),
  z.strictObject({
    ...base,
    operation: z.literal("read_channel"),
    arguments: z.strictObject({
      channelId,
      limit: z.number().int().min(1).max(50),
      since: z.number().int().nonnegative().optional(),
    }),
  }),
]);

/** Safe runtime projection; native errors, log paths and setup are omitted. */
export const ownerTestRuntimeProjectionSchema = z.object({
  pubkey,
  // The native registry owns canonical pair identity: never normalize results.
  relayUrl: z.string().max(2048),
  lifecycle: z.enum([
    "starting",
    "listening",
    "waking",
    "ready",
    "failed",
    "stopped",
  ]),
  pid: z.number().int().min(1).max(4_294_967_295).nullable(),
});

export type OwnerTestRequest = z.infer<typeof ownerTestRequestSchema>;
/** Bounded current cache/ref observations, never a past rejection diagnosis. */
export const cerberusProposalUiSnapshotSchema = z.strictObject({
  hookMounted: z.literal(true),
  knownAgentCached: z.boolean().nullable(),
  locallyManagedCached: z.boolean().nullable(),
  relayOwnedCached: z.boolean().nullable(),
  channelCached: z.boolean().nullable(),
  ownerMemberCached: z.boolean().nullable(),
  agentMemberCached: z.boolean().nullable(),
  dialogRequested: z.boolean(),
  pendingSameTarget: z.boolean().nullable(),
  managementSeenCount: z.number().int().min(0).max(100),
  managementBufferedCount: z.number().int().min(0).max(100),
});
export type CerberusProposalUiSnapshot = z.infer<
  typeof cerberusProposalUiSnapshotSchema
>;
export type OwnerTestCode =
  | "invalid_request"
  | "duplicate_request"
  | "scope_mismatch"
  | "not_ready"
  | "channel_not_owned"
  | "membership_failed"
  | "operation_failed"
  | "capacity_exceeded"
  | "response_too_large";
export type OwnerTestNativeCode =
  | OwnerTestCode
  | "timeout"
  | "busy"
  | "cache_full"
  | "invalid_response"
  | "hook_disabled"
  | "shutdown";
export type OwnerTestResponse = {
  schema: typeof OWNER_TEST_SCHEMA;
  requestId: string;
  status: "ok" | "error" | "unknown";
  result?: object;
  code?: OwnerTestNativeCode;
};
