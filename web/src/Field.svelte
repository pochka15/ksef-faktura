<script>
  // An input showing either the user's value (`set`, orange border) or the server's default.
  let {
    label,
    value,
    set,
    reset,
    oninput,
    type = "text",
    mono = false,
  } = $props();
</script>

<label class="field">
  <span class="label-row">
    {label}
    {#if set}
      <button class="reset" type="button" onclick={reset}>auto</button>
    {/if}
  </span>
  <input
    {type}
    {value}
    class:override={set}
    class:mono
    oninput={(e) => oninput(e.target.value)}
  />
</label>
