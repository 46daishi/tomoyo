<script>
    import { page } from '$app/state';
    import { onMount } from 'svelte';
    import { getWords, getMediaTagColors, getSentencesForWord, getAllSentences, updateSentenceTranslation, getLookupCounts, updateWordStatus } from '$lib/dictionary.js';
    import { getDb } from '$lib/db';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import SelectInput from '$lib/components/SelectInput.svelte';
    import { ICONS } from '$lib/icons';

    let mediaFilter = $state(
        page.url.searchParams.get('media') ? Number(page.url.searchParams.get('media')) : null
    );
    let searchQuery = $state('');
    let words = $state([]);
    let lookupCounts = $state({});
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
        const mediaId = mediaFilter; // read synchronously so $effect tracks this as a dependency
        const [wordRows, counts] = await Promise.all([
            getWords({ mediaId }),
            getLookupCounts({ mediaId }),
        ]);
        words = wordRows;
        lookupCounts = counts;
    }

    async function loadSentences() {
        const mediaId = mediaFilter; // read synchronously so $effect tracks this as a dependency
        allSentences = await getAllSentences({ mediaId });
        sentencesLoaded = true;
    }

    async function commitTranslation(sentence, value) {
        const translation = value.trim() || null;
        sentence.translation = translation;
        await updateSentenceTranslation({ sentenceText: sentence.sentence_text, translation });
    }

    $effect(() => {
        loadWords();
    });

    $effect(() => {
        if (activeTab === 'sentences') {
            loadSentences();
        }
    });

    onMount(() => {
        loadMediaOptions();
        loadTagColors();
    });

    const STATUS_LEVELS = [
        { label: 'New', color: '#6c7086' },
        { label: 'Recognized', color: '#89b4fa' },
        { label: 'Familiar', color: '#cba6f7' },
        { label: 'Learned', color: '#a6e3a1' },
        { label: 'Known', color: '#40a02b' },
    ];

    async function cycleWordStatus(word, event) {
        const current = word.status ?? 0;
        const direction = event.shiftKey ? -1 : 1;
        const next = (current + direction + STATUS_LEVELS.length) % STATUS_LEVELS.length;

        word.status = next; // optimistic — avoids waiting on a refetch
        await updateWordStatus({ wordId: word.id, status: next });
    }

    const FREQUENCY_TIER_COLORS = {
        red: '#FF4F4F',
        orange: '#FFA14F',
        yellow: '#FFED4F',
        green: '#78FF4F',
        blue: '#4FDCFF',
    };

    // Ranked by percentile among words with at least one lookup — based on
    // `words` (media-filtered) rather than `filteredWords`, so typing in the
    // search box doesn't shift anyone's color as the visible set shrinks.
    //
    // Cutoffs are computed as counts (ceil of each fraction), each floored
    // at the previous cutoff, so the top word is always red even with a
    // small pool — a strict percentile fraction (rank / total) can exceed
    // 0.1 for every word when total is small (e.g. 1/1 = 1.0), which would
    // leave red unused entirely.
    let lookupFrequencyMeta = $derived.by(() => {
        const ranked = words
            .map((w) => ({ id: w.id, count: lookupCounts[w.id] ?? 0 }))
            .filter((w) => w.count > 0)
            .sort((a, b) => b.count - a.count);

        const total = ranked.length;
        const meta = {};
        if (total === 0) return meta;

        const redCutoff = Math.max(1, Math.ceil(total * 0.1));
        const orangeCutoff = Math.max(redCutoff, Math.ceil(total * 0.3));
        const yellowCutoff = Math.max(orangeCutoff, Math.ceil(total * 0.5));
        const greenCutoff = Math.max(yellowCutoff, Math.ceil(total * 0.75));

        ranked.forEach((w, index) => {
            const rank = index + 1;
            let tier;
            if (rank <= redCutoff) tier = 'red';
            else if (rank <= orangeCutoff) tier = 'orange';
            else if (rank <= yellowCutoff) tier = 'yellow';
            else if (rank <= greenCutoff) tier = 'green';
            else tier = 'blue';

            meta[w.id] = { count: w.count, color: FREQUENCY_TIER_COLORS[tier] };
        });

        return meta;
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
                        <button
                            type="button"
                            class="status-bar"
                            style={`--status-color: ${STATUS_LEVELS[word.status ?? 0].color}`}
                            title={`Status: ${STATUS_LEVELS[word.status ?? 0].label} — click to advance, shift+click to go back`}
                            onclick={(e) => cycleWordStatus(word, e)}
                        ></button>
                        {#if lookupFrequencyMeta[word.id]}
                            <div
                                class="lookup-badge"
                                style={`--badge-color: ${lookupFrequencyMeta[word.id].color}`}
                            >
                                <span class="lookup-badge-icon">{@html ICONS.magnify}</span>
                                <span class="lookup-badge-count">{lookupFrequencyMeta[word.id].count}</span>
                            </div>
                        {/if}
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
                        <p class="sentence-text sentence-tab-text">{sentence.sentence_text}</p>
                        <div class="translation-edit-row">
                            <span class="translation-icon">{@html ICONS.translate}</span>
                            <input
                                class="translation-input"
                                type="text"
                                placeholder="Add a translation..."
                                value={sentence.translation ?? ''}
                                onblur={(e) => commitTranslation(sentence, e.target.value)}
                                onkeydown={(e) => {
                                    if (e.key === 'Enter') e.target.blur();
                                }}
                            />
                        </div>
                        {#if sentence.tags}
                            <div class="word-meta">
                                {#each sentence.tags.split(',') as tag}
                                    <span
                                        class="tag-pill"
                                        style={tagColors[tag] ? `--tag-color: ${tagColors[tag]}` : ''}
                                    >
                                        #{tag}
                                    </span>
                                {/each}
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
        position: relative;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 12px;
        padding: 1rem 1.25rem;
        transition: transform 0.15s ease, box-shadow 0.15s ease, border-color 0.15s ease;
    }

    .word-card:hover {
        transform: translateY(-3px);
        box-shadow: 0 10px 22px rgba(0, 0, 0, 0.35);
        border-color: color-mix(in srgb, var(--theme-primary, #36b7bd) 45%, var(--theme-border, #404040));
    }

    .status-bar {
        position: absolute;
        top: 0;
        left: 0;
        bottom: 0;
        width: 8px;
        border: none;
        padding: 0;
        margin: 0;
        cursor: pointer;
        background: var(--status-color, #6c7086);
        border-top-left-radius: 12px;
        border-bottom-left-radius: 12px;
        transition: width 0.15s ease;
    }

    .status-bar:hover {
        width: 12px;
    }

    .word-main {
        display: flex;
        align-items: baseline;
        gap: 0.6rem;
    }

    .word-spelling {
        font-size: 1.4rem;
        font-weight: 700;
        font-family: "Noto Sans JP", Inter, sans-serif;
        color: var(--theme-text, #f6f6f6);
    }

    .word-reading {
        font-size: 1rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .word-definitions {
        font-size: 1rem;
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

    .lookup-badge {
        position: absolute;
        top: 0.75rem;
        right: 0.9rem;
        display: flex;
        align-items: center;
        gap: 0.3rem;
        font-size: 0.72rem;
        font-weight: 700;
        color: var(--badge-color, #89dceb);
        background: color-mix(in srgb, var(--badge-color, #89dceb) 16%, transparent);
        border: 1px solid color-mix(in srgb, var(--badge-color, #89dceb) 40%, transparent);
        border-radius: 10px;
        padding: 0em 0.65em;
    }

    .lookup-badge-icon {
        font-family: "Symbols Nerd Font";
        display: flex;
        align-items: center;
        width: 0.6rem;
        height: 0.6rem;
    }

    .lookup-badge-icon :global(svg) {
        width: 100%;
        height: 100%;
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

    .sentence-tab-text {
        font-size: 1.5rem;
        font-weight: 600;
        line-height: 2rem;
    }

    .sentence-translation {
        margin: 0.25rem 0 0 0;
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .translation-edit-row {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-top: 0.6rem;
        padding: 0.2rem 0.2rem;
        border-radius: 8px;
    }

    .translation-icon {
        font-family: "Symbols Nerd Font";
        font-size: 1.2rem;
        display: flex;
        align-items: center;
        flex-shrink: 0;
        width: 1rem;
        height: 1rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .translation-icon :global(svg) {
        width: 100%;
        height: 100%;
    }

    .translation-input {
        flex: 1;
        min-width: 0;
        background: none;
        border: none;
        outline: none;
        font: inherit;
        font-size: 1rem;
        color: var(--theme-text, #f6f6f6);
    }

    .translation-input::placeholder {
        color: var(--theme-textSecondary, #b3b3b3);
        opacity: 0.7;
    }

    .empty-notice {
        color: var(--theme-textSecondary, #b3b3b3);
        text-align: center;
        margin-top: 2rem;
    }
</style>