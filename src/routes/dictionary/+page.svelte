<script>
    import { page } from '$app/state';
    import { onMount } from 'svelte';
    import { getWords, getMediaTagColors, getSentencesForWord } from '$lib/dictionary.js';
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
    let tagColors = $state({});
    let expandedWords = $state(new Set());
    let sentencesByWord = $state({});

    let activeTab = $state('words'); // 'words' | 'sentences' | 'frequent'
    let allSentences = $state([]);
    let sentencesLoaded = $state(false);

    async function loadMediaOptions() {
        const db = await getDb();
        const rows = await db.select('SELECT id, title FROM media ORDER BY title');
        mediaOptions = [
            { value: '', label: 'All media' },
            ...rows.map((m) => ({ value: String(m.id), label: m.title })),
        ];
    }

    async function loadTagColors() {
        tagColors = await getMediaTagColors();
    }

    async function toggleExpand(wordId) {
        const next = new Set(expandedWords);
        if (next.has(wordId)) {
            next.delete(wordId);
            expandedWords = next;
            return;
        }
        next.add(wordId);
        expandedWords = next;

        if (!sentencesByWord[wordId]) {
            const sentences = await getSentencesForWord(wordId);
            sentencesByWord = { ...sentencesByWord, [wordId]: sentences };
        }
    }

    async function loadWords() {
        words = await getWords({ mediaId: mediaFilter });
    }

    async function loadSentences() {
        const db = await getDb();
        allSentences = await db.select('SELECT * FROM word_sentences GROUP BY sentence_text ORDER BY created_at DESC');
        sentencesLoaded = true;
    }

    $effect(() => {
        loadWords();
    });

    $effect(() => {
        if (activeTab === 'sentences' && !sentencesLoaded) {
            loadSentences();
        }
    });

    onMount(() => {
        loadMediaOptions();
        loadTagColors();
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

    <div class="dict-tabs">
        <button
            type="button"
            class="tab-btn"
            class:active={activeTab === 'words'}
            onclick={() => (activeTab = 'words')}
        >
            Words
        </button>
        <button
            type="button"
            class="tab-btn"
            class:active={activeTab === 'sentences'}
            onclick={() => (activeTab = 'sentences')}
        >
            Sentences
        </button>
        <button
            type="button"
            class="tab-btn"
            class:active={activeTab === 'frequent'}
            onclick={() => (activeTab = 'frequent')}
        >
            Frequently looked up
        </button>
    </div>

    {#if activeTab === 'words'}
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
                        <div class="entry-pos">{word.word_type}</div>
                        <div class="word-definitions">
                            {JSON.parse(word.definitions).join('; ')}
                        </div>
                        <div class="word-meta">
                            {#if word.tags}
                                {#each word.tags.split(',') as tag}
                                    <span
                                        class="tag-pill"
                                        style={tagColors[tag] ? `--tag-color: ${tagColors[tag]}` : ''}
                                    >
                                        #{tag}
                                    </span>
                                {/each}
                            {/if}
                            <button 
                                type="button" 
                                class="sentence-count" 
                                onclick={() => toggleExpand(word.id)}
                            >
                                {word.sentence_count} sentence{word.sentence_count === 1 ? '' : 's'}
                            </button>
                        </div>

                        {#if expandedWords.has(word.id)}
                            <div class="sentences-panel">
                                {#if !sentencesByWord[word.id]}
                                    <div class="sentences-loading">Loading sentences...</div>
                                {:else if sentencesByWord[word.id].length === 0}
                                    <div class="sentences-empty">No sentences found.</div>
                                {:else}
                                    <ul class="sentences-list">
                                        {#each sentencesByWord[word.id] as sentence (sentence.id ?? sentence.sentence_text)}
                                            <li class="sentence-item">
                                                <p class="sentence-text">{sentence.sentence_text}</p>
                                                {#if sentence.translation}
                                                    <p class="sentence-translation">{sentence.translation}</p>
                                                {/if}
                                            </li>
                                        {/each}
                                    </ul>
                                {/if}
                            </div>
                        {/if}
                    </div>
                {/each}
            </div>
        {/if}
    {:else if activeTab === 'sentences'}
        {#if !sentencesLoaded}
            <p class="empty-notice">Loading sentences...</p>
        {:else if allSentences.length === 0}
            <p class="empty-notice">No sentences found.</p>
        {:else}
            <div class="word-list">
                {#each allSentences as sentence (sentence.id ?? sentence.sentence_text)}
                    <div class="word-card">
                        <p class="sentence-text">{sentence.sentence_text}</p>
                        {#if sentence.translation}
                            <p class="sentence-translation">{sentence.translation}</p>
                        {/if}
                        {#if sentence.tag}
                            <div class="word-meta">
                                <span
                                    class="tag-pill"
                                    style={tagColors[sentence.tag] ? `--tag-color: ${tagColors[sentence.tag]}` : ''}
                                >
                                    #{sentence.tag}
                                </span>
                            </div>
                        {/if}
                    </div>
                {/each}
            </div>
        {/if}
    {:else if activeTab === 'frequent'}
        <p class="empty-notice">
            TBD.
        </p>
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
        max-width: 1200px;
    }

    .dict-toolbar .modal-input {
        flex: 1;
    }

    .dict-tabs {
        display: flex;
        gap: 1.5rem;
        margin-bottom: 1.5rem;
        max-width: 1200px;
        border-bottom: 1px solid var(--theme-border, #404040);
    }

    .tab-btn {
        background: none;
        border: none;
        border-bottom: 2px solid transparent;
        padding: 0 0 0.6rem 0;
        font: inherit;
        font-size: 0.9rem;
        font-weight: 600;
        color: var(--theme-textSecondary, #b3b3b3);
        cursor: pointer;
        transition: color 0.15s ease, border-color 0.15s ease;
    }

    .tab-btn:hover {
        color: var(--theme-text, #f6f6f6);
    }

    .tab-btn.active {
        color: var(--theme-primary, #36b7bd);
        border-bottom-color: var(--theme-primary, #36b7bd);
    }

    .word-list {
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
        max-width: 1200px;
    }

    .word-card {
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 12px;
        padding: 1rem 1.25rem;
        transition: transform 0.15s ease, box-shadow 0.15s ease, border-color 0.15s ease;
    }

    .word-card:hover {
        transform: translateY(-3px);
        box-shadow: 0 10px 22px rgba(0, 0, 0, 0.35);
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
        font-size: 0.7rem;
        font-weight: 600;
        padding: 0.01em 0.6em;
        border-radius: 50px;
        color: var(--tag-color, #89b4fa);
        background: color-mix(in srgb, var(--tag-color, #89b4fa) 18%, transparent);
        border: 1px solid color-mix(in srgb, var(--tag-color, #89b4fa) 40%, transparent);
    }

    .sentence-count {
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        font-size: 0.78rem;
        color: var(--theme-textSecondary, #b3b3b3);
        margin-left: auto;
        cursor: pointer;
        text-decoration: underline;
        text-underline-offset: 2px;
        transition: color 0.15s ease;
    }

    .sentence-count:hover {
        color: var(--theme-text, #f6f6f6);
    }

    .sentences-panel {
        margin-top: 0.85rem;
        padding-top: 0.75rem;
        border-top: 1px solid color-mix(in srgb, var(--theme-border, #404040) 50%, transparent);
    }

    .sentences-loading,
    .sentences-empty {
        font-size: 0.82rem;
        color: var(--theme-textSecondary, #b3b3b3);
        font-style: italic;
    }

    .sentences-list {
        list-style: none;
        padding: 0;
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .sentence-item {
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 30%, transparent);
        border: 1px solid color-mix(in srgb, var(--theme-border, #404040) 40%, transparent);
        padding: 0.5rem 0.75rem;
        border-radius: 6px;
    }

    .sentence-text {
        margin: 0;
        font-size: 1.1rem;
        color: var(--theme-text, #f6f6f6);
        font-family: "Noto Sans JP", Inter, sans-serif;
    }

    .sentence-translation {
        margin: 0.25rem 0 0 0;
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .empty-notice {
        color: var(--theme-textSecondary, #b3b3b3);
        text-align: center;
        margin-top: 2rem;
    }
</style>