//! Agentrix Website Phase 7: commerce.
//!
//! Payments are held before they move: [`Commerce::propose`] records the intent
//! and files its approval card (the 202: accepted, not done), the owner approves
//! through the inbox, and [`Commerce::complete`] verifies that approval, reserves
//! the provider fee against exactly one site's budget, charges, and commits — or
//! releases the fee and marks the intent failed. Proposing twice under one
//! idempotency key returns one intent; completing twice charges once.

pub mod payments;

pub use payments::{
    Commerce, IntentState, LogProvider, NewPayment, PaymentError, PaymentIntent, PaymentProvider,
    Receipt,
};
