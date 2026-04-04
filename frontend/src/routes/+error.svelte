<script lang="ts">
  import { page } from '$app/state';
</script>

<svelte:head>
  <title>Something went wrong | supafast-jellyfin</title>
</svelte:head>

<section class="mx-auto max-w-2xl rounded-xl border border-red-300 bg-red-50 p-6">
  <h1 class="text-2xl font-bold text-red-900">Something went wrong</h1>
  <p class="mt-2 text-sm text-red-800">
    {page.error?.message ?? 'An unexpected error occurred while loading this page.'}
  </p>

  <div class="mt-4 flex gap-3">
    <a
      class="rounded-md bg-red-900 px-4 py-2 text-sm font-semibold text-white"
      href="/normalize"
    >
      Back to Normalize
    </a>
    <button
      class="rounded-md border border-red-900 px-4 py-2 text-sm font-semibold text-red-900"
      onclick={() => window.location.reload()}
      type="button"
    >
      Retry
    </button>
  </div>
</section>
