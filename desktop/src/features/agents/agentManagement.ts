import type {
  AgentPersona,
  CreatePersonaInput,
  RespondToMode,
  UpdatePersonaInput,
} from "@/shared/api/types";

export const AGENT_MANAGEMENT_REQUEST = "agent_management_request" as const;

export type AgentManagementCreateRequest = {
  type: typeof AGENT_MANAGEMENT_REQUEST;
  action: "create";
  requestId: string;
  request: {
    channelId: string;
    displayName: string;
    systemPrompt: string;
  };
};

export type AgentManagementUpdateRequest = {
  type: typeof AGENT_MANAGEMENT_REQUEST;
  action: "update";
  requestId: string;
  request: {
    channelId: string;
    agentName: string;
    displayName?: string;
    systemPrompt?: string;
    runtime?: string;
    provider?: string;
    model?: string;
    respondTo?: RespondToMode;
  };
};

export type ConnectorDraftRequest = {
  type: typeof AGENT_MANAGEMENT_REQUEST;
  version: 1;
  action: "connector.grant" | "connector.revoke";
  requestId: string;
  request: {
    channelId: string;
    targetName: string;
    gmailLabels?: string[];
  };
};

export type AgentManagementRequest =
  | AgentManagementCreateRequest
  | AgentManagementUpdateRequest;

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const UUID_V4_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function isSelectionHint(value: unknown): value is string {
  if (
    typeof value !== "string" ||
    value.trim() !== value ||
    value.length === 0 ||
    [...value].length > 120
  )
    return false;
  return (
    !/\p{Cc}/u.test(value) &&
    !value.includes("://") &&
    !value.startsWith("www.") &&
    !value.includes("@") &&
    !UUID_PATTERN.test(value) &&
    !(value.length === 64 && /^[0-9a-f]+$/i.test(value))
  );
}

/** Parse only the versioned connector proposal. Values remain hints, never authority. */
export function parseConnectorDraftRequest(
  value: unknown,
): ConnectorDraftRequest | null {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    return null;
  const payload = value as Record<string, unknown>;
  if (
    !hasOnlyKeys(payload, [
      "type",
      "version",
      "action",
      "requestId",
      "request",
    ]) ||
    Object.keys(payload).length !== 5 ||
    payload.type !== AGENT_MANAGEMENT_REQUEST ||
    payload.version !== 1 ||
    (payload.action !== "connector.grant" &&
      payload.action !== "connector.revoke") ||
    typeof payload.requestId !== "string" ||
    !UUID_V4_PATTERN.test(payload.requestId) ||
    typeof payload.request !== "object" ||
    payload.request === null ||
    Array.isArray(payload.request)
  )
    return null;
  const request = payload.request as Record<string, unknown>;
  if (
    !hasOnlyKeys(request, ["channelId", "targetName", "gmailLabels"]) ||
    typeof request.channelId !== "string" ||
    !UUID_PATTERN.test(request.channelId) ||
    !isSelectionHint(request.targetName) ||
    (request.gmailLabels !== undefined &&
      (!Array.isArray(request.gmailLabels) ||
        request.gmailLabels.length < 1 ||
        request.gmailLabels.length > 4 ||
        !request.gmailLabels.every(isSelectionHint)))
  )
    return null;
  return {
    type: AGENT_MANAGEMENT_REQUEST,
    version: 1,
    action: payload.action,
    requestId: payload.requestId,
    request: {
      channelId: request.channelId,
      targetName: request.targetName,
      ...(request.gmailLabels === undefined
        ? {}
        : { gmailLabels: request.gmailLabels }),
    },
  };
}

function isText(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isRespondTo(value: unknown): value is RespondToMode | undefined {
  return value === undefined || value === "owner-only" || value === "anyone";
}

function hasOnlyKeys(
  value: Record<string, unknown>,
  allowed: readonly string[],
) {
  return Object.keys(value).every((key) => allowed.includes(key));
}

/** Parses only the deliberately narrow no-secret agent-management request contract. */
export function parseAgentManagementRequest(
  value: unknown,
): AgentManagementRequest | null {
  if (typeof value !== "object" || value === null) return null;
  const payload = value as Record<string, unknown>;
  if (
    payload.type !== AGENT_MANAGEMENT_REQUEST ||
    !isText(payload.requestId) ||
    (payload.action !== "create" && payload.action !== "update") ||
    typeof payload.request !== "object" ||
    payload.request === null
  ) {
    return null;
  }
  const request = payload.request as Record<string, unknown>;

  if (payload.action === "create") {
    if (!hasOnlyKeys(request, ["channelId", "displayName", "systemPrompt"])) {
      return null;
    }
    if (
      !isText(request.channelId) ||
      !isText(request.displayName) ||
      !isText(request.systemPrompt)
    ) {
      return null;
    }
    return {
      type: AGENT_MANAGEMENT_REQUEST,
      action: "create",
      requestId: payload.requestId,
      request: {
        channelId: request.channelId,
        displayName: request.displayName,
        systemPrompt: request.systemPrompt,
      },
    };
  }

  if (
    !isRespondTo(request.respondTo) ||
    !hasOnlyKeys(request, [
      "channelId",
      "agentName",
      "displayName",
      "systemPrompt",
      "runtime",
      "provider",
      "model",
      "respondTo",
    ]) ||
    !isText(request.channelId) ||
    !isText(request.agentName)
  ) {
    return null;
  }
  const changes = {
    ...(isText(request.displayName)
      ? { displayName: request.displayName }
      : {}),
    ...(isText(request.systemPrompt)
      ? { systemPrompt: request.systemPrompt }
      : {}),
    ...(isText(request.runtime) ? { runtime: request.runtime } : {}),
    ...(isText(request.provider) ? { provider: request.provider } : {}),
    ...(isText(request.model) ? { model: request.model } : {}),
    ...(request.respondTo ? { respondTo: request.respondTo } : {}),
  };
  if (Object.keys(changes).length === 0) return null;
  return {
    type: AGENT_MANAGEMENT_REQUEST,
    action: "update",
    requestId: payload.requestId,
    request: {
      channelId: request.channelId,
      agentName: request.agentName,
      ...changes,
    },
  };
}

export function requestTargetsEditablePersona(
  persona: AgentPersona | undefined,
): persona is AgentPersona {
  return Boolean(persona && !persona.sourceTeam);
}

export function createInputFromRequest(
  request: Extract<AgentManagementRequest, { action: "create" }>,
): CreatePersonaInput {
  return {
    displayName: request.request.displayName,
    systemPrompt: request.request.systemPrompt,
  };
}

/** Overlay an approved agent-requested edit without resetting unrequested behavior. */
export function updateInputFromRequest(
  request: Extract<AgentManagementRequest, { action: "update" }>,
  current: UpdatePersonaInput,
): UpdatePersonaInput {
  const changes = request.request;
  return {
    ...current,
    displayName: changes.displayName ?? current.displayName,
    systemPrompt: changes.systemPrompt ?? current.systemPrompt,
    runtime: changes.runtime ?? current.runtime,
    provider: changes.provider ?? current.provider,
    model: changes.model ?? current.model,
    ...(changes.respondTo
      ? {
          behavior: {
            respondTo: changes.respondTo,
            respondToAllowlist: [],
            parallelism: current.behavior?.parallelism,
            sessionPolicy: current.behavior?.sessionPolicy,
          },
        }
      : {}),
  };
}
