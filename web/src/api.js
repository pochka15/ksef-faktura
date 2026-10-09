// Every request carries the token the server put into the page (or, under `vite`, the one in the URL).
const placeholder = "@@" + "TOKEN@@";
const token =
  window.__TOKEN__ && window.__TOKEN__ !== placeholder
    ? window.__TOKEN__
    : (new URLSearchParams(location.search).get("t") ?? "");

const withToken = (path) => `${path}?t=${token}`;

export const pdfUrl = (slug) => withToken(`/pdf/${slug}`);

async function post(path, body) {
  const response = await fetch(withToken(path), {
    method: "POST",
    body: JSON.stringify(body),
  });
  if (response.headers.get("content-type") === "application/pdf") {
    return response.blob();
  }
  const data = await response.json();
  if (!response.ok) throw new Error(data.error ?? `HTTP ${response.status}`);
  return data;
}

export async function getState() {
  const response = await fetch(withToken("/api/state"));
  const data = await response.json();
  if (!response.ok) throw new Error(data.error ?? `HTTP ${response.status}`);
  return data;
}

export const draft = (form) => post("/api/draft", { form });
export const draftPdf = (form) => post("/api/draft/pdf", { form });
export const save = (form) => post("/api/save", { form });
export const invoice = (number) => post("/api/invoice", { number });
export const openPdf = (number) => post("/api/open", { number });
export const plan = (number, env) => post("/api/plan", { number, env });
export const send = (number, env, confirm) =>
  post("/api/send", { number, env, confirm });
export const nbp = (currency, sale_date) =>
  post("/api/nbp", { currency, sale_date });
export const fetchFromKsef = (env, days) => post("/api/fetch", { env, days });
export const compare = (number, env, ksef_number) =>
  post("/api/compare", { number, env, ksef_number });
