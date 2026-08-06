<script>
    import { page } from '$app/state';
    import { onMount } from 'svelte';
    import { getWords, getMediaTagColors, getSentencesForWord, getAllSentences, updateSentenceTranslation, getLookupCounts, updateWordStatus, mineWordWithTags, deleteSentence, deleteWord, updateWordNotes, clearDictionaryData } from '$lib/dictionary.js';
    import { getFrequentUnknownWords, getMediaTagsForWordIds, dismissUnknownWords } from '$lib/lookupEvents.js';
    import { lookupAtPosition } from '$lib/lookup.js';
    import { getDb } from '$lib/db';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import SelectInput from '$lib/components/SelectInput.svelte';
    import StatusMenu from '$lib/components/StatusMenu.svelte';
    import { ICONS } from '$lib/icons';
    import { loadSettings } from '$lib/settings';
    import { confirm } from '@tauri-apps/plugin-dialog';
    import { STATUS_LEVELS } from '$lib/constants';
    import { getReviewStats } from '$lib/reviewStats.js';
    import { goto } from '$app/navigation';

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

    let activeTab = $state('words'); // 'words' | 'sentences' | 'frequent' | 'review'
    let allSentences = $state([]);
    let sentencesLoaded = $state(false);

    let frequentWords = $state([]);
    let frequentLoaded = $state(false);
    let frequentLimit = $state(10);

    const STAT_COLORS = {
        wordCount: '#36b7bd',
        sentenceCount: '#89b4fa',
        wordTime: '#36b7bd',
        sentenceTime: '#89b4fa',
        lastReview: '#cba6f7',
        currentStreak: '#fab387',
        longestStreak: '#f38ba8',
    };

    function emptyReviewStats() {
        return {
            wordReviewCount: 0,
            sentenceReviewCount: 0,
            wordTimeSeconds: 0,
            sentenceTimeSeconds: 0,
            lastReviewDate: null,
            currentStreak: 0,
            longestStreak: 0,
        };
    }
    
    let reviewStats = $state(emptyReviewStats());

    let reviewTotals = $derived.by(() => {
        const totalReviews = reviewStats.wordReviewCount + reviewStats.sentenceReviewCount;
        const wordReviewPct = totalReviews > 0 ? (reviewStats.wordReviewCount / totalReviews) * 100 : 0;
        const sentenceReviewPct = totalReviews > 0 ? (reviewStats.sentenceReviewCount / totalReviews) * 100 : 0;
    
        const totalTime = reviewStats.wordTimeSeconds + reviewStats.sentenceTimeSeconds;
        const wordTimePct = totalTime > 0 ? (reviewStats.wordTimeSeconds / totalTime) * 100 : 0;
        const sentenceTimePct = totalTime > 0 ? (reviewStats.sentenceTimeSeconds / totalTime) * 100 : 0;
    
        return { totalReviews, wordReviewPct, sentenceReviewPct, totalTime, wordTimePct, sentenceTimePct };
    });

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

    function viewFullSentences(word) {
        searchQuery = word.spelling;
        activeTab = 'sentences';
    }

    async function loadWords() {
        const mediaId = mediaFilter;
        const [wordRows, counts] = await Promise.all([
            getWords({ mediaId }),
            getLookupCounts({ mediaId }),
        ]);
        words = wordRows;
        lookupCounts = counts;
    }

    async function loadSentences() {
        const mediaId = mediaFilter;
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
    
        const merged = new Map();
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
        const { entry, surfaceText, mediaIds } = item;
        await mineWordWithTags({
            dictId: entry.id,
            spelling: entry.spellings[0] ?? surfaceText,
            reading: entry.readings[0] ?? '',
            definitions: entry.definitions,
            wordType: entry.pos.join(', '),
            mediaIds: mediaIds
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
        const tabParam = page.url.searchParams.get('tab');
        if (tabParam) activeTab = tabParam;
        settings = await loadSettings();
        sortBy = settings?.default_dictionary_sort || 'date';
        loadMediaOptions();
        loadTagColors();
    });

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
        word.status = status;
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

    let sentenceLimit = $derived(settings?.word_sentence_count || 5);

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

    async function handleDeleteSentence(sentence) {
        const yes = await confirm(
            'Delete this sentence? This removes it as an example everywhere it appears — mined words themselves are not affected.',
            { title: 'Delete sentence', kind: 'warning' }
        );
        if (!yes) return;
    
        await deleteSentence(sentence.sentence_text);
        await loadSentences();
    }

    async function handleDeleteWord(word) {
        const message = mediaFilter
            ? `Remove "${word.spelling}" from this media? If this is its only source, it will be deleted entirely — otherwise it stays in your dictionary under its other sources.`
            : `Delete "${word.spelling}" completely? This removes it from your dictionary entirely, across all media.`;
    
        const yes = await confirm(message, { title: 'Delete word', kind: 'warning' });
        if (!yes) return;
    
        await deleteWord({ wordId: word.id, mediaId: mediaFilter });
        await loadWords();
    }

    async function commitWordNotes(word, value) {
        const notes = value.trim() || null;
        word.notes = notes;
        await updateWordNotes({ wordId: word.id, notes });
    }

    async function handleClearDictionary() {
        const scopeLabel = mediaFilter
            ? mediaOptions.find((m) => m.value === String(mediaFilter))?.label ?? 'this media'
            : 'all media';
    
        const wordCount = filteredWords.length;
    
        const yes = await confirm(
            `Delete all ${wordCount} word${wordCount === 1 ? '' : 's'} and their lookup history for ${scopeLabel}? This cannot be undone.`,
            { title: 'Clear dictionary', kind: 'warning' }
        );
        if (!yes) return;
    
        await clearDictionaryData({ mediaId: mediaFilter });
        await loadWords();
    }

    async function loadReviewStats() {
        reviewStats = await getReviewStats(mediaFilter);
    }
    
    $effect(() => {
        if (activeTab === 'review') {
            mediaFilter;
            loadReviewStats();
        }
    });

    function formatDuration(seconds) {
        const h = Math.floor(seconds / 3600);
        const m = Math.floor((seconds % 3600) / 60);
        if (h > 0) return `${h}h ${m}m`;
        if (m > 0) return `${m}m`;
        return `${seconds}s`;
    }
    
    function formatReviewDate(dateStr) {
        if (!dateStr) return 'Never';
        const date = new Date(dateStr + 'T00:00:00');
        return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
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
        <ActionButton
                icon={ICONS.trash}
                variant="danger"
                size="tiny"
                onAction={handleClearDictionary}
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
        <button
            type="button"
            class="tab-btn"
            class:active={activeTab === 'review'}
            onclick={() => (activeTab = 'review')}
        >
            Review
        </button>
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
                            <div class="lookup-badge" style={`--badge-color: ${lookupFrequencyMeta[word.id].color}`}>
                                <span class="lookup-badge-count">{lookupFrequencyMeta[word.id].count}</span>
                                <span class="lookup-badge-icon">{@html ICONS.magnify}</span>
                            </div>
                        {/if}
                        
                        <div class="word-main">
                            <span class="word-spelling">{word.spelling}</span>
                            <span class="word-reading">{word.reading}</span>
                            <button
                                type="button"
                                class="word-delete-btn"
                                onclick={() => handleDeleteWord(word)}
                                title="Delete word"
                            >
                                {@html ICONS.trash}
                            </button>
                        </div>
                        <div class="entry-pos">{word.word_type}</div>
                        <div class="word-definitions">
                            {JSON.parse(word.definitions).join('; ')}
                        </div>
                        <div class="notes-edit-row">
                            <span class="notes-icon">{@html ICONS.note}</span>
                            <input
                                class="notes-input"
                                type="text"
                                placeholder="..."
                                value={word.notes ?? ''}
                                onblur={(e) => commitWordNotes(word, e.target.value)}
                                onkeydown={(e) => {
                                    if (e.key === 'Enter') e.target.blur();
                                }}
                            />
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
                                        {#each sentencesByWord[word.id].slice(0, sentenceLimit) as sentence (sentence.id ?? sentence.sentence_text)}
                                            <li class="sentence-item">
                                                <p class="sentence-text">{sentence.sentence_text}</p>
                                                {#if sentence.translation}
                                                    <p class="sentence-translation">{sentence.translation}</p>
                                                {/if}
                                            </li>
                                        {/each}
                                    </ul>
                                    {#if sentencesByWord[word.id].length > sentenceLimit}
                                        <button
                                            type="button"
                                            class="view-full-sentences-btn"
                                            onclick={() => viewFullSentences(word)}
                                        >
                                            View all
                                        </button>
                                    {/if}
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
                    <div class="word-card sentence-card">
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
                                    <span class="tag-pill" style={tagColors[tag] ? `--tag-color: ${tagColors[tag]}` : ''}>
                                        #{tag}
                                    </span>
                                {/each}
                            </div>
                        {/if}
                
                        <button
                            type="button"
                            class="sentence-delete-btn"
                            onclick={() => handleDeleteSentence(sentence)}
                            title="Delete sentence"
                        >
                            {@html ICONS.trash}
                        </button>
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
                            <span class="lookup-badge-count">{item.count}</span>
                            <span class="lookup-badge-icon">{@html ICONS.magnify}</span>
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
    {:else if activeTab === 'review'}
        <div class="review-tab">
            <!-- Quick Start Action Cards -->
            <div class="review-actions-grid">
                <button type="button" class="word-card review-action-card" onclick={() => goto(`/review/word${mediaFilter ? '?media=' + mediaFilter : ''}`)}>
                    <div class="action-icon-wrap" style="--accent-color: {STAT_COLORS.wordCount}">
                        <span class="action-icon">{@html ICONS.book}</span>
                    </div>
                    <div class="action-details">
                        <span class="action-title">Word Review</span>
                        <span class="action-sub">Review words in dictionary</span>
                    </div>
                </button>

                <button type="button" class="word-card review-action-card">
                    <div class="action-icon-wrap" style="--accent-color: {STAT_COLORS.sentenceCount}">
                        <span class="action-icon">{@html ICONS.translate}</span>
                    </div>
                    <div class="action-details">
                        <span class="action-title">Sentence Review</span>
                        <span class="action-sub">Review full sentences</span>
                    </div>
                </button>

                <button type="button" class="word-card review-action-card">
                    <div class="action-icon-wrap" style="--accent-color: {STAT_COLORS.lastReview}">
                        <span class="action-icon">{@html ICONS.settings}</span>
                    </div>
                    <div class="action-details">
                        <span class="action-title">Custom Review</span>
                        <span class="action-sub">Override default review settings</span>
                    </div>
                </button>
            </div>

            <!-- Section Heading -->
            <h2 class="review-stats-heading">
                {mediaFilter
                    ? `Review stats for ${mediaOptions.find((m) => m.value === String(mediaFilter))?.label ?? 'this media'}`
                    : 'Review stats for all media'}
            </h2>

            <!-- Combined & Simplified Streak + Last Review Card -->
            <div class="word-card streak-review-card">
                <div class="streak-stat">
                    <span class="stat-icon fire-icon">{@html ICONS.fire}</span>
                    <div class="stat-info">
                        <span class="stat-value">{reviewStats.currentStreak} <span class="stat-unit">days</span></span>
                        <span class="stat-label">Current Streak</span>
                    </div>
                </div>

                <div class="stat-divider"></div>

                <div class="streak-stat">
                    <span class="stat-icon fire-icon-longest">{@html ICONS.trophy}</span>
                    <div class="stat-info">
                        <span class="stat-value">{reviewStats.longestStreak} <span class="stat-unit">days</span></span>
                        <span class="stat-label">Longest Streak</span>
                    </div>
                </div>

                <div class="stat-divider"></div>

                <div class="streak-stat">
                    <span class="stat-icon calendar-icon">{@html ICONS.calendar ?? ICONS.book}</span>
                    <div class="stat-info">
                        <span class="stat-value">{formatReviewDate(reviewStats.lastReviewDate)}</span>
                        <span class="stat-label">Last Reviewed</span>
                    </div>
                </div>
            </div>

            <!-- Stats Overview Grid: Visual Distribution & Metric Cards -->
            <div class="stats-overview-grid">
                <!-- Visual Breakdown Bars -->
                <div class="word-card breakdown-panel">
                    <h3 class="panel-title">Review Breakdown</h3>

                    <div class="dist-group">
                        <div class="dist-meta">
                            <span class="dist-name">Total Reviews ({reviewTotals.totalReviews})</span>
                            <span class="dist-legend">
                                <span class="legend-dot word"></span> Words ({Math.round(reviewTotals.wordReviewPct)}%)
                                <span class="legend-dot sentence"></span> Sentences ({Math.round(reviewTotals.sentenceReviewPct)}%)
                            </span>
                        </div>
                        <div class="dist-bar-track">
                            <div class="dist-bar-fill word-fill" style="width: {reviewTotals.wordReviewPct}%"></div>
                            <div class="dist-bar-fill sentence-fill" style="width: {reviewTotals.sentenceReviewPct}%"></div>
                        </div>
                    </div>

                    <div class="dist-group">
                        <div class="dist-meta">
                            <span class="dist-name">Time Spent ({formatDuration(reviewTotals.totalTime)})</span>
                            <span class="dist-legend">
                                <span class="legend-dot word"></span> Words ({Math.round(reviewTotals.wordTimePct)}%)
                                <span class="legend-dot sentence"></span> Sentences ({Math.round(reviewTotals.sentenceTimePct)}%)
                            </span>
                        </div>
                        <div class="dist-bar-track">
                            <div class="dist-bar-fill word-fill" style="width: {reviewTotals.wordTimePct}%"></div>
                            <div class="dist-bar-fill sentence-fill" style="width: {reviewTotals.sentenceTimePct}%"></div>
                        </div>
                    </div>
                </div>

                <!-- Metrics Grid -->
                <div class="metrics-quad-grid">
                    <div class="word-card metric-card">
                        <div class="metric-icon-box" style="--icon-color: {STAT_COLORS.wordCount}">
                            {@html ICONS.book}
                        </div>
                        <div class="metric-body">
                            <span class="metric-value">{reviewStats.wordReviewCount}</span>
                            <span class="metric-label">Words Reviewed</span>
                        </div>
                    </div>

                    <div class="word-card metric-card">
                        <div class="metric-icon-box" style="--icon-color: {STAT_COLORS.sentenceCount}">
                            {@html ICONS.translate}
                        </div>
                        <div class="metric-body">
                            <span class="metric-value">{reviewStats.sentenceReviewCount}</span>
                            <span class="metric-label">Sentences Reviewed</span>
                        </div>
                    </div>

                    <div class="word-card metric-card">
                        <div class="metric-icon-box" style="--icon-color: {STAT_COLORS.wordTime}">
                            {@html ICONS.hourglass ?? ICONS.stats}
                        </div>
                        <div class="metric-body">
                            <span class="metric-value">{formatDuration(reviewStats.wordTimeSeconds)}</span>
                            <span class="metric-label">Time on Words</span>
                        </div>
                    </div>

                    <div class="word-card metric-card">
                        <div class="metric-icon-box" style="--icon-color: {STAT_COLORS.sentenceTime}">
                            {@html ICONS.hourglass ?? ICONS.stats}
                        </div>
                        <div class="metric-body">
                            <span class="metric-value">{formatDuration(reviewStats.sentenceTimeSeconds)}</span>
                            <span class="metric-label">Time on Sentences</span>
                        </div>
                    </div>
                </div>
            </div>
        </div>
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
        transition: transform 0.15s ease, box-shadow 0.15s ease, border-color 0.15s ease, background 0.15s ease;
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
        gap: 0.4rem;
        font-size: 0.85rem;
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

    .view-full-sentences-btn {
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        font-size: 0.78rem;
        color: var(--theme-textSecondary, #b3b3b3);
        cursor: pointer;
        text-decoration: underline;
        text-underline-offset: 2px;
        margin-top: 0.5rem;
        transition: color 0.15s ease;
    }

    .view-full-sentences-btn:hover {
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

    .sentence-delete-btn {
        font-family: "Symbols Nerd Font";
        position: absolute;
        bottom: 1.2rem;
        right: 0.9rem;
        background: none;
        border: none;
        padding: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 1.1rem;
        height: 1.1rem;
        color: var(--theme-textSecondary, #b3b3b3);
        cursor: pointer;
        opacity: 0;
        transition: opacity 0.15s ease, color 0.15s ease;
    }
    
    .sentence-delete-btn :global(svg) {
        width: 100%;
        height: 100%;
    }
    
    .sentence-card:hover .sentence-delete-btn {
        opacity: 1;
    }
    
    .sentence-delete-btn:hover {
        color: #f38ba8;
    }

    .word-delete-btn {
        font-family: "Symbols Nerd Font";
        background: none;
        border: none;
        padding: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 0.85rem;
        width: 1rem;
        height: 1rem;
        color: var(--theme-textSecondary, #b3b3b3);
        cursor: pointer;
        opacity: 0;
        transform: translateY(-1px);
        transition: opacity 0.15s ease, color 0.15s ease;
    }
    
    .word-delete-btn :global(svg) {
        width: 100%;
        height: 100%;
    }
    
    .word-card:hover .word-delete-btn {
        opacity: 1;
    }
    
    .word-delete-btn:hover {
        color: #f38ba8;
    }

    .notes-edit-row {
        display: flex;
        align-items: center;
        gap: 0.3rem;
        padding: 0.2rem 0.2rem;
        border-radius: 8px;
    }
    
    .notes-icon {
        font-family: "Symbols Nerd Font";
        font-size: 0.9rem;
        display: flex;
        align-items: center;
        flex-shrink: 0;
        width: 0.85rem;
        height: 0.85rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }
    
    .notes-icon :global(svg) {
        width: 100%;
        height: 100%;
    }
    
    .notes-input {
        flex: 1;
        min-width: 0;
        background: none;
        border: none;
        outline: none;
        font: inherit;
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }
    
    .notes-input::placeholder {
        color: var(--theme-textSecondary, #b3b3b3);
        opacity: 0.6;
    }

    /* ==========================================================================
       REVIEW TAB STYLES
       ========================================================================== */
    
    .review-tab {
        display: flex;
        flex-direction: column;
        gap: 1rem;
        max-width: 1200px;
        padding-bottom: 2rem;
    }

    /* Action Cards */
    .review-actions-grid {
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
        gap: 1rem;
    }

    .review-action-card {
        display: flex;
        align-items: center;
        gap: 1.2rem;
        color: var(--theme-text, #f6f6f6);
        text-align: left;
        cursor: pointer;
    }

    .review-action-card:hover {
        border-color: var(--theme-primary, #36b7bd);
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 8%, var(--theme-surface, #2d2d2d));
    }

    .action-icon-wrap {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 3rem;
        height: 3rem;
        flex-shrink: 0;
        border-radius: 12px;
        background: color-mix(in srgb, var(--accent-color, #36b7bd) 15%, transparent);
        color: var(--accent-color, #36b7bd);
    }

    .action-icon {
        font-family: "Symbols Nerd Font";
        font-size: 1.6rem;
        display: flex;
        width: 1.32rem;
        height: 1.6rem;
    }

    .action-icon :global(svg) {
        width: 100%;
        height: 100%;
    }

    .action-details {
        display: flex;
        flex-direction: column;
        gap: 0.2rem;
    }

    .action-title {
        font-size: 1.05rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
    }

    .action-sub {
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    /* Review Stats Heading */
    .review-stats-heading {
        font-size: 1.2rem;
        font-weight: 700;
        margin: 0.5rem 0 0 0;
        color: var(--theme-text, #f6f6f6);
    }

    /* Streak & Last Review Card */
    .streak-review-card {
        display: flex;
        align-items: center;
        justify-content: space-around;
        gap: 1.5rem;
        padding: 1.25rem 2rem;
        flex-wrap: wrap;
    }

    .streak-review-card .streak-stat {
        display: flex;
        align-items: center;
        gap: 1rem;
    }

    .streak-stat .stat-icon {
        font-family: "Symbols Nerd Font";
        display: flex;
        align-items: center;
        justify-content: center;
        width: 2.5rem;
        height: 2.5rem;
        flex-shrink: 0;
        border-radius: 100px;
    }

    .action-icon-wrap :global(svg),
    .metric-icon-box :global(svg),
    .streak-stat .stat-icon :global(svg) {
        width: 1.25rem;
        height: 1.25rem;
    }

    .streak-review-card .stat-icon :global(svg) {
        width: 100%;
        height: 100%;
    }

    .streak-review-card .stat-icon {
        background: color-mix(in srgb, var(--icon-color, #36b7bd) 14%, transparent);
        color: var(--icon-color, #36b7bd);
    }
    
    .streak-review-card .stat-icon.fire-icon {
        font-size: 1.6rem;
        --icon-color: #f38ba8;
    }
    
    .streak-review-card .stat-icon.fire-icon-longest {
        font-size: 1.3rem;
        --icon-color: #fab387;
    }
    
    .streak-review-card .stat-icon.calendar-icon {
        font-size: 1.1rem;
        --icon-color: #cba6f7;
    }

    .streak-review-card .stat-info {
        display: flex;
        flex-direction: column;
    }

    .streak-review-card .stat-value {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
        line-height: 1.2;
        font-variant-numeric: tabular-nums;
    }

    .streak-review-card .stat-unit {
        font-size: 0.8rem;
        font-weight: 500;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .streak-review-card .stat-label {
        font-size: 0.65rem;
        font-weight: 600;
        color: var(--theme-textSecondary, #b3b3b3);
        text-transform: uppercase;
        letter-spacing: 0.04em;
    }

    .streak-review-card .stat-divider {
        width: 1px;
        height: 2rem;
        background: color-mix(in srgb, var(--theme-border, #404040) 60%, transparent);
    }

    @media (max-width: 800px) {
        .streak-review-card .stat-divider {
            display: none;
        }
    }

    /* Stats Overview Grid */
    .stats-overview-grid {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 1rem;
    }

    @media (max-width: 900px) {
        .stats-overview-grid {
            grid-template-columns: 1fr;
        }
    }

    /* Breakdown Panel */
    .breakdown-panel {
        display: flex;
        flex-direction: column;
        gap: 1.15rem;
    }

    .panel-title {
        font-size: 01em;
        font-weight: 700;
        margin: 0;
        color: var(--theme-text, #f6f6f6);
    }

    .dist-group {
        display: flex;
        flex-direction: column;
        gap: 0.4rem;
    }

    .dist-meta {
        display: flex;
        justify-content: space-between;
        align-items: center;
        font-size: 0.78rem;
    }

    .dist-name {
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
    }

    .dist-legend {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.72rem;
    }

    .legend-dot {
        display: inline-block;
        width: 8px;
        height: 8px;
        border-radius: 50%;
        margin-right: 0.2rem;
    }

    .legend-dot.word, .dist-bar-fill.word-fill { background: #36b7bd; }
    .legend-dot.sentence, .dist-bar-fill.sentence-fill { background: #89b4fa; }

    .dist-bar-track {
        display: flex;
        height: 10px;
        width: 100%;
        border-radius: 999px;
        background: color-mix(in srgb, var(--theme-border, #404040) 40%, transparent);
        overflow: hidden;
    }

    .dist-bar-fill {
        height: 100%;
        transition: width 0.3s ease;
    }

    /* Metrics Quad Grid */
    .metrics-quad-grid {
        display: grid;
        grid-template-columns: repeat(2, 1fr);
        gap: 0.75rem;
    }

    .metric-card {
        display: flex;
        align-items: center;
        gap: 1rem;
    }

    .metric-icon-box {
        font-family: "Symbols Nerd Font";
        display: flex;
        align-items: center;
        justify-content: center;
        width: 2.5rem;
        height: 2.5rem;
        flex-shrink: 0;
        border-radius: 10px;
        background: color-mix(in srgb, var(--icon-color, #36b7bd) 14%, transparent);
        color: var(--icon-color, #36b7bd);
        font-size: 1.2rem;
    }

    .metric-icon-box :global(svg) {
        width: 1.1rem;
        height: 1.1rem;
    }

    .metric-body {
        display: flex;
        flex-direction: column;
        gap: 0.1rem;
    }

    .metric-value {
        font-size: 1.5rem;
        font-weight: 800;
        color: var(--theme-text, #f6f6f6);
        line-height: 1.1;
        font-variant-numeric: tabular-nums;
    }

    .metric-label {
        font-size: 0.72rem;
        font-weight: 600;
        color: var(--theme-textSecondary, #b3b3b3);
    }
</style>