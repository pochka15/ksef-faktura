// Invoice PDF. All text arrives pre-formatted from src/pdf.rs as JSON in sys.inputs.data, so this file is
// only layout: change boxes, sizes and order here without touching Rust.

#let d = json(bytes(sys.inputs.data))
#let L = d.labels
#let ink = luma(105)
#let shade = luma(226)
#let gap = 4pt
#let c = d.correction
#let title = if c == none { L.title } else { L.title_correction }

#set document(title: title + " " + d.number)
#set page(
  paper: "a4",
  margin: (x: 1.3cm, top: 1.3cm, bottom: 1.6cm),
  footer: context {
    set text(size: 7pt, fill: luma(100))
    h(1fr)
    [#L.page #counter(page).display() #L.of #counter(page).final().first()]
  },
)
#set text(font: "Liberation Sans", size: 8.5pt, lang: d.lang)
#set par(leading: 0.5em)

#let card(body, fill: none, height: auto, inset: 7pt) = block(
  width: 100%, height: height, inset: inset, radius: 5pt, stroke: 0.6pt + ink, fill: fill, body,
)

// Cards side by side with the height of the tallest one.
#let cards(..bodies) = layout(size => {
  let items = bodies.pos()
  let width = (size.width - (items.len() - 1) * gap) / items.len()
  let height = calc.max(..items.map(b => measure(block(width: width, inset: 7pt, b)).height))
  grid(columns: (1fr,) * items.len(), column-gutter: gap, ..items.map(b => card(height: height, b)))
})

#let party(label, p) = grid(
  columns: (auto, 1fr),
  column-gutter: 4pt,
  row-gutter: 3.5pt,
  [#label:], [#p.name],
  [#L.address:], [#p.address],
  [#L.at(p.id_label):], [#p.nip],
  ..if p.phone != none { ([#L.phone:], [#p.phone]) } else { () },
  ..if p.email != none { ([#L.email:], [#p.email]) } else { () },
)

#let centered(top, bottom: none) = align(center + horizon, {
  text(size: 10pt, weight: "bold", top)
  if bottom != none { linebreak(); bottom }
})

// --- header: title | number and dates
#let row-h = 30pt
#grid(
  columns: (1fr, 1fr),
  column-gutter: gap,
  card(height: 3 * row-h + 2 * gap, fill: shade, align(center + horizon, text(size: 16pt, weight: "bold", title))),
  stack(
    spacing: gap,
    card(height: row-h, centered(L.number + " " + d.number)),
    card(height: row-h, centered(d.issue_date, bottom: L.issue_date)),
    card(height: row-h, centered(d.sale_date, bottom: L.sale_date)),
  ),
)

#if d.ksef_number != none or d.place != none {
  v(2pt)
  align(right, {
    if d.place != none [#L.place: #d.place]
    if d.ksef_number != none and d.place != none { h(10pt) }
    if d.ksef_number != none [#L.ksef_number: *#d.ksef_number*]
  })
}

// --- correction: which invoice, why, and the buyer's data before
#if c != none {
  v(gap)
  card[
    #L.corrects *#c.number* #L.of_date #c.issue_date, #L.ksef_number: *#c.ksef_number* \
    #L.reason: *#c.reason*
    #if c.buyer_before != none {
      let b = c.buyer_before
      [\ #L.buyer_before: #b.name, #b.address, #L.at(b.id_label): #b.nip]
    }
  ]
}

#v(gap)
#cards(party(L.seller, d.seller), party(L.buyer, d.buyer))

// A refund has no payment terms.
#if not d.refund {
  v(gap)
  card[
    #L.payment_method: *#L.transfer* #h(4pt) #L.due: *#d.due* \
    #if d.bank != none [#L.bank: *#d.bank* #h(4pt)] #L.account: *#d.account*
    #if d.swift != none [#h(4pt) #L.swift: *#d.swift*]
    #if d.account_currency != none [| *#d.account_currency*]
  ]
}

// --- lines
#let th(body) = table.cell(fill: shade, align(center + horizon, body))
#let money-cols = (52pt, 60pt, 38pt, 52pt, 62pt)
#let lines-table(lines) = table(
  columns: (20pt, 1fr, 38pt, 30pt) + money-cols,
  stroke: 0.6pt + ink,
  inset: 5pt,
  align: (x, _) => if x == 1 { left } else if x in (0, 3, 6) { center } else { right },
  table.header(
    th(L.lp), th(L.name), th(L.qty), th(L.unit), th(L.net_price),
    th(L.net_value), th(L.vat_rate), th(L.vat_amount), th(L.gross),
  ),
  ..lines
    .map(l => (l.lp, l.name, l.qty, l.unit, l.net_price, l.net_value, l.vat_rate, l.vat_amount, l.gross))
    .flatten(),
)

// Totals, aligned under the money columns.
#let totals-table(label, total, rates) = align(right, table(
  columns: money-cols,
  stroke: 0.6pt + ink,
  inset: 5pt,
  align: (x, _) => if x in (0, 2) { center } else { right },
  table.cell(fill: shade, label), total.net, [X], total.vat, total.gross,
  ..rates
    .enumerate()
    .map(((i, r)) => (table.cell(fill: shade, if i == 0 { L.including } else { [] }), r.net, r.rate, r.vat, r.gross))
    .flatten(),
))

#if c == none {
  v(gap)
  lines-table(d.lines)
  v(2pt)
  totals-table(L.total, d.total, d.rates)
} else {
  for (heading, side) in ((L.before, c.before), (L.after, c.after)) {
    v(gap + 4pt)
    text(size: 9.5pt, weight: "bold", heading)
    v(0pt)
    if side.lines.len() > 0 { lines-table(side.lines); v(2pt) }
    totals-table(L.total, side.total, side.rates)
  }
  v(gap + 4pt)
  totals-table(L.difference, d.total, d.rates)
}

// --- amount due (or refunded, for a correction lowering the amount)
#v(16pt)
#box(stroke: 0.6pt + ink, radius: 5pt, inset: 8pt)[#if d.refund { L.to_refund } else { L.to_pay }: #text(size: 11pt, weight: "bold", d.to_pay)]
#if c == none {
  v(6pt)
  pad(left: 8pt)[#L.paid: #d.paid \ #L.remaining: #d.remaining]
}
#v(10pt)
#card[#L.in_words: #d.in_words]

// --- foreign currency: the NBP rate and the VAT in PLN
#if d.fx != none {
  v(gap)
  card[#d.fx.at(0) \ *#d.fx.at(1)*]
}

// --- issuer signature, right half like the seller/buyer split above
#v(gap)
#grid(
  columns: (1fr, 1fr),
  column-gutter: gap,
  [],
  card(height: 76pt, align(bottom + center, {
    if d.issuer_name != none { text(size: 10pt, weight: "bold", d.issuer_name); v(10pt) }
    text(size: 7pt, L.sig_issuer)
  })),
)
