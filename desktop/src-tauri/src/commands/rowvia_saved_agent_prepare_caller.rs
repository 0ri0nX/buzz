//! Private prepare-only Unix transport. Not registered as a Tauri command.
//! Production uses a fixed complete owner context; deployment pins must qualify
//! before Save. Tests substitute the whole context, never authority hooks.

use super::super::rowvia_saved_agent::{PreparedRowviaSavedAgent, RowviaSavedAgentPreparation};
use super::{C2Boundary, NativeRegistrationCallbacks};
use crate::managed_agents::SavedAgentRecipe;
use nix::spawn::{posix_spawn, PosixSpawnAttr, PosixSpawnFileActions, PosixSpawnFlags};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    process::{self, Pid, WaitId, WaitIdOptions, WaitOptions},
};
use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fmt,
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::{AsFd, AsRawFd, OwnedFd},
        unix::{ffi::OsStrExt, process::ExitStatusExt},
    },
    path::PathBuf,
    process::ExitStatus,
    time::{Duration, Instant},
};

const FRAME_LIMIT: usize = 64 * 1024;
const STDERR_LIMIT: usize = 64 * 1024;
const TRAFFIC_LIMIT: usize = 1024 * 1024;
const CLEANUP_LIMIT: Duration = Duration::from_millis(500);
const INVOCATION_LIMIT: Duration = Duration::from_secs(15);
// Fixed child-only bootstrap: restore the named entrypoint's cwd/argv without
// ambient shell lookup, global desktop chdir, or caller-supplied source text.
const NODE_BOOTSTRAP: &str = "process.chdir(process.argv[1]); process.argv.splice(1, 1); await import((await import('node:url')).pathToFileURL(process.argv[1]).href);";

pub(crate) struct OwnerSaveRequest {
    display_name: String,
    system_prompt: String,
    channel_id: String,
    recipe: SavedAgentRecipe,
}
impl OwnerSaveRequest {
    pub(in crate::commands::agents) fn from_selection(
        display_name: String,
        system_prompt: String,
        channel_id: String,
        recipe: SavedAgentRecipe,
    ) -> Self {
        Self {
            display_name,
            system_prompt,
            channel_id,
            recipe,
        }
    }
}

#[path = "rowvia_saved_agent_owner_host.rs"]
pub(crate) mod production;

impl<'de> Deserialize<'de> for OwnerSaveRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RequestVisitor;
        impl<'de> Visitor<'de> for RequestVisitor {
            type Value = OwnerSaveRequest;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a closed owner-save request object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                use serde::de::Error;
                let (mut name, mut prompt, mut channel, mut recipe) = (None, None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "displayName" if name.is_none() => name = Some(map.next_value::<String>()?),
                        "systemPrompt" if prompt.is_none() => {
                            prompt = Some(map.next_value::<String>()?)
                        }
                        "channelId" if channel.is_none() => {
                            channel = Some(map.next_value::<String>()?)
                        }
                        "recipe" if recipe.is_none() => {
                            recipe = Some(map.next_value::<SavedAgentRecipe>()?)
                        }
                        _ => return Err(M::Error::custom("invalid owner-save request")),
                    }
                }
                Ok(OwnerSaveRequest {
                    display_name: name
                        .ok_or_else(|| M::Error::custom("invalid owner-save request"))?,
                    system_prompt: prompt
                        .ok_or_else(|| M::Error::custom("invalid owner-save request"))?,
                    channel_id: channel
                        .ok_or_else(|| M::Error::custom("invalid owner-save request"))?,
                    recipe: recipe.ok_or_else(|| M::Error::custom("invalid owner-save request"))?,
                })
            }
        }
        deserializer.deserialize_map(RequestVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrepareFailure {
    Unavailable,
    Unknown,
}

/// Exact Main public projection, decoded only from a duplicate-free JSON map.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct PreparedProposal {
    version: u8,
    operation_id: String,
    agent_pubkey: String,
    recipe_sha256: String,
    canonical21: String,
    canonical21_sha256: String,
    preparation_retained: bool,
    bindings_ready: bool,
    runtime_ready: bool,
}
impl PreparedProposal {
    pub(in crate::commands::agents) fn operation_id(&self) -> &str {
        &self.operation_id
    }
}

impl<'de> Deserialize<'de> for PreparedProposal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ProposalVisitor;
        impl<'de> Visitor<'de> for ProposalVisitor {
            type Value = PreparedProposal;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a closed prepared-result object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                use serde::de::Error;
                let (
                    mut version,
                    mut operation,
                    mut agent,
                    mut recipe,
                    mut canonical,
                    mut canonical_hash,
                    mut retained,
                    mut bindings,
                    mut runtime,
                ) = (None, None, None, None, None, None, None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "version" if version.is_none() => version = Some(map.next_value::<u8>()?),
                        "operation_id" if operation.is_none() => {
                            operation = Some(map.next_value::<String>()?)
                        }
                        "agent_pubkey" if agent.is_none() => {
                            agent = Some(map.next_value::<String>()?)
                        }
                        "recipe_sha256" if recipe.is_none() => {
                            recipe = Some(map.next_value::<String>()?)
                        }
                        "canonical21" if canonical.is_none() => {
                            canonical = Some(map.next_value::<String>()?)
                        }
                        "canonical21_sha256" if canonical_hash.is_none() => {
                            canonical_hash = Some(map.next_value::<String>()?)
                        }
                        "preparation_retained" if retained.is_none() => {
                            retained = Some(map.next_value::<bool>()?)
                        }
                        "bindings_ready" if bindings.is_none() => {
                            bindings = Some(map.next_value::<bool>()?)
                        }
                        "runtime_ready" if runtime.is_none() => {
                            runtime = Some(map.next_value::<bool>()?)
                        }
                        _ => return Err(M::Error::custom("invalid prepared result")),
                    }
                }
                Ok(PreparedProposal {
                    version: version.ok_or_else(|| M::Error::custom("invalid prepared result"))?,
                    operation_id: operation
                        .ok_or_else(|| M::Error::custom("invalid prepared result"))?,
                    agent_pubkey: agent
                        .ok_or_else(|| M::Error::custom("invalid prepared result"))?,
                    recipe_sha256: recipe
                        .ok_or_else(|| M::Error::custom("invalid prepared result"))?,
                    canonical21: canonical
                        .ok_or_else(|| M::Error::custom("invalid prepared result"))?,
                    canonical21_sha256: canonical_hash
                        .ok_or_else(|| M::Error::custom("invalid prepared result"))?,
                    preparation_retained: retained
                        .ok_or_else(|| M::Error::custom("invalid prepared result"))?,
                    bindings_ready: bindings
                        .ok_or_else(|| M::Error::custom("invalid prepared result"))?,
                    runtime_ready: runtime
                        .ok_or_else(|| M::Error::custom("invalid prepared result"))?,
                })
            }
        }
        deserializer.deserialize_map(ProposalVisitor)
    }
}

/// Whole-transport accounting: FD3+FD0 outbound, FD1+FD4 inbound. Callback
/// response capacity is conservatively reserved before C2 dispatch; actual
/// bytes are charged once before queue/write. Reservations are not traffic.
#[derive(Default)]
struct TrafficBudget {
    outbound: usize,
    inbound: usize,
}

impl TrafficBudget {
    fn charge(used: &mut usize, length: usize) -> Result<(), ()> {
        let next = used
            .checked_add(length)
            .filter(|next| *next <= TRAFFIC_LIMIT)
            .ok_or(())?;
        *used = next;
        Ok(())
    }
    fn outbound(&mut self, length: usize) -> Result<(), ()> {
        Self::charge(&mut self.outbound, length)
    }
    fn inbound(&mut self, length: usize) -> Result<(), ()> {
        Self::charge(&mut self.inbound, length)
    }
    fn reserve_response(&self) -> Result<(), ()> {
        self.outbound
            .checked_add(FRAME_LIMIT)
            .filter(|next| *next <= TRAFFIC_LIMIT)
            .map(|_| ())
            .ok_or(())
    }
}

fn receive_callback(
    holder: &mut NativeRegistrationCallbacks,
    bytes: &[u8],
    boundary: &mut impl C2Boundary,
    budget: &mut TrafficBudget,
) -> Result<Option<Vec<u8>>, ()> {
    budget.inbound(bytes.len())?;
    // The actual response can depend on C2, so require one maximum frame before
    // any dispatch. Legitimate near-limit traffic may conservatively refuse.
    budget.reserve_response()?;
    let response = holder.receive_with(bytes, boundary).map_err(|_| ())?;
    if let Some(frame) = &response {
        budget.outbound(frame.len())?;
    }
    Ok(response)
}

/// Intentionally non-Deserialize. Only a whole trusted-context implementation
/// may construct this after checking the held executable and complete import
/// closure, fixed references and explicit reached-helper shared-group mode.
struct FixedInstallation {
    node: File,
    entrypoint: PathBuf,
    working_directory: PathBuf,
    // Held custody is retained across spawn and final read-only verification.
    _held_source_custody: Vec<File>,
}

struct ValidatedInvocation {
    original: RowviaSavedAgentPreparation,
    installation: FixedInstallation,
}

/// Private and indivisible: no production fallback is available to fake tests.
/// Preflight must complete installation/source checks BEFORE native save. It
/// derives UUIDv7, all authority values and paths from trusted public context.
/// The 15s deadline is cooperative around these synchronous operations and C2:
/// it detects overruns on return, but cannot interrupt a blocked implementation.
trait WholeContext {
    type Boundary: C2Boundary;
    fn preflight(&mut self, request: &OwnerSaveRequest) -> Result<ValidatedInvocation, ()>;
    fn save(
        &mut self,
        original: RowviaSavedAgentPreparation,
    ) -> Result<PreparedRowviaSavedAgent, ()>;
    fn startup(
        &mut self,
        original: &RowviaSavedAgentPreparation,
        prepared: &PreparedRowviaSavedAgent,
    ) -> Result<Vec<u8>, ()>;
    fn boundary(&mut self) -> &mut Self::Boundary;
    // Group disappearance alone cannot establish this: escaped sessions must be
    // tracked/settled by the whole qualified launch context, or return false.
    // `leader` is informational after its reap, never authority for a signal;
    // use separately held owned-descendant custody, never a cached numeric PGID.
    fn owned_descendants_settled(&mut self, leader: u32) -> bool;
    fn record_family_settlement(&mut self, _settled: SettledProcessFamily) {}
    fn revalidate_installation(&mut self, installation: &FixedInstallation) -> bool;
}

/// The private production entry uses the concrete whole-context supplier.
/// Missing deployment pins refuse before C2 Save; neither IPC nor ambient
/// environment can supply authority or fill its literal installation table.
pub(crate) fn save_rowvia_saved_agent_preparation<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    request: OwnerSaveRequest,
) -> Result<PreparedProposal, PrepareFailure> {
    production::save(app, &request)
}

fn remaining(deadline: Instant) -> Result<Duration, ()> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or(())
}

fn checked_frame(bytes: &[u8]) -> Result<&[u8], ()> {
    if bytes.is_empty()
        || bytes.len() > FRAME_LIMIT
        || bytes.last() != Some(&b'\n')
        || bytes[..bytes.len() - 1].contains(&b'\n')
    {
        return Err(());
    }
    let body = &bytes[..bytes.len() - 1];
    std::str::from_utf8(body).map_err(|_| ())?;
    Ok(body)
}

fn proposal(bytes: &[u8]) -> Result<PreparedProposal, ()> {
    let result: PreparedProposal = serde_json::from_slice(checked_frame(bytes)?).map_err(|_| ())?;
    if result.version != 1
        || !result.preparation_retained
        || result.bindings_ready
        || result.runtime_ready
        || result.canonical21.is_empty()
        || result.canonical21.len() > 4096
        || hex::encode(Sha256::digest(result.canonical21.as_bytes())) != result.canonical21_sha256
    {
        return Err(());
    }
    Ok(result)
}

fn run_with(
    context: &mut impl WholeContext,
    request: &OwnerSaveRequest,
) -> Result<PreparedProposal, PrepareFailure> {
    let deadline = Instant::now() + INVOCATION_LIMIT;
    let invocation = context
        .preflight(request)
        .map_err(|_| PrepareFailure::Unavailable)?;
    remaining(deadline).map_err(|_| PrepareFailure::Unavailable)?;
    // From this call onwards all failures are uncertain. Never retry a save,
    // callback retain, or transport delivery, including after read-only errors.
    let prepared = context
        .save(invocation.original.clone())
        .map_err(|_| PrepareFailure::Unknown)?;
    let mut holder = NativeRegistrationCallbacks::from_prepared(&invocation.original, &prepared)
        .map_err(|_| PrepareFailure::Unknown)?;
    let operation = (|| {
        remaining(deadline)?;
        let startup = context.startup(&invocation.original, &prepared)?;
        checked_frame(&startup)?;
        remaining(deadline)?;
        let mut child = OwnedNode::spawn(&invocation.installation)?;
        let leader = child.child.id();
        let exchange = child.exchange(&startup, deadline, &mut holder, context.boundary());
        // Cleanup runs on both success and failure, within its separate bound.
        let local_settled = child.settle();
        if let Some(settled) = child.take_family_settlement() {
            context.record_family_settlement(settled);
        }
        let settled_descendants = context.owned_descendants_settled(leader);
        if !local_settled || !settled_descendants {
            holder.delivery_failed();
            return Err(());
        }
        let result = proposal(&exchange?)?;
        remaining(deadline)?;
        if !context.revalidate_installation(&invocation.installation) {
            return Err(());
        }
        holder
            .finalize_prepared_with(
                &result.operation_id,
                &result.agent_pubkey,
                &result.recipe_sha256,
                &result.canonical21_sha256,
                context.boundary(),
            )
            .map_err(|_| ())?;
        remaining(deadline)?;
        Ok(result)
    })();
    operation.map_err(|_| PrepareFailure::Unknown)
}

fn pipe() -> io::Result<(File, File)> {
    let (reader, writer) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC)?;
    Ok((File::from(reader), File::from(writer)))
}

fn high_fd(file: File) -> io::Result<OwnedFd> {
    Ok(rustix::io::fcntl_dupfd_cloexec(&file, 10)?)
}

fn nonblocking(file: &impl AsFd) -> io::Result<()> {
    let flags = rustix::fs::fcntl_getfl(file)?;
    rustix::fs::fcntl_setfl(file, flags | rustix::fs::OFlags::NONBLOCK)?;
    Ok(())
}

/// Private child custody from posix_spawn; only the enclosing owner may reap.
struct OwnedLeader {
    pid: nix::unistd::Pid,
    status: Option<ExitStatus>,
}

impl OwnedLeader {
    fn id(&self) -> u32 {
        self.pid.as_raw() as u32
    }

    fn kernel_pid(&self) -> io::Result<Pid> {
        Pid::from_raw(self.pid.as_raw()).ok_or_else(|| io::Error::from_raw_os_error(libc::ECHILD))
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let pid = self.kernel_pid()?;
        if let Some((observed, status)) = process::waitpid(Some(pid), WaitOptions::NOHANG)? {
            if observed != pid {
                return Err(io::Error::from_raw_os_error(libc::ECHILD));
            }
            self.status = Some(ExitStatus::from_raw(status.as_raw()));
        }
        Ok(self.status)
    }
}

struct OwnedNode {
    child: OwnedLeader,
    input: Option<File>,
    requests: File,
    stderr: File,
    startup: Option<File>,
    result: File,
    settled: bool,
    cleanup_attempted: bool,
    leader_reaped: bool,
    family_settlement: Option<SettledProcessFamily>,
}

/// Constructed only after bounded kernel inspection under unreaped leader
/// custody. Valid for the separately reviewed fixed shared-group closure only;
/// it makes no claim about arbitrary same-UID or compromised helper code.
struct SettledProcessFamily {
    leader: u32,
}

fn group_has_live_members(leader: u32) -> io::Result<bool> {
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
            .is_none()
        {
            continue;
        }
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let (_, fields) = stat
            .rsplit_once(") ")
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
        let mut fields = fields.split_whitespace();
        let state = fields
            .next()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
        fields
            .next()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
        let group = fields
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidData))?;
        if group == leader && state != "Z" && state != "X" {
            return Ok(true);
        }
    }
    Ok(false)
}

impl OwnedNode {
    fn spawn(installation: &FixedInstallation) -> Result<Self, ()> {
        // Fixed entrypoints are reviewed named source snapshots, never caller
        // descriptor selectors. FD aliases could otherwise be remapped as 3/4.
        if installation.entrypoint.starts_with("/proc/self/fd") {
            return Err(());
        }
        let (startup_child, startup) = pipe().map_err(|_| ())?;
        let (result, result_child) = pipe().map_err(|_| ())?;
        let (input_child, input) = pipe().map_err(|_| ())?;
        let (requests, requests_child) = pipe().map_err(|_| ())?;
        let (stderr, stderr_child) = pipe().map_err(|_| ())?;
        // Every child endpoint is distinct and above all five remap targets.
        let child_fds = [
            input_child,
            requests_child,
            stderr_child,
            startup_child,
            result_child,
        ]
        .into_iter()
        .map(high_fd)
        .collect::<io::Result<Vec<_>>>()
        .map_err(|_| ())?;
        let node_child = high_fd(installation.node.try_clone().map_err(|_| ())?).map_err(|_| ())?;
        let _source_children: Vec<OwnedFd> = installation
            ._held_source_custody
            .iter()
            .map(|file| high_fd(file.try_clone()?))
            .collect::<io::Result<_>>()
            .map_err(|_| ())?;
        let executable = format!("/proc/self/fd/{}", node_child.as_raw_fd());
        let arguments = [
            CString::new(executable.as_bytes()),
            CString::new("--experimental-vm-modules"),
            CString::new("--input-type=module"),
            CString::new("--eval"),
            CString::new(NODE_BOOTSTRAP),
            CString::new(installation.working_directory.as_os_str().as_bytes()),
            CString::new(installation.entrypoint.as_os_str().as_bytes()),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ())?;
        let environment = [CString::new("LANG=C.UTF-8").map_err(|_| ())?];
        let mut actions = PosixSpawnFileActions::init().map_err(|_| ())?;
        for (target, source) in child_fds.iter().enumerate() {
            if source.as_raw_fd() < 10 {
                return Err(());
            }
            actions
                .add_dup2(source.as_raw_fd(), target as i32)
                .map_err(|_| ())?;
        }
        let mut attributes = PosixSpawnAttr::init().map_err(|_| ())?;
        attributes
            .set_pgroup(nix::unistd::Pid::from_raw(0))
            .map_err(|_| ())?;
        attributes
            .set_flags(PosixSpawnFlags::POSIX_SPAWN_SETPGROUP)
            .map_err(|_| ())?;
        // Complete fallible parent setup before any child exists. Each parent
        // endpoint owns a different pipe description from its child endpoint.
        for file in [&input, &requests, &stderr, &startup, &result] {
            nonblocking(file).map_err(|_| ())?;
        }
        let pid = posix_spawn(
            executable.as_str(),
            &actions,
            &attributes,
            &arguments,
            &environment,
        )
        .map_err(|_| ())?;
        // Immediately retain the returned child identity; no fallible setup can
        // intervene before its sole cleanup owner is constructed.
        let owned = Self {
            child: OwnedLeader { pid, status: None },
            input: Some(input),
            requests,
            stderr,
            startup: Some(startup),
            result,
            settled: false,
            cleanup_attempted: false,
            leader_reaped: false,
            family_settlement: None,
        };
        // No parent copy may keep a child pipe open and prevent cleanup EOF.
        drop(child_fds);
        Ok(owned)
    }

    fn exchange(
        &mut self,
        startup: &[u8],
        deadline: Instant,
        holder: &mut NativeRegistrationCallbacks,
        boundary: &mut impl C2Boundary,
    ) -> Result<Vec<u8>, ()> {
        let result = self.exchange_inner(startup, deadline, Some(&mut *holder), boundary);
        if result.is_err() {
            holder.delivery_failed();
        }
        result
    }

    fn exchange_inner(
        &mut self,
        startup: &[u8],
        deadline: Instant,
        mut holder: Option<&mut NativeRegistrationCallbacks>,
        boundary: &mut impl C2Boundary,
    ) -> Result<Vec<u8>, ()> {
        checked_frame(startup)?;
        let mut budget = TrafficBudget::default();
        budget.outbound(startup.len())?;
        let (mut startup_offset, mut response_offset, mut stderr_total) = (0, 0, 0usize);
        let mut response: Option<Vec<u8>> = None;
        let (mut requests_eof, mut stderr_eof, mut result_eof) = (false, false, false);
        let mut output = Vec::new();
        let mut exited = None;
        loop {
            let left = remaining(deadline)?;
            if exited.is_none() {
                exited = self.observe_exit().map_err(|_| ())?;
            }
            if requests_eof && stderr_eof && result_eof && exited.is_some() {
                if response.is_some() || self.startup.is_some() || exited != Some(true) {
                    return Err(());
                }
                if let Some(holder) = holder.as_deref_mut() {
                    holder.finish_input().map_err(|_| ())?;
                }
                return Ok(output);
            }
            let pending = [
                (!requests_eof).then(|| PollFd::new(&self.requests, PollFlags::IN)),
                (!stderr_eof).then(|| PollFd::new(&self.stderr, PollFlags::IN)),
                (!result_eof).then(|| PollFd::new(&self.result, PollFlags::IN)),
                self.startup
                    .as_ref()
                    .map(|file| PollFd::new(file, PollFlags::OUT)),
                if response.is_some() {
                    Some(PollFd::new(self.input.as_ref().ok_or(())?, PollFlags::OUT))
                } else {
                    None
                },
            ];
            let mut slots = Vec::with_capacity(5);
            let mut fds = Vec::with_capacity(5);
            for (index, descriptor) in pending.into_iter().enumerate() {
                if let Some(descriptor) = descriptor {
                    slots.push(index);
                    fds.push(descriptor);
                }
            }
            let timeout =
                Timespec::try_from(Duration::from_millis(left.as_millis().clamp(1, 20) as u64))
                    .map_err(|_| ())?;
            match poll(&mut fds, Some(&timeout)) {
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(_) => return Err(()),
            }
            let mut events = [PollFlags::empty(); 5];
            for (index, descriptor) in slots.into_iter().zip(&fds) {
                events[index] = descriptor.revents();
            }
            drop(fds);
            remaining(deadline)?;
            if events.iter().any(|events| events.contains(PollFlags::NVAL)) {
                return Err(());
            }
            // Drain all three readable streams each turn, including while a
            // callback response is blocked. A second request during an in-flight
            // response is refused by the qualified holder before dispatch.
            for index in 0..3 {
                if !events[index].intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR) {
                    continue;
                }
                let mut bytes = [0u8; 4096];
                let read = match index {
                    0 => self.requests.read(&mut bytes),
                    1 => self.stderr.read(&mut bytes),
                    _ => self.result.read(&mut bytes),
                };
                match read {
                    Ok(0) => match index {
                        0 => requests_eof = true,
                        1 => stderr_eof = true,
                        _ => result_eof = true,
                    },
                    Ok(length) => match index {
                        0 => {
                            if let Some(frame) = receive_callback(
                                holder.as_deref_mut().ok_or(())?,
                                &bytes[..length],
                                boundary,
                                &mut budget,
                            )? {
                                remaining(deadline)?;
                                response = Some(frame);
                                response_offset = 0;
                            }
                        }
                        1 => {
                            stderr_total = stderr_total.saturating_add(length);
                            if stderr_total > STDERR_LIMIT {
                                return Err(());
                            }
                        }
                        _ => {
                            budget.inbound(length)?;
                            if output.len() + length > FRAME_LIMIT {
                                return Err(());
                            }
                            output.extend_from_slice(&bytes[..length]);
                        }
                    },
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => return Err(()),
                }
            }
            remaining(deadline)?;
            if events[3].intersects(PollFlags::OUT | PollFlags::ERR | PollFlags::HUP) {
                let writer = self.startup.as_mut().ok_or(())?;
                match writer.write(&startup[startup_offset..]) {
                    Ok(0) => return Err(()),
                    Ok(length) => {
                        startup_offset += length;
                        if startup_offset == startup.len() {
                            writer.flush().map_err(|_| ())?;
                            self.startup.take();
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => return Err(()),
                }
            }
            if events[4].intersects(PollFlags::OUT | PollFlags::ERR | PollFlags::HUP) {
                let writer = self.input.as_mut().ok_or(())?;
                let bytes = response.as_ref().ok_or(())?;
                match writer.write(&bytes[response_offset..]) {
                    Ok(0) => return Err(()),
                    Ok(length) => {
                        response_offset += length;
                        if response_offset == bytes.len() {
                            writer.flush().map_err(|_| ())?;
                            holder
                                .as_deref_mut()
                                .ok_or(())?
                                .response_flushed()
                                .map_err(|_| ())?;
                            response = None;
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                    Err(_) => return Err(()),
                }
            }
        }
    }

    /// Observe only: WNOWAIT retains the leader zombie and therefore its PID
    /// through every possible group signal. Only this owner may reap the Child.
    fn observe_exit(&self) -> io::Result<Option<bool>> {
        if self.leader_reaped {
            return Err(io::Error::from_raw_os_error(libc::ECHILD));
        }
        Ok(process::waitid(
            WaitId::Pid(self.child.kernel_pid()?),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )?
        .map(|status| status.exited() && status.exit_status() == Some(0)))
    }

    /// This check fails on ECHILD, an already-reaped leader, or a changed group.
    /// It never uses a cached PID as permission to signal after losing custody.
    fn owns_unreaped_group(&self) -> bool {
        if self.observe_exit().is_err() {
            return false;
        }
        self.child
            .kernel_pid()
            .is_ok_and(|pid| matches!(process::getpgid(Some(pid)), Ok(group) if group == pid))
    }

    fn settle(&mut self) -> bool {
        if self.settled {
            return true;
        }
        if self.cleanup_attempted {
            return false;
        }
        self.cleanup_attempted = true;
        self.input.take();
        self.startup.take();
        // Never reap before the sole group signal. The original leader PID
        // remains reserved even if it has exited; no group signal/probe follows
        // the reap. Missing custody refuses cleanup rather than targeting a
        // numeric group that might now belong to somebody else.
        if !self.owns_unreaped_group() {
            return false;
        }
        let Ok(pid) = self.child.kernel_pid() else {
            return false;
        };
        if process::kill_process_group(pid, process::Signal::KILL).is_err() {
            return false;
        }
        let until = Instant::now() + CLEANUP_LIMIT;
        let mut eof = [false; 3];
        while Instant::now() < until {
            // Setup itself can fail before every fd became nonblocking. Never
            // risk an unbounded read while cleaning up a partially built child.
            let mut bytes = [0u8; 4096];
            if nonblocking(&self.requests).is_ok()
                && matches!(self.requests.read(&mut bytes), Ok(0))
            {
                eof[0] = true;
            }
            if nonblocking(&self.stderr).is_ok() && matches!(self.stderr.read(&mut bytes), Ok(0)) {
                eof[1] = true;
            }
            if nonblocking(&self.result).is_ok() && matches!(self.result.read(&mut bytes), Ok(0)) {
                eof[2] = true;
            }
            if eof.into_iter().all(|closed| closed)
                && matches!(self.observe_exit(), Ok(Some(_)))
                && matches!(group_has_live_members(self.child.id()), Ok(false))
            {
                // The group scan precedes reap. The zombie keeps this numeric
                // group generation reserved through the last permitted probe.
                self.family_settlement = Some(SettledProcessFamily {
                    leader: self.child.id(),
                });
                self.leader_reaped = matches!(self.child.try_wait(), Ok(Some(_)));
                self.settled = self.leader_reaped;
                if !self.settled {
                    self.family_settlement = None;
                }
                return self.settled;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if matches!(self.observe_exit(), Ok(Some(_))) {
            self.leader_reaped = matches!(self.child.try_wait(), Ok(Some(_)));
        }
        false
    }

    fn take_family_settlement(&mut self) -> Option<SettledProcessFamily> {
        if self.settled {
            self.family_settlement.take()
        } else {
            None
        }
    }
}

impl Drop for OwnedNode {
    fn drop(&mut self) {
        if !self.settled {
            let _ = self.settle();
        }
    }
}

#[cfg(test)]
#[path = "rowvia_saved_agent_prepare_caller_tests.rs"]
mod tests;
