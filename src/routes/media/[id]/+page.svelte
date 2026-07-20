<script>
    import { page } from '$app/state';
    import { getDb } from '$lib/db';

    let mediaId = $derived(Number(page.params.id));
    let media = $state(null);

    async function loadMedia(id) {
        const db = await getDb();
        const rows = await db.select('SELECT * FROM media WHERE id = $1', [id]);
        media = rows[0] ?? null;
    }

    $effect(() => {
        loadMedia(mediaId);
    });
</script>

{#if media}
    <h1>{media.title}</h1>
    <p>Status: {media.status}</p>
{:else}
    <p>Loading…</p>
{/if}