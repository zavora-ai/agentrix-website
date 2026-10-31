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

Live mail: Gmail by default (`GMAIL_USER`, `GMAIL_APP_PASSWORD` — an app
password, not the account password). `ImapMailbox` reads unseen INBOX mail over
TLS; `SmtpSender` delivers over STARTTLS. Ignored live tests touch a real
mailbox — `SMTP_TEST_TO` takes the probe mail:

```sh
cargo test -p agentrix-site-mail -- --ignored
```
