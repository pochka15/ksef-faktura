<script>
  // `sent` maps environment → KSeF number, e.g. { prod: "5265877635-20261031-…" }.
  let { sent } = $props();
  let envs = $derived(Object.keys(sent ?? {}));
</script>

<span class="row" style="gap: 6px">
  {#each envs as env}
    <span
      class="badge"
      class:ok={env === "prod"}
      class:primary={env !== "prod"}
      title={sent[env]}
    >
      <span class="dot"></span>{env === "prod" ? "in KSeF" : env}
    </span>
  {:else}
    <span class="badge">local only</span>
  {/each}
</span>
