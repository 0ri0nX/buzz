import { listen } from "@tauri-apps/api/event";
import { normalizeRelayUrl } from "@/features/communities/relayProbe";
import { messageMentionPubkeys } from "@/features/messages/lib/messageMentionPubkeys";
import { relayClient } from "@/shared/api/relayClient";
import {
  addChannelMembers,
  getRelayWsUrl,
  invokeTauri,
} from "@/shared/api/tauri";
import { createChannel } from "@/shared/api/tauriChannels";
import { getIdentity } from "@/shared/api/tauriIdentity";
import { sendChannelMessage } from "@/shared/api/tauriMessages";
import type { Channel } from "@/shared/api/types";
import {
  OWNER_TEST_SCHEMA,
  ownerTestRequestSchema,
  type OwnerTestCode,
  type OwnerTestRequest,
  type OwnerTestResponse,
} from "./ownerTestHookProtocol";

class HookFailure extends Error {
  readonly code: OwnerTestCode;
  constructor(code: OwnerTestCode) {
    super(code);
    this.code = code;
  }
}

/** A session-local allowlist; only hook-created private streams are accessible. */
export function createOwnerTestHandler() {
  const seen = new Set<string>();
  // At most one successful send per accepted request (the ledger caps at 128).
  // Only native-confirmed sends in this handler's lifetime establish targets.
  const sentEvents = new Map<
    string,
    { channelId: string; rootEventId: string }
  >();
  const channels = new Map<
    string,
    { channel: Channel; expected: string; added: boolean }
  >();
  let boundScope: string | null = null;
  let fenced = false;
  let queue = Promise.resolve();
  let queued = 0;
  const fence = () => {
    fenced = true;
    channels.clear();
    sentEvents.clear();
  };
  const dispose = relayClient.subscribeToConnectionState((state) => {
    if (boundScope !== null && (state === "idle" || state === "disconnected")) {
      fence();
    }
  });

  async function assertScope(request: OwnerTestRequest) {
    const scope = JSON.stringify(request.expected);
    if (fenced || (boundScope !== null && scope !== boundScope))
      throw new HookFailure("scope_mismatch");
    const identity = await getIdentity();
    const relayUrl = await getRelayWsUrl();
    if (
      fenced ||
      identity.pubkey !== request.expected.ownerPubkey ||
      normalizeRelayUrl(relayUrl) !== request.expected.relayUrl
    ) {
      if (boundScope !== null) fence();
      throw new HookFailure("scope_mismatch");
    }
    if (identity.locked || identity.lost || identity.resetFailed)
      throw new HookFailure("not_ready");
    boundScope = scope;
  }

  async function execute(
    request: OwnerTestRequest,
    markMutation: () => void,
  ): Promise<object> {
    await assertScope(request);
    if (request.operation === "status")
      return {
        ownerPubkey: request.expected.ownerPubkey,
        relayUrl: request.expected.relayUrl,
        ready: relayClient.getConnectionState() === "connected",
      };
    if (relayClient.getConnectionState() !== "connected")
      throw new HookFailure("not_ready");
    if (request.expected.ownerPubkey === request.expected.architectPubkey)
      throw new HookFailure("invalid_request");
    if (request.operation === "create_private_stream") {
      if (channels.size >= 32) throw new HookFailure("capacity_exceeded");
      const input = {
        name: request.arguments.name,
        channelType: "stream" as const,
        visibility: "private" as const,
        expectedRelayUrl: request.expected.relayUrl,
        expectedSignerPubkey: request.expected.ownerPubkey,
      };
      markMutation();
      const channel = await createChannel(input);
      await assertScope(request);
      if (
        !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
          channel.id,
        ) ||
        channel.channelType !== "stream" ||
        channel.visibility !== "private" ||
        channel.name !== input.name
      )
        throw new HookFailure("operation_failed");
      channels.set(channel.id, {
        channel,
        expected: JSON.stringify(request.expected),
        added: false,
      });
      return { channelId: channel.id };
    }
    const entry = channels.get(request.arguments.channelId);
    if (!entry || entry.expected !== JSON.stringify(request.expected))
      throw new HookFailure("channel_not_owned");
    if (request.operation === "add_architect") {
      markMutation();
      const result = await addChannelMembers({
        channelId: entry.channel.id,
        pubkeys: [request.expected.architectPubkey],
        role: "member",
        expectedRelayUrl: request.expected.relayUrl,
        expectedSignerPubkey: request.expected.ownerPubkey,
      });
      await assertScope(request);
      if (
        result.errors.length > 0 ||
        !result.added.includes(request.expected.architectPubkey)
      )
        throw new HookFailure("membership_failed");
      entry.added = true;
      return { added: true };
    }
    if (request.operation === "send_message") {
      if (!entry.added) throw new HookFailure("membership_failed");
      const parentEventId = request.arguments.parentEventId;
      let rootEventId: string | undefined;
      if (parentEventId !== undefined) {
        const parent = sentEvents.get(parentEventId);
        if (!parent || parent.channelId !== entry.channel.id)
          throw new HookFailure("invalid_request");
        rootEventId = parent.rootEventId;
        const root = sentEvents.get(rootEventId);
        if (
          !root ||
          root.channelId !== entry.channel.id ||
          root.rootEventId !== rootEventId ||
          (request.arguments.rootEventId !== undefined &&
            request.arguments.rootEventId !== rootEventId)
        )
          throw new HookFailure("invalid_request");
      }
      const recipients = messageMentionPubkeys(
        entry.channel,
        request.expected.ownerPubkey,
        [request.expected.architectPubkey],
      );
      if (
        recipients.length !== 1 ||
        recipients[0] !== request.expected.architectPubkey
      )
        throw new HookFailure("invalid_request");
      await assertScope(request);
      markMutation();
      const result = await sendChannelMessage(
        entry.channel.id,
        request.arguments.content,
        parentEventId ?? null,
        undefined,
        recipients,
        undefined,
        undefined,
        undefined,
        undefined,
        undefined,
        request.expected.relayUrl,
        request.expected.ownerPubkey,
        rootEventId,
      );
      await assertScope(request);
      if (
        typeof result.eventId !== "string" ||
        !/^[0-9a-f]{64}$/.test(result.eventId) ||
        sentEvents.has(result.eventId) ||
        result.parentEventId !== (parentEventId ?? null) ||
        result.rootEventId !== (rootEventId ?? null)
      )
        throw new HookFailure("operation_failed");
      sentEvents.set(result.eventId, {
        channelId: entry.channel.id,
        rootEventId: rootEventId ?? result.eventId,
      });
      return { eventId: result.eventId };
    }
    const events = await relayClient.fetchEvents(
      {
        kinds: [9],
        "#h": [entry.channel.id],
        limit: request.arguments.limit,
        ...(request.arguments.since === undefined
          ? {}
          : { since: request.arguments.since }),
      },
      {
        ownerPubkey: request.expected.ownerPubkey,
        relayUrl: request.expected.relayUrl,
      },
    );
    await assertScope(request);
    const bounded = events.slice(0, request.arguments.limit);
    if (
      bounded.some(
        (event) =>
          event.kind !== 9 ||
          !event.tags.some(
            (tag) => tag[0] === "h" && tag[1] === entry.channel.id,
          ),
      )
    )
      throw new HookFailure("operation_failed");
    if (
      bounded.some(
        (event) =>
          new TextEncoder().encode(JSON.stringify(event)).length > 16_384,
      ) ||
      new TextEncoder().encode(JSON.stringify(bounded)).length > 245_760
    )
      throw new HookFailure("response_too_large");
    return { events: bounded };
  }

  async function handle(payload: unknown): Promise<OwnerTestResponse> {
    const parsed = ownerTestRequestSchema.safeParse(payload);
    const candidateId =
      typeof payload === "object" && payload !== null && "requestId" in payload
        ? payload.requestId
        : undefined;
    const requestId =
      typeof candidateId === "string" && /^[0-9a-f-]{36}$/i.test(candidateId)
        ? candidateId
        : "00000000-0000-0000-0000-000000000000";
    const response = { schema: OWNER_TEST_SCHEMA, requestId } as const;
    if (!parsed.success)
      return { ...response, status: "error", code: "invalid_request" };
    if (seen.has(requestId))
      return { ...response, status: "error", code: "duplicate_request" };
    if (seen.size >= 128)
      return { ...response, status: "error", code: "capacity_exceeded" };
    seen.add(requestId);
    let mutationStarted = false;
    try {
      return {
        ...response,
        status: "ok",
        result: await execute(parsed.data, () => {
          mutationStarted = true;
        }),
      };
    } catch (error) {
      return {
        ...response,
        status:
          !mutationStarted && error instanceof HookFailure
            ? "error"
            : "unknown",
        code: error instanceof HookFailure ? error.code : "operation_failed",
      };
    }
  }

  const handler = (payload: unknown): Promise<OwnerTestResponse> => {
    if (queued >= 16) {
      const parsed = ownerTestRequestSchema.safeParse(payload);
      return Promise.resolve({
        schema: OWNER_TEST_SCHEMA,
        requestId: parsed.success
          ? parsed.data.requestId
          : "00000000-0000-0000-0000-000000000000",
        status: "error",
        code: "capacity_exceeded",
      });
    }
    queued++;
    const pending = queue.then(() => handle(payload));
    queue = pending.then(
      () => undefined,
      () => undefined,
    );
    return pending.finally(() => {
      queued--;
    });
  };
  return Object.assign(handler, { dispose, fence });
}

/** Installed only through the explicitly opted-in build's dynamic import. */
export async function installOwnerTestHook() {
  const handle = createOwnerTestHandler();
  const unlisten = await listen<unknown>(
    "rowvia-owner-test-request",
    async ({ payload }) => {
      const response = await handle(payload);
      try {
        await invokeTauri("rowvia_owner_test_reply", { response });
      } catch (error) {
        handle.fence();
        throw error;
      }
    },
  );
  return () => {
    unlisten();
    handle.dispose();
  };
}
