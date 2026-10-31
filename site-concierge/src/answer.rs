//! Grounded answers: extractive quotes over retrieved chunks, each with a citation.
//!
//! The concierge never paraphrases from memory: [`answer`] quotes the top chunks
//! verbatim and cites each quote's chunk, document and source. Below the grounding
//! threshold it says what it could not find — the caller files that as a gap
//! signal — instead of guessing.

use serde::{Deserialize, Serialize};

use crate::corpus::{Corpus, CorpusError, ScoredChunk, retrieve};
use crate::visitor::Visitor;

/// The minimum top-chunk score for a grounded answer; below it, the concierge
/// declines and the question becomes a gap signal.
pub const GROUNDING_THRESHOLD: f64 = 0.5;

/// One citation: the quote's chunk, document and evidence source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// The chunk quoted.
    pub chunk_id: String,
    /// Its document.
    pub doc: String,
    /// Its evidence source.
    pub source_uri: String,
    /// The quoted text, verbatim.
    pub quote: String,
}

/// An answer to a visitor: quotes plus citations, or an honest miss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    /// Who was answered.
    pub visitor: String,
    /// The answer text: quotes, or what could not be found.
    pub text: String,
    /// One citation per quote; empty on a miss.
    pub citations: Vec<Citation>,
    /// Whether the answer is grounded in retrieved chunks.
    pub grounded: bool,
}

/// Answers `visitor`'s `question` from `site`'s corpus: the top 3 chunks above
/// [`GROUNDING_THRESHOLD`], quoted and cited, or a miss naming the question.
pub async fn answer(
    corpus: &Corpus,
    site: &str,
    visitor: &Visitor,
    question: &str,
) -> Result<Answer, CorpusError> {
    debug_assert_eq!(visitor.site, site, "a visitor asks its own site");
    let hits = retrieve(corpus, site, question, 3).await?;
    let grounded: Vec<&ScoredChunk> = hits
        .iter()
        .filter(|hit| hit.score >= GROUNDING_THRESHOLD)
        .collect();
    if grounded.is_empty() {
        return Ok(Answer {
            visitor: visitor.id.clone(),
            text: format!("I could not find that in our pages: {question}"),
            citations: Vec::new(),
            grounded: false,
        });
    }
    let mut text = String::from("Here is what our pages say:\n");
    let mut citations = Vec::new();
    for (n, hit) in grounded.iter().enumerate() {
        text.push_str(&format!("{}. {}\n", n + 1, hit.chunk.text));
        citations.push(Citation {
            chunk_id: hit.chunk.chunk_id.clone(),
            doc: hit.chunk.doc.clone(),
            source_uri: hit.chunk.source_uri.clone(),
            quote: hit.chunk.text.clone(),
        });
    }
    Ok(Answer {
        visitor: visitor.id.clone(),
        text,
        citations,
        grounded: true,
    })
}
