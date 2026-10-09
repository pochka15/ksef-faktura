<script>
  let { comparison: c } = $props();
  let showVarying = $state(false);
</script>

<div class="result">
  {#if c.consistent}
    <div class="notice ok">
      Same structure and same fixed values as {c.reference}.
    </div>
  {/if}

  {#if c.missing.length}
    <h3>
      Only in the KSeF invoice <span class="muted">(consider adding)</span>
    </h3>
    <ul class="paths">
      {#each c.missing as p}<li class="mono minus">{p}</li>{/each}
    </ul>
  {/if}

  {#if c.extra.length}
    <h3>Only in ours</h3>
    <ul class="paths">
      {#each c.extra as p}<li class="mono plus">{p}</li>{/each}
    </ul>
  {/if}

  {#if c.different.length}
    <h3>Different values on fields that usually stay the same</h3>
    <table class="data">
      <thead><tr><th>Field</th><th>Ours</th><th>KSeF</th></tr></thead>
      <tbody>
        {#each c.different as d}
          <tr>
            <td class="mono">{d.path}</td>
            <td>{d.ours}</td>
            <td>{d.reference}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if c.varying.length}
    <button class="reset" onclick={() => (showVarying = !showVarying)}>
      {c.varying.length} per-invoice values differ as expected (dates, number, amounts)
      {showVarying ? "▴" : "▾"}
    </button>
    {#if showVarying}
      <table class="data" style="margin-top: 8px">
        <tbody>
          {#each c.varying as d}
            <tr>
              <td class="mono">{d.path}</td>
              <td class="mono">{d.ours}</td>
              <td class="mono muted">{d.reference}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  {/if}
</div>

<style>
  .result {
    margin-top: 16px;
  }

  h3 {
    font-size: 0.9rem;
    font-weight: 600;
    margin: 16px 0 8px;
  }

  .paths {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .paths li {
    padding: 3px 0;
  }

  .minus::before {
    content: "− ";
    color: var(--warn);
  }

  .plus::before {
    content: "+ ";
    color: var(--primary);
  }

  .result :global(table.data td) {
    padding: 8px 12px;
    font-size: 0.85rem;
    word-break: break-word;
  }
</style>
