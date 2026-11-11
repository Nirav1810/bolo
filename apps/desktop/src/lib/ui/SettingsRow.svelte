<script lang="ts">
  // One setting: title and description on the left, its control on the right, and
  // optional content below (progress, an inline form). T3 Code's SettingsRow.
  import type { Snippet } from "svelte";

  let {
    title,
    labelFor,
    titleId,
    description,
    badges,
    control,
    status,
    leading,
    children,
    class: className = "",
  }: {
    title: string;
    labelFor?: string;
    titleId?: string;
    description?: string | Snippet;
    badges?: Snippet;
    control?: Snippet;
    status?: Snippet;
    leading?: Snippet;
    children?: Snippet;
    class?: string;
  } = $props();
</script>

<div class="group/row px-4 py-3 {className}">
  <div class="flex items-center gap-3">
    {#if leading}<div class="flex shrink-0 items-center">{@render leading()}</div>{/if}
    <div class="flex min-w-0 flex-1 flex-col gap-0.5">
      <div class="flex min-h-5 flex-wrap items-center gap-1.5">
        {#if labelFor}
          <label for={labelFor} id={titleId} class="text-sm font-medium text-foreground">{title}</label>
        {:else}
          <h3 id={titleId} class="text-sm font-medium text-foreground">{title}</h3>
        {/if}
        {@render badges?.()}
      </div>
      {#if description}
        <div class="max-w-xl text-xs leading-[18px] text-pretty text-muted-foreground">
          {#if typeof description === "string"}{description}{:else}{@render description()}{/if}
        </div>
      {/if}
      {#if status}<div class="pt-0.5 text-xs">{@render status()}</div>{/if}
    </div>
    {#if control}<div class="flex shrink-0 items-center gap-1.5">{@render control()}</div>{/if}
  </div>
  {#if children}<div class="mt-2.5">{@render children()}</div>{/if}
</div>
