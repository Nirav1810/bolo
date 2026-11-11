<script lang="ts">
  // The overlay pill (M3 ticket 08, design in ticket 04). Rust decides what it shows and
  // when its window appears (src-tauri/src/pill.rs); this only draws the view, the
  // waveform and the partial text, and plays the enter, exit and state changes.
  import { onAppEvent, type AppEvent, type PillView } from "./lib/events";
  import { barHeight, resting, step } from "./lib/waveform";

  let view: PillView = $state({ state: "hidden" });
  /** The last view shown, kept on screen while the pill plays its exit. */
  let shown: PillView = $state({ state: "listening" });
  let partial = $state("");
  let bars: number[] = $state(resting());
  let contentWidth = $state(0);

  // Read by the frame loop; not state, so a level alone renders nothing.
  let level = 0;
  let head = 0;
  let last = 0;
  let frame = 0;
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");

  function apply(e: AppEvent) {
    switch (e.kind) {
      case "pill":
        if (e.view.state === "listening" && view.state !== "listening") {
          partial = "";
          bars = resting();
          head = 0;
          level = 0;
        }
        view = e.view;
        if (e.view.state !== "hidden") shown = e.view;
        if (e.view.state === "listening") startFrames();
        else stopFrames();
        break;
      case "level":
        level = e.rms;
        break;
      case "partial":
        partial = e.text;
        break;
    }
  }

  // One waveform update per display frame, however often levels arrive.
  function onFrame(t: number) {
    frame = requestAnimationFrame(onFrame);
    const dt = last ? Math.min(t - last, 100) : 1000 / 60;
    last = t;
    if (reduced.matches) return;
    const next = step(bars, head, barHeight(level), dt);
    if (next) ({ bars, head } = next);
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
    return () => {
      stopFrames();
      unlisten.then((f) => f());
    };
  });

  const sentence = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
  // Pill padding (14 + 16) and border (2) around the measured content.
  const width = $derived(contentWidth ? `${contentWidth + 32}px` : undefined);
</script>

<div class="stage">
  <div class="pw" class:out={view.state === "hidden"}>
    <div class="pill" style:width role={shown.state === "error" ? "alert" : "status"}>
      {#key shown.state}
        <div class="content" bind:offsetWidth={contentWidth}>
          {#if shown.state === "listening"}
            <span class="dot"></span>
            <span class="wave" aria-hidden="true">
              {#each bars as h, i (i)}<i style:height="{h}px"></i>{/each}
            </span>
            {#if partial}
              <span class="tail"><span>{partial}</span></span>
            {:else}
              <span class="muted">Listening…</span>
            {/if}
          {:else if shown.state === "transcribing"}
            <span class="spin" aria-hidden="true"></span><span class="muted">Transcribing</span>
          {:else if shown.state === "inserted"}
            <svg class="ic" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M3.5 8.5l3 3 6-7" stroke="var(--color-ok)" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            <span>Inserted</span>
          {:else if shown.state === "polishing"}
            <svg class="ic" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M8 2.5l1.3 3.2L12.5 7 9.3 8.3 8 11.5 6.7 8.3 3.5 7l3.2-1.3L8 2.5z" stroke="var(--color-accent)" stroke-width="1.4" stroke-linejoin="round" />
              <path d="M12.5 11v3M11 12.5h3" stroke="var(--color-accent)" stroke-width="1.4" stroke-linecap="round" />
            </svg>
            <span class="shimmer">Polishing</span>
          {:else if shown.state === "polished"}
            <svg class="ic" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M3.5 8.5l3 3 6-7" stroke="var(--color-accent)" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            <span>Polished</span>
          {:else if shown.state === "notice"}
            <svg class="ic" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <rect x="4" y="3" width="8" height="10.5" rx="1.5" stroke="var(--color-warn)" stroke-width="1.4" />
              <path d="M6.2 3V2.2h3.6V3" stroke="var(--color-warn)" stroke-width="1.4" stroke-linecap="round" />
              <path d="M6.5 7h3M6.5 9.5h3" stroke="var(--color-warn)" stroke-width="1.4" stroke-linecap="round" />
            </svg>
            <span class="msg">{shown.message}</span>
            {#if shown.hint}<span class="muted">{shown.hint}</span>{/if}
          {:else if shown.state === "error"}
            <svg class="ic" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M8 2.2l6 10.6H2L8 2.2z" stroke="var(--color-danger)" stroke-width="1.4" stroke-linejoin="round" />
              <path d="M8 6.5v3" stroke="var(--color-danger)" stroke-width="1.5" stroke-linecap="round" />
              <circle cx="8" cy="11.2" r=".8" fill="var(--color-danger)" />
            </svg>
            <span class="msg">{sentence(shown.message)}</span>
          {:else if shown.state === "cancelled"}
            <svg class="ic" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" stroke="var(--color-text-2)" stroke-width="1.6" stroke-linecap="round" />
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
    display: flex;
    align-items: flex-end;
    justify-content: center;
    height: 100vh;
    padding-bottom: 16px; /* the shadow's room; pill.rs BOTTOM_MARGIN */
    box-sizing: border-box;
    user-select: none;
    cursor: default;
  }

  .pw {
    transition:
      opacity var(--duration-base) var(--ease-out-ui),
      transform var(--duration-base) var(--ease-out-ui);
  }
  .pw.out {
    opacity: 0;
    transform: translateY(8px) scale(0.96);
    transition-duration: var(--duration-exit);
  }

  .pill {
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
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-pill);
    line-height: var(--text-pill--line-height);
    font-weight: var(--text-pill--font-weight);
    white-space: nowrap;
    transition: width var(--duration-resize) var(--ease-in-out-ui);
  }

  /* Measured for the pill's width, so the width can animate. */
  .content {
    display: flex;
    flex: none;
    align-items: center;
    gap: 10px;
    width: max-content;
    max-width: calc(min(var(--pill-max-width), 100vw - 40px) - 32px);
    animation: enter var(--duration-base) var(--ease-out-ui);
  }
  @keyframes enter {
    from {
      opacity: 0;
      filter: blur(2px);
    }
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--color-live);
    flex: none;
  }

  .wave {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 24px;
    flex: none;
  }
  .wave i {
    display: block;
    width: 3px;
    border-radius: 2px;
    background: var(--color-text);
    opacity: 0.85;
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
    color: var(--color-text-2);
    flex: none;
  }
  .ic {
    flex: none;
    display: block;
  }

  .spin {
    width: 14px;
    height: 14px;
    box-sizing: border-box;
    border: 2px solid var(--color-pill-track);
    border-top-color: var(--color-text);
    border-radius: 50%;
    animation: rot 0.7s linear infinite;
    flex: none;
  }
  @keyframes rot {
    to {
      transform: rotate(360deg);
    }
  }

  .shimmer {
    background: linear-gradient(90deg, var(--color-text-2) 0%, var(--color-text) 45%, var(--color-text-2) 90%);
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
    .pw.out {
      transform: none;
    }
    .pill {
      transition: none;
    }
    .content {
      animation-name: fade;
    }
    .spin,
    .shimmer {
      animation: none;
    }
    .shimmer {
      color: var(--color-text-2);
    }
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
</style>
