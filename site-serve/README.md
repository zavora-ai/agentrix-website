# agentrix-site-serve

The composed site: one server over every phase. It opens the product and kernel
stores in one data dir, seeds the demo site through the LaunchGate, hosts three
agent runners on a started kernel (Growth content agent with draft-filing tools,
Revenue qualifier-to-drafter Sequential team, Steward support agent with corpus
retrieval — all on Muse through Baseten), registers the guarded probe, watch and
digest workflows, and serves the edge from the current release.

`HarnessAgent` was evaluated and deferred: it is a workspace-oriented coding
harness, and site targets run better on `Runner`'s governed tool path with inbox
approvals. The reviews trigger shares the Growth runner in v1, reading its job
from the run input.

```sh
cargo run -p agentrix-site-serve -- --dry-run   # offline, scripted model
BASETEN_API_KEY=... cargo run -p agentrix-site-serve
```

Environment: `BASETEN_API_KEY` (required unless `--dry-run`), `BASETEN_MODEL`
(default `meta-models/muse-spark-open-1026`), `BASETEN_BASE_URL` (default
`https://inference.baseten.co/v1`), `QUIET_START_MIN`/`QUIET_END_MIN`,
`CERT_EXPIRY_DAYS`/`DOMAIN_EXPIRY_DAYS`/`BACKUP_AGE_DAYS`/`WATCH_ADVISORIES`.
