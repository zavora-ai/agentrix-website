# Agentrix Website — Visual Concept

**Introducing the world's first Website 4.0 for the agentic era.**

Status: complete vision draft. This document is normative for design and
demo work; [SUBSTRATE.md](SUBSTRATE.md) says how much of it ADK-Rust already
implements, [SLICES.md](SLICES.md) how we get there without shrinking it.

## 0. The idea in one frame

A Website 4.0 is not pages you navigate. It is **a place you enter, run by
agents you can see working**, built on an operating system (`adk-kernel`)
that schedules them, a harness (`adk-harness`) that governs them, and a
publishing stack (`adk-website`, `adk-site`, `adk-awp`) that serves two
audiences from one origin: people get a place, AI agents get a protocol.

The visual thesis: **make time, work, and trust visible.** Old websites hide
everything behind static pages. A Website 4.0 shows the business *happening*:
cards bloom where work finishes, agents hold their hands up when they need
the owner, the night shift leaves a legible trail.

Three non-negotiables:

1. **It must never look like a bot.** The human surface is a calm, real
   website — typography, photography, whitespace. Intelligence appears as
   competent staff, not chrome. No chatbot bubble as the centerpiece, no
   purple gradients, no "AI-powered" confetti.
2. **Autonomy must be visible to be believed.** Every autonomous act leaves
   a card. Every held act waits in the open. The owner approves the ladder
   once, not every click.
3. **One business, two audiences, one release.** The spatial World the
   visitor walks in and the AWP machine twins an AI agent consumes are the
   same signed release. Split-screen them and the digests match.

## 1. Design principles (inherited, extended)

From Agentrix OS (spatial field, verb UI, worlds, honest data) and ADK-Rust
(governed tool path, harness themes, strict schemas):

| # | Principle | What it forbids |
|---|---|---|
| 1 | The place is real first | Landing theatre that breaks reading, checkout, or accessibility |
| 2 | Agents are staff, not mascots | Cute avatars doing fake work; unnamed "AI" doing everything |
| 3 | Verbs over menus | Burying "track my order" three clicks deep when the visitor can say it |
| 4 | Work blooms where it finishes | Toasts and badges divorced from the place the work belongs |
| 5 | Holds are hospitality | Hiding approval gates, or apologizing for them |
| 6 | The night is a feature | A site that looks identical at 09:00 and after a worked night |
| 7 | Honest data, labeled | Mock spend, fake live voice, scripted "autonomy" presented as live |
| 8 | Dual audience, one truth | A degraded bot-skin for agents; scraped HTML as the agent contract |
| 9 | Calm at rest | Ambient motion when nothing is happening; the field sleeps |

## 2. Anatomy of a World

Every Agentrix site is a **World** with the same anatomy; businesses differ
in districts staffed, theme, and autonomy levels.

```
┌──────────────────────────────────────────────────────────────┐
│  SKY — the business at a glance (name, open state, weather)  │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│   STOREFRONT DISTRICT        STORY DISTRICT     (visitor)    │
│   shop · book · order        about · journal · proof         │
│                                                              │
│        ┌── CONCIERGE — the greeter, center of the field ──┐  │
│        │  "Karibu. Order, book, or just looking around?"  │  │
│        └──────────────────────────────────────────────────┘  │
│                                                              │
│   SERVICE DISTRICT           EVENTS DISTRICT                 │
│   support · returns · FAQ    shows · launches · live         │
├──────────────────────────────────────────────────────────────┤
│  GATE — verb bar: "Track my order…"  (text · voice · tap)    │
├──────────────────────────────────────────────────────────────┤
│  OWNER'S DOOR (authenticated) → BACK-OFFICE WORLD            │
│  Editor · Marketer · Watchman · Cashier · Keeper + Inbox     │
└──────────────────────────────────────────────────────────────┘
         Under the floorboards: AWP edge (agents only)
         /.well-known/awp.json · capabilities · signed log
```

### 2.1 The Sky

A single quiet band: business name in its own type, an **open state** a
human understands ("Open · kitchen till 22:00" / "Closed · opens 08:00,
the night team is on"), and ambient truth (season, local weather, a
campaign in flight). No hero carousel. The Sky is generated per release
and per charter state — when the Watchman held a publish at 03:12, the
Sky does not pretend otherwise.

### 2.2 Districts (visitor world)

- **Storefront** — products, booking, ordering. Cards are stock-true:
  price and availability come from capability calls, not page copy, so
  the human page and the agent twin cannot drift.
- **Story** — about, journal, proof (reviews, press). The Editor's
  published work lives here; drafts never do.
- **Service** — support threads, returns, sizing, care. Conversations are
  cards in place, attached to the product they concern.
- **Events** — launches, live sessions, tastings, streams. A live event
  is a district room, not a bolted-on embed (see §7).

Districts are spatial but not literal: on desktop a gentle field the
visitor can pan a few degrees; on mobile a stacked, district-headed
scroll. Spatial is a *memory aid* ("support is bottom-left"), never a
navigation tax.

### 2.3 The Concierge

Center of the field: the site's greeter, a named staff card (the business
names it; default "Concierge"). It speaks first, briefly, in the visitor's
language, offers the three most likely verbs for that district, and
remembers within the session. It is backed by the public partition only
(search, reply, signal) — it *cannot* publish, refund, or see back-office
data, and the UI must make that safety feel like good manners, not a
limitation: "I can check that order for you; refunds go through Amina at
the desk — shall I bring her in?"

### 2.4 The Gate (verb bar)

A persistent bottom bar, thumb-reachable: a text field with the business's
verb grammar (§5), voice input (push-to-talk; file ASR until realtime
ships, labeled honestly), and district shortcuts. The Gate is the primary
interface; menus are the fallback for browsing. Every verb resolves to a
declared capability — the Gate never free-forms an action the site has
not declared.

### 2.5 The Back-Office World (owner)

Behind the Owner's Door (authenticated, separate partition): the same
spatial language, staffed by job agents — Editor, Marketer, Watchman,
Cashier, Keeper — plus the **Inbox** (held acts), the **Charter** (what
the business is for, its KPIs, its guardrails), the **Ladder board**
(each job's autonomy level), and the **Night Report** (what happened
while the owner slept). Owner flips worlds with one gesture; the flip is
the Agentrix two-worlds move, partitions reconciled only by the kernel.

### 2.6 Under the floorboards (agentic audience)

Invisible to humans, first-class to agents: `/.well-known/awp.json`,
machine twins per page, signed status, tiered capabilities (A2A, MCP)
with consent receipts and held calls. In the visual concept this layer is
shown only in **Agent View** — a diagnostic toggle that renders the same
World as its machine twin (manifest, tiers, digests), used in demos to
prove one-release truth.

## 3. Card taxonomy

Cards are the unit of visible work, inherited from the Agentrix field.
Every card carries: agent (named staff), district, state, provenance
(Known/Assumed), and — when held — its decision affordances.

| Card | Blooms when | States | Human sees |
|---|---|---|---|
| Greeting | Visitor arrives (session) | live | Concierge's opening + likely verbs |
| Answer | Concierge/agent replies | streaming → settled | Grounded reply; sources expandable |
| Product | Storefront capability returns | live | Price/stock-true item, book/buy verbs |
| Thread | Support conversation moves | live, held | Chat in place; "with a person" when escalated |
| Draft | Back-office job proposes content | proposed → approved → published | Page-diff preview, claim citations |
| Campaign | Marketer proposes outreach | proposed → held → running → measured | Channels, budget cap, conversion trail |
| Held (decision) | Any act above its ladder tier | waiting → decided | Approve / edit / reject, single or batch |
| Incident | Watchman detects | raised → acting → resolved | What happened, what it did, what it held |
| Digest | Night/morning boundary | published | The Night Report: acts, spend, signals |
| Signal | Visitor asks for something undeclared | sent | "Passed to the owner" — one-way, screened |
| Release | Publish completes | published | Version, digest, rollback point |
| Event | Live session runs | scheduled → live → replay | Room card with agent host + human host |

Card rules: drag-to-fuse (visitor: "this product + this booking"; owner:
"these three drafts as one campaign") and fling-to-dismiss come from
Agentrix unchanged. Cards never appear from nowhere: each bloom animates
from its agent's station. Held cards are visually distinct by *posture*
(raised, edged in the harness waiting colour), not alarm red.

## 4. Visual language

- **Base**: the ADK-Rust harness tokens — burnt orange `#f74c00` light /
  navy `#0a1628` dark, WCAG themes — extended per business by its own
  brand extraction during migration (type, palette, photography). The
  system, not the business, owns layout and state colours; the business
  owns voice and imagery. A Zavora site must be recognizable as *theirs*
  first, as Website 4.0 second.
- **Type**: the business's own (extracted or chosen); system UI in the
  harness sans. No gradient headline text. No emoji as iconography.
- **Space**: generous, gallery-calm. Districts are separated by air and
  a heading in the business's voice, not boxes-in-boxes.
- **Photography/render**: real business photography wins; Muse Image
  renders fill gaps and are labeled "concept render" until replaced
  (the Ideate `thumb`/`hero` pattern, carried over).
- **The anti-bot test**: screenshot any visitor page, remove the verb
  bar — it must pass as a well-made 2026 website. Intelligence is
  revealed by behaviour over time, never by costume.

## 5. Verb grammar

The Gate understands verbs, not keywords. Grammar is declared per
district from the site's capability registry, so the UI can promise only
what the business can do:

| Form | Examples (storefront) | Resolves to |
|---|---|---|
| Ask | "Do you ship to Mombasa?" "Is this in my size?" | search_site + RAG-grounded answer |
| Transact | "Book Friday 19:00 for two" "Reorder the usual" | booking/order capability (consent + idempotency) |
| Track | "Where is my order?" "Is my return done?" | status capability, identity-checked |
| Make | "Gift-wrap it with a note" "Design one like this" | held capability or Editor job |
| Help | "This arrived broken" "Talk to a person" | support thread; human escalation verb always present |
| Follow | "Tell me when the blue one returns" | signal → back-office job wake |

Rules: every verb shows its consequence before it fires ("Send this
reply?" at the ladder tier that requires it); destructive/financial
verbs are never one-tap from the visitor side; the human-escalation
verb is always visible, in the visitor's language, and its presence is
a trust feature we point at in demos.

## 6. Motion and time

- **Rest is rest.** No ambient animation. The field breathes only when
  work happens.
- **Blooms** (280–420ms, ease-out, from the acting agent's station)
  mark new work; **settles** mark completion; **raises** mark holds.
  All respect `prefers-reduced-motion` (cross-fade only).
- **Time scrub (owner)**: the Night Report is scrubbable — drag across
  03:12 and watch the Watchman hold the publish, the Concierge answer
  in French, the spend meter stay under charter cap. This scrub is the
  single most persuasive visual in the system; it is a first-class
  screen, not a log viewer.
- **Streaming truth**: panels and replies stream token-by-token (SSE,
  as Ideate does); a card that is thinking shows *which* agent and
  *which stage* ("Engineer costing…"), never a bare spinner.

## 7. Signature moments (the demo lives here)

1. **The Arrival** — the World resolves around the visitor: Sky sets
   open state, Concierge greets in their language, districts settle.
   Ten seconds, zero clicks, unmistakably a *place*.
2. **The Verb** — "Book Friday for two, somewhere quiet" becomes a
   booking card with a held confirmation; the visitor edits the card,
   not a form.
3. **The Night Shift** — owner flips to Back-Office, time-scrubs the
   night: defense incident resolved, two emails sent in-ladder, one
   held, a draft bloomed. Approves the held two with one gesture.
4. **The Mirror** — Agent View: same World as machine twin; an AI agent
   buys over AWP with a consent receipt and a held payment; digests
   under both views match.
5. **The Birth** — a WordPress URL pasted into Zavora crystallizes into
   a World card by card, gate stamping verified claims, holding the
   rest. (Full choreography in STORYBOARD.md.)

## 8. Responsive and accessibility

- Mobile is one column, districts headed, Gate thumb-fixed; spatial
  becomes sequence, never a shrunk canvas. Spatial memory is preserved
  by district order and colour.
- WCAG targets ride ADK themes; cards are list-navigable; verb results
  are announced; every spatial action has a list equivalent.
- Performance is a trust feature: static-first human pages (nothing
  runs at request time), streaming only where work is live, images
  sized per slot (hero + 512px cards, the Ideate budget).

## 9. What this is not

- Not a chatbot skin on a static site (the World is the site).
- Not a fully autonomous site (the ladder and holds are the product).
- Not a protocol pitch with a demo attached (AWP is the floorboards;
  the World is the headline).
- Not Agentrix OS renamed (OS runs a person's life; Website runs a
  business's presence; they share kernel, field, and verb grammar).
