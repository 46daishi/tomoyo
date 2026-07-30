<script>
    import { page } from '$app/state';
    import { onMount } from 'svelte';
    import { getWords } from '$lib/dictionary.js';
    import { getDb } from '$lib/db';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import SelectInput from '$lib/components/SelectInput.svelte';
    import { ICONS } from '$lib/icons';

    let mediaFilter = $state(
        page.url.searchParams.get('media') ? Number(page.url.searchParams.get('media')) : null
    );
    let searchQuery = $state('');
    let words = $state([]);
    let mediaOptions = $state([{ value: '', label: 'All media' }]);

    async function loadMediaOptions() {
        const db = await getDb();
        const rows = await db.select('SELECT id, title FROM media ORDER BY title');
        mediaOptions = [
            { value: '', label: 'All media' },
            ...rows.map((m) => ({ value: String(m.id), label: m.title })),
        ];
    }

    async function loadWords() {
        words = await getWords({ mediaId: mediaFilter });
    }

    $effect(() => {
        loadWords();
    });

    onMount(() => {
        loadMediaOptions();
    });

    let filteredWords = $derived(
        searchQuery.trim()
            ? words.filter(
                  (w) =>
                      w.spelling.includes(searchQuery) ||
                      w.reading.includes(searchQuery) ||
                      JSON.parse(w.definitions).some((d) =>
                          d.toLowerCase().includes(searchQuery.toLowerCase())
                      )
              )
            : words
    );

    function handleMediaFilterChange(e) {
        mediaFilter = e.target.value ? Number(e.target.value) : null;
    }
</script>

<main class="page dictionary-page">
    <div class="dict-header">
        <ActionButton icon={ICONS.back} variant="primary" size="small" onAction={() => history.back()} />
        <h1>Dictionary</h1>
    </div>

    <div class="dict-toolbar">
        <input class="modal-input" placeholder="Search words or definitions" bind:value={searchQuery} />
        <SelectInput
            options={mediaOptions}
            value={mediaFilter ? String(mediaFilter) : ''}
            on:change={handleMediaFilterChange}
        />
    </div>

    {#if filteredWords.length === 0}
        <p class="empty-notice">No words found.</p>
    {:else}
        <div class="word-list">
            {#each filteredWords as word (word.id)}
                <div class="word-card">
                    <div class="word-main">
                        <span class="word-spelling">{word.spelling}</span>
                        <span class="word-reading">{word.reading}</span>
                    </div>
                    <div class="word-definitions">
                        {JSON.parse(word.definitions).join('; ')}
                    </div>
                    <div class="word-meta">
                        {#if word.tags}
                            {#each word.tags.split(',') as tag}
                                <span class="tag-pill">#{tag}</span>
                            {/each}
                        {/if}
                        <span class="sentence-count">
                            {word.sentence_count} sentence{word.sentence_count === 1 ? '' : 's'}
                        </span>
                    </div>
                </div>
            {/each}
        </div>
    {/if}
</main>

<style>
    .dictionary-page {
        padding: 2rem;
        box-sizing: border-box;
    }

    .dict-header {
        display: flex;
        align-items: center;
        gap: 1rem;
        margin-bottom: 1.5rem;
    }

    .dict-header h1 {
        font-size: 1.5rem;
        margin: 0;
    }

    .dict-toolbar {
        display: flex;
        gap: 0.75rem;
        margin-bottom: 1.5rem;
        max-width: 700px;
    }

    .dict-toolbar .modal-input {
        flex: 1;
    }

    .word-list {
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
        max-width: 700px;
    }

    .word-card {
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 12px;
        padding: 1rem 1.25rem;
    }

    .word-main {
        display: flex;
        align-items: baseline;
        gap: 0.6rem;
    }

    .word-spelling {
        font-size: 1.2rem;
        font-weight: 700;
        font-family: "Noto Sans JP", Inter, sans-serif;
        color: var(--theme-text, #f6f6f6);
    }

    .word-reading {
        font-size: 0.9rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .word-definitions {
        font-size: 0.9rem;
        color: var(--theme-text, #f6f6f6);
        margin-top: 0.3rem;
    }

    .word-meta {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-top: 0.6rem;
        flex-wrap: wrap;
    }

    .tag-pill {
        font-size: 0.8rem;
        font-weight: 600;
        padding: 0.01em 0.7em;
        border-radius: 100px;
        color: var(--tag-color, #89b4fa);
        background: color-mix(in srgb, var(--tag-color, #89b4fa) 18%, transparent);
        border: 1px solid color-mix(in srgb, var(--tag-color, #89b4fa) 40%, transparent);
    }

    .sentence-count {
        font-size: 0.78rem;
        color: var(--theme-textSecondary, #b3b3b3);
        margin-left: auto;
    }

    .empty-notice {
        color: var(--theme-textSecondary, #b3b3b3);
        text-align: center;
        margin-top: 2rem;
    }
</style>