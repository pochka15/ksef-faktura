<script>
  import { onMount, untrack } from "svelte";
  import * as api from "./api.js";
  import Notice from "./Notice.svelte";
  import Field from "./Field.svelte";

  // `editing`: slug of a saved invoice to change; `correcting`: slug of an invoice in KSeF to issue a
  // correcting invoice for; otherwise a new one.
  let { app, go, refresh, editing = null, correcting = null } = $props();

  const blank = () => ({
    name: "",
    quantity: "",
    unit: "",
    net_price: "",
    vat: "23",
  });

  // Only what the user set; null fields get the server's defaults (next number, dates from the issue date).
  let form = $state({
    buyer: null,
    language: null,
    number: null,
    issue_date: null,
    sale_date: null,
    due_date: null,
    place: null,
    currency: null,
    // { rate, table, date } as typed or from NBP; only for a currency other than PLN.
    exchange_rate: null,
    lines: [blank()],
    replaces: null,
    // Corrections: the original's number and why. `buyer` null = the buyer as on the original.
    corrects: null,
    reason: null,
  });
  let ready = $state(untrack(() => !editing && !correcting));
  let preview = $state(null);
  let draftError = $state("");
  let pdfUrl = $state("");
  let pdfBusy = $state(false);
  let saving = $state(false);
  let saveError = $state("");

  let inv = $derived(preview?.invoice);
  let isCorrection = $derived(!!form.corrects);
  let buyerKey = $derived(
    form.buyer ?? (isCorrection ? "" : (app.default_buyer ?? "")),
  );
  let lang = $derived(form.language ?? inv?.language ?? app.language);
  let hasLines = $derived(form.lines.some((l) => l.name.trim()));
  let clash = $derived(
    preview?.exists && form.replaces !== inv?.number ? inv?.number : null,
  );
  let canSave = $derived(
    ready &&
      preview &&
      !preview.errors.length &&
      (hasLines || isCorrection) &&
      !clash &&
      !saving,
  );

  onMount(async () => {
    if (correcting) {
      try {
        const { invoice } = await api.invoice(correcting);
        // Number, dates, buyer, currency and rate default to the original's on the server.
        Object.assign(form, {
          language: invoice.language,
          lines: invoice.lines.map(toRow),
          corrects: invoice.number,
          reason: "",
        });
        ready = true;
      } catch (e) {
        draftError = e.message;
      }
      return;
    }
    if (!editing) return;
    try {
      const { invoice } = await api.invoice(editing);
      const id = invoice.buyer.nip || invoice.buyer.tax_id;
      const key = app.buyers.find((b) => b.nip === id)?.key;
      const fx = invoice.exchange_rate;
      const c = invoice.correction;
      Object.assign(form, {
        // A correction that kept the original's buyer keeps it.
        buyer: c && !c.buyer_before ? null : (key ?? null),
        language: invoice.language,
        number: invoice.number,
        issue_date: invoice.issue_date,
        sale_date: invoice.sale_date,
        due_date: invoice.due_date,
        place: invoice.place ?? null,
        currency: invoice.currency ?? null,
        exchange_rate: fx
          ? { rate: comma(fx.rate), table: fx.table, date: fx.date }
          : null,
        lines: invoice.lines.map(toRow),
        replaces: invoice.number,
        corrects: c?.original.number ?? null,
        reason: c?.reason ?? null,
      });
      ready = true;
    } catch (e) {
      draftError = e.message;
    }
  });

  const comma = (v) =>
    lang === "pl" ? String(v).replace(".", ",") : String(v);

  function toRow(l) {
    const text = (v) => String(v);
    return {
      name: l.name,
      quantity: comma(l.quantity),
      unit: l.unit,
      net_price: comma(l.net_price),
      vat: text(l.vat),
      pkwiu: l.pkwiu ?? null,
    };
  }

  let cur = $derived(form.currency ?? inv?.currency ?? "PLN");
  let rateBusy = $state(false);
  let rateError = $state("");

  // The NBP rate of the last business day before the sale date (art. 31a).
  async function getRate() {
    rateBusy = true;
    rateError = "";
    try {
      form.exchange_rate = await api.nbp(cur, form.sale_date ?? inv?.sale_date);
    } catch (e) {
      rateError = e.message;
    } finally {
      rateBusy = false;
    }
  }

  function setRate(field, value) {
    form.exchange_rate = {
      ...(form.exchange_rate ?? { rate: "", table: "", date: "" }),
      [field]: value,
    };
  }

  // Preview the form as it changes: numbers and checks quickly, the PDF a little later.
  let draftSeq = 0;
  let pdfSeq = 0;
  $effect(() => {
    const body = $state.snapshot(form);
    if (!ready) return;
    const d = ++draftSeq;
    const p = ++pdfSeq;
    const t1 = setTimeout(async () => {
      try {
        const result = await api.draft(body);
        if (d === draftSeq) {
          preview = result;
          draftError = "";
        }
      } catch (e) {
        if (d === draftSeq) draftError = e.message;
      }
    }, 150);
    const t2 = setTimeout(async () => {
      pdfBusy = true;
      try {
        const blob = await api.draftPdf(body);
        if (p === pdfSeq) {
          if (pdfUrl) URL.revokeObjectURL(pdfUrl);
          pdfUrl = URL.createObjectURL(blob);
        }
      } catch {
        // the draft call shows the error
      } finally {
        if (p === pdfSeq) pdfBusy = false;
      }
    }, 650);
    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
    };
  });

  function copyLast() {
    form.lines = preview.last.lines.map(toRow);
  }

  async function submit() {
    saving = true;
    saveError = "";
    try {
      const result = await api.save($state.snapshot(form));
      await refresh();
      go(`/invoice/${result.invoice.slug}`);
    } catch (e) {
      saveError = e.message;
    } finally {
      saving = false;
    }
  }
</script>

<section class="page-head">
  <div>
    <div class="eyebrow">
      {#if isCorrection}
        Correction of {form.corrects}
      {:else}
        {editing ? "Edit invoice" : "New invoice"}
      {/if}
    </div>
    <h1>{inv?.number ?? "…"}</h1>
    <p class="lead">
      {#if isCorrection}
        A correcting invoice (faktura korygująca): change the lines below to
        what they should have been. The PDF shows the original's lines before
        and yours after; the totals are the difference.
      {/if}
      Saving writes the PDF to <span class="mono">{app.output_dir}</span> and keeps
      the FA(3) XML. It does not send anything to KSeF.
    </p>
  </div>
</section>

<div class="split editor">
  <div>
    <div class="card">
      <div class="card-head">
        <h2>Details</h2>
        <div class="segmented" role="group" aria-label="Language">
          {#each ["pl", "en"] as code}
            <button
              class:active={lang === code}
              onclick={() => (form.language = code)}
              >{code === "pl" ? "Polski" : "English"}</button
            >
          {/each}
        </div>
      </div>
      <div class="grid-fields">
        {#if isCorrection}
          <label class="field wide">
            Reason for the correction
            <input
              value={form.reason ?? ""}
              oninput={(e) => (form.reason = e.target.value)}
              placeholder="Błędna liczba godzin"
            />
          </label>
        {/if}
        <label class="field wide">
          Buyer
          <select
            value={buyerKey}
            onchange={(e) => (form.buyer = e.target.value || null)}
          >
            {#if isCorrection}
              <option value="">As on {form.corrects}</option>
            {/if}
            {#each app.buyers as b}
              <option value={b.key}>{b.name} · {b.nip}</option>
            {/each}
          </select>
        </label>
        <Field
          label="Number"
          value={form.number ?? inv?.number ?? ""}
          set={form.number !== null}
          reset={() => (form.number = null)}
          oninput={(v) => (form.number = v || null)}
          mono
        />
        <Field
          label="Issue date"
          type="date"
          value={form.issue_date ?? inv?.issue_date ?? ""}
          set={form.issue_date !== null}
          reset={() => (form.issue_date = null)}
          oninput={(v) => (form.issue_date = v || null)}
        />
        <Field
          label="Sale date"
          type="date"
          value={form.sale_date ?? inv?.sale_date ?? ""}
          set={form.sale_date !== null}
          reset={() => (form.sale_date = null)}
          oninput={(v) => (form.sale_date = v || null)}
        />
        <Field
          label="Payment due"
          type="date"
          value={form.due_date ?? inv?.due_date ?? ""}
          set={form.due_date !== null}
          reset={() => (form.due_date = null)}
          oninput={(v) => (form.due_date = v || null)}
        />
        <Field
          label="Place of issue"
          value={form.place ?? inv?.place ?? ""}
          set={form.place !== null}
          reset={() => (form.place = null)}
          oninput={(v) => (form.place = v || null)}
        />
        {#if !isCorrection}
          <Field
            label="Currency"
            value={cur}
            set={form.currency !== null}
            reset={() => (form.currency = null)}
            oninput={(v) => (form.currency = v.toUpperCase() || null)}
            mono
          />
        {/if}
      </div>
      {#if isCorrection}
        <p class="muted small" style="margin: 12px 0 0">
          Currency {cur}{#if inv?.exchange_rate}, NBP rate {inv.exchange_rate
              .rate}
            ({inv.exchange_rate.table}){/if}: as on the original. Picking a
          buyer from config instead of the original's sends the old data as
          "before".
        </p>
      {/if}
      {#if cur !== "PLN" && !isCorrection}
        <div class="fx">
          <div class="label-row">
            <span class="small"
              ><strong>NBP rate</strong>
              <span class="muted"
                >· PLN per {cur}, last business day before the sale date</span
              ></span
            >
            <button class="small" onclick={getRate} disabled={rateBusy}>
              {#if rateBusy}<span class="spinner"></span>{/if}
              Get NBP rate
            </button>
          </div>
          <div class="fx-fields">
            <label class="field"
              >Rate<input
                class="mono"
                placeholder="4,3128"
                value={form.exchange_rate?.rate ?? ""}
                oninput={(e) => setRate("rate", e.target.value)}
              /></label
            >
            <label class="field"
              >Table<input
                class="mono"
                placeholder="147/A/NBP/2026"
                value={form.exchange_rate?.table ?? ""}
                oninput={(e) => setRate("table", e.target.value)}
              /></label
            >
            <label class="field"
              >Date<input
                type="date"
                value={form.exchange_rate?.date ?? ""}
                oninput={(e) => setRate("date", e.target.value)}
              /></label
            >
          </div>
          {#if rateError}<Notice
              text={rateError}
              style="margin-top: 10px"
            />{/if}
        </div>
      {/if}
      <p class="muted small" style="margin: 12px 0 0">
        Fields follow the issue date until you change them (orange border);
        <em>auto</em> brings the default back.
      </p>
    </div>

    <div class="card">
      <div class="card-head">
        <h2>{isCorrection ? "Lines after the correction" : "Lines"}</h2>
        {#if preview?.last && !isCorrection}
          <button class="small" onclick={copyLast}
            >Copy lines from {preview.last.number}</button
          >
        {/if}
      </div>
      <div class="lines">
        <div class="line head">
          <span>Name</span><span>Qty</span><span>Unit</span><span
            >Net price</span
          ><span>VAT</span><span class="right">Net</span><span></span>
        </div>
        {#each form.lines as line, i}
          <div class="line">
            <input
              bind:value={line.name}
              placeholder="Usługi IT"
              aria-label="Name"
            />
            <input
              bind:value={line.quantity}
              placeholder="168"
              class="mono"
              aria-label="Quantity"
            />
            <input
              bind:value={line.unit}
              placeholder="godz."
              aria-label="Unit"
            />
            <input
              bind:value={line.net_price}
              placeholder="100,00"
              class="mono"
              aria-label="Net price"
            />
            <select bind:value={line.vat} aria-label="VAT rate">
              <option value="23">23%</option>
              <option value="8">8%</option>
              <option value="5">5%</option>
              <option value="np I">np I</option>
              <option value="np II">np II</option>
            </select>
            <span class="num net">{preview?.rows?.[i] ?? ""}</span>
            <button
              class="icon"
              title="Remove line"
              aria-label="Remove line"
              onclick={() => form.lines.splice(i, 1)}>✕</button
            >
          </div>
        {/each}
      </div>
      <button
        class="small"
        style="margin-top: 10px"
        onclick={() => form.lines.push(blank())}>+ Add line</button
      >
    </div>

    <div class="card totals">
      <div class="sums">
        <div>
          <div class="eyebrow">{isCorrection ? "Net difference" : "Net"}</div>
          <div class="num big">{preview?.totals.net ?? "–"}</div>
        </div>
        <div>
          <div class="eyebrow">{isCorrection ? "VAT difference" : "VAT"}</div>
          <div class="num big">{preview?.totals.vat ?? "–"}</div>
        </div>
        <div>
          <div class="eyebrow">
            {preview?.totals.refund ? "To refund" : "To pay"}
          </div>
          <div class="num big strong">
            {preview?.totals.due ?? "–"} <span class="muted">{cur}</span>
          </div>
        </div>
      </div>
      {#if preview && (hasLines || isCorrection)}
        <p class="muted small" style="margin: 12px 0 0">
          {preview.totals.in_words}
          {#if preview.totals.vat_pln !== null}
            <br />VAT in PLN at the NBP rate: {preview.totals.vat_pln} PLN
          {/if}
        </p>
      {/if}
    </div>

    <div style="margin-top: 16px">
      {#if draftError}
        <Notice text={draftError} />
      {/if}
      {#if clash}
        <div class="notice err">
          Invoice {clash} already exists. Change the number, or open it and use Edit.
        </div>
      {/if}
      {#if preview?.errors.length && hasLines}
        <div class="notice err">
          <ul>
            {#each preview.errors as e}<li>{e}</li>{/each}
          </ul>
        </div>
      {/if}
      {#if preview?.warnings.length}
        <div class="notice warn">
          <ul>
            {#each preview.warnings as w}<li>{w}</li>{/each}
          </ul>
        </div>
      {/if}
      {#if saveError}
        <Notice text={saveError} />
      {/if}
    </div>

    <div class="row" style="margin-top: 16px">
      <button class="primary" disabled={!canSave} onclick={submit}>
        {#if saving}<span class="spinner"></span>{/if}
        Save and write PDF
      </button>
      <button
        class="ghost"
        onclick={() =>
          go(editing || correcting ? `/invoice/${editing ?? correcting}` : "/")}
        >Cancel</button
      >
      {#if !hasLines && !isCorrection}
        <span class="muted small">Add a line to save.</span>
      {/if}
    </div>
  </div>

  <aside class="pdf-frame">
    <div class="caption">
      <span class="eyebrow">Preview · {preview?.pdf_name ?? ""}</span>
      {#if pdfBusy}<span class="spinner muted"></span>{/if}
    </div>
    {#if pdfUrl}
      <iframe title="PDF preview" src={`${pdfUrl}#view=FitH&toolbar=0`}
      ></iframe>
    {:else}
      <div class="placeholder card muted">Rendering…</div>
    {/if}
  </aside>
</div>

<style>
  .editor {
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 0.8fr);
  }

  @media (max-width: 980px) {
    .editor {
      grid-template-columns: 1fr;
    }
  }

  .fx {
    margin-top: 16px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  .fx-fields {
    display: grid;
    grid-template-columns: 1fr 1.4fr 1.2fr;
    gap: 12px;
    margin-top: 10px;
  }

  .lines {
    display: grid;
    gap: 8px;
  }

  .line {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 58px 62px 86px 68px 84px 28px;
    gap: 6px;
    align-items: center;
  }

  .line.head {
    font-size: 0.72rem;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted-fg);
  }

  .right {
    text-align: right;
  }

  .net {
    color: var(--muted-fg);
  }

  .sums {
    display: grid;
    grid-template-columns: 1fr 1fr 1.3fr;
    gap: 16px;
  }

  .sums .num {
    text-align: left;
  }

  .big {
    font-size: 1.25rem;
    margin-top: 4px;
  }

  .strong {
    font-weight: 700;
    color: var(--primary);
  }

  .placeholder {
    height: calc(100vh - 140px);
    min-height: 520px;
    display: grid;
    place-items: center;
  }

  @media (max-width: 760px) {
    .line {
      grid-template-columns: 1fr 1fr 1fr;
    }
    .line > :first-child {
      grid-column: 1 / -1;
    }
    .line.head {
      display: none;
    }
  }
</style>
