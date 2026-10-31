# agentrix-site-edge

Phase 1 read-only dual site: one release bundle — pages, provenance, content
digest — served three ways from the same bytes: human HTML at `/p/:slug`, agent
twins at `/twin/:slug`, and the public JSON CLI at `POST /cli`. Every response
carries the `x-release-digest` header; anonymous callers share a per-IP rate
limit (120/minute) and a read-only command allowlist.

The server refuses to start unless the digest match passes: each twin's facts
are compared field by field against the facts extracted back out of the rendered
human HTML, so the check reads what visitors read.

```sh
cargo run -p agentrix-site-edge            # serves http://127.0.0.1:8471
cargo test -p agentrix-site-edge          # the Phase 1 gate
```

Try it:

```sh
curl http://127.0.0.1:8471/p/services
curl http://127.0.0.1:8471/twin/services
curl -X POST http://127.0.0.1:8471/cli \
  -H 'content-type: application/json' -d '{"command":"twin contact"}'
```
