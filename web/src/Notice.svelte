<script>
  // A server message: its line breaks are kept, nothing else is.
  let { kind = "err", text, style = "" } = $props();
</script>

<div class="notice {kind}" {style}>
  {#each String(text).split("\n") as line}<div>{line}</div>{/each}
</div>
