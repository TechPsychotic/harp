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

  const PALETTES = {
    coral: { bg: ["#fff8f6", "#ffd3c8"], accent: "#d9344a", tones: ["#ef3b4f", "#ffab96", "#fff4f0"] },
    violet: { bg: ["#f8f6ff", "#dcd2ff"], accent: "#6342f5", tones: ["#7b5cff", "#c3b4ff", "#f6f3ff"] },
    mint: { bg: ["#f3fff9", "#c4f0db"], accent: "#0d8a57", tones: ["#1fbf7a", "#a3eac6", "#f2fff9"] },
    sky: { bg: ["#f4f8ff", "#cddfff"], accent: "#1f55e8", tones: ["#2f6bff", "#a9c4ff", "#f3f7ff"] },
    amber: { bg: ["#fffaf1", "#ffe0b0"], accent: "#b45309", tones: ["#ff9a1f", "#ffd193", "#fff8ee"] },
    lime: { bg: ["#fbfff0", "#def3a8"], accent: "#4d7c0f", tones: ["#8cc63f", "#d3ee98", "#fbfff1"] },
    pink: { bg: ["#fff5f8", "#ffd0de"], accent: "#d61f5e", tones: ["#ff4d7e", "#ffb0c6", "#fff4f7"] },
    lilac: { bg: ["#faf7ff", "#e7dcff"], accent: "#7c3aed", tones: ["#9b6bff", "#d9c8ff", "#fbf9ff"] },
    blue: { bg: ["#f5f6ff", "#d3d9ff"], accent: "#3442f0", tones: ["#4152ff", "#b8c0ff", "#f5f6ff"] },
  };

  // ---------- colour helpers ----------
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const mix = (a, b, t) => {
    const x = hex(a), y = hex(b);
    return "#" + x.map((v, i) => Math.round(v + (y[i] - v) * t).toString(16).padStart(2, "0")).join("");
  };
  const shade = (h, k) => mix(h, k > 0 ? "#ffffff" : "#000000", Math.abs(k));

  // A glossy sphere (or a glass bubble) drawn once into a small canvas.
  const SPRITE = 160;
  const sprites = new Map();
  function sprite(color, glass) {
    const key = color + (glass ? "g" : "");
    let s = sprites.get(key);
    if (s) return s;
    s = document.createElement("canvas");
    s.width = s.height = SPRITE;
    const c = s.getContext("2d");
    const r = SPRITE / 2 - 2;
    const g = c.createRadialGradient(SPRITE * 0.36, SPRITE * 0.32, r * 0.05, SPRITE / 2, SPRITE / 2, r);
    if (glass) {
      g.addColorStop(0, "rgba(255,255,255,0.75)");
      g.addColorStop(0.35, hexA(color, 0.35));
      g.addColorStop(0.85, hexA(shade(color, -0.15), 0.55));
      g.addColorStop(1, hexA(shade(color, -0.35), 0.8));
    } else {
      g.addColorStop(0, shade(color, 0.75));
      g.addColorStop(0.28, shade(color, 0.12));
      g.addColorStop(0.75, color);
      g.addColorStop(1, shade(color, -0.28));
    }
    c.fillStyle = g;
    c.beginPath();
    c.arc(SPRITE / 2, SPRITE / 2, r, 0, Math.PI * 2);
    c.fill();
    // Rim light from below, and a sharp highlight.
    const rim = c.createRadialGradient(SPRITE / 2, SPRITE * 0.86, 0, SPRITE / 2, SPRITE * 0.86, r * 0.7);
    rim.addColorStop(0, "rgba(255,255,255,0.28)");
    rim.addColorStop(1, "rgba(255,255,255,0)");
    c.fillStyle = rim;
    c.fill();
    c.fillStyle = "rgba(255,255,255,0.85)";
    c.beginPath();
    c.ellipse(SPRITE * 0.35, SPRITE * 0.3, r * 0.16, r * 0.1, -0.6, 0, Math.PI * 2);
    c.fill();
    if (glass) {
      c.fillStyle = "rgba(255,255,255,0.9)";
      for (const [x, y, d] of [[0.62, 0.58, 2.2], [0.44, 0.7, 1.6], [0.7, 0.4, 1.4]]) {
        c.beginPath();
        c.arc(SPRITE * x, SPRITE * y, d, 0, Math.PI * 2);
        c.fill();
      }
    }
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
        tone: roll < 45 ? 0 : roll < 75 ? 1 : 2,
        glass: roll % 7 === 0 && roll < 75,
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
  const band = document.querySelector(".band");
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
    const order = balls.slice().sort((p, q) => (p.z ?? p.r / 100) - (q.z ?? q.r / 100));
    for (const ball of order) {
      if (ball.x + ball.r < -50 || ball.x - ball.r > W + 50 || ball.y + ball.r < -50 || ball.y - ball.r > H + 50) continue;
      const d = ball.r * 2;
      ctx.globalAlpha = 1 - t;
      if (1 - t > 0.01) ctx.drawImage(sprite(a.pal.tones[ball.tone], ball.glass), ball.x - ball.r, ball.y - ball.r, d, d);
      ctx.globalAlpha = t;
      if (t > 0.01) ctx.drawImage(sprite(b.pal.tones[ball.tone], ball.glass), ball.x - ball.r, ball.y - ball.r, d, d);
    }
    ctx.globalAlpha = 1;
  }

  function paintSky(w) {
    const { a, b, t } = w;
    root.style.setProperty("--bg1", mix(a.pal.bg[0], b.pal.bg[0], t));
    root.style.setProperty("--bg2", mix(a.pal.bg[1], b.pal.bg[1], t));
    root.style.setProperty("--accent", mix(a.pal.accent, b.pal.accent, t));
  }

  function chrome() {
    nav.classList.toggle("scrolled", scrollY > 20);
    const br = band.getBoundingClientRect();
    nav.classList.toggle("on-dark", br.top < 60);
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
