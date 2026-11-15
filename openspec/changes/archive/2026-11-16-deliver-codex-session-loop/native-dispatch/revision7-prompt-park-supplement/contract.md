# Prompt temporary-gap park: narrow ABI supplement

Scope: engineering clarification required by the already-frozen same-attempt observation/necessary-SQL barrier. No product choice, new send authority, body limit, TTL, second writer or UI change. v6 9d1349f6 + control-receipt ec30e1bf + independent NDT v5 outputs8f6e5613 and coding gate f9539c09 remain historical immutable inputs. Root explicitly requested this narrow independent gate before treating the new ABI as accepted. Current actual WIP is preserved separately from final acceptance; the whole NativeDispatch phase remains unfinished.

## Exact capability surface

Existing actual poll signatures remain unchanged. Add these required methods to the same borrowed proxy capabilities:

~~~rust
trait PollIo {
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Result<IoProgress, AcpError>>;
    fn park(&mut self) -> Result<(), AcpError>;
}
trait DispatchPollIo {
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Result<DispatchIoProgress, DispatchIoError>>;
    fn park(&mut self) -> Result<(), DispatchIoError>;
}
~~~

The unit Result acknowledges one proxy action. It is NOT an IO result, actual provider poll witness, Written/Zero receipt, no-future-poll proof, observation receipt or write grant. The application must register its wake source before calling park. The adapter delegates park to the original borrowed PollIo; it does not substitute a writer or make a provider return Pending.

The successful fence must use exactly ONE proxy action, either poll OR park, and return Ok. A rejecting fence must use ZERO actions. Empty Ok, more than one action, poll+park, park+poll, or an Err after an action are invalid fences and keep the existing InvalidFence handling. Each invocation receives the original command's ephemeral proxy; it cannot retain it or create a second command.

## Exact proxy states

Current frozen overlay uses private ProxyOutcome:

~~~rust
enum ProxyOutcome {
    ApplicationParked,                 // no W.poll occurred
    ProtocolWait,                      // original protocol preconditions delayed W.poll
    Stopped,                           // original cell/Core already stopped; no W.poll
    Provider(Poll<Result<IoProgress, AcpError>>), // actual provider call, including true Pending
}
~~~

park increments the proxy action counter once, sets only ApplicationParked, and returns Ok(()). It does NOT set actual/in_poll, accepted progress, accepted/spent request state, completion, an observation acknowledgement, or a poll-count hook. Only the original poll path, after acquiring the SAME ProtocolState and rechecking expiry/cancellation/stop plus observed ack==sequence and held remaining==0, may invoke W.poll and set Provider(...). Existing actual-poll instrumentation is after that guard. ApplicationParked/ProtocolWait mean the original writer future remains waiting; neither is described as provider IO Pending. Proxy.actual remains true only on actual W.poll. InvalidFence known-zero classification continues to use actual=false or a genuine Provider(Pending), plus the actual cell's accepted byte count.

## Owner, gap and wake ordering

Prompt fence order remains original selected R3 owner -> target facts mutex -> source short gate -> actual protocol-state mutex (inside poll only). Same resource, revision, scope, RPC, configuration and original selected owner are checked. Stop/closing, stale selection, replaced resource and source quarantine still reject; they do not become temporary park authority.

If current target requirements/failed requirements/control holds or a necessary source-control requirement remain, store cx.waker().clone() in TargetFacts.writer_parked UNDER THE SAME facts guard that observed the gap; then call park and return Ok. Existing native target and running attempt remain owned. No selection/reservation/frame/ticket is replaced and no new RPC/Run/D is made.

A worker clearing a necessary fact removes ONLY that exact requirement after the original metadata transaction is reliably confirmed. It then releases the facts guard, takes the registered waker under the same mutex, releases the mutex, and wakes outside it. Control completion uses the same target wake path; reliable source-control completion decrements only its own source count and visits actual source target slots to wake parked writers. Failure leaves the gap. No raw observation, RPC/body text or bool can clear it.

The shared facts mutex makes check+register vs clear+take atomic: if clear wins first the writer sees no gap and goes through the actual poll guard; if register wins first the clearer takes the exact registered waker. Repeated/old fact completion cannot clear another requirement. The writer also remains subscribed to original Core.notify and its existing deadline/cancel/stop paths. Waking an already completed writer confers no IO authority. Poison recovery is teardown/wake only; it grants no new send.

Failure/close must release the source gate BEFORE taking target facts to wake; it must not invert target-facts -> source-gate writer ordering. Current fail_source overlay does this. No SQL await, process wait or wake callback is performed under owner/resource/source/protocol short guards.

## Byte-prefix and safety control boundary

Preserve the existing single writer rule: accepted==0 and queue.any(control) yields the original StepResult::Park before the application fence. A control enqueue's Core.notify wakes the original waiting writer; a resource waker wakes the same original writer. Thus an unstarted prompt waiting on a gap permits the existing zero-byte control priority path. No second writer or control injection is added.

After a prompt prefix accepted>0, a control MUST NOT be interleaved into an unfinished JSON line. Its original frame stays the current command; its original write deadline, cancellation and stop are authoritative. A queued control may remain blocked until that frame converges or the connection stops. Do not claim every safety response can immediately write in the middle of another frame. Partial/flush uncertainty remains Unknown and resource responsibility stays retained for real shutdown/convergence.

Native cancel, Unsupported error and authorized PermissionCancelled retain their own original fences/reservation/frame/ticket. They do not consult the prompt's temporary SQL gap to mint authority or bypass current scope/work/request checks. They may use the existing zero-byte control path while normal prompt input waits. This supplement does not add Selected/allow, native cancel from ordinary queue Cancel, or automatic deny for unknown permissions.

## Deadline, cancellation and loss

A gap does not itself make a transport terminal fact. The existing writer loop still checks original cell cancellation/deadline/stop before action and after every wake; the c737 deadline starvation repair is retained. If no bytes were ever accepted, actual deadline/cancel may yield genuine Zero. A positive prefix or flush uncertainty yields Unknown with the actual accepted count and original failure cause; Err(0) stays Unknown. ApplicationParked is not a basis for retry or slot release. Only the actual old ticket terminal plus reliable old Zero may enter the existing same-selected retry chain. Early native response/idle still cannot settle a pending write, and v2 aggregate idle never becomes Run success through park.

## Freeze and review scope

source-inputs.json records every actual current WIP source hash; overlay/ contains exact changes relative to immutable API03 (351-source freeze9e51f828). The overlay includes intervening ownership/Zero-retry/approved pool-close work solely to reconstruct compile input, not to expand this ABI gate into a full product review. Direct park review paths are transport.rs, transport/writer.rs, acp/native_dispatch.rs, native_dispatch/ports.rs, dispatch.rs, environment.rs, observation.rs and control.rs. The actual current cargo check --lib returned 0 with WIP warnings; no park behavioral test or independent80 has run. tests.md freezes required reachable tests and exact current/proposed test-only boundaries. Final acceptance must use a later complete candidate and all required gates.
