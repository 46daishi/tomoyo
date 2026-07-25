<script>
  import ActionButton from "$lib/components/ActionButton.svelte";
  import SelectInput from "$lib/components/SelectInput.svelte";
  import { ICONS } from "$lib/icons.js";
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { getDb, pickCoverImage, coverSrc } from '$lib/db';
  import { confirm } from '@tauri-apps/plugin-dialog';
  import MediaFormModal from '$lib/components/MediaFormModal.svelte';
  import { STATUS_COLORS, FILTER_OPTIONS, STATUS_OPTIONS } from '$lib/constants.js';

  let mediaList = $state([]);
  let statusFilter = $state('all');

  
  const STATUS_ORDER = Object.fromEntries(STATUS_OPTIONS.map((o, i) => [o.value, i]));

  async function loadMedia() {
      const db = await getDb();
      mediaList = await db.select('SELECT * FROM media ORDER BY updated_at DESC');
  }

  let filtered = $derived(
      (statusFilter === 'all'
          ? mediaList
          : mediaList.filter((m) => m.status === statusFilter)
      ).slice().sort((a, b) => {
          const rankDiff = STATUS_ORDER[a.status] - STATUS_ORDER[b.status];
          if (rankDiff !== 0) return rankDiff;
          return b.updated_at - a.updated_at; // most recently updated first, within same status
      })
  );

  let showModal = $state(false);
  let editingMedia = $state(null); // null = add mode, a media row = edit mode
  
  function openEditModal(media) {
      editingMedia = media;
      showModal = true;
  }

  async function handleDelete(media) {
      const yes = await confirm(
          `Delete "${media.title}"? This cannot be undone.`,
          { title: 'Delete media', kind: 'warning' }
      );
      if (!yes) return;
  
      const db = await getDb();
      await db.execute('DELETE FROM media WHERE id = $1', [media.id]);
      await loadMedia();
  }
  
  function handleCardKeydown(e, id) {
      if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          openMedia(id);
      }
  }

  function handleStatusFilterChange(e) {
          statusFilter = e.target.value;
    }

  function openMedia(id) {
      goto(`/media/${id}`);
  }

  onMount(loadMedia);
</script>

<main class="page home">
    <div class="toolbar">
        <SelectInput
                options={FILTER_OPTIONS}
                value={statusFilter}
                on:change={handleStatusFilterChange}
        />
        <ActionButton
            icon={ICONS.plus}
            variant="primary"
            size="tiny"
            onAction={() => showModal = true}
        />
    </div>
    
    <div class="grid">
        {#each filtered as media (media.id)}
            <div
                class="card"
                style="--accent: {media.color}"
                onclick={() => openMedia(media.id)}
                onkeydown={(e) => handleCardKeydown(e, media.id)}
                role="button"
                tabindex="0"
            >
                <div class="cover">
                    {#if media.cover_path}
                        <img src={coverSrc(media.cover_path)} alt={media.title} />
                    {:else}
                        <div class="cover-placeholder"></div>
                    {/if}
    
                    <div class="cover-actions" onclick={(e) => e.stopPropagation()}>
                        <ActionButton
                            icon={ICONS.edit}
                            variant="primary"
                            size="mini"
                            onAction={() => openEditModal(media)}
                        />
                        <ActionButton
                            icon={ICONS.trash}
                            variant="danger"
                            size="mini"
                            onAction={() => handleDelete(media)}
                        />
                    </div>
                </div>
                <div class="title">{media.title}</div>
                <div class="status">
                    <span class="status-dot" style="--dot-color: {STATUS_COLORS[media.status]}"></span>
                    {media.status}
                </div>
            </div>
        {/each}
    </div>

        {#if mediaList.length > 0 && filtered.length === 0}
            <p class="empty-notice">There are no media entries that match this query.</p>
        {/if}
    
        <MediaFormModal bind:show={showModal} media={editingMedia} onSaved={loadMedia} />
</main>

<div class="logo">
    <a href="https://x.com/46daishi" target="_blank" rel="noopener noreferrer"><img src="tomoyo_full.png" alt="tomoyo" /></a>
</div>
<nav class="side-nav" aria-label="App navigation">
  <div class="nav-actions">
      <ActionButton
          icon={ICONS.settings}
          variant="secondary"
          size="small"
          onAction={() => goto(`/settings`)}
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
          icon={ICONS.question}
          variant="secondary"
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

    .toolbar {
        display: flex;
        gap: 10px;
    }

    h1 {
        font-size: 1.7em;
        display: flex;
        align-items: center;
        gap: 0.4rem;
        margin-top: 15vh;
        margin-bottom: 0;
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
        gap: 1.5rem 2.5rem;
        width: 100%;
        max-width: 1100px; /* optional: stop cards from getting absurdly wide on a huge monitor */
        margin-top: 10px;
    }

    .status {
        display: flex;
        align-items: center;
        gap: 0.3rem;
        margin-left: 0.2rem;
        font-size: 0.75rem;
        color: var(--subtext0, #a6adc8);
        text-transform: capitalize;
    }
    
    .status-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: var(--dot-color, var(--subtext0, #a6adc8));
        flex-shrink: 0;
    }
    
    /* Very narrow: single column, tighter padding */
    @media (max-width: 550px) {
        .grid {
            grid-template-columns: repeat(2, minmax(120px, 1fr));
        }
        .content {
            padding-inline: 1rem;
        }
    }

    
    .card {
        display: flex;
        flex-direction: column;
        background: none;
        border: none;
        padding: 0;
        cursor: pointer;
        text-align: left;
        color: inherit;
    }
    
    .cover {
        position: relative;
        aspect-ratio: 2 / 3;
        width: 100%;
        border-radius: 8px;
        overflow: hidden;
        background: var(--surface1, #313244);
        border: 2px solid transparent;
        transition: border-color 0.15s ease, transform 0.15s ease;
        /* filter removed from here */
    }
    
    .cover img,
    .cover-placeholder {
        transition: filter 0.3s ease;
    }
    
    .card:hover .cover {
        border-color: var(--accent);
        transform: translateY(-2px);
    }
    
    .card:hover .cover img,
    .card:hover .cover-placeholder {
        filter: brightness(60%);
    }

    .cover-actions {
        position: absolute;
        top: 8px;
        right: 8px;
        display: flex;
        gap: 0.4rem;
        opacity: 0;
        transition: opacity 0.4s ease;
        z-index: 2;
    }

    .card:hover .cover-actions,
    .card:focus-visible .cover-actions {
        opacity: 1;
    }
    
    .cover img {
        width: 100%;
        height: 100%;
        object-fit: cover;
        display: block;
    }
    
    .cover-placeholder {
        width: 100%;
        height: 100%;
        background: linear-gradient(135deg, var(--surface1, #313244), var(--surface0, #1e1e2e));
    }
    
    .title {
        font-family: "Noto Sans JP", Inter, sans-serif;
        margin-top: 0.5rem;
        font-weight: 600;
        font-size: 0.9rem;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }
    
    .status {
        font-size: 0.75rem;
        color: var(--subtext0, #a6adc8);
        text-transform: capitalize;
    }

    .empty-notice {
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 1.3rem;
        text-align: center;
        margin-top: 2rem;
        opacity: 0.8;
    }
</style>
