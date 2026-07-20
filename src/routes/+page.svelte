<script>
  import ActionButton from "$lib/components/ActionButton.svelte";
  import SelectInput from "$lib/components/SelectInput.svelte";
  import { ICONS } from "$lib/icons.js";
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { getDb, pickCoverImage, coverSrc } from '$lib/db';

  let mediaList = $state([]);
  let statusFilter = $state('all');
  let showAddModal = $state(false);

  let newTitle = $state('');
  let newTag = $state('');
  let newColor = $state('#89b4fa');
  let newCoverPath = $state(null);

  const statusOptions = [
          { value: 'all', label: 'All' },
          { value: 'active', label: 'Active' },
          { value: 'paused', label: 'Paused' },
          { value: 'completed', label: 'Completed' },
          { value: 'dropped', label: 'Dropped' }
  ];

  async function loadMedia() {
      const db = await getDb();
      mediaList = await db.select('SELECT * FROM media ORDER BY updated_at DESC');
  }

  let filtered = $derived(
      statusFilter === 'all'
          ? mediaList
          : mediaList.filter((m) => m.status === statusFilter)
  );

  async function handlePickCover() {
          const path = await pickCoverImage();
          if (path) newCoverPath = path;
  }

  async function addMedia() {
          if (!newTitle.trim()) return;
          const db = await getDb();
          await db.execute(
              'INSERT INTO media (title, tag, color, cover_path) VALUES ($1, $2, $3, $4)',
              [newTitle, newTag || null, newColor, newCoverPath]
          );
          newTitle = '';
          newTag = '';
          newCoverPath = null;
          showAddModal = false;
          await loadMedia();
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
                options={statusOptions}
                value={statusFilter}
                on:change={handleStatusFilterChange}
        />
        <button onclick={() => (showAddModal = true)}>+ Add media</button>
    </div>
    
        <div class="grid">
            {#each filtered as media (media.id)}
                <button class="card" style="--accent: {media.color}" onclick={() => openMedia(media.id)}>
                    <div class="cover">
                        {#if media.cover_path}
                            <img src={coverSrc(media.cover_path)} alt={media.title} />
                        {:else}
                            <div class="cover-placeholder"></div>
                        {/if}
                    </div>
                    <div class="title">{media.title}</div>
                    <div class="status">{media.status}</div>
                </button>
            {/each}
        </div>
    
        {#if showAddModal}
            <div class="modal-backdrop" onclick={() => (showAddModal = false)}>
                <div class="modal" onclick={(e) => e.stopPropagation()}>
                    <h3>Add media</h3>
                    <button class="cover-picker" onclick={handlePickCover}>
                        {#if newCoverPath}
                            <img src={coverSrc(newCoverPath)} alt="cover preview" />
                        {:else}
                            <span>+ Choose cover</span>
                        {/if}
                    </button>
                    <input placeholder="Title" bind:value={newTitle} />
                    <input placeholder="Tag" bind:value={newTag} />
                    <input type="color" bind:value={newColor} />
                    <button onclick={addMedia}>Add</button>
                </div>
            </div>
        {/if}
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
      />
      <ActionButton
          icon={ICONS.stats}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.unmute}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.plus}
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
    }

    h1 {
        font-size: 1.7em;
        display: flex;
        align-items: center;
        gap: 0.4rem;
        margin-top: 15vh;
        margin-bottom: 0;
    }

    .session-label {
        font-size: 1.2em;
        margin: 0;
        display: flex;
        align-items: center;
        gap: 0.4rem;
    }

    .timer-controls {
        display: flex;
        gap: 1rem;
        align-items: center;
    }

    .profile-picker {
        margin-top: 1.5rem;
        transition: opacity 0.2s ease;
    }

    .logo img {
        width: 50px;
        height: 50px;
        object-fit: contain;
    }

    /* Shared pill button style (edit + extend) */
    .edit-btn {
        display: flex;
        align-items: center;
        justify-content: center;
        background-color: var(--theme-surface, #2d2d2d);
        color: var(--theme-text, #ffffff);
        border: 1px solid transparent;
        border-radius: 100px;
        padding: 0.3em 0.4em;
        font-size: 0.45em;
        cursor: pointer;
        box-shadow: 0 4px 8px var(--theme-shadow, rgba(0, 0, 0, 0.3));
        transition: all 0.3s ease;
        transform: translateY(1px);
    }

    .edit-btn:hover:not(:disabled) {
        border-color: var(--theme-primary, #36b7bd);
        background-color: var(--theme-button, #1a1a1a);
        transform: translateY(0px);
    }

    .edit-btn:active {
        transform: translateY(1px);
    }

    .edit-btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
        transform: none;
    }

    .extend-btn {
        font-size: 0.55em;
    }

    .name-input {
        background: transparent;
        border: none;
        border-bottom: 2px solid var(--theme-primary, #36b7bd);
        color: var(--theme-text, #ffffff);
        font-size: 1.7em;
        font-weight: bold;
        font-family: inherit;
        text-align: center;
        width: 16ch;
        outline: none;
        margin-top: 15vh;
    }

    .add-modal {
        width: 400px;
    }

    .ext-input {
        font-size: 2rem;
        font-weight: 700;
        width: 5ch;
    }

    .add-tabs {
        display: flex;
        background: var(--theme-background, #1a1a1a);
        border-radius: 12px;
        padding: 4px;
        width: 100%;
        gap: 4px;
    }

    .add-tab {
        flex: 1;
        border: none;
        border-radius: 10px;
        background: transparent;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.85rem;
        font-weight: 600;
        font-family: inherit;
        padding: 0.45rem 0;
        cursor: pointer;
        transition: background 0.2s, color 0.2s;
    }

    .add-tab.active {
        background: var(--theme-surface, #2d2d2d);
        color: var(--theme-text, #f6f6f6);
        box-shadow: 0 1px 4px rgba(0, 0, 0, 0.25);
    }

    .add-row {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        width: 100%;
    }

    .add-minutes {
        width: 4.5rem;
    }

    .add-name {
        flex: 1;
    }

    .modal-unit {
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.9rem;
        font-weight: 600;
        margin-right: 0.25rem;
    }

    @media (max-height: 372px) {
        .profile-picker {
            opacity: 0;
            pointer-events: none;
        }

        
    }

    @media (max-height: 450px) {
        .credit {
            opacity: 0 !important;
            pointer-events: none !important;
        }
    }

    /* ── Credit footer ───────────────────────────────────────────────────── */
    .credit {
        position: fixed;
        bottom: 1.2rem;
        left: 50%;
        transform: translateX(-50%) translateY(0);
        display: flex;
        align-items: center;
        gap: 0.35em;
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
        text-decoration: none;
        opacity: 0.30;
        transition: opacity 0.3s ease, transform 0.3s ease;
        white-space: nowrap;
        pointer-events: auto;
    }

    .credit:hover {
        opacity: 1;
        transform: translateX(-50%) translateY(-3px);
    }

    .credit-icon {
        width: 14px;
        height: 14px;
        object-fit: contain;
        opacity: 0.8;
        border-radius: 3px;
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
        gap: 3rem;
        width: 100%;
        max-width: 1100px; /* optional: stop cards from getting absurdly wide on a huge monitor */
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
        aspect-ratio: 2 / 3;
        width: 100%;
        border-radius: 8px;
        overflow: hidden;
        background: var(--surface1, #313244);
        border: 2px solid transparent;
        transition: border-color 0.15s ease, transform 0.15s ease;
    }
    
    .card:hover .cover {
        border-color: var(--accent);
        transform: translateY(-2px);
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
    
    .cover-picker {
        aspect-ratio: 2 / 3;
        width: 120px;
        border: 2px dashed var(--surface2, #45475a);
        border-radius: 8px;
        background: none;
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: pointer;
        overflow: hidden;
        color: var(--subtext0, #a6adc8);
        font-size: 0.8rem;
    }
    
    .cover-picker img {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }
</style>
