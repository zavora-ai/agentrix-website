//! Live charges through Stripe PaymentIntents: create and confirm in one call.
//!
//! Amounts convert from the ledger's micro-USD to Stripe's minor units at charge
//! time (1 cent = 10,000 micro); anything below one cent refuses rather than
//! rounding to zero. Reads `STRIPE_SECRET_KEY` when built with
//! [`from_env`](StripeProvider::from_env) — test-mode keys for validation.

use std::sync::Arc;
use std::time::Duration;

use agentrix_site_defense::{Breaker, BreakerCall};

use crate::payments::{PaymentError, PaymentIntent, PaymentProvider};

/// Charges intents through Stripe: the intent must carry a `payment_method`
/// (such as a saved `pm_*` id or a test `pm_card_visa`).
pub struct StripeProvider {
    secret: String,
    api_base: String,
    client: reqwest::blocking::Client,
    breaker: Arc<Breaker>,
}

impl StripeProvider {
    /// A provider on `secret`.
    pub fn new(secret: impl Into<String>) -> Result<Self, PaymentError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("agentrix-site/0.1")
            .build()
            .map_err(|e| PaymentError::Provider(e.to_string()))?;
        Ok(Self {
            secret: secret.into(),
            api_base: "https://api.stripe.com/v1".into(),
            client,
            breaker: Arc::new(Breaker::new("stripe", 5, Duration::from_secs(60))),
        })
    }

    /// Points the provider at another API base (proxies, mocks).
    pub fn with_api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = base.into();
        self
    }

    /// Guards the provider with `breaker` instead of the default.
    pub fn with_breaker(mut self, breaker: Arc<Breaker>) -> Self {
        self.breaker = breaker;
        self
    }

    /// A provider from `STRIPE_SECRET_KEY`.
    pub fn from_env() -> Result<Self, PaymentError> {
        let secret = std::env::var("STRIPE_SECRET_KEY")
            .map_err(|_| PaymentError::Provider("STRIPE_SECRET_KEY is not set".into()))?;
        Self::new(secret)
    }

    /// Minor units for `amount_micro`, or why the amount cannot charge.
    pub fn minor_units(amount_micro: i64) -> Result<String, PaymentError> {
        let cents = amount_micro / 10_000;
        if cents < 1 {
            return Err(PaymentError::Provider(format!(
                "amount {amount_micro} micro-USD is below one cent"
            )));
        }
        Ok(cents.to_string())
    }
}

impl PaymentProvider for StripeProvider {
    fn charge(&self, intent: &PaymentIntent) -> Result<String, PaymentError> {
        let method = intent
            .payment_method
            .as_deref()
            .ok_or_else(|| PaymentError::Provider("the intent has no payment method".into()))?;
        let failed = |e: reqwest::Error| PaymentError::Provider(e.to_string());
        let amount = Self::minor_units(intent.amount_micro)?;
        let currency = intent.currency.to_lowercase();
        let url = format!("{}/payment_intents", self.api_base);
        let secret = self.secret.clone();
        let purpose = intent.purpose.to_string();
        let answer: serde_json::Value = self
            .breaker
            .call(
                3,
                Duration::from_millis(500),
                || {
                    let answer = self
                        .client
                        .post(&url)
                        .basic_auth(secret.clone(), Option::<String>::None)
                        .form(&[
                            ("amount", amount.as_str()),
                            ("currency", currency.as_str()),
                            ("payment_method", method),
                            ("confirm", "true"),
                            ("description", purpose.as_str()),
                        ])
                        .send()
                        .map_err(|e| (e.to_string(), true))?;
                    let status = answer.status();
                    if status.is_server_error() || status.as_u16() == 429 {
                        return Err((format!("stripe status {status}"), true));
                    }
                    Ok(answer)
                },
                |(_, retryable): &(String, bool)| *retryable,
            )
            .map_err(|call| match call {
                BreakerCall::Open(open) => PaymentError::Provider(open.to_string()),
                BreakerCall::Error((reason, _)) => PaymentError::Provider(reason),
            })?
            .json()
            .map_err(failed)?;
        if let Some(id) = answer.get("id").and_then(serde_json::Value::as_str) {
            return Ok(id.to_string());
        }
        let message = answer
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("stripe returned no charge id");
        Err(PaymentError::Provider(message.to_string()))
    }
}
