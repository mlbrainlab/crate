<script lang="ts">
  import Header from "./lib/Header.svelte";
</script>

<div class="window">
  <Header />
  <main></main>
</div>

<style>
  .window {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  main {
    flex: 1;
    background: var(--bg);
  }
</style>
