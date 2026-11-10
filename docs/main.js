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
    const n = small ? 44 : 62;
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
  // Each returns, for every sphere, a target {x, y, r, z} in a local frame
  // (z: 0 far .. 1 near); place() then centres and scales it into the
  // section's area. The rounded shapes are real 3D arrangements, turned
  // slowly and seen in perspective; your pointer tilts them a little.
  const GOLD = 2.399963;
  const BPM = 104;
  const beat = (time) => Math.exp(-(((time * BPM) / 60) % 1) * 5); // a sharp hit that fades
  const tilt = { x: 0, y: 0 };

  // Points spread evenly over a sphere (or its upper half), each with a size.
  function shell(count, R, size, half, jitterSeed = 0) {
    const pts = [];
    for (let k = 0; k < count; k++) {
      const y = half ? (k + 0.5) / count : 1 - (2 * (k + 0.5)) / count;
      const rad = Math.sqrt(Math.max(0, 1 - y * y)), phi = k * GOLD + jitterSeed;
      pts.push({ x: Math.cos(phi) * rad * R, y: half ? -y * R : y * R, z: Math.sin(phi) * rad * R, r: size(k) });
    }
    return pts;
  }

  // Turn (yaw, then pitch, then roll) and look at it in perspective.
  function view(pts, yaw, pitch, roll, depthR, ox = 0, oy = 0) {
    const cy = Math.cos(yaw), sy = Math.sin(yaw), cp = Math.cos(pitch), sp = Math.sin(pitch), cr = Math.cos(roll), sr = Math.sin(roll);
    return pts.map((p) => {
      const x = p.x * cy - p.z * sy, z = p.x * sy + p.z * cy;
      const y2 = p.y * cp - z * sp, z2 = p.y * sp + z * cp;
      const x3 = x * cr - y2 * sr, y3 = x * sr + y2 * cr;
      const f = 1100 / (1100 + z2);
      return { x: ox + x3 * f, y: oy + y3 * f, r: p.r * f, z: Math.min(1, Math.max(0, 0.5 - z2 / (2 * depthR))) };
    });
  }

  function shape(mode, time, still) {
    const n = balls.length;
    const wob = reduce || still ? 0 : 1;
    const tx = still ? 0 : tilt.x, ty = still ? 0 : tilt.y;
    const size = (lo, span) => (k) => lo + span * balls[k % n].base;
    let out;
    switch (mode) {
      case "cluster": {
        // A ball of spheres, with a few inside so it never looks hollow.
        const outer = Math.round(n * 0.76);
        const pts = shell(outer, 210, size(32, 20)).concat(shell(n - outer, 120, (k) => 28 + 12 * balls[(k + outer) % n].base, false, 1.3));
        out = view(pts, time * 0.16 * wob + tx, 0.32 + ty, 0.12, 240);
        return { pts: out, fit: true };
      }
      case "helix": {
        // Two strands turning around each other: your taste, two ways.
        const pts = [];
        const half = Math.ceil(n / 2);
        for (let i = 0; i < n; i++) {
          const strand = i % 2, t = Math.floor(i / 2) / (half - 1);
          const a = t * Math.PI * 2.4 + strand * Math.PI;
          pts.push({ x: Math.cos(a) * 140, y: (t - 0.5) * 560, z: Math.sin(a) * 140, r: 21 + 12 * balls[i].base });
        }
        out = view(pts, time * 0.4 * wob + tx, 0.1 + ty, 0.22, 160);
        return { pts: out, fit: true };
      }
      case "floor": {
        // A dome resting on the floor, turning slowly.
        const outer = Math.round(n * 0.72);
        const pts = shell(outer, 230, size(30, 18), true).concat(shell(n - outer, 140, (k) => 26 + 12 * balls[(k + outer) % n].base, true, 0.7));
        out = view(pts, time * 0.12 * wob + tx, 0.28 + ty * 0.5, 0, 240);
        return { pts: out, fit: true, anchor: "bottom", shadow: true };
      }
      case "eq": {
        out = new Array(n);
        const cols = small ? 6 : 8, rows = Math.ceil(n / cols), R = 20;
        for (let i = 0; i < n; i++) {
          const col = i % cols, k = Math.floor(i / cols);
          const level = reduce || still ? 0.7 : 0.3 + 0.7 * (0.5 + 0.5 * Math.sin(time * 2.4 + col * 1.3) * Math.cos(time * 0.9 + col * 0.4));
          const on = k < Math.max(1, Math.round(level * rows));
          out[i] = { x: (col - (cols - 1) / 2) * R * 2.5, y: -R - k * R * 2.2, r: still ? R : on ? R : R * 0.28, z: 0.5 };
        }
        return { pts: out, fit: true, anchor: "bottom" };
      }
      case "wave": {
        out = new Array(n);
        for (let i = 0; i < n; i++) {
          const b = balls[i], u = i / (n - 1);
          out[i] = { x: (u - 0.5) * 900, y: Math.sin(u * Math.PI * 3 - time * 1.8 * wob) * 110, r: (10 + 14 * b.base) * (0.8 + 0.4 * Math.sin(u * 9 + time * wob)), z: b.base };
        }
        return { pts: out, fit: true };
      }
      case "duo": {
        // Two small balls (two devices) that swell on the same beat; little
        // spheres carry the music across between them.
        const hit = reduce || still ? 0.3 : beat(time);
        const runners = small ? 6 : 10, g = Math.floor((n - runners) / 2);
        out = [];
        for (const [side, ox] of [[1, -230], [-1, 230]]) {
          const pts = shell(g, 118 * (1 + 0.07 * hit), (k) => (20 + 16 * balls[k].base) * (1 + 0.12 * hit), false, side);
          out.push(...view(pts, time * 0.3 * wob * side + tx, 0.3 + ty, 0, 140, ox, 0));
        }
        for (let j = 0; out.length < n; j++) {
          const dir = j % 2 ? 1 : -1;
          const p = (time * 0.42 * wob + j / runners) % 1;
          const u = dir > 0 ? p : 1 - p;
          out.push({ x: -230 + 460 * u, y: -Math.sin(u * Math.PI) * (dir > 0 ? 170 : 110) - 40, r: 6 + 5 * Math.sin(u * Math.PI), z: 0.7 });
        }
        return { pts: out, fit: true, duo: [{ x: -230, y: 0 }, { x: 230, y: 0 }] };
      }
      case "orbit": {
        // A small planet of spheres with a tilted ring passing behind and in front.
        const core = Math.round(n * 0.34);
        const pts = shell(core, 100, size(24, 12));
        const ringN = n - core;
        const ring = [];
        for (let k = 0; k < ringN; k++) {
          const a = (k / ringN) * Math.PI * 2 + time * 0.25 * wob;
          ring.push({ x: Math.cos(a) * 255, y: 0, z: Math.sin(a) * 255, r: 12 + 3 * balls[(k + core) % n].base });
        }
        out = view(pts, time * 0.2 * wob + tx, 0.25 + ty, 0, 290).concat(view(ring, tx * 0.5, 0.36 + ty, -0.26, 290));
        return { pts: out, fit: true };
      }
      default: { // scatter: out past the edges of the screen, growing as they go
        out = new Array(n);
        for (let i = 0; i < n; i++) {
          const b = balls[i], a = i * GOLD, d = Math.max(W, H) * (0.8 + 0.3 * b.base);
          out[i] = { x: W / 2 + Math.cos(a) * d, y: H / 2 + Math.sin(a) * d, r: (40 + 60 * b.base) * 2, z: b.base };
        }
        return { pts: out, fit: false };
      }
    }
  }

  // The frame each shape is measured in, once per mode and screen size: the
  // shape at rest (not turned, no beat), so it never breathes out of its area.
  const bounds = new Map();
  function boundsOf(mode) {
    const key = mode + (small ? "s" : "l");
    let b = bounds.get(key);
    if (b) return b;
    let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity;
    // Turning changes the outline a little: measure a few angles.
    for (const t of [0, 2, 4, 6, 8]) {
      for (const p of shape(mode, t, t === 0).pts) {
        x0 = Math.min(x0, p.x - p.r); x1 = Math.max(x1, p.x + p.r);
        y0 = Math.min(y0, p.y - p.r); y1 = Math.max(y1, p.y + p.r);
      }
    }
    b = { x0, x1, y0, y1 };
    bounds.set(key, b);
    return b;
  }

  // A section's shape, centred in its area and scaled to fit it.
  function place(sec, time) {
    const s = shape(sec.mode, time, false);
    if (!s.fit) return s;
    const area = (sec.stage || sec.el).getBoundingClientRect();
    const { x0, x1, y0, y1 } = boundsOf(sec.mode);
    const wide = !small && (sec.mode === "duo" || sec.mode === "wave") ? 1.15 : 0.94;
    const k = Math.max(0, Math.min((area.width * wide) / (x1 - x0), (area.height * 0.94) / (y1 - y0), small ? 1.1 : 1.6));
    const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
    const ax = area.left + area.width / 2;
    const ay = s.anchor === "bottom" ? area.top + area.height * 0.95 - (y1 - cy) * k : area.top + area.height / 2;
    const at = (p) => { p.x = ax + (p.x - cx) * k; p.y = ay + (p.y - cy) * k; };
    for (const p of s.pts) { at(p); p.r *= k; }
    if (s.duo) { s.duo.forEach(at); s.marks = { a: s.duo[0], b: s.duo[1], k }; }
    if (s.shadow) s.floor = { x: ax, y: ay + (y1 - cy) * k, w: (x1 - x0) * k * 0.55 };
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

  let floor = null; // the dome's shadow while it's on screen
  function step(w, time, dt) {
    const e = morph(w.t);
    const lean = 1 - Math.exp(-dt * 3);
    const wantX = pointer.x < -9000 ? 0 : (pointer.x / W - 0.5) * 0.7, wantY = pointer.y < -9000 ? 0 : (pointer.y / H - 0.5) * 0.35;
    tilt.x += (wantX - tilt.x) * lean;
    tilt.y += (wantY - tilt.y) * lean;
    const A = place(w.a, time);
    const B = w.b === w.a ? A : place(w.b, time);
    const weight = Math.pow((w.a.mode === "duo" ? 1 - e : 0) + (w.b.mode === "duo" ? e : 0), 3);
    duo = weight > 0.02 ? { marks: A.marks || B.marks, weight } : null;
    const fw = (w.a.mode === "floor" ? 1 - e : 0) + (w.b.mode === "floor" ? e : 0);
    floor = fw > 0.02 ? { ...(A.floor || B.floor), weight: fw } : null;
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
        const f = Math.pow(1 - d / R, 2) * 22;
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
    if (floor && floor.w) {
      // A soft shadow where the dome meets the floor.
      ctx.save();
      ctx.translate(floor.x, floor.y);
      ctx.scale(1, 0.16);
      const g = ctx.createRadialGradient(0, 0, 0, 0, 0, floor.w);
      g.addColorStop(0, `rgba(0,0,0,${0.55 * floor.weight})`);
      g.addColorStop(0.6, hexA(accentNow, 0.12 * floor.weight));
      g.addColorStop(1, "rgba(0,0,0,0)");
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(0, 0, floor.w, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }
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
    ctx.moveTo(a.x, a.y - 40 * k);
    ctx.quadraticCurveTo((a.x + b.x) / 2, a.y - 300 * k, b.x, b.y - 40 * k);
    ctx.stroke();
    ctx.setLineDash([]);
    const phase = reduce ? 0.5 : ((time * BPM) / 60) % 1;
    for (const c of [a, b]) {
      for (const lag of [0, 0.5]) {
        const ph = (phase + lag) % 1;
        ctx.strokeStyle = hexA(accentNow, (1 - ph) * 0.45 * weight);
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(c.x, c.y, 135 * k * (1 + ph * 0.55), 0, Math.PI * 2);
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

  let lyricsSince = 0;
  function chrome() {
    nav.classList.toggle("scrolled", scrollY > 20);
    let current = null;
    for (const l of navLinks) {
      const el = document.querySelector(l.getAttribute("href"));
      if (el && el.getBoundingClientRect().top < H * 0.5) current = l;
    }
    navLinks.forEach((l) => l.classList.toggle("on", l === current));
    if (lyrics) {
      // Sung, not scrolled: while the section is on screen, one line at a time
      // fills in from left to right, then the next.
      const r = lyrics.getBoundingClientRect();
      const visible = r.bottom > 0 && r.top < H;
      const lines = lyrics.querySelectorAll("p");
      const LINE = 2.6;
      const now = performance.now() / 1000;
      if (visible && !lyricsSince) lyricsSince = now;
      if (!visible) lyricsSince = 0;
      const t = lyricsSince ? now - lyricsSince : 0;
      const i = Math.floor(t / LINE) % lines.length;
      const fill = reduce ? 1 : Math.min(1, (t % LINE) / (LINE * 0.8));
      lines.forEach((p, k) => {
        p.classList.toggle("on", k === i);
        p.classList.toggle("sung", lyricsSince > 0 && (k < i));
        if (k === i) p.style.setProperty("--fill", (fill * 100).toFixed(1) + "%");
      });
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

  // ---------- one section at a time ----------
  // With a mouse wheel, a trackpad or the keyboard, each gesture glides to the
  // next (or previous) section, centred. The download area at the end is
  // taller than the screen, so it scrolls freely. Touch screens keep their
  // own scrolling.
  // Stops: the centre of each section up to Privacy (09), or its top when it's
  // taller than the screen. Past Privacy, scrolling is free.
  const privacy = document.getElementById("privacy");
  const stops = () => {
    const y = scrollY, out = [];
    for (const s of sections) {
      const r = s.el.getBoundingClientRect();
      const top = r.top + y;
      if (s.el.classList.contains("hero")) out.push(0);
      else if (r.height > H - 20) out.push(Math.max(0, top));
      else out.push(Math.max(0, top + r.height / 2 - H / 2));
      if (s.el === privacy) break;
    }
    return out;
  };
  let busyUntil = 0, wheelSum = 0;
  function glide(dir) {
    const list = stops(), y = scrollY, freeTop = list[list.length - 1];
    // From Privacy on, scrolling is free (until you're back up at Privacy).
    if (y > freeTop + 4) return false;
    if (y >= freeTop - 4 && dir > 0) return false;
    let i = 0;
    for (let k = 0; k < list.length; k++) if (Math.abs(list[k] - y) < Math.abs(list[i] - y)) i = k;
    // Between two stops: the next one in the direction you're going.
    if (dir > 0 && list[i] <= y + 4) i++;
    else if (dir < 0 && list[i] >= y - 4) i--;
    i = Math.max(0, Math.min(list.length - 1, i));
    scrollTo({ top: list[i], behavior: reduce ? "auto" : "smooth" });
    busyUntil = performance.now() + (reduce ? 120 : 850);
    return true;
  }
  const finePointer = matchMedia("(hover: hover) and (pointer: fine)").matches;
  if (finePointer) {
    addEventListener("wheel", (e) => {
      if (e.ctrlKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) return; // zoom, sideways
      const now = performance.now();
      const dir = Math.sign(e.deltaY);
      const list = stops(), freeTop = list[list.length - 1];
      const free = scrollY > freeTop + 4 || (scrollY >= freeTop - 4 && dir > 0);
      if (free) return;
      e.preventDefault();
      // A trackpad's glide keeps sending events: one move per gesture.
      if (now < busyUntil) { busyUntil = Math.max(busyUntil, now + 160); return; }
      wheelSum += e.deltaY;
      if (Math.abs(wheelSum) < 24) return;
      wheelSum = 0;
      glide(dir);
    }, { passive: false });
    addEventListener("keydown", (e) => {
      if (e.target.closest && e.target.closest("input, textarea, select, [contenteditable]")) return;
      const down = ["ArrowDown", "PageDown", " "].includes(e.key) && !e.shiftKey;
      const up = ["ArrowUp", "PageUp"].includes(e.key) || (e.key === " " && e.shiftKey);
      if (!down && !up) return;
      if (performance.now() < busyUntil) { e.preventDefault(); return; }
      if (glide(down ? 1 : -1)) e.preventDefault();
    });
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
