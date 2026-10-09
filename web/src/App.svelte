<script>
  import { onMount } from "svelte";
  import { getState } from "./api.js";
  import Invoices from "./Invoices.svelte";
  import Editor from "./Editor.svelte";
  import Invoice from "./Invoice.svelte";
  import Ksef from "./Ksef.svelte";

  // Routes live in the hash: #/, #/new, #/edit/<slug>, #/correct/<slug>, #/invoice/<slug>, #/ksef
  let route = $state(parse(location.hash));
  let appState = $state(null);
  let loadError = $state("");

  function parse(hash) {
    const [page = "", slug = ""] = hash.replace(/^#\/?/, "").split("/");
    return { page, slug: decodeURIComponent(slug) };
  }

  function go(path) {
    location.hash = path;
  }

  async function refresh() {
    try {
      appState = await getState();
      loadError = "";
    } catch (e) {
      loadError = e.message;
    }
  }

  onMount(() => {
    refresh();
    const onHash = () => {
      route = parse(location.hash);
      window.scrollTo(0, 0);
    };
    // Config or token changes happen in the terminal; pick them up when coming back to the tab.
    const onFocus = () => refresh();
    window.addEventListener("hashchange", onHash);
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("hashchange", onHash);
      window.removeEventListener("focus", onFocus);
    };
  });

  const nav = [
    { page: "", label: "Invoices" },
    { page: "new", label: "New invoice" },
    { page: "ksef", label: "KSeF" },
  ];
  let current = $derived(
    ["invoice", "edit", "correct"].includes(route.page) ? "" : route.page,
  );
</script>

<header class="topbar">
  <div class="container bar">
    <a class="brand" href="#/">
      <span class="logo" aria-hidden="true"><span></span></span>
      <span>ksef</span>
    </a>
    <nav>
      {#each nav as item}
        <a href={`#/${item.page}`} class:active={current === item.page}
          >{item.label}</a
        >
      {/each}
    </nav>
    <div class="who">
      {#if appState}
        <span class="muted small seller">{appState.seller.name}</span>
        {#if appState.envs.length > 1}
          <span
            class="tokens"
            title="KSeF tokens in the Keychain (t in the terminal adds one)"
          >
            {#each appState.envs as env}
              <span class="badge" class:ok={appState.tokens[env]}>
                {#if appState.tokens[env]}<span class="dot"></span>{/if}{env}
              </span>
            {/each}
          </span>
        {/if}
      {/if}
    </div>
  </div>
</header>

<main class="container">
  {#if loadError}
    <div class="notice err" style="margin-top: 32px">
      {loadError}
      {#if loadError.includes("config.json")}
        <br />Fix it with <kbd>c</kbd> in the terminal, then come back to this tab.
      {/if}
    </div>
  {:else if !appState}
    <p class="muted" style="padding-top: 40px">Loading…</p>
  {:else}
    {#if appState.config_problems.length}
      <div class="notice warn" style="margin-top: 24px">
        config.json: {appState.config_problems.join("; ")}. Press <kbd>c</kbd> in
        the terminal to edit it.
      </div>
    {/if}
    {#if route.page === ""}
      <Invoices app={appState} {go} />
    {:else if route.page === "new"}
      {#key "new"}
        <Editor app={appState} {go} {refresh} />
      {/key}
    {:else if route.page === "edit"}
      {#key route.slug}
        <Editor app={appState} {go} {refresh} editing={route.slug} />
      {/key}
    {:else if route.page === "correct"}
      {#key route.slug}
        <Editor app={appState} {go} {refresh} correcting={route.slug} />
      {/key}
    {:else if route.page === "invoice"}
      {#key route.slug}
        <Invoice app={appState} slug={route.slug} {go} {refresh} />
      {/key}
    {:else if route.page === "ksef"}
      <Ksef app={appState} {refresh} {go} />
    {:else}
      <p style="padding-top: 40px">Nothing here. <a href="#/">Invoices</a></p>
    {/if}
  {/if}
</main>

<style>
  .topbar {
    position: sticky;
    top: 0;
    z-index: 10;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklch, var(--bg) 82%, transparent);
    backdrop-filter: blur(12px);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 32px;
    height: 60px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 700;
    font-size: 1.15rem;
    letter-spacing: -0.02em;
    text-decoration: none;
  }

  .logo {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: var(--fg);
    display: grid;
    place-items: center;
  }

  .logo span {
    width: 4px;
    height: 13px;
    border-radius: 2px;
    background: var(--primary);
  }

  nav {
    display: flex;
    gap: 22px;
  }

  nav a {
    font-size: 0.9rem;
    color: var(--muted-fg);
    text-decoration: none;
    transition: color 0.15s;
  }

  nav a:hover {
    color: var(--primary);
  }

  nav a.active {
    color: var(--primary);
    font-weight: 600;
  }

  .tokens {
    display: flex;
    gap: 4px;
  }

  .who {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 12px;
  }

  @media (max-width: 640px) {
    .bar {
      gap: 16px;
    }
    .seller {
      display: none;
    }
    nav {
      gap: 14px;
    }
  }
</style>
