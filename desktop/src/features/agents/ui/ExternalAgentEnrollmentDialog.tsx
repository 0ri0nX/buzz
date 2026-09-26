import * as React from "react";

import {
  useAddChannelMembersMutation,
  useChannelsQuery,
} from "@/features/channels/hooks";
import {
  completeExternalAgentEnrollment,
  getRelayWsUrl,
  prepareExternalAgentEnrollment,
  type ExternalAgentAuthorization,
  type ExternalAgentChallenge,
} from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";

export function ExternalAgentEnrollmentDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [name, setName] = React.useState("");
  const [agentPubkey, setAgentPubkey] = React.useState("");
  const [challenge, setChallenge] =
    React.useState<ExternalAgentChallenge | null>(null);
  const [proofEventJson, setProofEventJson] = React.useState("");
  const [authorization, setAuthorization] =
    React.useState<ExternalAgentAuthorization | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [pending, setPending] = React.useState(false);
  const [channelId, setChannelId] = React.useState("");
  const [memberAdded, setMemberAdded] = React.useState(false);
  const channelsQuery = useChannelsQuery({
    enabled: open && Boolean(authorization),
  });
  const addMembers = useAddChannelMembersMutation(null);
  const streams = (channelsQuery.data ?? []).filter(
    (channel) =>
      channel.channelType === "stream" &&
      !channel.archivedAt &&
      (channel.visibility === "open" || channel.isMember),
  );
  const selectedStream = streams.find((channel) => channel.id === channelId);
  const alreadyMember = selectedStream?.memberPubkeys.some(
    (pubkey) => pubkey.toLowerCase() === authorization?.agentPubkey,
  );

  React.useEffect(() => {
    if (!open) return;
    setName("");
    setAgentPubkey("");
    setChallenge(null);
    setProofEventJson("");
    setAuthorization(null);
    setError(null);
    setPending(false);
    setChannelId("");
    setMemberAdded(false);
  }, [open]);

  async function prepare() {
    setPending(true);
    setError(null);
    setChallenge(null);
    try {
      setChallenge(await prepareExternalAgentEnrollment(agentPubkey));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPending(false);
    }
  }

  async function addToStream() {
    if (!authorization || !selectedStream || alreadyMember) return;
    setPending(true);
    setError(null);
    try {
      if ((await getRelayWsUrl()) !== authorization.relayUrl) {
        throw new Error("Workspace changed since agent authorization.");
      }
      const result = await addMembers.mutateAsync({
        channelId: selectedStream.id,
        pubkeys: [authorization.agentPubkey],
        role: "bot",
        expectedRelayUrl: authorization.relayUrl,
        expectedSignerPubkey: authorization.ownerPubkey,
      });
      if (!result.added.includes(authorization.agentPubkey)) {
        throw new Error(
          result.errors[0]?.error ?? "Agent was not added to the stream.",
        );
      }
      setMemberAdded(true);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setPending(false);
    }
  }

  async function complete() {
    if (!challenge) return;
    setPending(true);
    setError(null);
    try {
      setAuthorization(
        await completeExternalAgentEnrollment({
          name,
          challenge: challenge.challenge,
          proofEventJson,
        }),
      );
      setChallenge(null);
    } catch (cause) {
      setChallenge(null);
      setError(`${String(cause)} Start a new challenge before retrying.`);
    } finally {
      setPending(false);
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (pending && !nextOpen) return;
        onOpenChange(nextOpen);
      }}
    >
      <DialogContent className="max-h-[calc(100vh-2rem)] max-w-2xl overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Authorize an external agent</DialogTitle>
          <DialogDescription>
            The agent keeps its private key and runs outside Buzz. This adds its
            public identity to this workspace; it does not start an agent.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <label
            htmlFor="external-agent-name"
            className="block space-y-1 text-sm"
          >
            <span>Agent display name</span>
            <Input
              id="external-agent-name"
              disabled={pending || Boolean(authorization)}
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label
            htmlFor="external-agent-pubkey"
            className="block space-y-1 text-sm"
          >
            <span>External agent public key (hex)</span>
            <Input
              id="external-agent-pubkey"
              disabled={pending || Boolean(challenge) || Boolean(authorization)}
              value={agentPubkey}
              onChange={(event) => setAgentPubkey(event.target.value)}
            />
          </label>
          {!challenge && !authorization ? (
            <Button
              disabled={pending || !name.trim() || !agentPubkey.trim()}
              onClick={() => void prepare()}
              type="button"
            >
              Create signing challenge
            </Button>
          ) : null}
          {challenge ? (
            <>
              <p className="text-sm">
                On the external host, sign a Nostr kind 1 event with its own
                key. Use the exact challenge below as content, one{" "}
                <code>p</code> tag for the owner shown below, and the current
                Unix time as <code>created_at</code>. Keep the signed event
                private and paste its full JSON here within five minutes.
              </p>
              <label
                htmlFor="external-agent-challenge"
                className="block space-y-1 text-sm"
              >
                <span>Challenge (event content)</span>
                <Textarea
                  id="external-agent-challenge"
                  readOnly
                  value={challenge.challenge}
                />
              </label>
              <p className="break-all text-xs">
                Owner p tag: {challenge.ownerPubkey}
              </p>
              <p className="break-all text-xs">
                Workspace relay: {challenge.relayUrl}
              </p>
              <label
                htmlFor="external-agent-proof"
                className="block space-y-1 text-sm"
              >
                <span>Signed proof event JSON</span>
                <Textarea
                  id="external-agent-proof"
                  maxLength={8192}
                  value={proofEventJson}
                  onChange={(event) => setProofEventJson(event.target.value)}
                />
              </label>
              <Button
                disabled={pending || !proofEventJson.trim()}
                onClick={() => void complete()}
                type="button"
              >
                Authorize public key
              </Button>
            </>
          ) : null}
          {authorization ? (
            <>
              <p className="text-sm">
                Owner announcement queued for {authorization.relayUrl}. On the
                external host, publish a kind 0 profile signed by the agent key
                with exactly one auth tag below. The profile must be the latest
                kind 0 event for this key. You can add the public key to a
                stream below even before its profile appears. If this tag is
                lost, repeat this flow with the same public key and name; a
                fresh signed proof recovers it without creating another
                announcement.
              </p>
              <label
                htmlFor="external-agent-auth"
                className="block space-y-1 text-sm"
              >
                <span>NIP-OA auth tag (JSON array)</span>
                <Textarea
                  id="external-agent-auth"
                  readOnly
                  value={authorization.authTag}
                />
              </label>
              <label
                htmlFor="external-agent-stream"
                className="block space-y-1 text-sm"
              >
                <span>Optional stream membership</span>
                <select
                  id="external-agent-stream"
                  className="w-full rounded-md border bg-background px-3 py-2"
                  disabled={pending || channelsQuery.isLoading}
                  value={channelId}
                  onChange={(event) => {
                    setChannelId(event.target.value);
                    setMemberAdded(false);
                  }}
                >
                  <option value="">Select a stream</option>
                  {streams.map((stream) => (
                    <option key={stream.id} value={stream.id}>
                      {stream.name}
                    </option>
                  ))}
                </select>
              </label>
              {alreadyMember || memberAdded ? (
                <p className="text-sm">
                  Agent is already a member of this stream.
                </p>
              ) : (
                <Button
                  disabled={pending || !selectedStream}
                  onClick={() => void addToStream()}
                  type="button"
                >
                  Add agent to stream as bot
                </Button>
              )}
            </>
          ) : null}
          {error ? (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          ) : null}
        </div>
      </DialogContent>
    </Dialog>
  );
}
