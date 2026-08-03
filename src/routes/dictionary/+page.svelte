<script>
    import { page } from '$app/state';
    import { onMount } from 'svelte';
    import { getWords, getMediaTagColors, getSentencesForWord, getAllSentences, updateSentenceTranslation, getLookupCounts, updateWordStatus, mineWordWithTags } from '$lib/dictionary.js';
    import { getFrequentUnknownWords, getMediaTagsForWordIds, dismissUnknownWords } from '$lib/lookupEvents.js';
    import { lookupAtPosition } from '$lib/lookup.js';
    import { getDb } from '$lib/db';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import SelectInput from '$lib/components/SelectInput.svelte';
    import StatusMenu from '$lib/components/StatusMenu.svelte';
    import { ICONS } from '$lib/icons';
    import { loadSettings } from '$lib/settings';

    let mediaFilter = $state(
        page.url.searchParams.get('media') ? Number(page.url.searchParams.get('media')) : null
    );
    let settings = $state(null);
    let statusFilter = $state(null); // words tab only — null means all statuses
    let sortBy = $state('date'); // words tab only — 'date' | 'lookup' | 'status'
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

    let frequentWords = $state([]);
    let frequentLoaded = $state(false);
    let frequentLimit = $state(10);

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

    async function loadFrequentWords() {
        frequentLimit = settings?.unknown_words_count || 10;
        const mediaId = mediaFilter;
        const rows = await getFrequentUnknownWords(mediaId, 1, frequentLimit);
    
        const resolved = (
            await Promise.all(
                rows.map(async (row) => {
                    const span = await lookupAtPosition(row.surface_text, 0, 0);
                    const entry = span?.entries?.[0] ?? null;
                    return entry ? { surfaceText: row.surface_text, count: row.count, entry } : null;
                })
            )
        ).filter(Boolean);
    
        const merged = new Map(); // entry.id -> { surfaceText, count, entry, surfaceTexts }
        for (const item of resolved) {
            const key = item.entry.id;
            const existing = merged.get(key);
            if (!existing) {
                merged.set(key, { ...item, _bestCount: item.count, surfaceTexts: [item.surfaceText] });
            } else {
                existing.count += item.count;
                existing.surfaceTexts.push(item.surfaceText);
                if (item.count > existing._bestCount) {
                    existing._bestCount = item.count;
                    existing.surfaceText = item.surfaceText;
                }
            }
        }
        const mergedItems = Array.from(merged.values()).map(({ _bestCount, ...rest }) => rest);
    
        const tagsByWordId = await getMediaTagsForWordIds(mergedItems.map((item) => item.entry.id));
    
        frequentWords = mergedItems.map((item) => ({ ...item, tags: tagsByWordId[item.entry.id] ?? [] }));
        frequentLoaded = true;
    }


    async function mineFrequentWord(item) {
        const { entry, surfaceText, tags } = item;
        await mineWordWithTags({
            dictId: entry.id,
            spelling: entry.spellings[0] ?? surfaceText,
            reading: entry.readings[0] ?? '',
            definitions: entry.definitions,
            wordType: entry.pos.join(', '),
            tags,
        });
    
        await loadWords();
        await loadFrequentWords();
    }

    async function dismissFrequentWord(item) {
        await dismissUnknownWords(item.surfaceTexts);
        await loadFrequentWords();
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

    $effect(() => {
        if (activeTab === 'frequent') {
            loadFrequentWords();
        }
    });

    onMount(async () => {
        settings = await loadSettings();
        sortBy = settings?.default_dictionary_sort || 'date'
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

    const statusOptions = [
        { value: '', label: 'All statuses' },
        ...STATUS_LEVELS.map((s, i) => ({ value: String(i), label: s.label })),
    ];

    const sortOptions = [
        { value: 'date', label: 'Date mined' },
        { value: 'lookup', label: 'Times looked up' },
        { value: 'status', label: 'Status' },
    ];

    let statusMenuWordId = $state(null);
    let statusMenuPos = $state({ x: 0, y: 0 });

    function openStatusMenu(word, event) {
        const rect = event.currentTarget.getBoundingClientRect();
        statusMenuPos = { x: rect.right + 8, y: rect.top };
        statusMenuWordId = word.id;
    }

    function closeStatusMenu() {
        statusMenuWordId = null;
    }

    async function selectWordStatus(word, status) {
        word.status = status; // optimistic — avoids waiting on a refetch
        closeStatusMenu();
        await updateWordStatus({ wordId: word.id, status });
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

    const SORT_COMPARATORS = {
        date: (a, b) => (b.created_at ?? 0) - (a.created_at ?? 0),
        lookup: (a, b) => (lookupCounts[b.id] ?? 0) - (lookupCounts[a.id] ?? 0),
        status: (a, b) => (a.status ?? 0) - (b.status ?? 0),
    };

    let filteredWords = $derived(
        words
            .filter((w) =>
                searchQuery.trim()
                    ? w.spelling.includes(searchQuery) ||
                      w.reading.includes(searchQuery) ||
                      JSON.parse(w.definitions).some((d) =>
                          d.toLowerCase().includes(searchQuery.toLowerCase())
                      )
                    : true
            )
            .filter((w) => (statusFilter === null ? true : (w.status ?? 0) === statusFilter))
            .sort(SORT_COMPARATORS[sortBy])
    );

    let filteredSentences = $derived(
        searchQuery.trim()
            ? allSentences.filter(
                  (s) =>
                      s.sentence_text.includes(searchQuery) ||
                      (s.translation ?? '').toLowerCase().includes(searchQuery.toLowerCase())
              )
            : allSentences
    );

    let filteredFrequentWords = $derived(
        searchQuery.trim()
            ? frequentWords.filter((w) => {
                  const q = searchQuery.toLowerCase();
                  return (
                      w.surfaceText.includes(searchQuery) ||
                      (w.entry.spellings?.[0] ?? '').includes(searchQuery) ||
                      (w.entry.readings?.[0] ?? '').includes(searchQuery) ||
                      (w.entry.definitions ?? []).some((d) => d.toLowerCase().includes(q))
                  );
              })
            : frequentWords
    );

    function handleMediaFilterChange(e) {
        mediaFilter = e.target.value ? Number(e.target.value) : null;
    }

    function handleStatusFilterChange(e) {
        statusFilter = e.target.value ? Number(e.target.value) : null;
    }

    function handleSortChange(e) {
        sortBy = e.target.value;
    }
</script>

<main class="page dictionary-page">
    <div class="dict-header">
        <ActionButton icon={ICONS.back} variant="primary" size="small" onAction={() => history.back()} />
        <h1>Dictionary</h1>
    </div>

    <div class="dict-toolbar">
        <input class="modal-input" placeholder="Search words, definitions, or sentences" bind:value={searchQuery} />
        <SelectInput
            options={mediaOptions}
            value={mediaFilter ? String(mediaFilter) : ''}
            on:change={handleMediaFilterChange}
        />
        {#if activeTab === 'words'}
            <SelectInput
                options={statusOptions}
                value={statusFilter === null ? '' : String(statusFilter)}
                on:change={handleStatusFilterChange}
            />
            <div class="sort-control">
                <span class="sort-icon">{@html ICONS.sort}</span>
                <SelectInput
                    options={sortOptions}
                    value={sortBy}
                    on:change={handleSortChange}
                />
            </div>
        {/if}
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
        {#if settings?.track_unknown_words}
            <button
                type="button"
                class="tab-btn"
                class:active={activeTab === 'frequent'}
                onclick={() => (activeTab = 'frequent')}
            >
                Frequently looked up
            </button>
        {/if}
    </div>

    <div class="dict-content">
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
                            title={`Status: ${STATUS_LEVELS[word.status ?? 0].label} — click to change`}
                            aria-haspopup="menu"
                            aria-expanded={statusMenuWordId === word.id}
                            onclick={(e) => openStatusMenu(word, e)}
                        ></button>
                        {#if statusMenuWordId === word.id}
                            <StatusMenu
                                x={statusMenuPos.x}
                                y={statusMenuPos.y}
                                levels={STATUS_LEVELS}
                                current={word.status ?? 0}
                                onSelect={(status) => selectWordStatus(word, status)}
                                onClose={closeStatusMenu}
                            />
                        {/if}
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
        {:else if filteredSentences.length === 0}
            <p class="empty-notice">
                {allSentences.length === 0 ? 'No sentences found.' : 'No sentences match your search.'}
            </p>
        {:else}
            <div class="word-list">
                {#each filteredSentences as sentence (sentence.id ?? sentence.sentence_text)}
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
        {#if !frequentLoaded}
            <p class="empty-notice">Loading frequently looked up words...</p>
        {:else if filteredFrequentWords.length === 0}
            <p class="empty-notice">
                {frequentWords.length === 0
                    ? 'No frequently looked up words outside your dictionary yet.'
                    : 'No results match your search.'}
            </p>
        {:else}
            <div class="word-list word-grid">
                {#each filteredFrequentWords as item (item.entry.id)}
                    <div class="word-card">
                        <div class="lookup-badge frequent-badge">
                            <span class="lookup-badge-icon">{@html ICONS.magnify}</span>
                            <span class="lookup-badge-count">{item.count}</span>
                        </div>
                        <div class="word-main">
                            <span class="word-spelling">{item.entry.spellings?.[0] ?? item.surfaceText}</span>
                            <span class="word-reading">{item.entry.readings?.[0] ?? ''}</span>
                        </div>
                        <div class="entry-pos">{item.entry.pos?.join(', ') ?? ''}</div>
                        <div class="word-definitions">
                            {(item.entry.definitions ?? []).join('; ')}
                        </div>
                        <div class="word-meta">
                            {#each item.tags as tag}
                                <span
                                    class="tag-pill"
                                    style={tagColors[tag] ? `--tag-color: ${tagColors[tag]}` : ''}
                                >
                                    #{tag}
                                </span>
                            {/each}
                            <button type="button" class="mine-btn" onclick={() => mineFrequentWord(item)}>
                                {ICONS.plus}
                            </button>
                            <button
                                type="button"
                                class="dismiss-btn"
                                onclick={() => dismissFrequentWord(item)}
                                title="Dismiss — stop tracking this word's lookups"
                            >
                                {ICONS.minus}
                            </button>
                        </div>
                    </div>
                {/each}
            </div>
        {/if}
    {/if}
    </div>
</main>

<style>
    button.mine-btn,
    button.dismiss-btn {
        font-family: "Symbols Nerd Font";
    }

    .dictionary-page {
        padding: 2rem;
        box-sizing: border-box;
        height: 100vh;
        overflow: hidden;
        display: flex;
        flex-direction: column;
        min-height: 0;
    }

    .dict-content {
        flex: 1;
        min-height: 0;
        overflow-y: scroll;
        padding-right: 1rem;
        padding-top: 0.2rem;
    }

    .dict-content::-webkit-scrollbar {
        width: 6px;
    }

    .dict-content::-webkit-scrollbar-track {
        background: transparent;
    }

    .dict-content::-webkit-scrollbar-thumb {
        background: var(--theme-border, #404040);
        border-radius: 3px;
    }

    .dict-content::-webkit-scrollbar-thumb:hover {
        background: var(--theme-textSecondary, #b3b3b3);
    }

    .dict-header {
        display: flex;
        align-items: center;
        gap: 1rem;
        margin-bottom: 1.5rem;
        flex-shrink: 0;
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
        flex-shrink: 0;
    }

    .dict-toolbar .modal-input {
        flex: 1;
    }

    .sort-control {
        display: flex;
        align-items: center;
        gap: 0.4rem;
    }

    .sort-icon {
        font-family: "Symbols Nerd Font";
        display: flex;
        align-items: center;
        flex-shrink: 0;
        width: 1rem;
        height: 1rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .sort-icon :global(svg) {
        width: 100%;
        height: 100%;
    }

    .dict-tabs {
        display: flex;
        gap: 1.5rem;
        margin-bottom: 1.5rem;
        max-width: 1200px;
        border-bottom: 1px solid var(--theme-border, #404040);
        flex-shrink: 0;
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

    .word-list.word-grid {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        align-items: stretch;
        column-gap: 0.75rem;
        row-gap: 2rem;
    }

    .word-grid .word-card {
        height: 100%;
        padding-bottom: 0;
        display: flex;
        flex-direction: column;
    }

    .word-grid .word-card .word-meta {
        margin-top: auto;
        padding-bottom: 1rem;
    }

    .word-card {
        position: relative;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 12px;
        padding: 1rem 1.25rem;
        transition: transform 0.15s ease, box-shadow 0.15s ease, border-color 0.15s ease;
        will-change: transform;
        transform: translateZ(0);
    }

    .word-card:hover {
        transform: translateY(-3px);
        box-shadow: 0 10px 22px rgba(0, 0, 0, 0.35);
    }

    .status-bar {
        position: absolute;
        top: 0;
        left: 0;
        bottom: 0;
        width: 7px;
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
        width: 10px;
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
        color: var(--tag-color, #89b4fa);
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

    .mine-btn {
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 18%, transparent);
        border: 1px solid color-mix(in srgb, var(--theme-primary, #36b7bd) 40%, transparent);
        color: var(--theme-primary, #36b7bd);
        font: inherit;
        font-size: 0.78rem;
        font-weight: 600;
        padding: 0.35rem 0.75rem;
        border-radius: 999px;
        cursor: pointer;
        margin-left: auto;
        transition: background 0.15s ease, color 0.15s ease;
    }

    .mine-btn:hover {
        background: var(--theme-primary, #36b7bd);
        color: var(--theme-surface, #1e1e2e);
    }

    .dismiss-btn {
        background: color-mix(in srgb, var(--theme-textSecondary, #b3b3b3) 12%, transparent);
        border: 1px solid color-mix(in srgb, var(--theme-textSecondary, #b3b3b3) 35%, transparent);
        color: var(--theme-textSecondary, #b3b3b3);
        font: inherit;
        font-size: 0.78rem;
        font-weight: 600;
        padding: 0.35rem 0.75rem;
        border-radius: 999px;
        cursor: pointer;
        transition: background 0.15s ease, color 0.15s ease, border-color 0.15s ease;
    }
    
    .dismiss-btn:hover {
        background: color-mix(in srgb, var(--theme-textSecondary, #b3b3b3) 25%, transparent);
        color: var(--theme-text, #f6f6f6);
        border-color: var(--theme-textSecondary, #b3b3b3);
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