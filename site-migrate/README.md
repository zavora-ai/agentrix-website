# agentrix-site-migrate

Phase 9 credential-free migration: public pages are fetched through an
auth-less `Fetch` trait (`FileFetch` serves local files offline), parsed into
text, links, products and metadata, and built into a candidate release whose
sources are the evidence URLs. The parity scorer rates content, links,
products and metadata per axis; below 0.95 overall the `MigrationParity` gate
check refuses and the candidate is never served. At or above threshold,
`file_go_live` files the owner's card with the full report attached.

```sh
cargo test -p agentrix-site-migrate   # the Phase 9 gate
```

Live fetching reads public URLs over HTTP (`HttpFetch`): 15-second timeout,
5 MB body cap, redirects followed. Proven against a loopback server in tests;
no credentials exist on this path.
