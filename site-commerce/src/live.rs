//! Live charges through Stripe PaymentIntents: create and confirm in one call.
//!
//! Amounts convert from the ledger's micro-USD to Stripe's minor units at charge
//! time (1 cent = 10,000 micro); anything below one cent refuses rather than
//! rounding to zero. Reads `STRIPE_SECRET_KEY` when built with
//! [`from_env`](StripeProvider::from_env) — test-mode keys for validation.

use std::time::Duration;

use crate::payments::{PaymentError, PaymentIntent, PaymentProvider};

/// Charges intents through Stripe: the intent must carry a `payment_method`
/// (such as a saved `pm_*` id or a test `pm_card_visa`).
pub struct StripeProvider {
    secret: String,
    client: reqwest::blocking::Client,
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
            client,
        })
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
        let answer: serde_json::Value = self
            .client
            .post("https://api.stripe.com/v1/payment_intents")
            .basic_auth(self.secret.clone(), Option::<String>::None)
            .form(&[
                ("amount", Self::minor_units(intent.amount_micro)?.as_str()),
                ("currency", intent.currency.to_lowercase().as_str()),
                ("payment_method", method),
                ("confirm", "true"),
                ("description", intent.purpose.as_str()),
            ])
            .send()
            .map_err(failed)?
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
