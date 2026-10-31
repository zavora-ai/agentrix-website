//! Agentrix Website Phase 5: email plus support.
//!
//! The Chief of Staff target reads a [`Mailbox`], drafts one held reply per unread
//! mail, classifies each draft's [`Tier`], and files it as a draft card — nothing
//! sends on intake. The [`Outbox`] has exactly two send paths: [`Outbox::send_public`]
//! takes only [`Tier::Public`] drafts, and [`Outbox::send_approved`] takes any
//! draft plus an approved send card naming it. A tier-above send without approval
//! is impossible: there is no third path, and the attempt files the send card it
//! needs instead of sending.

pub mod live;
pub mod mail;

pub use live::{ImapMailbox, SmtpSender, parse_rfc822};
pub use mail::{
    DraftReply, FileMailbox, InboundMail, LogSender, Mailbox, Outbox, SendError, Sender, SentMail,
    Tier,
};
