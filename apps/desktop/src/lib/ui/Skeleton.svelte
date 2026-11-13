<script lang="ts">
  // What stands in for a page's cards while they load.
  let { rows = 3, tall = false }: { rows?: number; tall?: boolean } = $props();
</script>

<div class="flex flex-col gap-3" aria-hidden="true">
  {#each Array(rows) as _, i (i)}
    <div class="animate-pulse rounded-xl bg-muted {tall ? 'h-24' : 'h-16'}"></div>
  {/each}
</div>
