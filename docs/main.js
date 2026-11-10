// Harp's site: a field of glossy spheres behind the page. Each section gives
// them a shape and a colour; between two sections the spheres blend from one
// shape to the next exactly as far as you've scrolled, so they always line up
// with the page. Every shape is measured and centred in its area.
// Plain 2D canvas with pre-drawn sprites: light enough for phones. Motion
// stops for people who ask for less of it.
(() => {
  "use strict";
  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const root = document.documentElement;
  const nav = document.getElementById("nav");
  const canvas = document.getElementById("field");
  const ctx = canvas.getContext("2d");

  // ---------- smooth scrolling (desktop wheels; touch scrolling stays native) ----------
  let lenis = null;
  if (!reduce && window.Lenis) {
    lenis = new window.Lenis({ duration: 1.15, easing: (t) => 1 - Math.pow(1 - t, 3.2), smoothWheel: true });
    root.classList.add("lenis-on");
    document.querySelectorAll('a[href^="#"]').forEach((a) =>
      a.addEventListener("click", (e) => {
        const id = a.getAttribute("href");
        const el = id === "#top" ? 0 : document.querySelector(id);
        if (el === null) return;
        e.preventDefault();
        lenis.scrollTo(el, { duration: 1.4 });
      }),
    );
  }

  // ---------- colour helpers ----------
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const mix = (a, b, t) => {
    const x = hex(a), y = hex(b);
    return "#" + x.map((v, i) => Math.round(v + (y[i] - v) * t).toString(16).padStart(2, "0")).join("");
  };
  const hexA = (h, a) => {
    const [r, g, b] = hex(h);
    return `rgba(${r},${g},${b},${a})`;
  };

  // Each section's accent, a deep glow for the sky behind it, and four sphere
  // tones (accent, light, deep, pearl) lit for a dark background.
  const PALETTE = (accent, glow) => ({
    accent, glow,
    tones: [accent, mix(accent, "#ffffff", 0.45), mix(accent, "#05050a", 0.32), mix(accent, "#ece9f5", 0.82)],
  });
  const PALETTES = {
    coral: PALETTE("#ff5a6e", "#3d1119"),
    violet: PALETTE("#9b7bff", "#24164a"),
    mint: PALETTE("#34d399", "#0b3326"),
    sky: PALETTE("#5b8cff", "#10224a"),
    amber: PALETTE("#ffb020", "#3a2608"),
    lime: PALETTE("#a3e635", "#22320a"),
    pink: PALETTE("#ff5c93", "#3d1027"),
    lilac: PALETTE("#c084fc", "#2c1544"),
    blue: PALETTE("#6b7bff", "#161c4a"),
  };

  // ---------- sprites ----------
  // A lit body with a soft bloom, light bounced up from below, a rim in the
  // sphere's own colour, a soft and a sharp highlight. Glass ones are
  // see-through with a bright edge. Far ones get a blurred copy.
  const CORE = 128, PAD = 26, SPRITE = CORE + PAD * 2;
  const sprites = new Map();
  function sprite(color, glass, far) {
    const key = color + (glass ? "g" : "s") + (far ? "f" : "");
    let s = sprites.get(key);
    if (s) return s;
    s = document.createElement("canvas");
    s.width = s.height = SPRITE;
    const c = s.getContext("2d");
    if (far) {
      if ("filter" in c) c.filter = "blur(3px)";
      c.globalAlpha = 0.75;
      c.drawImage(sprite(color, glass, false), 0, 0);
      sprites.set(key, s);
      return s;
    }
    const m = SPRITE / 2, r = CORE / 2;
    const at = (fx, fy) => [m - r + CORE * fx, m - r + CORE * fy];
    c.save();
    c.shadowColor = hexA(color, glass ? 0.35 : 0.55);
    c.shadowBlur = PAD * 0.9;
    const body = c.createRadialGradient(...at(0.36, 0.3), r * 0.04, m, m, r);
    if (glass) {
      body.addColorStop(0, "rgba(255,255,255,0.55)");
      body.addColorStop(0.4, hexA(color, 0.16));
      body.addColorStop(0.85, hexA(color, 0.28));
      body.addColorStop(1, hexA(mix(color, "#ffffff", 0.3), 0.75));
    } else {
      body.addColorStop(0, mix(color, "#ffffff", 0.6));
      body.addColorStop(0.22, mix(color, "#ffffff", 0.18));
      body.addColorStop(0.62, color);
      body.addColorStop(0.92, mix(color, "#000000", 0.55));
      body.addColorStop(1, mix(color, "#000000", 0.4));
    }
    c.fillStyle = body;
    c.beginPath();
    c.arc(m, m, r, 0, Math.PI * 2);
    c.fill();
    c.restore();
    c.save();
    c.beginPath();
    c.arc(m, m, r, 0, Math.PI * 2);
    c.clip();
    const bounce = c.createRadialGradient(...at(0.62, 0.92), 0, ...at(0.62, 0.92), r * 0.75);
    bounce.addColorStop(0, hexA(mix(color, "#ffffff", 0.5), glass ? 0.55 : 0.45));
    bounce.addColorStop(1, hexA(color, 0));
    c.fillStyle = bounce;
    c.fillRect(0, 0, SPRITE, SPRITE);
    const rim = c.createRadialGradient(m, m, r * 0.72, m, m, r);
    rim.addColorStop(0, "rgba(255,255,255,0)");
    rim.addColorStop(1, hexA(mix(color, "#ffffff", 0.55), glass ? 0.65 : 0.28));
    c.fillStyle = rim;
    c.fillRect(0, 0, SPRITE, SPRITE);
    const spec = c.createRadialGradient(...at(0.35, 0.28), 0, ...at(0.35, 0.28), r * 0.42);
    spec.addColorStop(0, "rgba(255,255,255,0.75)");
    spec.addColorStop(1, "rgba(255,255,255,0)");
    c.fillStyle = spec;
    c.fillRect(0, 0, SPRITE, SPRITE);
    c.fillStyle = "rgba(255,255,255,0.95)";
    c.beginPath();
    c.ellipse(...at(0.33, 0.25), r * 0.11, r * 0.065, -0.65, 0, Math.PI * 2);
    c.fill();
    c.restore();
    sprites.set(key, s);
    return s;
  }

  // ---------- the spheres ----------
  let W = 0, H = 0, dpr = 1, small = false;
  let balls = [];
  function build() {
    const n = small ? 36 : 62;
    balls = [];
    for (let i = 0; i < n; i++) {
      const roll = (i * 7919) % 100;
      balls.push({
        x: W / 2, y: H * 0.35, r: 0, z: 0.5, ox: 0, oy: 0,
        base: 0.3 + 0.7 * Math.pow(((i * 37) % 97) / 96, 1.6), // many small, a few big
        tone: roll < 38 ? 0 : roll < 60 ? 1 : roll < 82 ? 2 : 3,
        glass: roll % 6 === 0,
        seed: (i * 0.618034) % 1,
      });
    }
  }
  function resize() {
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    W = innerWidth;
    H = innerHeight;
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    const was = small;
    small = W < 680;
    if (!balls.length || was !== small) build();
  }

  // ---------- sections ----------
  const sections = [...document.querySelectorAll("[data-mode]")].map((el) => ({
    el,
    mode: el.dataset.mode,
    pal: PALETTES[el.dataset.palette] || PALETTES.coral,
    stage: el.querySelector(".stage"),
  }));
  const lyrics = document.querySelector(".lyrics");
  const navLinks = [...document.querySelectorAll(".links a")];

  // The section at the middle of the screen, the next one, and how far (0..1)
  // the page has scrolled from the first's centre to the second's.
  function where() {
    const mid = H / 2;
    const rects = sections.map((s) => s.el.getBoundingClientRect());
    let i = 0;
    for (let k = 0; k < sections.length; k++) if (rects[k].top + rects[k].height / 2 <= mid) i = k;
    const j = Math.min(i + 1, sections.length - 1);
    const ca = rects[i].top + rects[i].height / 2, cb = rects[j].top + rects[j].height / 2;
    const t = i === j ? 0 : Math.min(1, Math.max(0, (mid - ca) / (cb - ca)));
    return { a: sections[i], b: sections[j], t };
  }
  const ease = (t) => t * t * (3 - 2 * t);

  // ---------- shapes ----------
  // Each returns, for every sphere, a target {x, y, r, z} in a local frame;
  // place() then centres and scales it into the section's area.
  const GOLD = 2.399963;
  const BPM = 104;
  const beat = (time) => Math.exp(-(((time * BPM) / 60) % 1) * 5); // a sharp hit that fades
  let s_g = 0; // where the second cluster starts

  function shape(mode, time) {
    const n = balls.length;
    const out = new Array(n);
    const wob = reduce ? 0 : 1;
    switch (mode) {
      case "cluster": {
        for (let i = 0; i < n; i++) {
          const b = balls[i];
          const d = Math.sqrt(i + 0.5) * 31;
          const a = i * GOLD + time * 0.06 * wob;
          out[i] = {
            x: Math.cos(a) * d + Math.sin(time * 0.8 + b.seed * 9) * 5 * wob,
            y: Math.sin(a) * d * 0.86 + Math.cos(time * 0.7 + b.seed * 9) * 5 * wob,
            r: 13 + 42 * b.base, z: b.base,
          };
        }
        return { pts: out, fit: true, still: true };
      }
      case "floor": {
        // A mound: rows from the floor up, each a little shorter.
        const M = Math.ceil((Math.sqrt(8 * n + 1) - 1) / 2) + 1, R = 24;
        let i = 0, k = 0;
        while (i < n) {
          const left = Math.min(Math.max(1, M - k), n - i);
          for (let c = 0; c < left; c++, i++) {
            const b = balls[i];
            out[i] = { x: (c - (left - 1) / 2) * R * 2.02, y: -R - k * R * 1.74 + Math.sin(time * 1.4 + b.seed * 6) * 1.2 * wob, r: R * (0.82 + 0.3 * b.base), z: 0.3 + 0.5 * b.seed };
          }
          k++;
        }
        return { pts: out, fit: true, anchor: "bottom", still: true };
      }
      case "eq": {
        const cols = small ? 6 : 8, rows = Math.ceil(n / cols), R = 20;
        for (let i = 0; i < n; i++) {
          const col = i % cols, k = Math.floor(i / cols);
          const level = reduce ? 0.7 : 0.3 + 0.7 * (0.5 + 0.5 * Math.sin(time * 2.4 + col * 1.3) * Math.cos(time * 0.9 + col * 0.4));
          const on = k < Math.max(1, Math.round(level * rows));
          out[i] = { x: (col - (cols - 1) / 2) * R * 2.5, y: -R - k * R * 2.2, r: on ? R : R * 0.28, z: 0.5 };
        }
        return { pts: out, fit: true, anchor: "bottom", still: true, fullR: true };
      }
      case "wave": {
        for (let i = 0; i < n; i++) {
          const b = balls[i], u = i / (n - 1);
          out[i] = { x: (u - 0.5) * 900, y: Math.sin(u * Math.PI * 3 - time * 1.8 * wob) * 110, r: (10 + 14 * b.base) * (0.8 + 0.4 * Math.sin(u * 9 + time * wob)), z: b.base };
        }
        return { pts: out, fit: true, still: true };
      }
      case "duo": {
        // Two clusters (two devices) whose centres hit on the same beat, and
        // the whole of each swells with it; small spheres carry the music
        // across between them.
        const hit = reduce ? 0.4 : beat(time);
        const runners = small ? 6 : 10, g = Math.floor((n - runners) / 2);
        let i = 0;
        for (const cx of [-210, 210]) {
          for (let k = 0; k < g; k++, i++) {
            const b = balls[i];
            if (k === 0) { out[i] = { x: cx, y: 0, r: 46 * (1 + 0.22 * hit), z: 0.95 }; continue; }
            const d = Math.sqrt(k + 0.5) * 30 * (1 + 0.05 * hit);
            const a = k * GOLD + time * 0.05 * wob * (cx < 0 ? 1 : -1);
            out[i] = { x: cx + Math.cos(a) * d, y: Math.sin(a) * d * 0.9, r: 10 + 26 * b.base, z: 0.2 + 0.6 * b.base };
          }
        }
        for (let j = 0; i < n; i++, j++) {
          const dir = j % 2 ? 1 : -1;
          const p = (time * 0.42 * wob + j / Math.max(1, n - 2 * g)) % 1;
          const u = dir > 0 ? p : 1 - p;
          out[i] = { x: -210 + 420 * u, y: -Math.sin(u * Math.PI) * (dir > 0 ? 150 : 95) - 30, r: 6 + 5 * Math.sin(u * Math.PI), z: 0.6 };
        }
        s_g = g;
        return { pts: out, fit: true, still: true, duo: true };
      }
      case "orbit": {
        for (let i = 0; i < n; i++) {
          const b = balls[i], ring = i % 3 === 0 ? 0 : 1;
          const R = ring ? 300 : 170;
          const a = (i / n) * Math.PI * 2 * (ring ? 1 : 3) + time * (ring ? 0.25 : -0.4) * wob;
          const depth = (Math.sin(a) + 1) / 2;
          out[i] = { x: Math.cos(a) * R, y: Math.sin(a) * R * 0.38, r: (12 + 16 * b.base) * (0.65 + 0.6 * depth), z: depth };
        }
        return { pts: out, fit: true, still: true };
      }
      case "frame": {
        // Small spheres drifting up the screen's edges, clear of the text.
        const band = Math.max(28, (W - Math.min(W, 1400)) / 2 + 30), span = H + 120;
        for (let i = 0; i < n; i++) {
          const b = balls[i], side = i % 2;
          const fx = 0.3 + 0.55 * ((i * 0.618034) % 1), phase = (i * 0.381966) % 1;
          const y = (((phase * span - time * (14 + 10 * b.base) * wob) % span) + span) % span - 60;
          out[i] = { x: side ? W - band * fx : band * fx, y, r: i < (small ? 22 : 36) ? (6 + 14 * b.base) * (small ? 0.6 : 1) : 0, z: b.base };
        }
        return { pts: out, fit: false };
      }
      default: { // scatter: out past the edges of the screen, growing as they go
        for (let i = 0; i < n; i++) {
          const b = balls[i], a = i * GOLD, d = Math.max(W, H) * (0.8 + 0.3 * b.base);
          out[i] = { x: W / 2 + Math.cos(a) * d, y: H / 2 + Math.sin(a) * d, r: (40 + 60 * b.base) * 2, z: b.base };
        }
        return { pts: out, fit: false };
      }
    }
  }

  // The frame each shape is measured in, once per mode and screen size: the
  // shape at rest (no beat, no drift), so it never breathes out of its area.
  const bounds = new Map();
  function boundsOf(mode) {
    const key = mode + (small ? "s" : "l");
    let b = bounds.get(key);
    if (b) return b;
    const pts = shape(mode, 0).pts;
    let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity;
    for (const p of pts) {
      const r = mode === "eq" ? 20 : p.r;
      x0 = Math.min(x0, p.x - r); x1 = Math.max(x1, p.x + r);
      y0 = Math.min(y0, p.y - r); y1 = Math.max(y1, p.y + r);
    }
    b = { x0, x1, y0, y1 };
    bounds.set(key, b);
    return b;
  }

  // A section's shape, centred in its area and scaled to fit it.
  function place(sec, time) {
    const s = shape(sec.mode, time);
    if (!s.fit) return s;
    const area = (sec.stage || sec.el).getBoundingClientRect();
    const { x0, x1, y0, y1 } = boundsOf(sec.mode);
    const wide = !small && (sec.mode === "duo" || sec.mode === "wave") ? 1.18 : 0.92;
    const k = Math.min((area.width * wide) / (x1 - x0), (area.height * 0.92) / (y1 - y0), small ? 1.1 : 1.6);
    const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
    const ax = area.left + area.width / 2;
    const ay = s.anchor === "bottom" ? area.top + area.height * 0.96 - (y1 - cy) * k : area.top + area.height / 2;
    for (const p of s.pts) {
      p.x = ax + (p.x - cx) * k;
      p.y = ay + (p.y - cy) * k;
      p.r *= k;
    }
    if (s.duo) s.marks = { a: s.pts[0], b: s.pts[s_g], k };
    return s;
  }

  // ---------- pointer ----------
  const pointer = { x: -9999, y: -9999 };
  addEventListener("pointermove", (e) => { pointer.x = e.clientX; pointer.y = e.clientY; }, { passive: true });
  document.addEventListener("pointerleave", () => { pointer.x = pointer.y = -9999; });

  // ---------- one frame ----------
  let duo = null; // { marks, weight } while the two devices are on screen
  // Each shape holds while its section is in view and changes over the
  // middle stretch between two sections, so you see shapes, not a jumble.
  const morph = (t) => ease(Math.min(1, Math.max(0, (t - 0.28) / 0.44)));

  function step(w, time, dt) {
    const e = morph(w.t);
    const A = place(w.a, time);
    const B = w.b === w.a ? A : place(w.b, time);
    const weight = Math.pow((w.a.mode === "duo" ? 1 - e : 0) + (w.b.mode === "duo" ? e : 0), 3);
    duo = weight > 0.02 ? { marks: A.marks || B.marks, weight } : null;
    // Follow the blended target closely (so the spheres stay with the page)
    // but smoothly: a time-based ease, the same at any frame rate.
    const follow = reduce ? 1 : 1 - Math.exp(-dt * 10);
    const push = 1 - Math.exp(-dt * 6);
    for (let i = 0; i < balls.length; i++) {
      const b = balls[i], p = A.pts[i], q = B.pts[i];
      const tx = p.x + (q.x - p.x) * e, ty = p.y + (q.y - p.y) * e, tr = p.r + (q.r - p.r) * e;
      // Your pointer gently parts them.
      let px = 0, py = 0;
      const dx = tx - pointer.x, dy = ty - pointer.y, d = Math.hypot(dx, dy), R = 110 + tr;
      if (d < R && d > 0.5 && !reduce) {
        const f = Math.pow(1 - d / R, 2) * 46;
        px = (dx / d) * f;
        py = (dy / d) * f;
      }
      b.ox += (px - b.ox) * push;
      b.oy += (py - b.oy) * push;
      if (b.r === 0) { b.x = tx; b.y = ty; b.r = tr; }
      b.x += (tx - b.x) * follow;
      b.y += (ty - b.y) * follow;
      b.r += (tr - b.r) * follow;
      b.z = p.z + (q.z - p.z) * e;
    }
  }

  // ---------- drawing ----------
  let accentNow = "#ff5a6e";
  function draw(w, time) {
    const { a, b } = w, t = morph(w.t);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);
    if (duo && duo.marks) drawDuo(time);
    const order = balls.slice().sort((p, q) => p.z - q.z);
    const k = SPRITE / CORE;
    for (const ball of order) {
      const x = ball.x + ball.ox, y = ball.y + ball.oy;
      if (ball.r < 0.5 || x + ball.r < -60 || x - ball.r > W + 60 || y + ball.r < -60 || y - ball.r > H + 60) continue;
      const far = ball.z < 0.2;
      const d = ball.r * 2 * k, sx = x - ball.r * k, sy = y - ball.r * k;
      if (1 - t > 0.01) {
        ctx.globalAlpha = 1 - t;
        ctx.drawImage(sprite(a.pal.tones[ball.tone], ball.glass, far), sx, sy, d, d);
      }
      if (t > 0.01) {
        ctx.globalAlpha = t;
        ctx.drawImage(sprite(b.pal.tones[ball.tone], ball.glass, far), sx, sy, d, d);
      }
    }
    ctx.globalAlpha = 1;
  }

  // Behind the two devices: the path the music travels, and ripples on every
  // beat, at the same moment on both.
  function drawDuo(time) {
    const { marks: { a, b, k }, weight } = duo;
    ctx.save();
    ctx.strokeStyle = hexA(accentNow, 0.3 * weight);
    ctx.setLineDash([3, 9]);
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(a.x, a.y - 30 * k);
    ctx.quadraticCurveTo((a.x + b.x) / 2, a.y - 270 * k, b.x, b.y - 30 * k);
    ctx.stroke();
    ctx.setLineDash([]);
    const phase = reduce ? 0.5 : ((time * BPM) / 60) % 1;
    for (const c of [a, b]) {
      for (const lag of [0, 0.5]) {
        const ph = (phase + lag) % 1;
        ctx.strokeStyle = hexA(accentNow, (1 - ph) * 0.45 * weight);
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(c.x, c.y, 46 * k * (1.6 + ph * 1.8), 0, Math.PI * 2);
        ctx.stroke();
      }
    }
    ctx.restore();
  }

  let lastSky = "";
  function paintSky(w) {
    const { a, b } = w, t = morph(w.t);
    const accent = mix(a.pal.accent, b.pal.accent, t), glow = mix(a.pal.glow, b.pal.glow, t);
    accentNow = accent;
    if (accent + glow === lastSky) return;
    lastSky = accent + glow;
    root.style.setProperty("--accent", accent);
    root.style.setProperty("--glow", glow);
  }

  function chrome() {
    nav.classList.toggle("scrolled", scrollY > 20);
    let current = null;
    for (const l of navLinks) {
      const el = document.querySelector(l.getAttribute("href"));
      if (el && el.getBoundingClientRect().top < H * 0.5) current = l;
    }
    navLinks.forEach((l) => l.classList.toggle("on", l === current));
    if (lyrics) {
      const r = lyrics.getBoundingClientRect();
      const lines = lyrics.querySelectorAll("p");
      const u = (H * 0.8 - (r.top + r.height / 2)) / (H * 0.6);
      const i = Math.max(0, Math.min(lines.length - 1, Math.floor(u * lines.length)));
      lines.forEach((p, k) => p.classList.toggle("on", k === i));
    }
  }

  // ---------- loop ----------
  resize();
  addEventListener("resize", () => { resize(); bounds.clear(); });
  const start = performance.now();
  let last = start;
  function frame(now) {
    const dt = Math.min(0.05, Math.max(0, (now - last) / 1000));
    last = now;
    if (lenis) lenis.raf(now);
    const time = (now - start) / 1000;
    const w = where();
    paintSky(w);
    chrome();
    step(w, time, dt);
    draw(w, time);
    if (!reduce) requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
  if (reduce) {
    addEventListener("scroll", () => requestAnimationFrame(frame), { passive: true });
    addEventListener("resize", () => requestAnimationFrame(frame));
  }

  // ---------- text coming into view ----------
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      const el = e.target;
      el.classList.add("in");
      io.unobserve(el);
      // The stagger is for arriving only; hovering stays instant.
      if (el.style.transitionDelay) setTimeout(() => (el.style.transitionDelay = ""), 1400);
    }
  }, { threshold: 0.12, rootMargin: "0px 0px -30px 0px" });
  document.querySelectorAll(".copy, .card, .lyrics, .hero-text, .hero-side, .dl-head, .dl-card").forEach((el) => {
    el.classList.add("reveal");
    // Cards in a group arrive one after another.
    if (el.classList.contains("card") || el.classList.contains("dl-card")) {
      const sib = [...el.parentElement.children].indexOf(el);
      el.style.transitionDelay = `${Math.min(sib, 5) * 70}ms`;
    }
    io.observe(el);
  });
})();
