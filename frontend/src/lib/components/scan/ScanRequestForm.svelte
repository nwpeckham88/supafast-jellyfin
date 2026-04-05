<script lang="ts">
  import type { MediaType } from '$lib/models/normalize';

  interface Props {
    mediaType?: MediaType;
    busy?: boolean;
    onSubmit: () => void;
    onMediaTypeChange: (next: MediaType) => void;
  }

  let {
    mediaType = 'tv',
    busy = false,
    onSubmit,
    onMediaTypeChange
  }: Props = $props();

  function handleSubmit(event: SubmitEvent) {
    event.preventDefault();
    onSubmit();
  }
</script>

<form
  class="glass fade-up rounded-2xl p-5 md:p-6"
  onsubmit={handleSubmit}
>
  <h2 class="text-xl font-semibold">Start A Scan Preview</h2>
  <p class="mt-2 text-sm text-slate-700">
    Generate a cached session before execute/rollback. This view is read-only and safe.
  </p>

  <div class="mt-5 grid gap-4 md:grid-cols-2">
    <label class="flex flex-col gap-2 text-sm font-medium">
      Media Type
      <select
        class="rounded-lg border border-slate-300 bg-white px-3 py-2"
        value={mediaType}
        onchange={(event) =>
          onMediaTypeChange((event.currentTarget as HTMLSelectElement).value as MediaType)}
      >
        <option value="tv">TV</option>
        <option value="movie">Movie</option>
      </select>
    </label>
  </div>

  <button
    class="mt-5 rounded-lg bg-teal-700 px-4 py-2 text-sm font-semibold text-white transition hover:bg-teal-800 disabled:cursor-not-allowed disabled:bg-slate-400"
    disabled={busy}
    type="submit"
  >
    {busy ? 'Scanning...' : 'Create Scan'}
  </button>
</form>