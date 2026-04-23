const BASE_CLASSES = ["transition-colors", "duration-500", "rounded-md"];
const HIGHLIGHT_CLASSES = ["bg-amber-100/60", "dark:bg-amber-900/30"];
const HOLD_MS = 1800;

let pending: { el: HTMLElement; timer: number } | null = null;

function clearPending() {
  if (!pending) return;
  window.clearTimeout(pending.timer);
  pending.el.classList.remove(...HIGHLIGHT_CLASSES);
  pending = null;
}

export function scrollAndHighlight(root: HTMLElement | null, ms: number) {
  if (!root) return;
  const rows = [
    ...root.querySelectorAll("[data-timestamp-ms]"),
  ] as HTMLElement[];
  const sorted = rows
    .map((el) => ({
      el,
      t: parseInt(el.getAttribute("data-timestamp-ms") ?? "0", 10),
    }))
    .sort((a, b) => a.t - b.t);
  let best: HTMLElement | null = null;
  for (const { el, t } of sorted) {
    if (t <= ms) best = el;
    else break;
  }
  if (!best) return;
  best.scrollIntoView({ behavior: "smooth", block: "nearest" });

  clearPending();
  const target = best;
  target.classList.add(...BASE_CLASSES, ...HIGHLIGHT_CLASSES);
  const timer = window.setTimeout(() => {
    target.classList.remove(...HIGHLIGHT_CLASSES);
    if (pending?.el === target) pending = null;
  }, HOLD_MS);
  pending = { el: target, timer };
}
