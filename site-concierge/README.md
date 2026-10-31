# agentrix-site-concierge

Phase 4 concierge plus signals: each site owns a chunk corpus keyed by
`(site, chunk)`, retrieved per site in SQL so cross-site reads are structural,
not policed. Answers quote retrieved chunks verbatim with chunk, document and
source citations; anything below the grounding threshold becomes a content-gap
signal instead of a guess. Conversation events emit the four typed signals —
content gaps, review sightings, lead captures, support escalations. Visitors are
random server-issued ids, never network-derived.

```sh
cargo test -p agentrix-site-concierge   # the Phase 4 gate
```
