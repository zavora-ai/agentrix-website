//! Live mail: IMAP intake and SMTP sending, with Gmail defaults.
//!
//! [`ImapMailbox`] reads unseen mail over TLS; [`SmtpSender`] delivers replies.
//! Both take explicit hosts or Gmail's (`imap.gmail.com:993`,
//! `smtp.gmail.com:587`) with an app password from the environment — Gmail no
//! longer takes account passwords for IMAP or SMTP.

use crate::mail::{InboundMail, Mailbox, SendError, Sender, SentMail};
use mailparse::MailHeaderMap as _;

/// Reads unseen mail from an IMAP INBOX over TLS.
pub struct ImapMailbox {
    host: String,
    port: u16,
    user: String,
    pass: String,
}

impl ImapMailbox {
    /// A mailbox on `host:port` logging in as `user`.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        user: impl Into<String>,
        pass: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            user: user.into(),
            pass: pass.into(),
        }
    }

    /// Gmail's mailbox: `imap.gmail.com:993` with an app password.
    pub fn gmail(user: impl Into<String>, app_password: impl Into<String>) -> Self {
        Self::new("imap.gmail.com", 993, user, app_password)
    }

    /// Gmail's mailbox from `GMAIL_USER` and `GMAIL_APP_PASSWORD`.
    pub fn gmail_from_env() -> Result<Self, SendError> {
        let var = |name: &str| {
            std::env::var(name).map_err(|_| SendError::Sender(format!("{name} is not set")))
        };
        Ok(Self::gmail(var("GMAIL_USER")?, var("GMAIL_APP_PASSWORD")?))
    }
}

/// Parses one RFC 822 message into `(from, subject, body text)`: pure, so tests
/// pin it against a fixture without a server.
pub fn parse_rfc822(bytes: &[u8]) -> Result<(String, String, String), SendError> {
    let parsed = mailparse::parse_mail(bytes).map_err(|e| SendError::Sender(e.to_string()))?;
    let header = |name: &str| parsed.headers.get_first_value(name).unwrap_or_default();
    let body = parsed.get_body().unwrap_or_default();
    Ok((header("From"), header("Subject"), body))
}

impl Mailbox for ImapMailbox {
    fn unread(&self) -> Result<Vec<InboundMail>, SendError> {
        let failed = |e: imap::Error| SendError::Sender(e.to_string());
        let tls = native_tls::TlsConnector::builder()
            .build()
            .map_err(|e| SendError::Sender(e.to_string()))?;
        let client = imap::connect((self.host.as_str(), self.port), self.host.as_str(), &tls)
            .map_err(failed)?;
        let mut session = client
            .login(self.user.as_str(), self.pass.as_str())
            .map_err(|(e, _)| failed(e))?;
        session.select("INBOX").map_err(failed)?;
        let uids = session.search("UNSEEN").map_err(failed)?;
        let mut mail = Vec::new();
        for uid in uids.iter().take(50) {
            let fetched = session.fetch(uid.to_string(), "(RFC822)").map_err(failed)?;
            for message in fetched.iter() {
                let Some(bytes) = message.body() else {
                    continue;
                };
                let (from, subject, body) = parse_rfc822(bytes)?;
                mail.push(InboundMail {
                    id: format!("imap-{uid}"),
                    from,
                    subject: subject.trim().to_string(),
                    body: body.trim().to_string(),
                });
            }
        }
        let _ = session.logout();
        Ok(mail)
    }
}

/// Delivers mail over SMTP with STARTTLS.
pub struct SmtpSender {
    host: String,
    user: String,
    pass: String,
}

impl SmtpSender {
    /// A sender relaying through `host` (port 587, STARTTLS) as `user`.
    pub fn relay(
        host: impl Into<String>,
        user: impl Into<String>,
        pass: impl Into<String>,
    ) -> Self {
        Self {
            host: host.into(),
            user: user.into(),
            pass: pass.into(),
        }
    }

    /// Gmail's sender: `smtp.gmail.com` with an app password.
    pub fn gmail(user: impl Into<String>, app_password: impl Into<String>) -> Self {
        Self::relay("smtp.gmail.com", user, app_password)
    }

    /// Gmail's sender from `GMAIL_USER` and `GMAIL_APP_PASSWORD`.
    pub fn gmail_from_env() -> Result<Self, SendError> {
        let var = |name: &str| {
            std::env::var(name).map_err(|_| SendError::Sender(format!("{name} is not set")))
        };
        Ok(Self::gmail(var("GMAIL_USER")?, var("GMAIL_APP_PASSWORD")?))
    }

    /// Builds the message without sending: pure, so tests pin it.
    pub fn message(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<lettre::Message, SendError> {
        lettre::Message::builder()
            .from(
                self.user
                    .parse()
                    .map_err(|e| SendError::Sender(format!("bad from: {e}")))?,
            )
            .to(to
                .parse()
                .map_err(|e| SendError::Sender(format!("bad to: {e}")))?)
            .subject(subject)
            .body(body.to_string())
            .map_err(|e| SendError::Sender(e.to_string()))
    }
}

impl Sender for SmtpSender {
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<SentMail, SendError> {
        use lettre::transport::smtp::authentication::Credentials;
        use lettre::{SmtpTransport, Transport as _};
        let email = self.message(to, subject, body)?;
        let mailer = SmtpTransport::relay(&self.host)
            .map_err(|e| SendError::Sender(e.to_string()))?
            .credentials(Credentials::new(self.user.clone(), self.pass.clone()))
            .build();
        mailer
            .send(&email)
            .map_err(|e| SendError::Sender(e.to_string()))?;
        Ok(SentMail {
            id: format!("smtp-{}", uuid::Uuid::new_v4().as_simple()),
            draft: String::new(),
            to: to.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
        })
    }
}
