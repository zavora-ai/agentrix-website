import React, { useMemo } from "react";
import type { Caption } from "@remotion/captions";
import { createTikTokStyleCaptions } from "@remotion/captions";
import {
  AbsoluteFill,
  CanvasImage,
  Easing,
  Sequence,
  interpolate,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";

const ORANGE = "#f74c00";
const NAVY = "#0a1628";
const INK = "#171623";
const PAPER = "#ffffff";
const MUTED = "#5d5a75";

const fade = (frame: number, start: number, dur = 18) =>
  interpolate(frame, [start, start + dur], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });
const rise = (frame: number, start: number, dur = 20) =>
  interpolate(frame, [start, start + dur], [26, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: Easing.bezier(0.16, 1, 0.3, 1),
  });

type Scene = {
  label: string;
  title: string;
  caption: string;
  seconds: number;
  dark?: boolean;
  visual: React.ReactNode;
};

const Stat: React.FC<{ n: string; t: string; delay: number }> = ({ n, t, delay }) => {
  const frame = useCurrentFrame();
  return (
    <div style={{ opacity: fade(frame, delay), translate: `0 ${rise(frame, delay)}px`, borderTop: `4px solid ${ORANGE}`, paddingTop: 14 }}>
      <div style={{ fontSize: 64, fontWeight: 800, letterSpacing: -2, lineHeight: 1 }}>{n}</div>
      <div style={{ fontSize: 21, color: MUTED, marginTop: 8, lineHeight: 1.35 }}>{t}</div>
    </div>
  );
};

const STATS = [
  { n: "43%", t: "of all websites run WordPress — over 200 million sites built on yesterday's model.", s: "W3Techs, Oct 2025" },
  { n: "58%", t: "fewer clicks for the #1 result when AI answers first instead of your website.", s: "Ahrefs · 300,000 keywords" },
  { n: "47 hrs", t: "the average business takes to reply to a website lead. Buyers decide in 5 minutes.", s: "Harvard Business Review / MIT" },
  { n: "$4.4M", t: "average cost of one data breach. Small firms lose about $120,000 per attack.", s: "IBM Cost of a Data Breach 2025" },
];
const StatsCarousel: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const per = 3.1 * fps;
  const active = Math.min(STATS.length - 1, Math.floor(frame / per));
  const local = frame - active * per;
  const st = STATS[active];
  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column", justifyContent: "center", padding: "48px 52px" }}>
      <div key={active} style={{ opacity: fade(local, 2, 14), translate: `0 ${rise(local, 2, 16)}px` }}>
        <div style={{ fontSize: 128, fontWeight: 850, letterSpacing: -4, lineHeight: 1, color: ORANGE }}>{st.n}</div>
        <div style={{ fontSize: 33, lineHeight: 1.4, marginTop: 22, maxWidth: 760 }}>{st.t}</div>
        <div style={{ fontSize: 19, color: MUTED, marginTop: 18 }}>{st.s}</div>
      </div>
      <div style={{ display: "flex", gap: 10, marginTop: 44 }}>
        {STATS.map((_, i) => (
          <span key={i} style={{ width: i === active ? 44 : 16, height: 8, borderRadius: 4, background: i === active ? ORANGE : "#ddd9ee", transition: "none" }} />
        ))}
      </div>
      <div style={{ fontSize: 18, color: MUTED, marginTop: 16 }}>{active + 1} of {STATS.length} — the cost of a website that just sits there</div>
    </div>
  );
};

const Img: React.FC<{ src: string }> = ({ src }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  return (
    <CanvasImage
      src={staticFile(src)}
      style={{
        width: "100%",
        height: "100%",
        objectFit: "cover",
        objectPosition: "top left",
        scale: interpolate(frame, [0, 8 * fps], [1.05, 1], { extrapolateRight: "clamp" }),
      }}
    />
  );
};

const Chips: React.FC<{ items: string[] }> = ({ items }) => {
  const frame = useCurrentFrame();
  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: 12, alignContent: "center", height: "100%", padding: 36 }}>
      {items.map((s, i) => (
        <span key={s} style={{ opacity: fade(frame, 10 + i * 9), translate: `0 ${rise(frame, 10 + i * 9)}px`, fontSize: 25, fontWeight: 650, border: "1.5px solid #d8d5e8", borderRadius: 999, padding: "10px 20px", background: "#fbfaff" }}>{s}</span>
      ))}
    </div>
  );
};

export const SCENES: Scene[] = [
  {
    label: "THE PROBLEM",
    title: "Websites sit there. Businesses can't afford that anymore.",
    caption: "The brochure web is failing on numbers, not opinions. Sources on the final frame.",
    seconds: 13,
    visual: <StatsCarousel />,
  },
  {
    label: "BUILDING IN PUBLIC · DAY 2",
    title: "Introducing Agentrix Website.",
    caption: "The world's first Website 4.0 for the agentic era. By Zavora Technologies — built on ADK-Rust and Meta Muse. Today we show you the concept.",
    seconds: 8,
    dark: true,
    visual: <Img src="concept/01-visitor-world.png" />,
  },
  {
    label: "THE AUDIENCE MOVED",
    title: "Agents now make up 50% of all internet traffic.",
    caption: "Half the web's traffic is now machines reading, comparing, and buying for people (Imperva). A website that cannot serve agents is invisible to half its audience.",
    seconds: 11,
    visual: <Chips items={["People get a place", "Agents get an answer", "Same business", "Same prices", "Same truth", "Served in seconds"]} />,
  },
  {
    label: "THE SOLUTION · SIMPLE VERSION",
    title: "Website 4.0: a website that works.",
    caption: "It greets, sells, publishes, supports, and defends itself — while the owner approves anything that matters. If your website sits there, it's 3.0 or older.",
    seconds: 12,
    dark: true,
    visual: <Img src="concept/04-concept2-website.png" />,
  },
  {
    label: "MEET THE STAFF",
    title: "Meet the website's staff.",
    caption: "Everything on your website is now an agent: the greeter, the shop, every product, the news, the newsletter. It still looks like the website your customers trust — it just works like a team.",
    seconds: 11,
    visual: <Img src="concept/01-visitor-world.png" />,
  },
  {
    label: "CONCEPT 2 · EVERYTHING IS AN AGENT",
    title: "The display is an agent. So is every product.",
    caption: "On familiar retail bones: the Display Agent re-ranks by intent, each product answers, watches prices, and confirms its own orders. News, newsletter, live events — all staffed.",
    seconds: 12,
    visual: <Img src="concept/04-concept2-website.png" />,
  },
  {
    label: "CONCEPT 3 · CENTER STAGE, IN 3D",
    title: "Pick a product up. It introduces itself.",
    caption: "Click and the field recedes; the product takes center stage — hooks, triggers, its own voice, a drag-to-rotate 3D render. Spatial, but never at the cost of trust.",
    seconds: 11,
    visual: <Img src="concept/08-v4-3d-stage.png" />,
  },
  {
    label: "BUSINESS AUTONOMY",
    title: "Your website is now fully autonomous.",
    caption: "It generates leads, converts visitors, supports customers, remarkets to the ones who left, updates the owner every morning, and manages its own costs and stock. The owner sets the goals and approves anything risky.",
    seconds: 12,
    visual: <Chips items={["Generate leads", "Convert visitors", "Support customers", "Remarket", "Owner updates daily", "Manage costs", "Manage inventory", "Owner approves risk"]} />,
  },
  {
    label: "CONCEPT 5 · THE AGENT INTERFACE",
    title: "For agents, the CLI is the interface.",
    caption: "No pages to scrape: a public terminal with verified tiers, capabilities, consent receipts, held calls. One site agent serves thousands of agents at once through kernel queues.",
    seconds: 12,
    visual: <Img src="concept/05-concept2-cli.png" />,
  },
  {
    label: "THE JOURNEY IN",
    title: "WordPress in. Website 4.0 out.",
    caption: "No login, no export file. Paste your address and the public site becomes an Agentrix Website — with the outcomes businesses actually want. Illustrative journeys, Day 3 makes them real.",
    seconds: 11,
    visual: (
      <div style={{ display: "flex", flexDirection: "column", gap: 20, padding: 40, height: "100%", justifyContent: "center" }}>
        {[
          ["WordPress site", "Agentrix Website", "becomes the authority in its niche"],
          ["Shopify store", "Agentrix Website", "sells 70% more, staffed around the clock"],
          ["Quiet blog", "Agentrix Website", "grows to 100k subscribers who actually read"],
        ].map(([a, b, c], i) => (
          <div key={a} style={{ display: "flex", alignItems: "center", gap: 16, fontSize: 25, fontWeight: 650 }}>
            <span style={{ border: "1.5px solid #d8d5e8", borderRadius: 12, padding: "10px 16px", background: "#fff" }}>{a}</span>
            <span style={{ color: ORANGE, fontSize: 30 }}>→</span>
            <span style={{ border: `1.5px solid ${ORANGE}`, borderRadius: 12, padding: "10px 16px", background: "#fff7ed" }}>{b}</span>
            <span style={{ fontSize: 21, color: MUTED }}>{c}</span>
          </div>
        ))}
        <div style={{ fontSize: 18, color: MUTED }}>Illustrative outcomes — the migration itself needs no credentials and no downtime.</div>
      </div>
    ),
  },
  {
    label: "THE LEDGER · WHAT THIS COST",
    title: "$100 built the foundation. $50 remains.",
    caption: "Day 1–2: research, the full Muse provider in ADK-Rust, Ideate with Meta, four visual concepts, this video. Day 3 we code it live — in public, Apache 2.0. Tell us where the $50 should go.",
    seconds: 13,
    dark: true,
    visual: (
      <div style={{ padding: 44, height: "100%", display: "flex", flexDirection: "column", justifyContent: "center", gap: 26, color: "#fff" }}>
        <div style={{ fontSize: 30 }}><b style={{ color: ORANGE }}>$100</b> — foundation · research · concepts · design · plan ✓</div>
        <div style={{ height: 16, borderRadius: 8, background: "#2a2a44", overflow: "hidden" }}><div style={{ width: "66%", height: "100%", background: ORANGE }} /></div>
        <div style={{ fontSize: 30 }}><b style={{ color: ORANGE }}>$50</b> — implementation · Day 3 · coding in public</div>
        <div style={{ fontSize: 34, fontWeight: 800, color: "#fff", border: `2px solid ${ORANGE}`, borderRadius: 14, padding: "14px 20px", textAlign: "center" }}>Day 3 · live coding → github.com/jkmaina/agentrix-website</div>
        <div style={{ fontSize: 24, color: "#c9c6e8" }}>Public repo · Apache 2.0 · What should Day 3 build first — migration, the concierge, or the night shift? Comment below.</div>
      </div>
    ),
  },
];

const CaptionBar: React.FC<{ scene: Scene; dark?: boolean }> = ({ scene, dark }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const captions: Caption[] = useMemo(() => {
    const words = (scene.title + " " + scene.caption).split(/\s+/).filter(Boolean);
    const totalMs = scene.seconds * 1000 - 600;
    const per = totalMs / Math.max(1, words.length);
    return words.map((w, i) => ({
      text: (i === 0 ? "" : " ") + w,
      startMs: 300 + i * per,
      endMs: 300 + (i + 1) * per,
      timestampMs: 300 + i * per,
      confidence: 1,
    }));
  }, [scene]);
  const { pages } = useMemo(
    () => createTikTokStyleCaptions({ captions, combineTokensWithinMilliseconds: 2800 }),
    [captions],
  );
  const nowMs = (frame / fps) * 1000;
  const page = pages.find((pg) => nowMs >= pg.startMs && nowMs < pg.startMs + 2900) ?? pages[pages.length - 1];
  if (!page) return null;
  return (
    <div style={{ position: "absolute", left: 90, bottom: 92, width: 680, pointerEvents: "none" }}>
      <div style={{ fontSize: 27, fontWeight: 650, lineHeight: 1.4, whiteSpace: "pre-wrap", color: dark ? "#fff" : INK }}><span style={{ fontSize: 15, fontWeight: 800, letterSpacing: 2, color: ORANGE, display: "block", marginBottom: 6 }}>● LIVE CAPTION</span>
        {page.tokens.map((tok) => (
          <span key={tok.fromMs} style={{ color: tok.fromMs <= nowMs && tok.toMs > nowMs ? ORANGE : undefined }}>{tok.text}</span>
        ))}
      </div>
    </div>
  );
};

const SceneView: React.FC<{ scene: Scene; index: number }> = ({ scene, index }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const dark = scene.dark;
  return (
    <AbsoluteFill style={{ background: dark ? NAVY : PAPER, color: dark ? "#fff" : INK, fontFamily: "ui-sans-serif, system-ui, sans-serif" }}>
      <div style={{ position: "absolute", top: 0, left: 0, right: 0, height: 64, display: "flex", alignItems: "center", gap: 14, padding: "0 90px", borderBottom: dark ? "1px solid #24365c" : "1px solid #e9e6f5", background: dark ? "rgba(10,22,40,.92)" : "rgba(255,255,255,.92)" }}>
        <span style={{ width: 12, height: 12, borderRadius: "50%", background: ORANGE }} />
        <span style={{ fontSize: 21, fontWeight: 800, letterSpacing: 2.2 }}>DAY 2 — AGENTRIX WEBSITE</span>
        <span style={{ marginLeft: "auto", fontSize: 16, fontWeight: 650, color: dark ? "#93a5c4" : MUTED }}>Website 4.0 · Building in public</span>
      </div>
      <div style={{ position: "absolute", left: 90, top: 132, width: 700, opacity: fade(frame, 2), translate: `0 ${rise(frame, 2)}px` }}>
        <div style={{ fontSize: 21, fontWeight: 700, letterSpacing: 2.5, color: ORANGE }}>
          {String(index + 1).padStart(2, "0")} / {SCENES.length} &nbsp;·&nbsp; {scene.label}
        </div>
        <h1 style={{ fontSize: 62, lineHeight: 1.06, letterSpacing: -1.5, margin: "26px 0 0" }}>{scene.title}</h1>
        <p style={{ fontSize: 26, lineHeight: 1.45, color: dark ? "#c9d4e8" : MUTED, marginTop: 24 }}>{scene.caption}</p>
        <div style={{ width: 84, height: 5, background: ORANGE, borderRadius: 3, marginTop: 34 }} />
        <div style={{ marginTop: 22, fontSize: 17, color: dark ? "#93a5c4" : "#9a97ad" }}>AGENTRIX WEBSITE · ZAVORA TECHNOLOGIES · DAY 2</div>
      </div>
      <div
        style={{
          position: "absolute",
          right: 70,
          top: 112,
          width: 940,
          height: 820,
          borderRadius: 26,
          overflow: "hidden",
          background: dark ? "#101f3a" : "#fbfaff",
          border: dark ? "1px solid #24365c" : "1px solid #e6e2f2",
          opacity: fade(frame, 8, 22),
          translate: `0 ${rise(frame, 8, 24)}px`,
        }}
      >
        {scene.visual}
      </div>
      <CaptionBar scene={scene} dark={dark} />
      <div style={{ position: "absolute", left: 90, right: 70, bottom: 44, height: 5, background: dark ? "#24365c" : "#e9e6f5", borderRadius: 3 }}>
        <div style={{ height: "100%", background: ORANGE, borderRadius: 3, width: `${Math.min(100, (frame / (scene.seconds * fps)) * 100)}%` }} />
      </div>
    </AbsoluteFill>
  );
};

export const Day2Showcase: React.FC = () => {
  const { fps } = useVideoConfig();
  let at = 0;
  return (
    <AbsoluteFill style={{ background: NAVY }}>
      {SCENES.map((scene, i) => {
        const from = at;
        at += scene.seconds * fps;
        return (
          <Sequence key={scene.label} from={from} durationInFrames={scene.seconds * fps}>
            <SceneView scene={scene} index={i} />
          </Sequence>
        );
      })}
    </AbsoluteFill>
  );
};

export const TOTAL_SECONDS = SCENES.reduce((a, s) => a + s.seconds, 0);
