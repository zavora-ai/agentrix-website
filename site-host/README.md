# agentrix-site-host

Phase 0 host scaffold: one site on one kernel over durable SQLite stores, with a
fail-loud self-check. It opens the run, approval, agenda, partition, watermark, signal,
charter and KPI stores in one database, registers the seven site processes' standing
triggers plus the KPI tracker's nightly trigger and watermarked cron source, files a
probe signal through a `SignalSink`, and reports every line green — or exits 1 naming
the first line that failed.

The trigger table mirrors `adk-website`'s `jobs::processes` (same kinds, jobs, playbooks
and targets) without depending on the website stack, so the scaffold stays lean. It
starts no kernel and serves no edge; Phase 1 puts the drivers behind the triggers.

```sh
cargo run -p agentrix-site-host -- data   # opens data/kernel.db, prints the report
cargo test -p agentrix-site-host          # the report goes green twice on a scratch dir
```

The crate depends on `adk-rust` by relative path (`../../adk-rust`), so the two
checkouts stay siblings.
