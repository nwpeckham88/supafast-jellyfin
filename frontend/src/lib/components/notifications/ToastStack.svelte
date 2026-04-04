<script lang="ts">
  import { notificationsStore } from '$lib/stores/notifications';

  const notifications = notificationsStore;

  function levelClass(level: 'info' | 'success' | 'error'): string {
    if (level === 'success') {
      return 'border-emerald-300 bg-emerald-50 text-emerald-900';
    }

    if (level === 'error') {
      return 'border-red-300 bg-red-50 text-red-900';
    }

    return 'border-sky-300 bg-sky-50 text-sky-900';
  }
</script>

<section class="pointer-events-none fixed right-4 top-4 z-50 flex w-full max-w-sm flex-col gap-2">
  {#each $notifications as notification (notification.id)}
    <article
      class={`pointer-events-auto rounded-lg border px-3 py-2 text-sm shadow ${levelClass(notification.level)}`}
      role="status"
    >
      <div class="flex items-start justify-between gap-3">
        <p>{notification.message}</p>
        <button
          class="rounded border border-current px-2 py-0.5 text-xs"
          onclick={() => notificationsStore.dismiss(notification.id)}
          type="button"
        >
          Close
        </button>
      </div>
    </article>
  {/each}
</section>
