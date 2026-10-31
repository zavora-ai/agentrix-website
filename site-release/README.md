# agentrix-site-release

Phase 3 publish plus rollback: admitted releases in SQLite with one `current`
pointer per site and an audit log of admissions, publishes and refusals. The
`LaunchGate` admits a candidate only when integrity, parity, an approved
release card naming the digest, and policy all pass; publish and rollback are
both `set_current`, and the edge's shared `Current` pointer moves both audiences
from the next request on. Migration parity (Phase 9) plugs in behind the
`ParityCheck` trait beside twin parity.

```sh
cargo test -p agentrix-site-release   # the Phase 3 gate
```
