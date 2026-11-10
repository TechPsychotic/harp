// Parallax: every [data-speed] element moves at its own rate relative to the
// scroll, measured from the centre of its section, so layers drift past each
// other. Off for people who ask for reduced motion.
(() => {
  const nav = document.getElementById("nav");
  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const items = [...document.querySelectorAll("[data-speed]")].map((el) => ({
    el,
    speed: parseFloat(el.dataset.speed) || 0,
    scene: el.closest("[data-scene]") || document.body,
    base: getComputedStyle(el).transform,
  }));

  let ticking = false;
  const update = () => {
    ticking = false;
    nav.classList.toggle("scrolled", window.scrollY > 20);
    if (reduce) return;
    const vh = window.innerHeight;
    // Gentler on small screens, where there's less room to drift.
    const scale = window.innerWidth < 640 ? 0.5 : 1;
    for (const it of items) {
      const r = it.scene.getBoundingClientRect();
      if (r.bottom < -vh || r.top > 2 * vh) continue;
      const fromCentre = r.top + r.height / 2 - vh / 2;
      const y = (-fromCentre * it.speed * scale).toFixed(1);
      const rest = it.base && it.base !== "none" ? " " + it.base : "";
      it.el.style.transform = `translate3d(0, ${y}px, 0)${rest}`;
    }
    // The lyrics follow the scroll, a line at a time.
    const lyr = document.querySelector(".lyrics");
    if (lyr) {
      const r = lyr.getBoundingClientRect();
      const lines = lyr.querySelectorAll("p");
      const t = 1 - (r.top + r.height / 2) / vh;
      const i = Math.max(0, Math.min(lines.length - 1, Math.floor(t * lines.length * 1.2)));
      lines.forEach((p, k) => p.classList.toggle("on", k === i));
    }
  };
  const onScroll = () => {
    if (!ticking) {
      ticking = true;
      requestAnimationFrame(update);
    }
  };
  window.addEventListener("scroll", onScroll, { passive: true });
  window.addEventListener("resize", onScroll);
  update();

  // Sections fade up as they come into view.
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          e.target.classList.add("in");
          io.unobserve(e.target);
        }
      }
    },
    { threshold: 0.15, rootMargin: "0px 0px -40px 0px" },
  );
  document.querySelectorAll(".reveal").forEach((el) => io.observe(el));
})();
