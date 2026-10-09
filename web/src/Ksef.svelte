<script>
  import { untrack } from "svelte";
  import * as api from "./api.js";
  import Notice from "./Notice.svelte";
  import Comparison from "./Comparison.svelte";

  let { app, refresh, go } = $props();

  // Only prod unless the app runs with --test-envs; then you pick one each time.
  const multi = untrack(() => app.envs.length > 1);
  let env = $state(multi ? null : untrack(() => app.envs[0]));
  let days = $state(90);
  let fetching = $state(false);
  let fetchError = $state("");
  let fetchMessage = $state("");

  let ours = $state(untrack(() => app.invoices[0]?.number ?? ""));
  let reference = $state("");
  let comparison = $state(null);
  let compareError = $state("");

  async function runFetch() {
    fetching = true;
    fetchError = "";
    fetchMessage = "";
    try {
      const r = await api.fetchFromKsef(env, Number(days));
      fetchMessage = `${r.count} invoice${r.count === 1 ? "" : "s"} issued in the last ${days} days on KSeF ${env}.`;
      await refresh();
    } catch (e) {
      fetchError = e.message;
    } finally {
      fetching = false;
    }
  }

  async function runCompare(f) {
    reference = `${f.env}/${f.ksef_number}`;
    compareError = "";
    comparison = null;
    try {
      comparison = await api.compare(ours, f.env, f.ksef_number);
    } catch (e) {
      compareError = e.message;
    }
  }
</script>

<section class="page-head">
  <div>
    <div class="eyebrow">KSeF</div>
    <h1>Your invoices in KSeF</h1>
    <p class="lead">
      Download what you already issued (with this app or another program) and
      check that new invoices have the same XML shape. Fetching only reads;
      nothing is sent.
    </p>
  </div>
</section>

<div class="card">
  <div class="card-head">
    <h2>Fetch</h2>
  </div>
  <div class="row">
    {#if multi}
      <div class="segmented" role="group" aria-label="Environment">
        {#each app.envs as e}
          <button class:active={env === e} onclick={() => (env = e)}>{e}</button
          >
        {/each}
      </div>
    {/if}
    <label class="row small">
      last
      <input
        type="number"
        min="1"
        max="90"
        bind:value={days}
        style="width: 76px"
      />
      days
    </label>
    <button
      class="primary"
      onclick={runFetch}
      disabled={!env || fetching || !app.tokens[env]}
    >
      {#if fetching}<span class="spinner"></span>{/if}
      {env ? `Fetch from ${multi ? env : "KSeF"}` : "Fetch"}
    </button>
  </div>
  {#if !env}
    <p class="muted small" style="margin: 12px 0 0">
      Pick an environment. Your earlier real invoices are in
      <strong>prod</strong>.
    </p>
  {:else if !app.tokens[env]}
    <div class="notice warn" style="margin-top: 12px">
      No {env} token in the Keychain. Press <kbd>t</kbd> in the terminal; the token
      needs the "przeglądanie faktur" (view invoices) permission.
    </div>
  {/if}
  {#if fetchError}<Notice text={fetchError} style="margin-top: 12px" />{/if}
  {#if fetchMessage}<div class="notice ok" style="margin-top: 12px">
      {fetchMessage}
    </div>{/if}
</div>

<div class="card" style="padding-bottom: 8px">
  <div class="card-head">
    <div>
      <h2>Downloaded</h2>
      <p class="muted small">Compare one of them with your invoice:</p>
    </div>
    <select bind:value={ours} style="width: auto">
      {#each app.invoices as inv}
        <option value={inv.number}
          >{inv.number} · {inv.buyer.slice(0, 30)}</option
        >
      {/each}
    </select>
  </div>
  {#if app.fetched.length === 0}
    <p class="muted small">Nothing yet.</p>
  {:else}
    <table class="data">
      <thead>
        <tr>
          <th>Number</th>
          <th>Issued</th>
          <th>Buyer</th>
          <th class="num">Gross</th>
          <th>KSeF number</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each app.fetched as f}
          <tr class:selected={reference === `${f.env}/${f.ksef_number}`}>
            <td class="mono"><strong>{f.number}</strong></td>
            <td class="mono muted">{f.issue_date}</td>
            <td class="buyer">{f.buyer}</td>
            <td class="num">{f.gross}</td>
            <td class="mono muted small">
              <span class="badge" class:ok={f.env === "prod"}>{f.env}</span>
              {f.ksef_number}
            </td>
            <td>
              <button
                class="small"
                disabled={!ours}
                onclick={() => runCompare(f)}>Compare</button
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

{#if compareError || comparison}
  <div class="card" style="margin-bottom: 48px">
    <div class="card-head">
      <h2>{ours} vs {reference.split("/")[1]}</h2>
      <button
        class="small"
        onclick={() =>
          go(`/invoice/${app.invoices.find((i) => i.number === ours)?.slug}`)}
        >Open {ours}</button
      >
    </div>
    {#if compareError}<Notice text={compareError} />{/if}
    {#if comparison}<Comparison {comparison} />{/if}
  </div>
{/if}

<style>
  .buyer {
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  tr.selected td {
    background: var(--primary-soft);
  }
</style>
