<script lang="ts">
  // A link inside a sentence. It opens in the browser (the webview never navigates).
  // A link that stands alone is a Button: `ghost sm`, label, then `ExternalLink`.
  import type { Snippet } from "svelte";
  import { api } from "../api";

  let { href, children }: { href: string; children: Snippet } = $props();
</script>

<button
  type="button"
  class="focus-ring inline rounded-sm font-medium text-foreground underline decoration-field-edge underline-offset-4 transition-colors hover:decoration-foreground"
  onclick={() => api.openLink(href)}>{@render children()}</button
>
