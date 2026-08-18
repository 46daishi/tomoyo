<script>
    import { onMount } from 'svelte';
    import { page } from '$app/state';
    import { getDb, coverSrc } from '$lib/db';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import SelectInput from '$lib/components/SelectInput.svelte';
    import { ICONS } from '$lib/icons';
    import { goto } from '$app/navigation';

    let mediaFilter = $state(/** @type {number | null} */ (null));
    let mediaOptions = $state(/** @type {Array<{ value: string, label: string }>} */ ([{ value: '', label: 'All media' }]));
    let sessions = $state(/** @type {Array<Record<string, any>>} */ ([]));
    let loaded = $state(false);
    let expandedWords = $state(new Set());

    let requestId = 0;

    async function loadMediaOptions() {
        const db = await getDb();
        const rows = await db.select('SELECT id, title FROM media ORDER BY title');
        mediaOptions = [{ value: '', label: 'All media' }, ...rows.map((m) => ({ value: String(m.id), label: m.title }))];
    }

    async function loadSessions(filter) {
        const my = ++requestId;
        const db = await getDb();
        const rows = await db.select(
            `SELECT s.id, s.media_id, s.started_at, s.ended_at, s.last_updated_at, s.moji_read, s.sentences_read,
                    m.title AS media_title, m.cover_path AS media_cover, m.color AS media_color,
                    (SELECT COUNT(DISTINCT ws.word_id) FROM word_sentences ws WHERE ws.session_id = s.id) AS words_mined,
                    (SELECT GROUP_CONCAT(spelling, '\x1f')
                     FROM (SELECT DISTINCT w.spelling AS spelling
                           FROM word_sentences ws
                           JOIN words w ON w.id = ws.word_id
                           WHERE ws.session_id = s.id)) AS mined_spellings
             FROM sessions s
             LEFT JOIN media m ON m.id = s.media_id
             WHERE $1 IS NULL OR s.media_id = $1
             ORDER BY s.started_at DESC`,
            [filter]
        );
        if (requestId !== my) return;
        sessions = rows.map((row) => ({
            ...row,
            minedWords: row.mined_spellings ? row.mined_spellings.split('\x1f') : [],
        }));
        loaded = true;
    }

    $effect(() => {
        loadSessions(mediaFilter);
    });

    onMount(() => {
        loadMediaOptions();
        const initial = page.url.searchParams.get('media');
        if (initial) mediaFilter = Number(initial) || null;
    });

    /** @param {number} unixSeconds */
    function formatDate(unixSeconds) {
        return new Date(unixSeconds * 1000).toLocaleDateString('ja-JP', { year: 'numeric', month: 'short', day: 'numeric' });
    }

    /** @param {number} unixSeconds */
    function formatTime(unixSeconds) {
        return new Date(unixSeconds * 1000).toLocaleTimeString('ja-JP', { hour: '2-digit', minute: '2-digit' });
    }

    /** @param {number | null} start @param {number | null} end @param {number | null} lastUpdated */
    function formatDuration(start, end, lastUpdated) {
        if (!start) return '—';
        const seconds = Math.max(0, (end ?? lastUpdated ?? start) - start);
        const h = Math.floor(seconds / 3600);
        const m = Math.floor((seconds % 3600) / 60);
        return h > 0 ? `${h}h ${m}m` : `${m}m`;
    }

    function toggleWords(sessionId) {
        const next = new Set(expandedWords);
        if (next.has(sessionId)) next.delete(sessionId);
        else next.add(sessionId);
        expandedWords = next;
    }
</script>

<main class="log-page">
    <header class="log-header">
        <ActionButton icon={ICONS.back} variant="primary" size="small" onAction={() => history.back()} />
        <h1>Log</h1>
        <SelectInput
            options={mediaOptions}
            value={mediaFilter ? String(mediaFilter) : ''}
            on:change={(e) => {
                const v = e.target.value;
                mediaFilter = v ? Number(v) : null;
            }}
        />
    </header>

    {#if !loaded}
        <p class="empty-notice">Loading sessions...</p>
    {:else if sessions.length === 0}
        <p class="empty-notice">No reading sessions yet.</p>
    {:else}
        <div class="session-grid">
            {#each sessions as session (session.id)}
                {@const cover = session.media_cover ? coverSrc(session.media_cover) : null}
                {@const wordList = session.minedWords}
                <div class="session-card" style={`--accent: ${session.media_color ?? '#89b4fa'}`}>
                    <div class="session-head">
                        <div class="session-cover-wrap">
                            {#if cover}
                                <img class="session-cover" src={cover} alt={session.media_title ?? ''} />
                            {:else}
                                <div class="session-cover session-cover-placeholder" aria-hidden="true"></div>
                            {/if}
                        </div>
                        <div class="session-info">
                            <div class="session-media-title">
                                {#if session.media_id}
                                    <button type="button" class="media-link" onclick={() => goto(`/media/${session.media_id}`)}>
                                        {session.media_title ?? `Media #${session.media_id}`}
                                    </button>
                                {:else}
                                    <span class="no-media">No media</span>
                                {/if}
                            </div>
                            <div class="session-meta">
                                <span class="session-date"><span class="meta-icon">{ICONS.calendar}</span>{formatDate(session.started_at)} · {formatTime(session.started_at)}</span>
                                <span class="session-duration"><span class="meta-icon">{ICONS.clock}</span>{formatDuration(session.started_at, session.ended_at, session.last_updated_at)}</span>
                            </div>
                            <div class="session-stats">
                                <span class="stat">
                                    <span class="stat-icon" style="--icon-color: #89b4fa">{ICONS.book_open}</span>
                                    <span class="stat-value">{session.moji_read.toLocaleString()}</span> 文字 read
                                </span>
                                <span class="stat">
                                    <span class="stat-icon" style="--icon-color: #a6e3a1">{ICONS.translate}</span>
                                    <span class="stat-value">{session.sentences_read.toLocaleString()}</span> sentences
                                </span>
                                {#if wordList.length > 0}
                                    <button
                                        type="button"
                                        class="stat stat-btn"
                                        onclick={() => toggleWords(session.id)}
                                        title={expandedWords.has(session.id) ? 'Hide mined words' : 'Show mined words'}
                                    >
                                        <span class="stat-icon" style="--icon-color: #cba6f7">{ICONS.book}</span>
                                        <span class="stat-value">{session.words_mined.toLocaleString()}</span> words mined
                                    </button>
                                {:else}
                                    <span class="stat">
                                        <span class="stat-icon" style="--icon-color: #cba6f7">{ICONS.book}</span>
                                        <span class="stat-value">0</span> words mined
                                    </span>
                                {/if}
                            </div>
                        </div>
                    </div>

                    {#if expandedWords.has(session.id) && wordList.length > 0}
                        <div class="session-words">
                            <div class="words-head">
                                <span class="words-label">Words mined ({wordList.length})</span>
                            </div>
                            <div class="words-chips">
                                {#each wordList as word, i (word + i)}
                                    <span class="word-chip">{word}</span>
                                {/each}
                            </div>
                        </div>
                    {/if}
                </div>
            {/each}
        </div>
    {/if}
</main>

<style>
    .log-page {
        box-sizing: border-box;
        height: 100vh;
        overflow: hidden;
        display: flex;
        flex-direction: column;
        min-height: 0;
        width: 100%;
        max-width: 1100px;
        margin-inline: auto;
        padding: 2rem;
        gap: 1rem;
    }

    .log-header {
        display: flex;
        align-items: center;
        gap: 0.75rem;
    }

    .log-header h1 {
        margin: 0;
        font-size: 1.5rem;
        flex: 1;
    }

    .empty-notice {
        text-align: center;
        color: var(--theme-textSecondary, #b3b3b3);
        font-style: italic;
        padding: 3rem 0;
    }

    .session-grid {
        flex: 1;
        min-height: 0;
        overflow-y: scroll;
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
        padding-bottom: 1rem;
        padding-right: 1rem;
    }

    .session-grid::-webkit-scrollbar {
        width: 6px;
    }

    .session-grid::-webkit-scrollbar-track {
        background: transparent;
    }

    .session-grid::-webkit-scrollbar-thumb {
        background: var(--theme-border, #404040);
        border-radius: 3px;
    }

    .session-grid::-webkit-scrollbar-thumb:hover {
        background: var(--theme-textSecondary, #b3b3b3);
    }

    .session-card {
        position: relative;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-left: 4px solid var(--accent, #89b4fa);
        border-radius: 12px;
        padding: 1rem 1.25rem;
        display: flex;
        flex-direction: column;
        gap: 0.85rem;
        transition: transform 0.15s ease, box-shadow 0.15s ease, border-color 0.15s ease, background 0.15s ease;
        will-change: transform;
        transform: translateZ(0);
    }

    .session-card:hover {
        transform: translateY(-3px);
        box-shadow: 0 10px 22px rgba(0, 0, 0, 0.35);
    }

    .session-head {
        display: grid;
        grid-template-columns: auto 1fr;
        align-items: center;
        gap: 1rem;
    }

    .session-cover-wrap {
        position: relative;
        aspect-ratio: 2 / 3;
        width: 80px;
        flex-shrink: 0;
        border-radius: 8px;
        overflow: hidden;
        background: var(--surface1, #313244);
    }

    .session-cover {
        width: 100%;
        height: 100%;
        object-fit: cover;
        display: block;
    }

    .session-cover-placeholder {
        background: color-mix(in srgb, var(--accent, #89b4fa) 25%, var(--theme-surface, #2d2d2d));
    }

    .session-info {
        display: flex;
        flex-direction: column;
        gap: 0.4rem;
        min-width: 0;
    }

    .media-link {
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        font-size: 1.1rem;
        font-weight: 600;
        color: var(--accent, #89b4fa);
        cursor: pointer;
        text-align: left;
    }
    .media-link:hover {
        text-decoration: underline;
    }

    .no-media {
        font-size: 1rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .session-meta {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 0.75rem;
    }

    .session-date,
    .session-duration {
        display: inline-flex;
        align-items: center;
        gap: 0.3rem;
        font-size: 0.85rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .meta-icon {
        font-family: "Symbols Nerd Font";
        color: var(--accent, #89b4fa);
    }

    .session-stats {
        display: flex;
        flex-wrap: wrap;
        align-items: baseline;
        gap: 0.25rem 1rem;
        font-family: "Noto Sans JP", Inter, sans-serif;
        font-size: 0.88rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .stat {
        display: inline-flex;
        align-items: baseline;
        gap: 0.3rem;
    }

    .stat-btn {
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        color: inherit;
        cursor: pointer;
        text-decoration: underline;
        text-underline-offset: 2px;
        transition: color 0.15s ease;
    }

    .stat-btn:hover {
        color: var(--theme-text, #f6f6f6);
    }

    .stat-icon {
        font-family: "Symbols Nerd Font";
        font-weight: normal;
        color: var(--icon-color, #36b7bd);
        font-size: 0.85rem;
    }

    .stat-value {
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
    }

    .session-words {
        margin-top: 0.25rem;
        padding-top: 0.75rem;
        border-top: 1px solid color-mix(in srgb, var(--theme-border, #404040) 50%, transparent);
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .words-head {
        display: flex;
        align-items: center;
        justify-content: space-between;
    }

    .words-label {
        font-size: 0.75rem;
        font-weight: 600;
        color: var(--theme-textSecondary, #b3b3b3);
        text-transform: uppercase;
        letter-spacing: 0.04em;
    }

    .words-chips {
        display: flex;
        flex-wrap: wrap;
        gap: 0.35rem;
    }

    .word-chip {
        font-size: 0.82rem;
        font-weight: 500;
        background: color-mix(in srgb, var(--accent, #89b4fa) 12%, transparent);
        border: 1px solid color-mix(in srgb, var(--accent, #89b4fa) 28%, transparent);
        color: var(--theme-text, #f6f6f6);
        padding: 0.2rem 0.55rem;
        border-radius: 6px;
    }
</style>