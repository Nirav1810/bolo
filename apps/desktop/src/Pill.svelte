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
  import { variant } from "./lib/api";
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
   * Whether the idle button is grown into its hint. It opens as soon as the mouse is
   * over it and closes a moment after the mouse leaves, so brushing past or slipping
   * off the edge doesn't make it flicker.
   */
  let open = $state(false);
  /** Bolo Nightly's pill is violet, like its icon, so it's never taken for Bolo's. */
  let nightly = $state(false);
  let closing: ReturnType<typeof setTimeout> | undefined;
  const CLOSE_AFTER = 220;

  // Read by the frame loop; not state, so a level alone renders nothing.
  let level = 0;
  let wave = resting();
  let last = 0;
  let frame = 0;
  const barEls: HTMLElement[] = [];
  const slots = Array.from({ length: BARS });
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");

  function setOpen(want: boolean) {
    clearTimeout(closing);
    if (want) open = true;
    else if (open) closing = setTimeout(() => (open = false), CLOSE_AFTER);
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
          clearTimeout(closing);
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
      .then((v) => (nightly = variant(v).nightly))
      .catch(() => {});
    // The view may have gone out before this page loaded (the idle button at startup).
    invoke<PillView>("pill_view").then((v) => {
      if (view.state === "hidden") apply({ kind: "pill", view: v });
    });
    return () => {
      stopFrames();
      clearTimeout(closing);
      unlisten.then((f) => f());
    };
  });

  // Where the clickable parts are, in CSS pixels from the window's top-left, for the
  // mouse hook. The idle button gets a bigger target than it looks, so it's easy to find
  // and easy to stay on.
  type Hit = { region: string; x: number; y: number; w: number; h: number };
  let reported = "";
  function report() {
    if (!pillEl || view.state === "hidden") return;
    const box = (el: Element, region: string, pad = 0): Hit => {
      const r = el.getBoundingClientRect();
      return { region, x: Math.round(r.left - pad), y: Math.round(r.top - pad), w: Math.round(r.width + 2 * pad), h: Math.round(r.height + 2 * pad) };
    };
    const pad = bar ? 14 : shown.state === "idle" ? 8 : 0;
    const regions = [box(pillEl, "pill", pad)];
    for (const el of pillEl.querySelectorAll<HTMLElement>("[data-hit]")) regions.push(box(el, el.dataset.hit ?? "pill", 3));
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

  const sentence = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
  /** The idle button at rest: a small bar with no content. */
  const bar = $derived(shown.state === "idle" && !open);
  /** What the content crossfades on. Idle's hint stays mounted and fades with `open`. */
  const contentKey = $derived(shown.state);
  /** A one-off flourish on the pill itself for some states. */
  const tone = $derived(
    shown.state === "inserted" || shown.state === "polished" ? "ok" : shown.state === "error" ? "err" : shown.state === "listening" ? "live" : "",
  );
  // Pill padding (14 + 16) and border (2) around the measured content.
  const width = $derived(bar ? "44px" : contentWidth ? `${contentWidth + 32}px` : undefined);
  // Each bar's colour along the waveform, primary blue into violet.
  const barColor = (i: number) => `color-mix(in oklch, var(--color-primary-text), var(--pill-accent-2) ${Math.round((100 * i) / (BARS - 1))}%)`;
</script>

<div class="stage">
  <div class="pw" class:out={view.state === "hidden"} ontransitionend={report}>
    <div
      class="pill {tone}"
      class:bar
      class:nightly
      class:open={shown.state === "idle" && open}
      style:width
      bind:this={pillEl}
      role={shown.state === "error" ? "alert" : "status"}
      aria-label={bar ? "Bolo is ready" : undefined}
    >
      {#key contentKey}
        <div class="content" class:hint={shown.state === "idle"} bind:offsetWidth={contentWidth}>
          {#if shown.state === "idle"}
            <span class="mic" aria-hidden="true">
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                <rect x="5.25" y="1.5" width="5.5" height="8.5" rx="2.75" fill="currentColor" />
                <path d="M3 7.5a5 5 0 0 0 10 0M8 12.5v2" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
              </svg>
            </span>
            <span class="say">Click or hold <kbd>{shown.key}</kbd> to dictate</span>
            {#if nightly}<span class="tag">Nightly</span>{/if}
          {:else if shown.state === "listening"}
            <span class="dot"></span>
            <span class="wave" aria-hidden="true">
              {#each slots as _, i (i)}<i bind:this={barEls[i]} style:background={barColor(i)}></i>{/each}
            </span>
            {#if partial}
              <span class="tail"><span>{partial}</span></span>
            {:else}
              <span class="muted">Listening…</span>
            {/if}
            {#if shown.hands_free}
              <span class="buttons">
                <span class="button" class:hot={hovered === "cancel"} data-hit="cancel" aria-label="Cancel">
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                    <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" />
                  </svg>
                </span>
                <span class="button primary" class:hot={hovered === "stop"} data-hit="stop" aria-label="Stop and insert">
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                    <path d="M3.5 8.5l3 3 6-7" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round" />
                  </svg>
                </span>
              </span>
            {/if}
          {:else if shown.state === "transcribing"}
            <span class="dots" aria-hidden="true"><i></i><i></i><i></i></span><span class="muted">Transcribing</span>
          {:else if shown.state === "inserted"}
            <span class="badge ok" aria-hidden="true">
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                <path class="draw" pathLength="1" d="M3.5 8.5l3 3 6-7" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </span>
            <span>Inserted</span>
          {:else if shown.state === "polishing"}
            <svg class="ic twinkle" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M8 2.5l1.3 3.2L12.5 7 9.3 8.3 8 11.5 6.7 8.3 3.5 7l3.2-1.3L8 2.5z" fill="var(--color-primary-text)" />
              <path d="M12.5 11v3M11 12.5h3" stroke="var(--pill-accent-2)" stroke-width="1.4" stroke-linecap="round" />
            </svg>
            <span class="shimmer">Polishing</span>
          {:else if shown.state === "polished"}
            <span class="badge polish" aria-hidden="true">
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                <path d="M8 1.5l1.6 4L13.5 7l-3.9 1.5L8 12.5 6.4 8.5 2.5 7l3.9-1.5L8 1.5z" fill="currentColor" />
              </svg>
            </span>
            <span>Polished</span>
          {:else if shown.state === "notice"}
            <svg class="ic" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <rect x="4" y="3" width="8" height="10.5" rx="1.5" stroke="var(--color-warning-foreground)" stroke-width="1.4" />
              <path d="M6.2 3V2.2h3.6V3" stroke="var(--color-warning-foreground)" stroke-width="1.4" stroke-linecap="round" />
              <path d="M6.5 7h3M6.5 9.5h3" stroke="var(--color-warning-foreground)" stroke-width="1.4" stroke-linecap="round" />
            </svg>
            <span class="msg">{shown.message}</span>
            {#if shown.hint}<kbd>{shown.hint}</kbd>{/if}
          {:else if shown.state === "error"}
            <svg class="ic" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M8 2.2l6 10.6H2L8 2.2z" stroke="var(--color-destructive-foreground)" stroke-width="1.4" stroke-linejoin="round" />
              <path d="M8 6.5v3" stroke="var(--color-destructive-foreground)" stroke-width="1.5" stroke-linecap="round" />
              <circle cx="8" cy="11.2" r=".8" fill="var(--color-destructive-foreground)" />
            </svg>
            <span class="msg">{sentence(shown.message)}</span>
          {:else if shown.state === "cancelled"}
            <svg class="ic spin-in" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" stroke="var(--color-muted-foreground)" stroke-width="1.6" stroke-linecap="round" />
            </svg>
            <span class="muted">Cancelled</span>
          {/if}
        </div>
      {/key}
    </div>
  </div>
</div>

<style>
  /* The window is transparent and click-through; only the pill is drawn. */
  :global(html.pill-page),
  :global(html.pill-page body) {
    background: transparent;
    margin: 0;
    overflow: hidden;
  }

  .stage {
    --pill-accent-2: oklch(0.66 0.2 305); /* violet, the far end of the waveform */
    display: flex;
    align-items: flex-end;
    justify-content: center;
    height: 100vh;
    padding-bottom: 16px; /* the shadow's room; pill.rs BOTTOM_MARGIN */
    box-sizing: border-box;
    user-select: none;
    cursor: default;
  }

  /* In: springs up from a little below and smaller. Out: quick, no bounce. */
  .pw {
    transform-origin: 50% 100%;
    transition:
      opacity 160ms var(--ease-out-ui),
      transform var(--duration-pop) var(--ease-spring-pop);
  }
  .pw.out {
    opacity: 0;
    transform: translateY(10px) scale(0.82);
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
    padding: 0 16px 0 14px;
    box-sizing: border-box;
    overflow: hidden;
    border-radius: 999px;
    background: var(--color-pill);
    border: 1px solid var(--color-pill-border);
    box-shadow: var(--shadow-pill);
    color: var(--color-foreground);
    font-family: var(--font-sans);
    font-size: 13px;
    line-height: 18px;
    font-weight: 500;
    letter-spacing: -0.005em;
    white-space: nowrap;
    transition:
      width var(--duration-settle) var(--ease-spring-settle),
      height var(--duration-settle) var(--ease-spring-settle),
      padding var(--duration-settle) var(--ease-spring-settle),
      transform var(--duration-pop) var(--ease-spring-pop),
      opacity var(--duration-fast) var(--ease-out-ui),
      background-color var(--duration-base) var(--ease-out-ui);
  }

  /* The idle button at rest: a small bar, quiet until the mouse comes near. */
  .pill.bar {
    height: 8px;
    padding: 0;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.18);
    opacity: 0.8;
  }
  /* Bolo Nightly: a violet bar at rest and a violet edge in every state. */
  .pill.nightly {
    border-color: color-mix(in srgb, var(--pill-accent-2) 55%, transparent);
  }
  .pill.nightly.bar {
    background: linear-gradient(90deg, var(--color-primary), var(--pill-accent-2));
    border-color: transparent;
    opacity: 0.9;
  }
  .tag {
    padding: 0 7px;
    border-radius: 999px;
    font-size: 11px;
    line-height: 18px;
    font-weight: 600;
    color: var(--pill-accent-2);
    background: color-mix(in srgb, var(--pill-accent-2) 16%, transparent);
    flex: none;
  }

  /* Hovered: it grows into the hint and lifts a touch. */
  .pill.open {
    padding: 0 14px 0 6px;
    transform: translateY(-3px);
  }

  /* Success: a ring ripples out once. Error: a short shake. */
  .pill.ok {
    animation: ripple 700ms var(--ease-out-ui);
  }
  @keyframes ripple {
    from {
      box-shadow:
        var(--shadow-pill),
        0 0 0 0 color-mix(in srgb, var(--color-success) 55%, transparent);
    }
    to {
      box-shadow:
        var(--shadow-pill),
        0 0 0 10px color-mix(in srgb, var(--color-success) 0%, transparent);
    }
  }
  .pill.err {
    animation: shake 420ms var(--ease-out-ui);
  }
  @keyframes shake {
    20% {
      transform: translateX(-5px);
    }
    40% {
      transform: translateX(4px);
    }
    60% {
      transform: translateX(-3px);
    }
    80% {
      transform: translateX(1.5px);
    }
  }

  /* Measured for the pill's width, so the width can animate. */
  .content {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
    width: max-content;
    max-width: calc(min(var(--pill-max-width), 100vw - 40px) - 32px);
    animation:
      content-fade 200ms var(--ease-out-ui),
      content-pop var(--duration-pop) var(--ease-spring-pop);
  }
  @keyframes content-fade {
    from {
      opacity: 0;
      filter: blur(3px);
    }
  }
  @keyframes content-pop {
    from {
      transform: scale(0.9) translateY(3px);
    }
  }

  /* Idle's hint stays mounted; it fades in as the bar opens and out as it closes. */
  .content.hint {
    gap: 8px;
    opacity: 0;
    transition: opacity 120ms var(--ease-out-ui);
    animation: none;
  }
  .open .content.hint {
    opacity: 1;
    transition: opacity 200ms var(--ease-out-ui) 60ms;
  }
  .mic {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 999px;
    color: var(--color-primary-foreground);
    background: linear-gradient(135deg, var(--color-primary), var(--pill-accent-2));
    box-shadow: 0 2px 8px -2px color-mix(in srgb, var(--color-primary) 70%, transparent);
    flex: none;
    transform: scale(0.4) rotate(-30deg);
    transition: transform 140ms var(--ease-out-ui);
  }
  .open .mic {
    transform: none;
    transition: transform var(--duration-pop) var(--ease-spring-pop) 40ms;
  }
  .say {
    color: var(--color-muted-foreground);
  }
  kbd {
    display: inline-block;
    padding: 0 6px;
    margin: 0 1px;
    border-radius: 6px;
    background: var(--color-pill-track);
    color: var(--color-foreground);
    font: inherit;
    font-size: 12px;
    line-height: 18px;
    flex: none;
  }

  /* Hands-free: Cancel and Stop. Clicks reach them through Rust's mouse hook, and so
     does hover (the `hot` class). */
  .buttons {
    display: flex;
    gap: 6px;
    flex: none;
    margin-left: 2px;
  }
  .button {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 999px;
    color: var(--color-foreground);
    background: var(--color-pill-track);
    transition:
      transform 360ms var(--ease-spring-pop),
      background-color var(--duration-fast) var(--ease-out-ui),
      box-shadow var(--duration-fast) var(--ease-out-ui);
  }
  .button.hot {
    transform: scale(1.18);
    background: color-mix(in srgb, var(--color-destructive-foreground) 22%, var(--color-pill-track));
  }
  .button.primary {
    color: var(--color-primary-foreground);
    background: var(--color-primary);
  }
  .button.primary.hot {
    background: var(--color-primary);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--color-primary) 28%, transparent);
  }

  /* The live dot breathes with the voice and sends out a slow pulse. */
  .dot {
    position: relative;
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--color-live);
    flex: none;
    transform: scale(calc(1 + var(--lvl) * 0.45));
  }
  .dot::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--color-live);
    animation: pulse 1.4s var(--ease-out-ui) infinite;
  }
  @keyframes pulse {
    from {
      opacity: 0.55;
      transform: scale(1);
    }
    to {
      opacity: 0;
      transform: scale(2.6);
    }
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
    width: 3px;
    height: 3px;
    border-radius: 999px;
  }

  /* One line; older words fade off the left edge, the newest stays in view. */
  .tail {
    display: flex;
    justify-content: flex-end;
    overflow: hidden;
    min-width: 0;
    mask-image: linear-gradient(to right, transparent, #000 28px);
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
    color: var(--color-muted-foreground);
    flex: none;
  }
  .ic {
    flex: none;
    display: block;
    animation: icon-pop var(--duration-pop) var(--ease-spring-pop) 40ms both;
  }
  @keyframes icon-pop {
    from {
      transform: scale(0.3) rotate(-25deg);
    }
  }
  .spin-in {
    animation-name: spin-in;
  }
  @keyframes spin-in {
    from {
      transform: scale(0.3) rotate(-120deg);
    }
  }

  /* Inserted and Polished: a filled badge that pops, the tick drawing itself in. */
  .badge {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 999px;
    color: #fff;
    flex: none;
    animation: icon-pop var(--duration-pop) var(--ease-spring-pop) 30ms both;
  }
  .badge.ok {
    background: var(--color-success);
  }
  .badge.polish {
    background: linear-gradient(135deg, var(--color-primary), var(--pill-accent-2));
  }
  .draw {
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: draw 280ms var(--ease-out-ui) 140ms forwards;
  }
  @keyframes draw {
    to {
      stroke-dashoffset: 0;
    }
  }

  /* Transcribing: three dots hopping in turn. */
  .dots {
    display: flex;
    gap: 3px;
    flex: none;
  }
  .dots i {
    width: 5px;
    height: 5px;
    border-radius: 999px;
    background: var(--color-primary-text);
    animation: hop 900ms var(--ease-in-out-ui) infinite;
  }
  .dots i:nth-child(2) {
    animation-delay: 120ms;
    background: color-mix(in oklch, var(--color-primary-text), var(--pill-accent-2) 50%);
  }
  .dots i:nth-child(3) {
    animation-delay: 240ms;
    background: var(--pill-accent-2);
  }
  @keyframes hop {
    0%,
    60%,
    100% {
      transform: translateY(0);
    }
    30% {
      transform: translateY(-5px);
    }
  }

  .twinkle {
    animation:
      icon-pop var(--duration-pop) var(--ease-spring-pop) 40ms both,
      twinkle 1.6s var(--ease-in-out-ui) 600ms infinite;
  }
  @keyframes twinkle {
    50% {
      transform: scale(1.15) rotate(18deg);
    }
  }

  .shimmer {
    background: linear-gradient(90deg, var(--color-muted-foreground) 0%, var(--color-foreground) 45%, var(--color-muted-foreground) 90%);
    background-size: 200% 100%;
    background-clip: text;
    color: transparent;
    animation: sh 1.6s linear infinite;
  }
  @keyframes sh {
    from {
      background-position: 100% 0;
    }
    to {
      background-position: -100% 0;
    }
  }

  /* Reduced motion keeps the fades and drops the movement; the waveform holds still. */
  @media (prefers-reduced-motion: reduce) {
    .pw,
    .pw.out,
    .pill,
    .pill.open,
    .button.hot,
    .mic,
    .open .mic,
    .dot {
      transform: none;
    }
    .pill,
    .button,
    .mic {
      transition: opacity var(--duration-fast) linear;
    }
    .content {
      animation: content-fade 160ms linear;
    }
    .pill.err,
    .pill.ok,
    .ic,
    .badge,
    .dot::after,
    .dots i,
    .twinkle,
    .shimmer {
      animation: none;
    }
    .draw {
      animation: none;
      stroke-dashoffset: 0;
    }
    .shimmer {
      color: var(--color-muted-foreground);
    }
  }
</style>
