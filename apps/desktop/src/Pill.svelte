<script lang="ts">
  // The overlay pill (M3 ticket 08, design in ticket 04, motion reworked after the UI
  // overhaul). Rust decides what it shows and when its window appears
  // (src-tauri/src/pill.rs); this only draws the view, the waveform and the partial
  // text, and plays the enter, exit and state changes.
  //
  // The window is click-through: Rust's mouse hook does hover, clicks and drags
  // (src-tauri/src/pointer.rs). So this reports where its clickable parts are, and hears
  // back which one the mouse is over (the `hover` event).
  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { Check, CircleAlert, Mic, Sparkles, TriangleAlert, X } from "@lucide/svelte";
  import { sentence, variant } from "./lib/api";
  import { onAppEvent, type AppEvent, type PillView } from "./lib/events";
  import { BARS, MAX, MIN, loudness, resting, step } from "./lib/waveform";

  let view: PillView = $state({ state: "hidden" });
  /** The last view shown, kept on screen while the pill plays its exit. */
  let shown = $state<PillView>({ state: "listening", hands_free: false } as PillView);
  let partial = $state("");
  let contentWidth = $state(0);
  let pillEl: HTMLElement | undefined = $state();
  /** The part of the pill under the mouse, from Rust's mouse hook. */
  let hovered = $state<"pill" | "stop" | "cancel" | null>(null);
  /**
   * Whether the idle button is grown into its hint. It opens only once the mouse has
   * rested on it for a moment (so passing over it on the way somewhere else does
   * nothing), and closes a moment after the mouse leaves (so slipping off the edge
   * doesn't make it flicker).
   */
  let open = $state(false);
  /** Bolo Nightly's pill is violet, like its icon, so it's never taken for Bolo's. */
  let nightly = $state(false);
  let product = $state("Bolo");
  let timer: ReturnType<typeof setTimeout> | undefined;
  const OPEN_AFTER = 180;
  const CLOSE_AFTER = 260;

  // Read by the frame loop; not state, so a level alone renders nothing.
  let level = 0;
  let wave = resting();
  let last = 0;
  let frame = 0;
  const barEls: HTMLElement[] = [];
  const slots = Array.from({ length: BARS });
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");

  function setOpen(want: boolean) {
    clearTimeout(timer);
    if (want === open) return;
    timer = setTimeout(() => (open = want), want ? OPEN_AFTER : CLOSE_AFTER);
  }

  function apply(e: AppEvent) {
    switch (e.kind) {
      case "pill":
        if (e.view.state === "listening" && view.state !== "listening") {
          partial = "";
          wave = resting();
          level = 0;
          drawWave();
        }
        view = e.view;
        if (e.view.state !== "hidden") shown = e.view;
        if (e.view.state === "idle") setOpen(e.view.hover);
        else {
          clearTimeout(timer);
          open = false;
        }
        if (e.view.state === "listening") startFrames();
        else stopFrames();
        break;
      case "hover":
        hovered = e.region;
        break;
      case "level":
        level = loudness(e.rms);
        break;
      case "partial":
        partial = e.text;
        break;
    }
  }

  // The waveform is written straight to the DOM once per display frame, outside Svelte's
  // reactivity, so a frame costs a handful of style writes and nothing else.
  function drawWave() {
    for (let i = 0; i < barEls.length; i++) {
      const el = barEls[i];
      if (!el) continue;
      const h = wave.bars[i];
      el.style.height = `${h.toFixed(2)}px`;
      el.style.opacity = `${0.55 + (0.45 * (h - MIN)) / (MAX - MIN)}`;
    }
    pillEl?.style.setProperty("--lvl", wave.env.toFixed(3));
  }

  function onFrame(t: number) {
    frame = requestAnimationFrame(onFrame);
    const dt = last ? Math.min(t - last, 64) : 1000 / 60;
    last = t;
    if (reduced.matches) return;
    step(wave, level, t, dt);
    drawWave();
  }

  function startFrames() {
    if (frame) return;
    last = 0;
    frame = requestAnimationFrame(onFrame);
  }

  function stopFrames() {
    cancelAnimationFrame(frame);
    frame = 0;
  }

  $effect(() => {
    const unlisten = onAppEvent(apply);
    getVersion()
      .then((v) => {
        ({ nightly, product } = variant(v));
      })
      .catch(() => {});
    // The view may have gone out before this page loaded (the idle button at startup).
    invoke<PillView>("pill_view").then((v) => {
      if (view.state === "hidden") apply({ kind: "pill", view: v });
    });
    return () => {
      stopFrames();
      clearTimeout(timer);
      unlisten.then((f) => f());
    };
  });

  // Where the clickable parts are, in CSS pixels from the window's top-left, for the
  // mouse hook. The resting bar gets a little slack around it so it's easy to land on,
  // but not so much that the pointer catches it just passing by; the open pill gets
  // barely any, so leaving it is where it looks like it ends.
  type Hit = { region: string; x: number; y: number; w: number; h: number };
  let reported = "";
  function report() {
    if (!pillEl || view.state === "hidden") return;
    const box = (el: Element, region: string, px = 0, py = px): Hit => {
      const r = el.getBoundingClientRect();
      return { region, x: Math.round(r.left - px), y: Math.round(r.top - py), w: Math.round(r.width + 2 * px), h: Math.round(r.height + 2 * py) };
    };
    const regions = bar ? [box(pillEl, "pill", 6, 6)] : [box(pillEl, "pill", shown.state === "idle" ? 2 : 0)];
    for (const el of pillEl.querySelectorAll<HTMLElement>("[data-hit]")) regions.push(box(el, el.dataset.hit ?? "pill", 2));
    const json = JSON.stringify(regions);
    if (json === reported) return;
    reported = json;
    invoke("pill_regions", { regions }).catch(() => {});
  }

  // Re-measure after each change of view and whenever the pill resizes or settles.
  $effect(() => {
    void view;
    void open;
    requestAnimationFrame(report);
  });
  $effect(() => {
    if (!pillEl) return;
    const ro = new ResizeObserver(() => report());
    ro.observe(pillEl);
    return () => ro.disconnect();
  });

  /** The idle button at rest: a small bar with no content. */
  const bar = $derived(shown.state === "idle" && !open);
  /** What the content crossfades on. Idle's hint stays mounted and fades with `open`. */
  const contentKey = $derived(shown.state);
  // Pill padding (12 + 14) and border (2) around the measured content.
  const width = $derived(bar ? "40px" : contentWidth ? `${contentWidth + 28}px` : undefined);
</script>

<div class="stage">
  <div class="pw" class:out={view.state === "hidden"} ontransitionend={report}>
    <div
      class="pill"
      class:bar
      class:nightly
      class:open={shown.state === "idle" && open}
      style:width
      bind:this={pillEl}
      role={shown.state === "error" ? "alert" : "status"}
      aria-label={bar ? `${product} is ready` : undefined}
    >
      {#key contentKey}
        <div class="content" class:hint={shown.state === "idle"} bind:offsetWidth={contentWidth}>
          {#if shown.state === "idle"}
            <Mic class="ic muted" size={14} aria-hidden="true" />
            <span class="muted">Click or hold <kbd class="key">{shown.key}</kbd> to dictate</span>
            {#if nightly}<span class="badge">Nightly</span>{/if}
          {:else if shown.state === "listening"}
            <span class="dot"></span>
            <span class="wave" aria-hidden="true">
              {#each slots as _, i (i)}<i bind:this={barEls[i]}></i>{/each}
            </span>
            {#if partial}
              <span class="tail"><span>{partial}</span></span>
            {:else}
              <span class="muted">Listening…</span>
            {/if}
            {#if shown.hands_free}
              <!-- Not real buttons: the window is click-through and Rust's mouse hook
                   presses them (data-hit) and reports hover (`hot`). -->
              <span class="buttons">
                <span class="button" class:hot={hovered === "cancel"} data-hit="cancel" aria-label="Cancel">
                  <X size={12} aria-hidden="true" />
                </span>
                <span class="button primary" class:hot={hovered === "stop"} data-hit="stop" aria-label="Stop and insert">
                  <Check size={12} aria-hidden="true" />
                </span>
              </span>
            {/if}
          {:else if shown.state === "transcribing"}
            <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span><span class="muted">Transcribing…</span>
          {:else if shown.state === "inserted"}
            <!-- Drawn by hand so the tick can draw itself in. -->
            <svg class="ic success" width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path class="draw" pathLength="1" d="M3.5 8.5l3 3 6-7" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            <span>Inserted</span>
          {:else if shown.state === "polishing"}
            <Sparkles class="ic primary breathe" size={14} aria-hidden="true" />
            <span class="muted">Polishing…</span>
          {:else if shown.state === "polished"}
            <Sparkles class="ic primary" size={14} aria-hidden="true" />
            <span>Polished</span>
          {:else if shown.state === "notice"}
            <TriangleAlert class="ic warning" size={14} aria-hidden="true" />
            <span class="msg">{sentence(shown.message)}</span>
            {#if shown.hint}<kbd class="key">{shown.hint}</kbd>{/if}
          {:else if shown.state === "error"}
            <CircleAlert class="ic error" size={14} aria-hidden="true" />
            <span class="msg">{sentence(shown.message)}</span>
          {:else if shown.state === "cancelled"}
            <X class="ic muted" size={14} aria-hidden="true" />
            <span class="muted">Cancelled</span>
          {/if}
        </div>
      {/key}
    </div>
  </div>
</div>

<style>
  /* The window is transparent and click-through; only the pill is drawn. Every colour,
     size and duration here is a token from theme.css, so the pill speaks the windows'
     language. */
  :global(html.pill-page),
  :global(html.pill-page body) {
    background: transparent;
    margin: 0;
    overflow: hidden;
  }

  /* One curve for everything: fast start, long soft landing, no overshoot. */
  .stage {
    display: flex;
    align-items: flex-end;
    justify-content: center;
    height: 100vh;
    padding-bottom: 16px; /* the shadow's room; pill.rs BOTTOM_MARGIN */
    box-sizing: border-box;
    user-select: none;
    cursor: default;
  }

  /* In: rises a few pixels into place. Out: quicker, the same way back. */
  .pw {
    transform-origin: 50% 100%;
    transition:
      opacity var(--duration-base) var(--ease-out-ui),
      transform var(--duration-resize) var(--ease-out-ui);
  }
  .pw.out {
    opacity: 0;
    transform: translateY(6px) scale(0.97);
    transition:
      opacity var(--duration-exit) var(--ease-out-ui),
      transform var(--duration-exit) var(--ease-out-ui);
  }

  .pill {
    --lvl: 0;
    display: flex;
    align-items: center;
    height: var(--pill-height);
    max-width: min(var(--pill-max-width), 100vw - 40px);
    padding: 0 14px 0 12px;
    box-sizing: border-box;
    overflow: hidden;
    border-radius: 999px;
    background: var(--pill-bg);
    border: 1px solid var(--pill-border);
    box-shadow: var(--pill-shadow);
    color: var(--foreground);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    line-height: var(--text-sm--line-height);
    font-weight: var(--weight-medium);
    white-space: nowrap;
    transition:
      width var(--duration-resize) var(--ease-out-ui),
      height var(--duration-resize) var(--ease-out-ui),
      padding var(--duration-resize) var(--ease-out-ui),
      border-color var(--duration-base) var(--ease-out-ui),
      box-shadow var(--duration-base) var(--ease-out-ui);
  }

  /* The idle button at rest: a slim, quiet capsule. */
  .pill.bar {
    height: 8px;
    padding: 0;
    box-shadow: var(--pill-bar-shadow);
  }
  /* Bolo Nightly: a violet edge in every state, and a violet bar at rest. */
  .pill.nightly {
    border-color: var(--nightly-edge);
  }
  .pill.nightly.bar {
    background: var(--nightly);
    border-color: transparent;
  }

  /* Measured for the pill's width, so the width can animate. */
  .content {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
    width: max-content;
    max-width: calc(min(var(--pill-max-width), 100vw - 40px) - 28px);
    animation: content-in var(--duration-base) var(--ease-out-ui);
  }
  @keyframes content-in {
    from {
      opacity: 0;
      transform: scale(0.98);
    }
  }

  /* Idle's hint stays mounted; it fades in once the bar has mostly opened and out at
     once when it closes. */
  .content.hint {
    opacity: 0;
    transition: opacity var(--duration-exit) var(--ease-out-ui);
    animation: none;
  }
  .open .content.hint {
    opacity: 1;
    transition: opacity var(--duration-base) var(--ease-out-ui) 80ms;
  }

  /* A key, drawn as Kbd.svelte draws it. */
  .key {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    height: 20px;
    min-width: 20px;
    padding: 0 6px;
    margin: 0 2px;
    vertical-align: baseline; /* its text sits on the same line as the label beside it */
    border-radius: var(--radius-md);
    border: 1px solid var(--input);
    background: var(--control);
    box-shadow: var(--elev-control), var(--bevel);
    color: var(--foreground);
    font: inherit;
    font-size: var(--text-xs);
    line-height: var(--text-xs--line-height);
    font-weight: var(--weight-medium);
    flex: none;
  }

  /* The Nightly marker, drawn as Badge's `nightly` variant. */
  .badge {
    display: inline-flex;
    align-items: center;
    box-sizing: border-box;
    height: 20px;
    padding: 0 6px;
    border-radius: var(--radius-md);
    border: 1px solid transparent;
    background: var(--nightly-tint);
    color: var(--nightly-foreground);
    font-size: var(--text-xs);
    line-height: var(--text-xs--line-height);
    font-weight: var(--weight-medium);
    flex: none;
  }

  /* Hands-free: Cancel and Stop. Clicks reach them through Rust's mouse hook, and so
     does hover (the `hot` class). */
  .buttons {
    display: flex;
    gap: 4px;
    flex: none;
    margin-left: 4px;
  }
  .button {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 999px;
    color: var(--muted-foreground);
    background: var(--pill-track);
    transition:
      background-color var(--duration-fast) var(--ease-out-ui),
      color var(--duration-fast) var(--ease-out-ui);
  }
  .button.hot {
    color: var(--foreground);
    background: var(--pill-hot);
  }
  .button.primary {
    color: var(--primary-foreground);
    background: var(--primary);
  }
  .button.primary.hot {
    background: var(--primary-hover);
  }

  /* The live dot swells a little with the voice. */
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 999px;
    background: var(--live);
    flex: none;
    transform: scale(calc(1 + var(--lvl) * 0.3));
  }

  .wave {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 24px;
    flex: none;
    contain: layout style;
  }
  .wave i {
    display: block;
    width: 2.5px;
    height: 3px;
    border-radius: 999px;
    background: var(--primary-text);
  }

  /* One line; older words fade off the left edge, the newest stays in view. The mask
     reads only alpha, so any opaque colour does. */
  .tail {
    display: flex;
    justify-content: flex-end;
    overflow: hidden;
    min-width: 0;
    mask-image: linear-gradient(to right, transparent, var(--foreground) 28px);
  }
  .tail span {
    white-space: nowrap;
  }

  .msg {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .muted {
    color: var(--muted-foreground);
    flex: none;
  }

  /* State icons (lucide components, so their classes are global). */
  .content :global(.ic) {
    flex: none;
    display: block;
  }
  .content :global(.ic.muted) {
    color: var(--muted-foreground);
  }
  .content :global(.ic.primary) {
    color: var(--primary-text);
  }
  .content :global(.ic.success) {
    color: var(--success-foreground);
  }
  .content :global(.ic.warning) {
    color: var(--warning-foreground);
  }
  .content :global(.ic.error) {
    color: var(--destructive-foreground);
  }

  /* Inserted: the tick draws itself in. */
  .draw {
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: draw var(--duration-resize) var(--ease-out-ui) 60ms forwards;
  }
  @keyframes draw {
    to {
      stroke-dashoffset: 0;
    }
  }

  /* Transcribing: three dots fading in turn. */
  .dots {
    display: flex;
    gap: 3px;
    flex: none;
  }
  .dots i {
    width: 4px;
    height: 4px;
    border-radius: 999px;
    background: var(--muted-foreground);
    animation: fade 1s var(--ease-in-out-ui) infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 160ms;
  }
  .dots i:nth-child(3) {
    animation-delay: 320ms;
  }
  @keyframes fade {
    0%,
    100% {
      opacity: 0.3;
    }
    40% {
      opacity: 1;
    }
  }

  .content :global(.breathe) {
    animation: breathe 1.4s var(--ease-in-out-ui) infinite;
  }
  @keyframes breathe {
    50% {
      opacity: 0.4;
    }
  }

  /* Reduced motion keeps the fades and drops the movement; the waveform holds still. */
  @media (prefers-reduced-motion: reduce) {
    .pw,
    .pw.out,
    .dot {
      transform: none;
    }
    .pill {
      transition:
        border-color var(--duration-fast) linear,
        box-shadow var(--duration-fast) linear;
    }
    .content {
      animation: none;
    }
    .dots i,
    .content :global(.breathe) {
      animation: none;
    }
    .draw {
      animation: none;
      stroke-dashoffset: 0;
    }
  }
</style>
