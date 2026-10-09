import { z } from "zod";
import { normalizeRelayUrl } from "@/features/communities/relayProbe";

/** Versioned, deliberately narrow temporary owner workflow protocol. */
export const OWNER_TEST_SCHEMA = "rowvia.buzz.owner-test/v1";
const pubkey = z.string().regex(/^[0-9a-f]{64}$/);
const eventId = z.string().regex(/^[0-9a-f]{64}$/);
const channelId = z.string().uuid();
const base = {
  schema: z.literal(OWNER_TEST_SCHEMA),
  requestId: z.string().uuid(),
  expected: z.strictObject({
    ownerPubkey: pubkey,
    architectPubkey: pubkey,
    relayUrl: z
      .string()
      .max(2048)
      .url()
      .refine((url) => /^(?:https?|wss?):\/\//.test(url))
      .transform((url) => normalizeRelayUrl(url) ?? url),
  }),
};

/** Strict validation rejects extra arguments and unrestricted operations. */
export const ownerTestRequestSchema = z.discriminatedUnion("operation", [
  z.strictObject({
    ...base,
    operation: z.literal("status"),
    arguments: z.strictObject({}),
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

export type OwnerTestRequest = z.infer<typeof ownerTestRequestSchema>;
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
