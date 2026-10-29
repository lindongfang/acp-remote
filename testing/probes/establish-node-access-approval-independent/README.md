# Independent application-policy acceptance

This standalone Rust executable path-depends on the ordinary product library.
It uses only public APIs; it does not include product sources, use cfg(test),
construct actor fields, insert trust records, or install an approval issuer.

Build offline with its Cargo.lock and a tester-owned CARGO_TARGET_DIR under
`.tmp/approval-acceptance-20261030/probe-target`. Run as the real Windows node
owner, passing an absolute **new** runtime directory beneath that owner-only
`.tmp` root. The probe refuses to overwrite an existing run directory.

The probe temporarily owns 127.0.0.1:30452. It creates and protects a directory
before generating a localhost TLS certificate; the TLS client pins that exact
certificate and does not bypass validation. The node uses real SQLite, CurrentUser
DPAPI and owner named pipes. Pairing goes through actual TLS lookup/claim plus
explicit approval via the ordinary owner IPC API. This is not a simulated trust
row or a claim that the Rust client is a browser.

Real P-256/JWS AuthClient proofs authenticate through a lease obtained from
NodeRuntime.application_policy(). Opaque actors come only from authenticate.
Runtime capability dispatchers increment a counter inside the supplied closure;
the product service decides whether to invoke it. Missing, expired, boundary,
future-unverified and clock-failure cases must all reject without increment.
The positive spy control is outside the product gate and grants no permission.

The controlled clock additionally holds a real runtime request inside the shared
client gate while a real owner revoke waits. Releasing it must produce Unverified,
not dispatch, after which revoke commits and the old actor becomes invalid.
Challenge leases are tested for Drop cancellation, independent scope ownership,
4100 issue/drop cycles, replay and unknown/revoked-key rejection.

External policy/derived-clock/actor handles remain alive across owner stop. The
probe checks admission closure, prepared signed authentication and current-actor
rejection before awaiting shutdown, then checks rejection after join. It starts
a second real node on the released port to reject a foreign actor, and finally
reacquires the first data lock and listener.

Actual scopes and limitations are documented in
`testing/runs/establish-node-access/2026-10-30-approval-acceptance.md`.
The fake capability name/counter and controlled clock are adapters; no real
Harness is launched and no successful runtime approval exists. Do not infer
unexecuted panic/OS-kill, every lifecycle interleaving, isolated DB I/O failure or
future-issuer behavior from a passing run. All generated keys/databases/logs/target
stay under `.tmp`; this directory contains source and dependency metadata only.
