# Local SQLite worker ownership patch

Source: the locally cached sqlx-sqlite 0.8.6 crate, SHA-256 c2d12fe70b2c1b4401038055f90f151b78208de1f9f89a7dbfd41587a10c3eea, upstream VCS bab1b022bd56a64f9a08b46b36b97c5cff19d77e. Original normalized Cargo.toml, LICENSE-MIT and LICENSE-APACHE are retained. The original 56-file manifest is recorded with this candidate.

This patch adds an optional explicit worker registry, ownership of the original thread JoinHandle, separate shutdown and join facts, and private per-scope cfg(test) controls. It does not change sqlx-core pool algorithms or the upstream close/close_hard ABI. Unscoped connections retain upstream behavior and have no new physical-close evidence. No third-party version upgrade or new package is requested. The application supervision and Node integration are separate work.
