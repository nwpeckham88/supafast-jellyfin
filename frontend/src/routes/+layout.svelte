<script lang="ts">
  import '../app.css';

  let { children } = $props();
</script>

<main class="mx-auto w-full max-w-6xl px-4 py-6 md:px-8 md:py-10">
  {@render children()}
</main>