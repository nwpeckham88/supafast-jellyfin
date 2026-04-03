<script lang="ts">
  import type { ScanResult } from '$lib/models/normalize';

  interface Props {
    result?: ScanResult | null;
    loading?: boolean;
  }

  let { result = null, loading = false }: Props = $props();
</script>

<section class="glass fade-up rounded-2xl p-5 md:p-6">
  <div class="flex items-center justify-between">
    <h2 class="text-xl font-semibold">Session Summary</h2>
    {#if result}
      <code class="rounded bg-slate-900 px-2 py-1 text-xs text-slate-100">{result.scan_id}</code>
    {/if}
  </div>

  {#if loading}
    <p class="mt-3 text-sm text-slate-700">Loading scan session...</p>
  {:else if !result}
    <p class="mt-3 text-sm text-slate-700">No active scan. Create one to preview grouped shows.</p>
  {:else}
    <dl class="mt-4 grid gap-3 text-sm md:grid-cols-4">
      <div class="rounded-lg border border-slate-200 bg-white p-3">
        <dt class="text-slate-600">Shows</dt>
        <dd class="text-lg font-semibold">{result.summary.total_shows}</dd>
      </div>
      <div class="rounded-lg border border-slate-200 bg-white p-3">
        <dt class="text-slate-600">Episodes</dt>
        <dd class="text-lg font-semibold">{result.summary.total_episodes}</dd>
      </div>
      <div class="rounded-lg border border-slate-200 bg-white p-3">
        <dt class="text-slate-600">Merges</dt>
        <dd class="text-lg font-semibold text-amber-700">{result.summary.merges}</dd>
      </div>
      <div class="rounded-lg border border-slate-200 bg-white p-3">
        <dt class="text-slate-600">Conflicts</dt>
        <dd class="text-lg font-semibold text-orange-700">{result.summary.conflicts}</dd>
      </div>
    </dl>
  {/if}
</section>