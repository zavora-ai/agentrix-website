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
    label: "BUILDING IN PUBLIC · DAY 2",
    title: "Yesterday, a thesis. Today, the concept.",
    caption: "Agentrix Website — the world's first Website 4.0 for the agentic era. By Zavora Technologies, built on ADK-Rust and Meta Muse.",
    seconds: 8,
    dark: true,
    visual: <Img src="concept/01-visitor-world.png" />,
  },
  {
    label: "THE PROBLEM",
    title: "Websites sit there. Businesses can't afford that anymore.",
    caption: "The brochure web is failing on numbers, not opinions. Sources on the final frame.",
    seconds: 13,
    visual: (
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 34, padding: 40, height: "100%", alignContent: "center" }}>
        <Stat delay={8} n="43%" t="of all websites run WordPress — over 200M sites on yesterday's model (W3Techs)" />
        <Stat delay={20} n="58%" t="fewer clicks for the #1 result when an AI Overview answers first (Ahrefs, 300k keywords)" />
        <Stat delay={32} n="47 hrs" t="average business reply time to a website lead — buyers decide in 5 minutes (HBR/MIT)" />
        <Stat delay={44} n="$4.4M" t="average cost of one data breach; small firms lose ~$120k per attack (IBM 2025)" />
      </div>
    ),
  },
  {
    label: "THE AUDIENCE MOVED",
    title: "Your next visitor may not be a person.",
    caption: "People now send AI to the web for them. Sites built only for human eyes are invisible to the agents doing the reading, comparing, and buying.",
    seconds: 11,
    visual: <Chips items={["Human visitor → a place", "AI agent → a protocol", "Same business", "Same release", "Verified, never scraped", "Zero-click is the default"]} />,
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
    label: "CONCEPT 1 · THE VISITOR WORLD",
    title: "A place you enter, staff you can see.",
    caption: "Districts, a concierge, a verb bar. Remove the intelligence and it still passes as a beautiful, normal website — that's the point.",
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
    label: "CONCEPT 4 · THE OWNER'S WORLD",
    title: "Autonomy with a probation ladder.",
    caption: "Scrub the night: the Watchman held a bad publish at 03:12, two replies sent in-ladder, one refund waiting for a human. The owner approves the ladder — not every click.",
    seconds: 12,
    visual: <Chips items={["Observe", "Propose", "Act with approval", "Act reversibly", "Act within budget", "Money + DNS: always held", "Freeze switch, always"]} />,
  },
  {
    label: "CONCEPT 5 · THE AGENT INTERFACE",
    title: "For agents, the CLI is the interface.",
    caption: "No pages to scrape: a public terminal with verified tiers, capabilities, consent receipts, held calls. One site agent serves thousands of agents at once through kernel queues.",
    seconds: 12,
    visual: <Img src="concept/05-concept2-cli.png" />,
  },
  {
    label: "CONCEPT 6 · THE BIRTH",
    title: "Migration without credentials.",
    caption: "Paste a WordPress URL. The public site is crawled, rebuilt, gate-checked — verified claims stamped, the rest held for the owner. A Website 4.0, born in minutes.",
    seconds: 11,
    visual: <Chips items={["Paste a URL", "Crawl (public only)", "Rebuild as twins", "LaunchGate verifies", "Owner signs off", "World goes live"]} />,
  },
  {
    label: "THE LEDGER · WHAT THIS COST",
    title: "$100 built the foundation. $50 remains.",
    caption: "Day 1–2: research, the full Muse provider in ADK-Rust, Ideate with Meta, four visual concepts, this video. Day 3 is coding: $50 of implementation budget, spent in public. Tell us where it should go.",
    seconds: 13,
    dark: true,
    visual: (
      <div style={{ padding: 44, height: "100%", display: "flex", flexDirection: "column", justifyContent: "center", gap: 26, color: "#fff" }}>
        <div style={{ fontSize: 30 }}><b style={{ color: ORANGE }}>$100</b> — foundation · research · concepts · design · plan ✓</div>
        <div style={{ height: 16, borderRadius: 8, background: "#2a2a44", overflow: "hidden" }}><div style={{ width: "66%", height: "100%", background: ORANGE }} /></div>
        <div style={{ fontSize: 30 }}><b style={{ color: ORANGE }}>$50</b> — implementation · Day 3 · coding in public</div>
        <div style={{ fontSize: 24, color: "#c9c6e8" }}>What should Day 3 build first — migration, the concierge, or the night shift? Comment below.</div>
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
    () => createTikTokStyleCaptions({ captions, combineTokensWithinMilliseconds: 1600 }),
    [captions],
  );
  const nowMs = (frame / fps) * 1000;
  const page = pages.find((pg) => nowMs >= pg.startMs && nowMs < pg.startMs + 1700) ?? pages[pages.length - 1];
  if (!page) return null;
  return (
    <div style={{ position: "absolute", left: 90, right: 70, bottom: 66, display: "flex", justifyContent: "center", pointerEvents: "none" }}>
      <div style={{ maxWidth: 1240, textAlign: "center", fontSize: 30, fontWeight: 700, lineHeight: 1.3, whiteSpace: "pre", color: dark ? "#fff" : INK, background: dark ? "rgba(10,22,40,.72)" : "rgba(255,255,255,.86)", borderRadius: 14, padding: "10px 22px" }}>
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
      <div style={{ position: "absolute", left: 90, top: 120, width: 700, opacity: fade(frame, 2), translate: `0 ${rise(frame, 2)}px` }}>
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
          top: 100,
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
