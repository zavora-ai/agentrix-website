//! The human shop: product listing and detail over the served release and the
//! live stock book. `?intent=` re-ranks the listing by intent overlap — the
//! display agent's deterministic core, no model in the path.

use std::sync::Arc;

use agentrix_site_commerce::Commerce;
use agentrix_site_edge::{Current, Page};
use axum::{
    Router,
    extract::{Path, Query, State},
    response::Html,
    routing::get,
};
use serde::Deserialize;

/// What the shop reads.
#[derive(Clone)]
pub struct ShopState {
    /// The served release.
    pub current: Arc<Current>,
    /// Live stock and prices.
    pub commerce: Arc<Commerce>,
    /// The site served.
    pub site: String,
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 1)
        .map(|word| word.to_lowercase())
        .collect()
}

/// Ranks `products` for `intent` by token overlap across title, intro and
/// facts, stable over the release order. Empty intent keeps release order.
pub fn rank<'a>(intent: &str, products: &[&'a Page]) -> Vec<(&'a Page, f64)> {
    let wants = tokens(intent);
    let mut scored: Vec<(&Page, f64, usize)> = products
        .iter()
        .enumerate()
        .map(|(n, page)| {
            let hay = format!(
                "{} {} {}",
                page.title,
                page.intro,
                page.facts
                    .iter()
                    .map(|fact| fact.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            let words = tokens(&hay);
            let hits = wants.iter().filter(|tok| words.contains(tok)).count();
            let score = if wants.is_empty() {
                0.0
            } else {
                hits as f64 / wants.len() as f64
            };
            (*page, score, n)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.2.cmp(&b.2)));
    scored
        .into_iter()
        .map(|(page, score, _)| (page, score))
        .collect()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Debug, Deserialize)]
struct ListingQuery {
    #[serde(default)]
    intent: String,
}

/// The listing: every product with its price and live stock, intent-ranked.
async fn listing(
    State(state): State<Arc<ShopState>>,
    Query(query): Query<ListingQuery>,
) -> Html<String> {
    let release = state.current.get();
    let products: Vec<&Page> = release
        .pages
        .iter()
        .filter(|page| page.kind == awp_types::ContentKind::Product)
        .collect();
    let mut items = String::new();
    for (page, _) in rank(&query.intent, &products) {
        let price = page
            .facts
            .iter()
            .find(|fact| fact.id == "price")
            .map(|fact| fact.text.as_str())
            .unwrap_or("price on request");
        let stock = state
            .commerce
            .stock_of(&state.site, &page.slug)
            .await
            .ok()
            .flatten();
        let availability = match stock {
            Some(row) if row.qty > 0 => format!("{} in stock", row.qty),
            Some(_) => "out of stock".into(),
            None => "availability on request".into(),
        };
        items.push_str(&format!(
            "      <li><a href=\"/shop/{}\">{}</a> — {} ({})</li>\n",
            escape(&page.slug),
            escape(&page.title),
            escape(price),
            availability,
        ));
    }
    Html(format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n  <head><meta charset=\"utf-8\"><title>Shop{}</title></head>\n  <body><main><h1>Shop{}</h1><ul>\n{}</ul><footer>Release <code>{}</code></footer></main></body>\n</html>\n",
        if query.intent.is_empty() {
            String::new()
        } else {
            format!(" — {}", escape(&query.intent))
        },
        if query.intent.is_empty() {
            String::new()
        } else {
            format!(" for “{}”", escape(&query.intent))
        },
        items,
        release.digest,
    ))
}

/// One product: its facts plus live stock.
async fn detail(State(state): State<Arc<ShopState>>, Path(sku): Path<String>) -> Html<String> {
    let release = state.current.get();
    let page = release
        .pages
        .iter()
        .find(|page| page.slug == sku && page.kind == awp_types::ContentKind::Product);
    let Some(page) = page else {
        return Html("<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>Unknown product</title></head><body><main><h1>Unknown product.</h1></main></body>\n</html>\n".into());
    };
    let mut facts = String::new();
    for fact in &page.facts {
        facts.push_str(&format!("      <li>{}</li>\n", escape(&fact.text)));
    }
    let stock = state
        .commerce
        .stock_of(&state.site, &page.slug)
        .await
        .ok()
        .flatten();
    let availability = match stock {
        Some(row) if row.qty > 0 => format!("{} in stock", row.qty),
        Some(_) => "out of stock".into(),
        None => "availability on request".into(),
    };
    Html(format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n  <head><meta charset=\"utf-8\"><title>{}</title></head>\n  <body><main><h1>{}</h1><p>{}</p><p>{}</p><ul>\n{}</ul><footer>Release <code>{}</code></footer></main></body>\n</html>\n",
        escape(&page.title),
        escape(&page.title),
        escape(&page.intro),
        availability,
        facts,
        release.digest,
    ))
}

/// The shop routes over shared `state`.
pub fn shop_routes(state: Arc<ShopState>) -> Router {
    Router::new()
        .route("/shop", get(listing))
        .route("/shop/{sku}", get(detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentrix_site_edge::shop_site;

    #[test]
    fn intent_ranks_coffee_first() {
        let pages = shop_site();
        let products: Vec<&Page> = pages
            .iter()
            .filter(|page| page.kind == awp_types::ContentKind::Product)
            .collect();
        let ranked = rank("coffee roast", &products);
        assert_eq!(ranked[0].0.slug, "arabica-250g");
        assert!(ranked[0].1 > ranked[1].1);
        let plain = rank("", &products);
        assert_eq!(
            plain[0].0.slug, "sukuma-bunch",
            "empty intent keeps release order"
        );
    }
}
