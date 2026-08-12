<script>
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { page } from '$app/state';
    import { getDb, coverSrc } from '$lib/db';
    import { loadSettings } from '$lib/settings.js';
    import { confirm } from '@tauri-apps/plugin-dialog';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import SelectInput from '$lib/components/SelectInput.svelte';
    import HeatMap from '$lib/components/HeatMap.svelte';
    import BarChart from '$lib/components/BarChart.svelte';
    import LineChart from '$lib/components/LineChart.svelte';
    import DonutChart from '$lib/components/DonutChart.svelte';
    import { ICONS } from '$lib/icons';
    import { STATUS_LEVELS, STATUS_COLORS } from '$lib/constants.js';
    import { formatMinutes } from '$lib/utils/chartFormatters.js';
    import { getMediaStats } from '$lib/sessions.js';
    import { getVocabularyCoverage } from '$lib/coverage.js';
    import { getVn, formatVnLength, formatVnRating, vndbUrl } from '$lib/vndb.js';
    import { openUrl } from '@tauri-apps/plugin-opener';
    import {
        getReadingStats, getReadingStreak, getActivityYears, getMojiActivityByYear,
        getDailyMoji, getMonthlyMoji, getWordsMinedByDay, getWordStatusCounts,
        getProfileStats, getVocabularyGrowth, getReviewActivityByDay, getMediaBreakdown,
        clearStatistics, TIMEFRAME_OPTIONS,
    } from '$lib/statsPage.js';

    let timeframe = $state('all');
    let mediaFilter = $state(/** @type {number | null} */ (null));
    let mediaOptions = $state(/** @type {Array<{ value: string, label: string }>} */ ([{ value: '', label: 'All media' }]));
    let settings = $state(/** @type {Record<string, any> | null} */ (null));
    let mediaInfo = $state(/** @type {Record<string, any> | null} */ (null));
    let vnInfo = $state(/** @type {Record<string, any> | null} */ (null));
    let vnInfoRequestId = 0;

    // Native state initialized as provided
    let stats = $state({
        totalMoji: 0, totalSentences: 0, totalSeconds: 0, lastRead: /** @type {number | null} */ (null),
        sessionCount: 0, mojiPerHour: 0, sentencesPerHour: 0, avgSentenceLength: 0,
    });
    let streak = $state({ currentStreak: 0, longestStreak: 0 });
    let heatmapYear = $state(new Date().getFullYear());
    let yearOptions = $state(/** @type {Array<{ value: string, label: string }>} */ ([]));
    let yearActivity = $state(/** @type {Array<{ date: string, studyMinutes: number }>} */ ([]));
    let dailyMoji = $state(/** @type {Array<Record<string, any>>} */ ([]));
    let monthlyMoji = $state(/** @type {Array<{ key: string, moji: number, minutes: number }>} */ ([]));
    let mediaStats = $state(/** @type {Record<string, any> | null} */ (null));
    let vocabCoverage = $state(/** @type {Record<string, any> | null} */ (null));
    let wordsMinedDaily = $state(/** @type {Array<Record<string, any>>} */ ([]));
    let wordStatusCounts = $state(/** @type {Array<{ status: number, count: number }>} */ ([]));
    let profileStats = $state({ mediaCount: 0, firstUsed: /** @type {number | null} */ (null), wordCount: 0, sentenceCount: 0 });
    let vocabGrowth = $state(/** @type {Array<Record<string, any>>} */ ([]));
    let reviewActivity = $state(/** @type {Array<Record<string, any>>} */ ([]));
    let mediaBreakdown = $state(/** @type {Array<{ id: number, title: string, color: string | null, coverPath: string | null, totalMoji: number, totalSeconds: number, sessionCount: number }>} */ ([]));

    async function loadMediaOptions() {
        const db = await getDb();
        const rows = await db.select('SELECT id, title FROM media ORDER BY title');
        mediaOptions = [{ value: '', label: 'All media' }, ...rows.map((/** @type {any} */ m) => ({ value: String(m.id), label: m.title }))];
    }

    // Guards each media-dependent async loader so a slow response for a
    // previously selected media can't overwrite the current one (which made
    // the heatmap sometimes keep the previous media's colour).
    function makeLatest() {
        let last = 0;
        return {
            next: () => ++last,
            get: () => last,
        };
    }
    const latestInfo = makeLatest();
    const latestStats = makeLatest();
    const latestYearActivity = makeLatest();
    const latestYears = makeLatest();
    const latestMeta = makeLatest();
    const latestCharts = makeLatest();
    const latestGrowth = makeLatest();

    async function loadMediaInfo() {
        const my = latestInfo.next();
        if (!mediaFilter) {
            mediaInfo = null;
            return;
        }
        const db = await getDb();
        const rows = await db.select('SELECT id, title, color, cover_path, status, tag, vndb_id, created_at FROM media WHERE id = $1', [mediaFilter]);
        if (latestInfo.get() === my) mediaInfo = rows[0] ?? null;
    }

    async function loadVnInfo() {
        const id = mediaInfo?.vndb_id;
        const my = ++vnInfoRequestId;
        if (!id) {
            vnInfo = null;
            return;
        }
        try {
            const vn = await getVn(id);
            if (vnInfoRequestId === my) vnInfo = vn;
        } catch (err) {
            console.error('VNDB fetch failed:', err);
            if (vnInfoRequestId === my) vnInfo = null;
        }
    }

    /** @param {string} id */
    async function openVndb(id) {
        const url = vndbUrl(id);
        if (url) await openUrl(url);
    }

    async function loadStats() {
        const my = latestStats.next();
        const next = await getReadingStats({ mediaId: mediaFilter, timeframe });
        if (latestStats.get() !== my) return;
        stats = next;
        const [nextStreak, nextDaily, nextMonthly] = await Promise.all([
            getReadingStreak(mediaFilter),
            getDailyMoji(mediaFilter, 30),
            getMonthlyMoji(mediaFilter, 12),
        ]);
        if (latestStats.get() !== my) return;
        streak = nextStreak;
        dailyMoji = nextDaily;
        monthlyMoji = nextMonthly;
    }

    async function loadYearActivity() {
        const my = latestYearActivity.next();
        const next = await getMojiActivityByYear(mediaFilter, heatmapYear);
        if (latestYearActivity.get() === my) yearActivity = next;
    }

    async function loadActivityYears() {
        const my = latestYears.next();
        const years = await getActivityYears(mediaFilter);
        if (latestYears.get() !== my) return;
        yearOptions = years.map((y) => ({ value: String(y), label: `${y}年` }));
        if (!years.includes(heatmapYear)) {
            heatmapYear = years[0] ?? new Date().getFullYear();
        }
    }

    async function loadMediaMeta() {
        const my = latestMeta.next();
        if (!mediaFilter) {
            mediaStats = null;
            vocabCoverage = null;
            return;
        }
        const stats = await getMediaStats(mediaFilter);
        const coverage = settings?.estimate_coverage ? await getVocabularyCoverage(mediaFilter) : null;
        if (latestMeta.get() === my) {
            mediaStats = stats;
            vocabCoverage = coverage;
        }
    }

    async function loadMediaCharts() {
        const my = latestCharts.next();
        const words = await getWordsMinedByDay(mediaFilter, 30);
        const statuses = await getWordStatusCounts(mediaFilter);
        if (latestCharts.get() === my) {
            wordsMinedDaily = words;
            wordStatusCounts = statuses;
        }
    }

    async function loadGrowthCharts() {
        const my = latestGrowth.next();
        const [growth, reviews, breakdown] = await Promise.all([
            getVocabularyGrowth(mediaFilter),
            getReviewActivityByDay(mediaFilter, 30),
            getMediaBreakdown(timeframe),
        ]);
        if (latestGrowth.get() === my) {
            vocabGrowth = growth;
            reviewActivity = reviews;
            mediaBreakdown = breakdown;
        }
    }

    $effect(() => {
        timeframe;
        mediaFilter;
        loadStats();
    });

    $effect(() => {
        mediaFilter;
        settings?.estimate_coverage;
        loadMediaMeta();
    });

    $effect(() => {
        mediaFilter;
        loadMediaCharts();
    });

    $effect(() => {
        mediaFilter;
        timeframe;
        loadGrowthCharts();
    });

    $effect(() => {
        mediaFilter;
        loadActivityYears();
    });

    $effect(() => {
        mediaFilter;
        heatmapYear;
        loadYearActivity();
    });

    $effect(() => {
        mediaFilter;
        loadMediaInfo();
    });

    $effect(() => {
        mediaInfo?.vndb_id;
        loadVnInfo();
    });

    onMount(async () => {
        const mediaParam = Number(page.url.searchParams.get('media'));
        if (mediaParam && Number.isFinite(mediaParam)) {
            mediaFilter = mediaParam;
        }
        loadMediaOptions();
        settings = await loadSettings();
        profileStats = await getProfileStats();
    });

    /** @param {Event} e */
    function handleMediaFilterChange(e) {
        const el = /** @type {HTMLSelectElement} */ (e.currentTarget);
        mediaFilter = el.value ? Number(el.value) : null;
    }
    /** @param {Event} e */
    function handleTimeframeChange(e) {
        const el = /** @type {HTMLSelectElement} */ (e.currentTarget);
        timeframe = el.value;
    }
    async function handleClearStatistics() {
        const scopeLabel = mediaFilter
            ? mediaOptions.find((m) => m.value === String(mediaFilter))?.label ?? 'this media'
            : 'all media';
        const yes = await confirm(
            `Clear all reading, review and lookup statistics for ${scopeLabel}? This cannot be undone.`,
            { title: 'Clear statistics', kind: 'warning' }
        );
        if (!yes) return;
        await clearStatistics(mediaFilter);
        await Promise.all([loadStats(), loadMediaCharts(), loadGrowthCharts(), loadActivityYears(), loadYearActivity()]);
    }
    /** @param {Event} e */
    function handleYearChange(e) {
        const el = /** @type {HTMLSelectElement} */ (e.currentTarget);
        heatmapYear = el.value ? Number(el.value) : new Date().getFullYear();
    }
    /** @param {number} totalSeconds */
    function formatDuration(totalSeconds) {
        const h = Math.floor(totalSeconds / 3600);
        const m = Math.floor((totalSeconds % 3600) / 60);
        return h > 0 ? `${h}h ${m}m` : `${m}m`;
    }
    /** @param {number | null} unixSeconds */
    function formatLastRead(unixSeconds) {
        if (!unixSeconds) return 'Never';
        return new Date(unixSeconds * 1000).toLocaleDateString("ja-JP", { month: 'short', day: 'numeric', year: 'numeric' });
    }
    /** @param {number | null | undefined} unixSeconds */
    function formatMediaDate(unixSeconds) {
        if (!unixSeconds) return '—';
        return new Date(unixSeconds * 1000).toLocaleDateString('ja-JP', { year: 'numeric', month: 'short', day: 'numeric' });
    }
    /** @param {number | null | undefined} unixSeconds */
    function formatFirstUsed(unixSeconds) {
        if (!unixSeconds) return '—';
        return new Date(unixSeconds * 1000).toLocaleDateString('ja-JP', { year: 'numeric', month: 'short', day: 'numeric' });
    }
    /** @param {string} key */
    function formatDayLabel(key) {
        return new Date(key + 'T00:00:00').toLocaleDateString("ja-JP", { month: 'short', day: 'numeric' });
    }
    /** @param {string} key */
    function formatMonthLabel(key) {
        const [y, m] = key.split('-');
        return new Date(Number(y), Number(m) - 1, 1).toLocaleDateString("ja-JP", { month: 'short', year: 'numeric' });
    }
    /** @param {number} v */
    function formatMoji(v) {
        return `${v.toLocaleString()} 文字`;
    }
    /** @param {number} v */
    function formatWords(v) {
        return `${v.toLocaleString()} words`;
    }
    /** @param {number} v */
    function formatReviews(v) {
        return `${v.toLocaleString()} reviews`;
    }
    /** @param {number} seconds */
    function formatBreakdownTime(seconds) {
        const h = Math.floor(seconds / 3600);
        const m = Math.floor((seconds % 3600) / 60);
        return h > 0 ? `${h}h ${m}m` : `${m}m`;
    }

    let subtitle = $derived.by(() => {
        const tf = TIMEFRAME_OPTIONS.find((o) => o.value === timeframe)?.label ?? 'All time';
        const media = mediaOptions.find((m) => m.value === String(mediaFilter))?.label;
        return media && media !== 'All media' ? `${tf} · ${media}` : tf;
    });

    let coverSrcValue = $derived(mediaInfo?.cover_path ? coverSrc(mediaInfo.cover_path) : null);
    let profilePicSrc = $derived(settings?.profile_picture ? coverSrc(settings.profile_picture) : null);
    let username = $derived((settings?.username ?? 'Reader').trim());
    let initials = $derived((username.charAt(0) || 'R').toUpperCase());

    let donutData = $derived(
        STATUS_LEVELS.map((lvl, status) => {
            const entry = wordStatusCounts.find((e) => e.status === status);
            return { label: lvl.label, value: entry?.count ?? 0, color: lvl.color };
        })
    );

    let breakdownMaxMoji = $derived(Math.max(...mediaBreakdown.map((m) => m.totalMoji), 1));

    let tiles = $derived([
        {
            icon: ICONS.book_open,
            color: '#89b4fa',
            value: `${stats.totalMoji.toLocaleString()} 文字`,
            meta: `${stats.totalSentences.toLocaleString()} sentences read`,
        },
        {
            icon: ICONS.clock,
            color: '#89dceb',
            value: formatDuration(stats.totalSeconds),
            meta: `across ${stats.sessionCount.toLocaleString()} sessions`,
        },
        {
            icon: ICONS.fire2,
            color: '#f38ba8',
            value: `${streak.currentStreak}日 streak`,
            meta: `best ${streak.longestStreak}日`,
        },
        {
            icon: ICONS.calendar2,
            color: '#cba6f7',
            value: formatLastRead(stats.lastRead),
            meta: `last reading session`,
        },
    ]);
</script>

<main class="analytics-layout">
    <header class="layout-header">
        <div class="title-group">
            <ActionButton icon={ICONS.back} variant="primary" size="small" onAction={() => history.back()} />
            <div>
                <h1>Your Stats</h1>
                <p class="subtitle">{subtitle}</p>
            </div>
        </div>
        <div class="filter-group">
            <SelectInput options={TIMEFRAME_OPTIONS} value={timeframe} on:change={handleTimeframeChange} />
            <SelectInput options={mediaOptions} value={mediaFilter ? String(mediaFilter) : ''} on:change={handleMediaFilterChange} />
            <ActionButton
                icon={ICONS.trash}
                label="Clear stats"
                variant="danger"
                size="small"
                onAction={handleClearStatistics}
            />
        </div>
    </header>

    <!-- Tier 1: Headline stats -->
    <section class="stat-grid">
        {#each tiles as tile (tile.meta)}
            <div class="stat-tile" style="--icon-color: {tile.color}">
                <span class="stat-icon">{@html tile.icon}</span>
                <span class="stat-value">{tile.value}</span>
                <span class="stat-label">{tile.meta}</span>
            </div>
        {/each}
    </section>

    <div class="layout-grid">
        <!-- Tier 2: Profile, cover & pace -->
        <aside class="sidebar">
            <div class="panel profile-card">
                <div class="profile-frame">
                    <button
                        class="avatar-btn"
                        type="button"
                        aria-label="Go to settings"
                        onclick={() => goto('/settings')}
                    >
                        {#if profilePicSrc}
                            <img class="avatar" src={profilePicSrc} alt="" />
                        {:else}
                            <div class="avatar avatar-placeholder">{initials}</div>
                        {/if}
                    </button>
                </div>
                <div class="cover-title profile-name">{username}'s stats</div>
                <ul class="profile-stats">
                    <li class="profile-stat">
                        <span class="meta-icon" style="color: var(--theme-primary, #36b7bd)">{@html ICONS.book_open}</span>
                        <span class="meta-label">Media</span>
                        <span class="meta-value">{profileStats.mediaCount.toLocaleString()}</span>
                    </li>
                    <li class="profile-stat">
                        <span class="meta-icon" style="color: var(--theme-primary, #36b7bd)">{@html ICONS.calendar}</span>
                        <span class="meta-value">{formatFirstUsed(profileStats.firstUsed)}</span>
                    </li>
                    <li class="profile-stat">
                        <span class="meta-icon" style="color: var(--theme-primary, #36b7bd)">{@html ICONS.book}</span>
                        <span class="meta-label">Words</span>
                        <span class="meta-value">{profileStats.wordCount.toLocaleString()}</span>
                    </li>
                    <li class="profile-stat">
                        <span class="meta-icon" style="color: var(--theme-primary, #36b7bd)">{@html ICONS.translate}</span>
                        <span class="meta-label">Sentences</span>
                        <span class="meta-value">{profileStats.sentenceCount.toLocaleString()}</span>
                    </li>
                </ul>
            </div>

            {#if mediaInfo}
                {@const info = mediaInfo}
                <div class="panel cover-panel">
                    <div
                        class="cover-frame"
                        style="--cover-accent: {info.color || 'var(--theme-primary, #36b7bd)'}"
                        onclick={() => goto(`/media/${info.id}`)}
                        onkeydown={(e) => {
                            if (e.key === 'Enter' || e.key === ' ') {
                                e.preventDefault();
                                goto(`/media/${info.id}`);
                            }
                        }}
                        role="button"
                        tabindex="0"
                        title={info.title}
                    >
                        {#if coverSrcValue}
                            <img src={coverSrcValue} alt={mediaInfo.title} />
                        {:else}
                            <div class="cover-placeholder"></div>
                        {/if}
                    </div>
                    <div class="cover-title">{mediaInfo.title}</div>
                    {#if info.vndb_id}
                        <div class="vndb-row">
                            <button class="vndb-link" onclick={() => openVndb(info.vndb_id)}>v{info.vndb_id.replace(/^v/i, '')}</button>
                        </div>
                    {/if}
                    <ul class="media-meta">
                        <li>
                            <span class="meta-icon" style="color: {mediaInfo.color || '#fab387'}">{@html ICONS.star}</span>
                            <span class="meta-label">Status</span>
                            <span class="meta-value-group">
                                <span class="meta-dot" style="background: {STATUS_COLORS[/** @type {'active' | 'planned' | 'paused' | 'dropped' | 'completed'} */ (mediaInfo.status)] || '#6c7086'}"></span>
                                <span class="meta-value">{mediaInfo.status}</span>
                            </span>
                        </li>
                        <li>
                            <span class="meta-icon" style="color: {mediaInfo.color || '#fab387'}">{@html ICONS.half_circle}</span>
                            <span class="meta-label">Vocab coverage</span>
                            <span class="meta-value">
                                {vocabCoverage?.gathering ? 'Gathering…' : vocabCoverage?.percentage != null ? `${vocabCoverage.percentage}%` : '—'}
                            </span>
                        </li>
                        <li>
                            <span class="meta-icon" style="color: {mediaInfo.color || '#fab387'}">{@html ICONS.plus}</span>
                            <span class="meta-label">Words mined</span>
                            <span class="meta-value">{(mediaStats?.words_mined ?? 0).toLocaleString()}</span>
                        </li>
                        <li>
                            <span class="meta-icon" style="color: {mediaInfo.color || '#fab387'}">{@html ICONS.calendar2}</span>
                            <span class="meta-label">Created</span>
                            <span class="meta-value">{formatMediaDate(mediaInfo.created_at)}</span>
                        </li>
                        <li>
                            <span class="meta-icon" style="color: {mediaInfo.color || '#fab387'}">#</span>
                            <span class="meta-label">Tag</span>
                            <span class="meta-value" style="color: {mediaInfo.color || '#fab387'}">{mediaInfo.tag || '—'}</span>
                        </li>
                        {#if info.vndb_id}
                            <li>
                                <span class="meta-icon" style="color: {info.color || '#fab387'}">{@html ICONS.clock}</span>
                                <span class="meta-label">Avg length</span>
                                <span class="meta-value">{vnInfo ? formatVnLength(vnInfo) : '—'}</span>
                            </li>
                            <li>
                                <span class="meta-icon" style="color: {info.color || '#fab387'}">{@html ICONS.star}</span>
                                <span class="meta-label">VNDB rating</span>
                                <span class="meta-value">{vnInfo ? formatVnRating(vnInfo) : '—'}</span>
                            </li>
                        {/if}
                    </ul>
                </div>
            {/if}

            <div class="panel">
                <div class="panel-head" style="--icon-color: #a6e3a1">
                    <span class="panel-icon">{@html ICONS.bolt}</span>
                    <h2>Reading pace</h2>
                </div>
                <ul class="pace-list">
                    <li>
                        <span class="pace-icon" style="--icon-color: #a6e3a1">{@html ICONS.book_open}</span>
                        <span class="pace-body">
                            <span class="pace-name">Reading speed</span>
                            <span class="pace-value">{stats.mojiPerHour.toFixed(0)} 文字 <small>/hr</small></span>
                        </span>
                    </li>
                    <li>
                        <span class="pace-icon" style="--icon-color: #89b4fa">{@html ICONS.translate}</span>
                        <span class="pace-body">
                            <span class="pace-name">Sentences</span>
                            <span class="pace-value">{stats.sentencesPerHour.toFixed(1)} <small>/hr</small></span>
                        </span>
                    </li>
                    <li>
                        <span class="pace-icon" style="--icon-color: #cba6f7">{@html ICONS.book}</span>
                        <span class="pace-body">
                            <span class="pace-name">Avg sentence length</span>
                            <span class="pace-value">{stats.avgSentenceLength.toFixed(1)} 文字 <small>/sentence</small></span>
                        </span>
                    </li>
                </ul>
            </div>

            <div class="panel">
                <div class="panel-head" style="--icon-color: #89b4fa">
                    <span class="panel-icon">{@html ICONS.book_open}</span>
                    <h2>By media</h2>
                </div>
                {#if mediaBreakdown.length === 0}
                    <p class="empty-note">No reading sessions yet.</p>
                {:else}
                    <ul class="media-breakdown">
                        {#each mediaBreakdown as m (m.id)}
                            <li>
                                <div
                                    class="media-breakdown-row"
                                    onclick={() => goto(`/media/${m.id}`)}
                                    onkeydown={(e) => {
                                        if (e.key === 'Enter' || e.key === ' ') {
                                            e.preventDefault();
                                            goto(`/media/${m.id}`);
                                        }
                                    }}
                                    role="button"
                                    tabindex="0"
                                    title={m.title}
                                >
                                {#if m.coverPath}
                                    <span class="mb-cover"><img src={coverSrc(m.coverPath)} alt="" /></span>
                                {:else}
                                    <span class="mb-cover mb-cover-placeholder" style="--mb-accent: {m.color || 'var(--theme-primary, #36b7bd)'}"></span>
                                {/if}
                                <span class="mb-body">
                                    <span class="mb-title">{m.title}</span>
                                    <span class="mb-bar-track">
                                        <span
                                            class="mb-bar"
                                            style="width: {Math.max(4, (m.totalMoji / breakdownMaxMoji) * 100)}%; background: {m.color || 'var(--theme-primary, #36b7bd)'}"
                                        ></span>
                                    </span>
                                </span>
                                <span class="mb-values">
                                    <span class="mb-value">{formatBreakdownTime(m.totalSeconds)}</span>
                                    <span class="mb-sub">{m.totalMoji.toLocaleString()} 文字</span>
                                </span>
                                </div>
                            </li>
                        {/each}
                    </ul>
                {/if}
            </div>
        </aside>

        <!-- Tier 3: Deep dive charts -->
        <div class="main-charts">
            <div class="panel chart-panel">
                <div class="panel-head" style="--icon-color: #89dceb">
                    <span class="panel-icon">{@html ICONS.calendar2}</span>
                    <h2>Last 30 days</h2>
                </div>
                <!-- LineChart consumes the dailyMoji array -->
                <LineChart
                    data={dailyMoji}
                    series={[
                    { key: 'moji', color: 'var(--theme-primary, #36b7bd)', label: '文字', formatValue: formatMoji },
                    { key: 'minutes', color: 'var(--theme-accent, #8fb2e8)', label: 'Minutes', formatValue: formatMinutes }
                ]}
                formatLabel={formatDayLabel}
                />
            </div>
            <div class="panel chart-panel">
                <div class="panel-head" style="--icon-color: #cba6f7">
                    <span class="panel-icon">{@html ICONS.stats}</span>
                    <h2>Monthly totals</h2>
                </div>
                <!-- BarChart consumes the monthlyMoji array -->
                <BarChart
                    data={monthlyMoji}
                    series={[
                    { key: 'moji', color: 'var(--theme-primary, #36b7bd)', label: '文字', formatValue: formatMoji },
                    { key: 'minutes', color: 'var(--theme-accent, #8fb2e8)', label: 'Minutes', formatValue: formatMinutes }
                ]}
                formatLabel={formatMonthLabel}
                />
            </div>

            <div class="chart-grid">
                <div class="panel chart-panel">
                    <div class="panel-head" style="--icon-color: #f9e2af">
                        <span class="panel-icon">{@html ICONS.plus}</span>
                        <h2>Words mined</h2>
                    </div>
                    <BarChart
                        data={wordsMinedDaily}
                        series={[{ key: 'mined', color: 'var(--theme-primary, #36b7bd)', label: 'Words mined', formatValue: formatWords }]}
                        formatLabel={formatDayLabel}
                        showAxisLabels={false}
                    />
                </div>
                <div class="panel chart-panel">
                    <div class="panel-head" style="--icon-color: #94e2d5">
                        <span class="panel-icon">{@html ICONS.book}</span>
                        <h2>Dictionary status</h2>
                    </div>
                    <DonutChart data={donutData} />
                </div>

                <div class="panel chart-panel">
                    <div class="panel-head" style="--icon-color: #a6e3a1">
                        <span class="panel-icon">{@html ICONS.star}</span>
                        <h2>Vocabulary growth</h2>
                    </div>
                    <LineChart
                        data={vocabGrowth}
                        series={[{ key: 'total', color: 'var(--theme-primary, #36b7bd)', label: 'Total words', formatValue: formatWords }]}
                        formatLabel={formatDayLabel}
                        showAxisLabels={false}
                    />
                </div>
                <div class="panel chart-panel">
                    <div class="panel-head" style="--icon-color: #f9e2af">
                        <span class="panel-icon">{@html ICONS.check}</span>
                        <h2>Reviews</h2>
                    </div>
                    <BarChart
                        data={reviewActivity}
                        series={[{ key: 'reviews', color: 'var(--theme-accent, #8fb2e8)', label: 'Reviews', formatValue: formatReviews }]}
                        formatLabel={formatDayLabel}
                        showAxisLabels={false}
                    />
                </div>
            </div>
        </div>
    </div>

    <!-- Tier 4: Activity heatmap, full width -->
    <div class="panel heatmap-panel">
        <div class="panel-head heatmap-head">
            <span class="panel-icon" style="--icon-color: #89b4fa">
                {@html ICONS.calendar}
            </span>
            <h2>Activity</h2>
            <div class="heatmap-year-select">
                <SelectInput options={yearOptions} value={heatmapYear ? String(heatmapYear) : ''} on:change={handleYearChange} />
            </div>
        </div>
        <div class="heatmap-wrapper">
            <!-- HeatMap utilizes the yearActivity state -->
            <HeatMap data={yearActivity} year={heatmapYear} primaryColor={mediaInfo?.color || 'var(--theme-primary, #36b7bd)'} formatValue={(m) => `${m.toLocaleString()} 文字`} />
        </div>
    </div>
</main>

<style>
    .analytics-layout {
        max-width: 1280px;
        margin: 0 auto;
        padding: 2rem;
        box-sizing: border-box;
    }

    .layout-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: 1.5rem;
        margin-bottom: 1.75rem;
        flex-wrap: wrap;
    }

    .title-group {
        display: flex;
        align-items: center;
        gap: 1.25rem;
    }

    .title-group h1 {
        font-size: 1.5rem;
        font-weight: 700;
        margin: 0;
        letter-spacing: -0.02em;
        color: var(--theme-text, #f6f6f6);
    }

    .subtitle {
        margin: 0.15rem 0 0;
        font-size: 0.85rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .filter-group {
        display: flex;
        gap: 1rem;
    }

    /* Headline stat tiles */
    .stat-grid {
        display: grid;
        grid-template-columns: repeat(4, 1fr);
        gap: 1rem;
        margin-bottom: 1.75rem;
    }

    .stat-tile {
        font-family: "Noto Sans JP";
        position: relative;
        display: flex;
        flex-direction: column;
        gap: 0.45rem;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 12px;
        padding: 1.1rem 1.25rem;
        transition: transform 0.15s ease, box-shadow 0.15s ease, border-color 0.15s ease;
        overflow: hidden;
    }

    .stat-tile:hover {
        transform: translateY(-3px);
        box-shadow: 0 10px 22px var(--theme-shadow, rgba(0, 0, 0, 0.35));
    }

    .stat-icon {
        font-family: "Symbols Nerd Font";
        font-weight: normal;
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
        margin-bottom: 0.15rem;
    }

    .stat-value {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
        line-height: 1.1;
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .stat-label {
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    /* Layout grid */
    .layout-grid {
        display: grid;
        grid-template-columns: 320px 1fr;
        gap: 1.5rem;
        align-items: start;
        margin-bottom: 1.5rem;
    }

    .panel {
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 12px;
        padding: 1.25rem 1.5rem;
        margin-bottom: 1.5rem;
        transition: box-shadow 0.2s ease, transform 0.2s ease;
    }

    .panel:hover {
        box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
        transform: translateY(-1px);
    }

    .panel-head {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        margin: 0 0 1.1rem 0;
        padding-bottom: 0.8rem;
        border-bottom: 1px solid color-mix(in srgb, var(--theme-border, #404040) 70%, transparent);
    }

    .panel-head h2 {
        font-size: 0.95rem;
        font-weight: 700;
        margin: 0;
        color: var(--theme-text, #f6f6f6);
        letter-spacing: -0.01em;
    }

    .panel-icon {
        font-family: "Symbols Nerd Font";
        font-weight: normal;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 1.9rem;
        height: 1.9rem;
        flex-shrink: 0;
        border-radius: 8px;
        background: color-mix(in srgb, var(--icon-color, #36b7bd) 14%, transparent);
        color: var(--icon-color, #36b7bd);
        font-size: 1rem;
    }

    /* Cover / profile card */
    .cover-panel {
        padding: 1rem 1.5rem;
        display: flex;
        flex-direction: column;
    }

    .profile-card {
        display: flex;
        flex-direction: column;
        padding: 1.1rem 1.5rem;
    }

    .cover-frame {
        position: relative;
        width: 100%;
        max-width: 168px;
        margin: 0 auto;
        aspect-ratio: 2 / 3;
        border-radius: 8px;
        overflow: hidden;
        background: linear-gradient(
            135deg,
            color-mix(in srgb, var(--cover-accent, var(--theme-primary, #36b7bd)) 35%, #000),
            var(--theme-surface, #2d2d2d)
        );
        border: 1px solid var(--theme-border, #404040);
        cursor: pointer;
        transition: border-color 0.15s ease, transform 0.15s ease;
    }

    .cover-frame:hover {
        border-color: var(--cover-accent, var(--theme-primary, #36b7bd));
        transform: translateY(-2px);
    }

    .cover-frame img,
    .cover-placeholder {
        width: 100%;
        height: 100%;
        object-fit: cover;
        display: block;
    }

    .cover-placeholder {
        background: linear-gradient(135deg, color-mix(in srgb, var(--cover-accent, var(--theme-primary, #36b7bd)) 35%, #000), var(--theme-surface, #2d2d2d));
    }

    .profile-frame {
        display: flex;
        justify-content: center;
        align-items: center;
        padding: 0 0 0.6rem;
    }

    .avatar-btn {
        width: 96px;
        height: 96px;
        padding: 0;
        border: none;
        background: none;
        border-radius: 50%;
        cursor: pointer;
        outline: none;
        transition: filter 0.15s ease, box-shadow 0.15s ease;
    }

    .avatar-btn:hover {
        filter: brightness(90%);
    }

    .avatar-btn:focus-visible {
        box-shadow: 0 0 0 3px color-mix(in srgb, var(--theme-primary, #36b7bd) 60%, transparent);
    }

    .avatar {
        width: 100%;
        height: 100%;
        border-radius: 50%;
        object-fit: cover;
        border: 2px solid color-mix(in srgb, var(--theme-primary, #36b7bd) 45%, var(--theme-border, #404040));
        flex-shrink: 0;
        box-sizing: border-box;
    }

    .avatar-placeholder {
        display: flex;
        align-items: center;
        justify-content: center;
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 16%, var(--theme-surface, #2d2d2d));
        color: var(--theme-primary, #36b7bd);
        font-family: "Noto Sans JP", Inter, sans-serif;
        font-size: 2.2rem;
        font-weight: 700;
    }
    .cover-title {
        font-family: "Noto Sans JP";
        text-align: center;
        font-weight: 600;
        font-size: 0.95rem;
        color: var(--theme-text, #f6f6f6);
        padding: 0.5rem 0.5rem 0.25rem;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .media-meta {
        font-family: "Noto Sans JP";
        list-style: none;
        margin: 0.5rem 0 0;
        padding: 0.6rem 0.1rem 0;
        border-top: 1px solid color-mix(in srgb, var(--theme-border, #404040) 60%, transparent);
        display: flex;
        flex-direction: column;
        gap: 0.45rem;
    }

    .media-meta li {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        font-size: 0.8rem;
    }

    .meta-dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        flex-shrink: 0;
    }

    .meta-icon {
        font-family: "Symbols Nerd Font";
        font-weight: normal;
        font-size: 0.85rem;
        line-height: 1;
        flex-shrink: 0;
    }

    .meta-value-group {
        margin-left: auto;
        display: flex;
        align-items: center;
        gap: 0.4rem;
    }

    .profile-stats {
        list-style: none;
        margin: 0.5rem 0 0;
        padding: 0.6rem 0.1rem 0;
        border-top: 1px solid color-mix(in srgb, var(--theme-border, #404040) 60%, transparent);
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 0.5rem 0.75rem;
    }

    .profile-stat {
        font-family: "Noto Sans JP";
        display: flex;
        align-items: center;
        gap: 0.4rem;
        font-size: 0.8rem;
        min-width: 0;
    }

    .profile-stat .meta-label {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .profile-stat .meta-value {
        margin-left: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .meta-label {
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .meta-value {
        margin-left: auto;
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
        font-variant-numeric: tabular-nums;
        display: flex;
        align-items: center;
        gap: 0.35rem;
    }

    .vndb-row {
        display: flex;
        justify-content: center;
        padding: 0 0.5rem;
    }

    .vndb-link {
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        font-size: 0.8rem;
        font-weight: 600;
        cursor: pointer;
        color: var(--theme-primary, #36b7bd);
        transition: color 0.15s ease;
        white-space: nowrap;
    }

    .vndb-link:hover {
        color: var(--theme-primaryHover, #17a4ab);
        text-decoration: underline;
    }

    /* Pace list */
    .pace-list {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: 0.95rem;
    }

    .pace-list li {
        display: flex;
        align-items: center;
        gap: 0.8rem;
    }

    .pace-icon {
        font-family: "Symbols Nerd Font";
        font-weight: normal;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 2.25rem;
        height: 2.25rem;
        flex-shrink: 0;
        border-radius: 8px;
        /*background: color-mix(in srgb, var(--icon-color, #36b7bd) 12%, transparent);*/
        color: var(--icon-color, #36b7bd);
        font-size: 1.05rem;
    }

    .pace-body {
        display: flex;
        flex-direction: column;
        gap: 0rem;
        min-width: 0;
    }

    .pace-name {
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .pace-value {
        font-family: "Noto Sans JP";
        font-size: 1.05rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
        font-variant-numeric: tabular-nums;
    }

    .pace-value small {
        font-weight: 400;
        font-size: 0.78rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    /* Media breakdown (All media sidebar panel) */
    .media-breakdown {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: 0.8rem;
    }

    .media-breakdown-row {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        cursor: pointer;
        border-radius: 8px;
        padding: 0.15rem 0.3rem;
        margin: -0.15rem -0.3rem;
        transition: background 0.15s ease;
        outline: none;
    }

    .media-breakdown-row:hover,
    .media-breakdown-row:focus-visible {
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 10%, transparent);
    }

    .mb-cover {
        width: 32px;
        height: 48px;
        flex-shrink: 0;
        border-radius: 4px;
        overflow: hidden;
        background: var(--theme-surface, #2d2d2d);
    }

    .mb-cover img {
        width: 100%;
        height: 100%;
        object-fit: cover;
        display: block;
    }

    .mb-cover-placeholder {
        background: linear-gradient(
            135deg,
            color-mix(in srgb, var(--mb-accent, var(--theme-primary, #36b7bd)) 35%, #000),
            var(--theme-surface, #2d2d2d)
        );
    }

    .mb-body {
        display: flex;
        flex-direction: column;
        gap: 0.3rem;
        min-width: 0;
        flex: 1;
    }

    .mb-title {
        font-family: "Noto Sans JP";
        font-size: 0.82rem;
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .mb-bar-track {
        display: block;
        height: 5px;
        border-radius: 3px;
        background: color-mix(in srgb, var(--theme-border, #404040) 45%, transparent);
        overflow: hidden;
    }

    .mb-bar {
        display: block;
        height: 100%;
        border-radius: 3px;
    }

    .mb-values {
        display: flex;
        flex-direction: column;
        align-items: flex-end;
        gap: 0.1rem;
        flex-shrink: 0;
    }

    .mb-value {
        font-family: "Noto Sans JP";
        font-size: 0.82rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
        font-variant-numeric: tabular-nums;
    }

    .mb-sub {
        font-size: 0.7rem;
        color: var(--theme-textSecondary, #b3b3b3);
        font-variant-numeric: tabular-nums;
    }

    .empty-note {
        margin: 0;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.85rem;
        text-align: center;
        padding: 1rem 0;
    }

    .main-charts {
        display: flex;
        flex-direction: column;
        gap: 0;
    }

    .chart-panel {
        display: flex;
        flex-direction: column;
    }

    .chart-grid {
        display: grid;
        grid-template-columns: 1.4fr 1fr;
        gap: 1.5rem;
        align-items: stretch;
    }

    .chart-grid .panel {
        margin-bottom: 0;
    }

    .chart-grid .chart-panel :global(.line-chart-wrapper),
    .chart-grid .chart-panel :global(.bar-chart-wrapper) {
        display: flex;
        flex-direction: column;
        justify-content: flex-end;
        flex: 1;
    }

    /* Full-width heatmap */
    .heatmap-panel {
        padding-bottom: 1.75rem;
    }

    .heatmap-head .heatmap-year-select {
        margin-left: auto;
    }

    @media (max-width: 924px) {
        .layout-grid {
            grid-template-columns: 1fr;
        }
        .stat-grid {
            grid-template-columns: repeat(2, 1fr);
        }
        .chart-grid {
            grid-template-columns: 1fr;
        }
    }

    @media (max-width: 560px) {
        .analytics-layout {
            padding: 1.25rem;
        }
        .stat-grid {
            grid-template-columns: 1fr;
        }
        .filter-group {
            width: 100%;
        }
        .filter-group :global(.select-input) {
            flex: 1;
        }
    }
</style>
