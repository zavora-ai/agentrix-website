# agentrix-site-mail

Phase 5 email plus support: the Chief of Staff intake drafts one held reply per
unread mail (file mailbox offline, live IMAP behind the same trait), tiers each
draft — Public, Standard, Sensitive — and files its draft card. The outbox has
exactly two send paths: Public drafts send directly, anything above needs an
approved send card naming the draft. A tier-above attempt without approval is
refused and files the send card it needs; nothing sends twice.

```sh
cargo test -p agentrix-site-mail   # the Phase 5 gate
```
