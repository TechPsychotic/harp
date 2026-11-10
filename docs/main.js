// Harp's site: a field of glossy spheres behind the page. Each section gives
// them a shape (a cluster, a pile on the floor, equalizer bars, a lyric wave,
// two groups in step, an orbit) and a colour, and the sky changes with it.
// Plain 2D canvas: pre-drawn sphere sprites, springs and simple collisions,
// light enough for phones. Motion stops for people who ask for less of it.
(() => {
  "use strict";
  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const root = document.documentElement;
  const nav = document.getElementById("nav");
  const canvas = document.getElementById("field");
  const ctx = canvas.getContext("2d");

  // ---------- colour helpers ----------
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const mix = (a, b, t) => {
    const x = hex(a), y = hex(b);
    return "#" + x.map((v, i) => Math.round(v + (y[i] - v) * t).toString(16).padStart(2, "0")).join("");
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

  // Spheres drawn once into small canvases: a lit body with a soft bloom, a
  // reflected light from below, a rim in the sphere's own colour, a soft and a
  // sharp highlight. Glass ones are see-through with a bright edge. Far ones
  // get a blurred copy (depth of field).
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
      const src = sprite(color, glass, false);
      if ("filter" in c) c.filter = "blur(3px)";
      c.globalAlpha = 0.75;
      c.drawImage(src, 0, 0);
      sprites.set(key, s);
      return s;
    }
    const m = SPRITE / 2, r = CORE / 2;
    const at = (fx, fy) => [m - r + CORE * fx, m - r + CORE * fy];
    c.save();
    // Bloom: the body casts a glow of its own colour onto the dark.
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
    // Light bounced up from below.
    const bounce = c.createRadialGradient(...at(0.62, 0.92), 0, ...at(0.62, 0.92), r * 0.75);
    bounce.addColorStop(0, hexA(mix(color, "#ffffff", 0.5), glass ? 0.55 : 0.45));
    bounce.addColorStop(1, hexA(color, 0));
    c.fillStyle = bounce;
    c.fillRect(0, 0, SPRITE, SPRITE);
    // Rim (fresnel): the edge catches light.
    const rim = c.createRadialGradient(m, m, r * 0.72, m, m, r);
    rim.addColorStop(0, "rgba(255,255,255,0)");
    rim.addColorStop(1, hexA(mix(color, "#ffffff", 0.55), glass ? 0.65 : 0.28));
    c.fillStyle = rim;
    c.fillRect(0, 0, SPRITE, SPRITE);
    // Soft highlight, then a sharp one.
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
  function hexA(h, a) {
    const [r, g, b] = hex(h);
    return `rgba(${r},${g},${b},${a})`;
  }

  // ---------- the spheres ----------
  let W = 0, H = 0, dpr = 1, small = false;
  let balls = [];
  function build() {
    const n = small ? 28 : 46;
    balls = [];
    for (let i = 0; i < n; i++) {
      const roll = (i * 7919) % 100;
      balls.push({
        x: W / 2 + Math.cos(i) * 80, y: H * 0.4 + Math.sin(i) * 80, vx: 0, vy: 0,
        r: 10, base: 0.3 + 0.7 * Math.pow(((i * 37) % 97) / 96, 1.6), // relative size: many small, a few big
        tone: roll < 38 ? 0 : roll < 60 ? 1 : roll < 82 ? 2 : 3,
        glass: roll % 6 === 0,
        seed: i * 1.7,
      });
    }
  }

  function resize() {
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    W = innerWidth;
    H = innerHeight;
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    const wasSmall = small;
    small = W < 680;
    if (!balls.length || wasSmall !== small) build();
  }

  // ---------- sections ----------
  const sections = [...document.querySelectorAll("[data-mode]")].map((el) => ({
    el,
    mode: el.dataset.mode,
    pal: PALETTES[el.dataset.palette] || PALETTES.coral,
    stage: el.querySelector(".stage"),
  }));
    const lyrics = document.querySelector(".lyrics");

  // Where we are: the section at the middle of the screen, how far into the next, and its stage.
  function where() {
    const mid = H / 2;
    let i = 0;
    for (let k = 0; k < sections.length; k++) {
      const r = sections[k].el.getBoundingClientRect();
      if (r.top + r.height / 2 <= mid) i = k;
    }
    const a = sections[i], b = sections[Math.min(i + 1, sections.length - 1)];
    const ra = a.el.getBoundingClientRect(), rb = b.el.getBoundingClientRect();
    const ca = ra.top + ra.height / 2, cb = rb.top + rb.height / 2;
    const t = b === a ? 0 : Math.min(1, Math.max(0, (mid - ca) / (cb - ca)));
    const active = t < 0.5 ? a : b;
    const sr = (active.stage || active.el).getBoundingClientRect();
    return { a, b, t, active, stage: sr };
  }

  // ---------- shapes ----------
  const GOLD = 2.399963;
  function targets(mode, st, time) {
    const n = balls.length;
    const cx = st.left + st.width / 2, cy = st.top + st.height / 2;
    const S = Math.max(0.45, Math.min(st.width, st.height) / 520);
    const out = new Array(n);
    for (let i = 0; i < n; i++) {
      const b = balls[i];
      const wob = reduce ? 0 : 1;
      switch (mode) {
        case "cluster": {
          const R = Math.min(st.width, st.height) * 0.44;
          const d = Math.sqrt((i + 0.5) / n) * R;
          const a = i * GOLD + time * 0.05 * wob;
          out[i] = { x: cx + Math.cos(a) * d + Math.sin(time * 0.7 + b.seed) * 6 * wob, y: cy + Math.sin(a) * d * 0.82 + Math.cos(time * 0.6 + b.seed) * 6 * wob, r: (12 + 50 * b.base) * S };
          break;
        }
        case "eq": {
          const cols = small ? 5 : 7;
          const col = i % cols, k = Math.floor(i / cols);
          const cw = st.width / cols;
          const rows = Math.ceil(n / cols);
          const r = Math.min(cw * 0.34, (st.height - 4 * rows) / (rows * 2));
          const level = reduce ? 0.7 : 0.35 + 0.65 * (0.5 + 0.5 * Math.sin(time * 2.4 + col * 1.3) * Math.cos(time * 0.9 + col * 0.4));
          const on = k < Math.max(1, Math.round(level * rows));
          out[i] = { x: st.left + cw * (col + 0.5), y: st.top + st.height - r - k * (2 * r + 4), r: on ? r : r * 0.25 };
          break;
        }
        case "wave": {
          const u = i / (n - 1);
          const y = cy + Math.sin(u * Math.PI * 3 - time * 1.8 * wob) * st.height * 0.22;
          out[i] = { x: st.left + st.width * (0.04 + 0.92 * u), y, r: (10 + 14 * b.base) * S * (0.8 + 0.4 * Math.sin(u * 9 + time * wob)) };
          break;
        }
        case "duo": {
          const runners = 3;
          if (i < runners) {
            const p = (time * 0.32 * wob + i / runners) % 1;
            const x = st.left + st.width * (0.27 + 0.46 * p);
            out[i] = { x, y: cy - Math.sin(p * Math.PI) * st.height * 0.22, r: 12 * S };
            break;
          }
          const j = i - runners, side = j % 2, m = Math.floor(j / 2), per = Math.ceil((n - runners) / 2);
          const R = Math.min(st.width * 0.24, st.height * 0.4);
          const d = Math.sqrt((m + 0.5) / per) * R;
          const a = m * GOLD;
          const pulse = 1 + 0.04 * Math.sin(time * 3.2 * wob); // both groups breathe in step
          out[i] = { x: st.left + st.width * (side ? 0.78 : 0.22) + Math.cos(a) * d * pulse, y: cy + Math.sin(a) * d * pulse, r: (20 + 22 * b.base) * S };
          break;
        }
        case "orbit": {
          const ring = i % 3 === 0 ? 0 : 1;
          const R = Math.min(st.width, st.height * 1.6) * (ring ? 0.46 : 0.26);
          const a = (i / n) * Math.PI * 2 * (ring ? 1 : 3) + time * (ring ? 0.25 : -0.4) * wob;
          const depth = (Math.sin(a) + 1) / 2;
          out[i] = { x: cx + Math.cos(a) * R, y: cy + Math.sin(a) * R * 0.38, r: (12 + 16 * b.base) * S * (0.65 + 0.6 * depth), z: depth };
          break;
        }
        case "frame": {
          // Small spheres drifting up the edges of the screen, clear of the text.
          const side = i % 2;
          const band = Math.max(28, (W - Math.min(W, 1400)) / 2 + 30);
          const fx = 0.3 + 0.55 * ((i * 0.618034) % 1), phase = (i * 0.381966) % 1, span = H + 120;
          const x = side ? W - band * fx : band * fx;
          const y = (((phase * span - time * (14 + 10 * b.base) * wob) % span) + span) % span - 60;
          out[i] = { x, y, r: (6 + 14 * b.base) * (small ? 0.6 : 1) };
          break;
        }
        case "scatter": {
          const a = i * GOLD;
          const d = Math.max(W, H) * (0.75 + 0.3 * b.base);
          out[i] = { x: W / 2 + Math.cos(a) * d, y: H / 2 + Math.sin(a) * d, r: (40 + 60 * b.base) * S * 2.2 };
          break;
        }
        default:
          out[i] = null; // floor: physics
      }
    }
    return out;
  }

  // ---------- physics ----------
  const pointer = { x: -9999, y: -9999 };
  addEventListener("pointermove", (e) => { pointer.x = e.clientX; pointer.y = e.clientY; }, { passive: true });
  addEventListener("pointerleave", () => { pointer.x = pointer.y = -9999; });

  let mode = "cluster", modeSince = 0;
  function step(w, time) {
    if (w.active.mode !== mode) {
      mode = w.active.mode;
      modeSince = time;
      if (mode === "floor") for (const b of balls) { b.vx += (Math.random() - 0.5) * 2; b.vy -= Math.random() * 2; }
    }
    const st = w.stage;
    const tg = targets(mode, st, time);
    const S = Math.max(0.45, Math.min(st.width, st.height) / 520);
    const collide = mode === "cluster" || mode === "floor" || mode === "duo";
    for (let i = 0; i < balls.length; i++) {
      const b = balls[i], t = tg[i];
      if (t) {
        if (reduce) { b.x = t.x; b.y = t.y; b.r = t.r; b.vx = b.vy = 0; continue; }
        const k = mode === "scatter" ? 0.02 : 0.045;
        b.vx = (b.vx + (t.x - b.x) * k) * 0.82;
        b.vy = (b.vy + (t.y - b.y) * k) * 0.82;
        b.r += (t.r - b.r) * 0.08;
        b.z = t.z;
      } else {
        // The pile: gravity, the stage's floor and walls.
        const r = (14 + 22 * b.base) * S;
        b.r += (r - b.r) * 0.1;
        b.vy += 0.6;
        b.vx *= 0.995;
        b.z = undefined;
      }
      // Your pointer nudges them aside.
      const dx = b.x - pointer.x, dy = b.y - pointer.y, d2 = dx * dx + dy * dy, R = 120 + b.r;
      if (d2 < R * R && d2 > 1) {
        const d = Math.sqrt(d2), f = (1 - d / R) * 2.2;
        b.vx += (dx / d) * f;
        b.vy += (dy / d) * f;
      }
      b.x += b.vx;
      b.y += b.vy;
      if (!t) {
        const floor = st.top + st.height, left = st.left, right = st.left + st.width;
        if (b.y + b.r > floor) { b.y = floor - b.r; b.vy *= -0.45; b.vx *= 0.9; }
        if (b.x - b.r < left) { b.x = left + b.r; b.vx *= -0.5; }
        if (b.x + b.r > right) { b.x = right - b.r; b.vx *= -0.5; }
      }
    }
    if (collide && !reduce) {
      for (let pass = 0; pass < 2; pass++) {
        for (let i = 0; i < balls.length; i++) {
          const a = balls[i];
          for (let j = i + 1; j < balls.length; j++) {
            const b = balls[j];
            const dx = b.x - a.x, dy = b.y - a.y, min = a.r + b.r - 2, d2 = dx * dx + dy * dy;
            if (d2 >= min * min || d2 === 0) continue;
            const d = Math.sqrt(d2), push = (min - d) / 2, nx = dx / d, ny = dy / d;
            a.x -= nx * push; a.y -= ny * push;
            b.x += nx * push; b.y += ny * push;
            if (mode === "floor") {
              const rel = (b.vx - a.vx) * nx + (b.vy - a.vy) * ny;
              if (rel < 0) { const imp = rel * 0.6; a.vx += nx * imp; a.vy += ny * imp; b.vx -= nx * imp; b.vy -= ny * imp; }
            }
          }
        }
      }
    }
  }

  // ---------- drawing ----------
  function draw(w) {
    const { a, b, t } = w;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);
    const order = balls.slice().sort((p, q) => (p.z ?? p.base) - (q.z ?? q.base));
    const k = SPRITE / CORE;
    for (const ball of order) {
      if (ball.x + ball.r < -60 || ball.x - ball.r > W + 60 || ball.y + ball.r < -60 || ball.y - ball.r > H + 60) continue;
      const far = (ball.z ?? ball.base) < 0.2;
      const d = ball.r * 2 * k, x = ball.x - ball.r * k, y = ball.y - ball.r * k;
      if (1 - t > 0.01) {
        ctx.globalAlpha = 1 - t;
        ctx.drawImage(sprite(a.pal.tones[ball.tone], ball.glass, far), x, y, d, d);
      }
      if (t > 0.01) {
        ctx.globalAlpha = t;
        ctx.drawImage(sprite(b.pal.tones[ball.tone], ball.glass, far), x, y, d, d);
      }
    }
    ctx.globalAlpha = 1;
  }

  let lastSky = "";
  function paintSky(w) {
    const { a, b, t } = w;
    const accent = mix(a.pal.accent, b.pal.accent, t), glow = mix(a.pal.glow, b.pal.glow, t);
    if (accent + glow === lastSky) return;
    lastSky = accent + glow;
    root.style.setProperty("--accent", accent);
    root.style.setProperty("--glow", glow);
  }

  const navLinks = [...document.querySelectorAll(".links a")];
  function chrome() {
    nav.classList.toggle("scrolled", scrollY > 20);
    // The link for the section you're in is underlined in its accent.
    let current = null;
    for (const l of navLinks) {
      const el = document.querySelector(l.getAttribute("href"));
      if (el) { const r = el.getBoundingClientRect(); if (r.top < H * 0.5) current = l; }
    }
    navLinks.forEach((l) => l.classList.toggle("on", l === current));
    if (lyrics) {
      const r = lyrics.getBoundingClientRect();
      const lines = lyrics.querySelectorAll("p");
      // Bottom of the screen: the first line; top: the last.
      const u = (H * 0.8 - (r.top + r.height / 2)) / (H * 0.6);
      const i = Math.max(0, Math.min(lines.length - 1, Math.floor(u * lines.length)));
      lines.forEach((p, k) => p.classList.toggle("on", k === i));
    }
  }

  // ---------- loop ----------
  resize();
  addEventListener("resize", resize);
  let start = performance.now();
  function frame(now) {
    const time = (now - start) / 1000;
    const w = where();
    paintSky(w);
    chrome();
    step(w, time);
    draw(w);
    if (!reduce) requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
  if (reduce) addEventListener("scroll", () => requestAnimationFrame(frame), { passive: true });

  // ---------- text coming into view ----------
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
  }, { threshold: 0.12, rootMargin: "0px 0px -30px 0px" });
  document.querySelectorAll(".copy, .card, .lyrics, .hero-text, .hero-side, .dl-head, .dl-card").forEach((el) => {
    el.classList.add("reveal");
    io.observe(el);
  });
})();
