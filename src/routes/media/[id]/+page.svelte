<script>
    import { page } from '$app/state';
    import { getDb } from '$lib/db';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import { ICONS } from '$lib/icons';
    import { coverSrc } from '$lib/db';

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

<main class="page home">
    {#if media}
        <h1>{media.title}</h1>
        <p>Status: {media.status}</p>
        {#if media.cover_path}
            <div class="logo">
                <img src={coverSrc(media.cover_path)} alt={media.title} />
            </div>
        {:else}
            <div class="cover-placeholder"></div>
        {/if}
        
    {:else}
        <p>Loading…</p>
    {/if}
</main>

<nav class="side-nav" aria-label="App navigation">
  <div class="nav-actions">
      <ActionButton
          icon={ICONS.back}
          variant="primary"
          size="small"
          onAction={() => history.back()}
      />
      <ActionButton
          icon={ICONS.edit}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.stats}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.book}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.play}
          variant="primary"
          size="small"
      />
  </div>
</nav>
<style>
    .page.home {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 1rem;
        box-sizing: border-box;
        width: 100%;
        padding-top: 2rem;
        padding-right: calc(1rem + 48px + 1.5rem);
        padding-left: calc(1rem + 48px + 1.5rem);
        padding-bottom: 2rem; /* breathing room at the bottom of the scroll */
        max-height: 100vh;
        overflow-y: auto;
    }
    
    .logo img {
        width: 55px;
        height: 90px;
        object-fit: cover;
    }

    .side-nav {
        top: 9.4rem;
    }
</style>