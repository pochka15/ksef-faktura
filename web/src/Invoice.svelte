<script>
  import { onMount, untrack } from "svelte";
  import * as api from "./api.js";
  import Notice from "./Notice.svelte";
  import SentBadges from "./SentBadges.svelte";
  import Comparison from "./Comparison.svelte";

  let { app, slug, go, refresh } = $props();

  let data = $state(null);
  let error = $state("");
  let pdfKey = $state(0);
  let message = $state("");

  // Send flow: pick env → review the plan → type the confirmation → send. Only prod unless the app runs with
  // --test-envs; then nothing is preselected.
  const multi = untrack(() => app.envs.length > 1);
  let env = $state(multi ? null : untrack(() => app.envs[0]));
  let plan = $state(null);
  let planError = $state("");
  let confirm = $state("");
  let sending = $state(false);
  let sendResult = $state(null);
  let sendError = $state("");

  // Compare with an invoice downloaded from KSeF.
  const first = untrack(() => app.fetched[0]);
  let reference = $state(first ? `${first.env}/${first.ksef_number}` : "");
  let comparison = $state(null);
  let compareError = $state("");

  async function load() {
    try {
      data = await api.invoice(slug);
      error = "";
    } catch (e) {
      error = e.message;
    }
  }

  onMount(load);

  function pickEnv(e) {
    env = e;
    plan = null;
    planError = "";
    confirm = "";
    sendResult = null;
    sendError = "";
  }

  async function review() {
    planError = "";
    sendError = "";
    sendResult = null;
    try {
      plan = await api.plan(data.invoice.number, env);
      confirm = "";
    } catch (e) {
      plan = null;
      planError = e.message;
    }
  }

  async function doSend() {
    sending = true;
    sendError = "";
    try {
      sendResult = await api.send(data.invoice.number, env, confirm);
      plan = null;
      await load();
      await refresh();
      pdfKey++;
    } catch (e) {
      sendError = e.message;
    } finally {
      sending = false;
    }
  }

  async function openPdf() {
    try {
      const r = await api.openPdf(data.invoice.number);
      message = `Opened ${r.pdf}`;
    } catch (e) {
      message = e.message;
    }
  }

  async function runCompare() {
    compareError = "";
    comparison = null;
    const [refEnv, ksefNumber] = reference.split("/");
    try {
      comparison = await api.compare(data.invoice.number, refEnv, ksefNumber);
    } catch (e) {
      compareError = e.message;
    }
  }

  let hasToken = $derived(app.tokens[env]);
  let confirmed = $derived(plan && confirm.trim() === plan.confirm);
</script>

{#snippet linesTable(lines)}
  <table class="data">
    <thead>
      <tr>
        <th>Name</th>
        <th class="num">Qty</th>
        <th class="num">Net price</th>
        <th class="num">VAT</th>
        <th class="num">Net</th>
      </tr>
    </thead>
    <tbody>
      {#each lines as l}
        <tr>
          <td>{l.name}</td>
          <td class="num">{l.quantity} {l.unit}</td>
          <td class="num">{l.net_price}</td>
          <td class="num">{l.vat}</td>
          <td class="num">{l.net}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/snippet}

{#if error}
  <Notice text={error} style="margin-top: 32px" />
{:else if data}
  {@const inv = data.invoice}
  <section class="page-head">
    <div>
      <div class="eyebrow">
        {data.correction ? "Correction" : "Invoice"} · {inv.language === "pl"
          ? "Polski"
          : "English"}
      </div>
      <h1 class="mono-head">{inv.number}</h1>
      <p class="lead">
        {inv.buyer.name} · issued {inv.issue_date} · due {inv.due_date}
      </p>
      <div style="margin-top: 12px">
        <SentBadges sent={data.summary.sent} />
      </div>
    </div>
    <div class="row">
      <button onclick={openPdf}>Open PDF</button>
      <button
        disabled={data.locked}
        title={data.locked
          ? "Already in KSeF: a correction invoice is needed instead"
          : ""}
        onclick={() => go(`/edit/${slug}`)}>Edit</button
      >
      {#if !data.correction && Object.keys(data.sent).length}
        <button
          disabled={!!data.cannot_correct}
          title={data.cannot_correct ??
            "Issue a correcting invoice (faktura korygująca) for this one"}
          onclick={() => go(`/correct/${slug}`)}>Correct</button
        >
      {/if}
    </div>
  </section>
  {#if message}
    <div class="notice info" style="margin-bottom: 16px">{message}</div>
  {/if}
  {#if data.correction}
    {@const c = data.correction}
    <div class="notice info" style="margin-bottom: 16px">
      Corrects <a href={`#/invoice/${c.original_slug}`}>{c.original.number}</a>
      of {c.original.issue_date} · KSeF
      <span class="mono">{c.original.ksef_number}</span><br />
      Reason: <strong>{c.reason}</strong>
      {#if c.buyer_before}<br />Buyer's data changed (was: {c.buyer_before}){/if}
    </div>
  {/if}
  {#each data.corrections as k}
    <div class="notice warn" style="margin-bottom: 16px">
      Corrected by <a href={`#/invoice/${k.slug}`}>{k.number}</a>.
    </div>
  {/each}

  <div class="split">
    <div>
      <div class="card">
        {#if data.correction}
          <div class="eyebrow side">
            Before · {data.correction.before_gross} gross
          </div>
          {@render linesTable(data.correction.before)}
          <div class="eyebrow side">
            After · {data.correction.after_gross} gross
          </div>
          {#if data.lines.length}
            {@render linesTable(data.lines)}
          {:else}
            <p class="muted small">No lines: the whole invoice is cancelled.</p>
          {/if}
        {:else}
          {@render linesTable(data.lines)}
        {/if}
        <div class="total-row">
          <span class="muted small"
            >{data.correction ? "difference: " : ""}net {data.totals.net} + VAT
            {data.totals.vat}</span
          >
          <span class="num gross">
            {#if data.correction}<span class="muted small"
                >{data.totals.refund ? "to refund" : "to pay"}</span
              >{/if}
            {data.totals.due}
            {data.totals.currency}</span
          >
        </div>
      </div>

      <div class="card">
        <div class="card-head">
          <div>
            <h2>Checks</h2>
            <p class="muted small">Own rules and the official FA(3) schema.</p>
          </div>
          {#if data.errors.length === 0}
            <span class="badge ok"><span class="dot"></span>valid</span>
          {:else}
            <span class="badge warn"><span class="dot"></span>fix first</span>
          {/if}
        </div>
        {#each data.errors as e}<Notice text={e} />{/each}
        {#each data.warnings as w}<div class="notice warn">{w}</div>{/each}
        {#if !data.errors.length && !data.warnings.length}
          <p class="muted small" style="margin: 0">
            XML: <span class="mono">{data.dir}/invoice.xml</span>
          </p>
        {/if}
      </div>

      <div class="card send">
        <div class="card-head">
          <div>
            <h2>Send to KSeF</h2>
            <p class="muted small">
              Reviewed first; sent only after you type the confirmation.
            </p>
          </div>
          {#if multi}
            <div class="segmented" role="group" aria-label="Environment">
              {#each app.envs as e}
                <button class:active={env === e} onclick={() => pickEnv(e)}
                  >{e}</button
                >
              {/each}
            </div>
          {/if}
        </div>

        {#if env === "test"}
          <p class="muted small">
            The public test environment: seller, buyer and bank are swapped for
            fake data, the number gets a <span class="mono">-T…</span> suffix. Safe
            to repeat.
          </p>
        {:else if env === "prod"}
          <p class="small prod-note">
            Production legally issues the invoice. It cannot be undone, only
            corrected.
          </p>
        {/if}

        {#if !env}
          <p class="muted small">
            Pick where to send it: <strong>test</strong> to try (fake data),
            <strong>prod</strong> to issue it for real.
          </p>
        {:else if !hasToken}
          <div class="notice warn">
            No {env} token in the Keychain yet. Press <kbd>t</kbd> in the terminal
            (tutorials/02 for test), then come back.
          </div>
        {/if}

        {#if !env}
          <!-- nothing to review before an environment is picked -->
        {:else if data.sent[env] && env !== "test"}
          <div class="notice ok">
            Already in KSeF {env}:
            <span class="mono">{data.sent[env].ksef_number}</span>
          </div>
        {:else if !plan}
          <button
            style="margin-top: 6px"
            onclick={review}
            disabled={!!data.errors.length}
            >Review send to {multi ? env : "KSeF"}</button
          >
          {#if planError}<Notice
              text={planError}
              style="margin-top: 12px"
            />{/if}
        {:else}
          <div class="inset" style="margin-top: 6px">
            <pre class="plain">{plan.summary}</pre>
          </div>
          <label class="field" style="margin-top: 14px">
            <span>
              Type <span class="mono confirm-word">{plan.confirm}</span> to send
            </span>
            <div class="row">
              <input
                class="grow mono"
                bind:value={confirm}
                placeholder={plan.confirm}
                autocomplete="off"
                spellcheck="false"
              />
              <button
                class="primary"
                disabled={!confirmed || sending || !hasToken}
                onclick={doSend}
              >
                {#if sending}<span class="spinner"></span>{/if}
                Send to {multi ? env : "KSeF"}
              </button>
              <button
                class="ghost"
                onclick={() => (plan = null)}
                disabled={sending}>Cancel</button
              >
            </div>
          </label>
          {#if sending}
            <p class="muted small">
              Logging in, encrypting, waiting for KSeF; this can take a minute.
            </p>
          {/if}
        {/if}

        {#if sendError}
          <Notice text={sendError} style="margin-top: 12px" />
        {/if}
        {#if sendResult}
          <div class="notice ok" style="margin-top: 12px">
            Accepted by KSeF {env}:
            <strong class="mono">{sendResult.sent.ksef_number}</strong>
            {#each sendResult.log as l}<div class="small">{l}</div>{/each}
          </div>
        {/if}

        {#if Object.keys(data.sent).length}
          <div class="history">
            <div class="eyebrow">History</div>
            {#each Object.entries(data.sent) as [e, r]}
              <div class="history-row">
                <span
                  class="badge"
                  class:ok={e === "prod"}
                  class:primary={e !== "prod"}>{e}</span
                >
                <span class="mono grow">{r.ksef_number}</span>
                <span class="muted small"
                  >{r.sent_at.replace("T", " ").replace("Z", " UTC")}</span
                >
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <div class="card">
        <div class="card-head">
          <div>
            <h2>Compare with KSeF</h2>
            <p class="muted small">
              Is this XML shaped like an invoice you already sent (e.g. from
              another program)?
            </p>
          </div>
        </div>
        {#if app.fetched.length === 0}
          <p class="muted small" style="margin: 0">
            Nothing fetched yet. <a href="#/ksef"
              >Fetch your invoices from KSeF</a
            > first.
          </p>
        {:else}
          <div class="row">
            <select class="grow" bind:value={reference}>
              {#each app.fetched as f}
                <option value={`${f.env}/${f.ksef_number}`}
                  >{f.number} · {f.issue_date} · {f.env}</option
                >
              {/each}
            </select>
            <button onclick={runCompare}>Compare</button>
          </div>
          {#if compareError}<Notice
              text={compareError}
              style="margin-top: 12px"
            />{/if}
          {#if comparison}<Comparison {comparison} />{/if}
        {/if}
      </div>
    </div>

    <aside class="pdf-frame">
      <div class="caption">
        <span class="eyebrow">PDF · {data.pdf.split("/").pop()}</span>
      </div>
      {#key pdfKey}
        <iframe
          title="Invoice PDF"
          src={`${api.pdfUrl(slug)}#view=FitH&toolbar=0`}
        ></iframe>
      {/key}
    </aside>
  </div>
{:else}
  <p class="muted" style="padding-top: 40px">Loading…</p>
{/if}

<style>
  .mono-head {
    font-variant-numeric: tabular-nums;
  }

  .total-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 14px 12px 2px;
    border-top: 1px solid var(--border);
  }

  .side {
    padding: 10px 12px 2px;
  }

  .side:not(:first-child) {
    margin-top: 14px;
  }

  .gross {
    font-size: 1.15rem;
    font-weight: 700;
    color: var(--primary);
  }

  .send .notice {
    margin-top: 10px;
  }

  .prod-note {
    color: var(--err);
  }

  .confirm-word {
    background: var(--muted);
    padding: 1px 6px;
    border-radius: 5px;
  }

  .history {
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  .history-row {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-top: 10px;
  }
</style>
