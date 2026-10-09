<script>
  import SentBadges from "./SentBadges.svelte";

  let { app, go } = $props();
</script>

<section class="page-head">
  <div>
    <div class="eyebrow">Your invoices</div>
    <h1>Invoices</h1>
    <p class="lead">
      Generated here as PDF and FA(3) XML. Nothing reaches KSeF until you open
      an invoice and send it.
    </p>
  </div>
  <button class="primary" onclick={() => go("/new")}>New invoice →</button>
</section>

<div class="card" style="padding: 8px 8px 4px">
  {#if app.invoices.length === 0}
    <div class="empty">
      <h2>No invoices yet</h2>
      <p class="muted">Write the first one. Next month, copy its lines.</p>
      <button class="primary" onclick={() => go("/new")}>New invoice</button>
    </div>
  {:else}
    <table class="data">
      <thead>
        <tr>
          <th style="padding-top: 12px">Number</th>
          <th style="padding-top: 12px">Issued</th>
          <th style="padding-top: 12px">Buyer</th>
          <th class="num" style="padding-top: 12px">Gross</th>
          <th style="padding-top: 12px">KSeF</th>
        </tr>
      </thead>
      <tbody>
        {#each app.invoices as inv (inv.slug)}
          <tr class="link" onclick={() => go(`/invoice/${inv.slug}`)}>
            <td class="mono"><strong>{inv.number}</strong></td>
            <td class="mono muted">{inv.issue_date}</td>
            <td class="buyer">{inv.buyer}</td>
            <td class="num">{inv.gross_text} {inv.currency}</td>
            <td><SentBadges sent={inv.sent} /></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .buyer {
    max-width: 380px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
