<script>
    import { page } from '$app/state';
    import { getDb } from '$lib/db';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import { ICONS } from '$lib/icons';
    import { coverSrc } from '$lib/db';
    import { fly } from 'svelte/transition';
    import { STATUS_COLORS } from '$lib/constants.js';

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

    let sessionRunning = $state(false);
    let sessionSeconds = $state(0);
    let timerHandle = null;
    
    function formatTime(totalSeconds) {
        const h = Math.floor(totalSeconds / 3600);
        const m = Math.floor((totalSeconds % 3600) / 60);
        const s = totalSeconds % 60;
        const pad = (n) => String(n).padStart(2, '0');
        return `${pad(h)}:${pad(m)}:${pad(s)}`;
    }
    
    function toggleSession() {
        if (sessionRunning) {
            clearInterval(timerHandle);
            timerHandle = null;
            sessionRunning = false;
            sessionSeconds = 0;
        } else {
            sessionRunning = true;
            timerHandle = setInterval(() => {
                sessionSeconds += 1;
            }, 1000);
        }
    }
    
    $effect(() => {
        return () => {
            if (timerHandle) clearInterval(timerHandle);
        };
    });

    import { startClipboardListener, stopClipboardListener } from '$lib/clipboardListener.js';
    
    let currentSentence = $state('');
    
    function handleClipboardChange(text) {
        currentSentence = text;
        // later: run JP-detection + tokenizer here before accepting it
    }
    
    $effect(() => {
        if (sessionRunning) {
            startClipboardListener(handleClipboardChange);
        } else {
            stopClipboardListener();
        }
    
        return () => {
            stopClipboardListener();
        };
    });
</script>

<main class="page home">
    {#if media}
        <div class="media-header">
            <div class="cover">
                {#if media.cover_path}
                    <img src={coverSrc(media.cover_path)} alt={media.title} />
                {:else}
                    <div class="cover-placeholder"></div>
                {/if}
            </div>

            <div class="media-info">
                <div class="title-row">
                    <h1 class="media-title">{media.title}</h1>
                    {#if media.tag}
                        <span class="tag-pill" style="--tag-color: {media.color}">#{media.tag}</span>
                    {/if}
                </div>
            
                <div class="media-meta">
                    <span class="status">
                        <span class="status-dot" style="--dot-color: {STATUS_COLORS[media.status]}"></span>
                        {media.status}
                    </span>
                </div>
            </div>
        </div>

        <div class="sentence-window">
            {#if currentSentence}
                <p class="sentence-text">{currentSentence}</p>
            {:else}
                <p class="sentence-placeholder">Waiting for a sentence…</p>
            {/if}
        </div>
    {:else}
        <p>Loading…</p>
    {/if}
</main>

<div class="logo">
    <a href="https://x.com/46daishi" target="_blank" rel="noopener noreferrer"><img src="/tomoyo_full.png" alt="tomoyo" /></a>
</div>
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
                icon={sessionRunning ? ICONS.pause : ICONS.play}
                variant="primary"
                size="small"
                onAction={toggleSession}
      />
      {#if sessionRunning}
          <span class="session-timer" transition:fly={{ y: -8, duration: 200 }}>
              {formatTime(sessionSeconds)}
          </span>
      {/if}
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

    .session-timer {
        font-size: 0.75rem;
        font-weight: 600;
        color: var(--theme-textSecondary, #b3b3b3);
        font-variant-numeric: tabular-nums; /* keeps digit widths consistent so it doesn't jitter as numbers change */
        text-align: center;
    }
    
    .title-row {
        display: flex;
        align-items: center;
        gap: 0.7rem;
        flex-wrap: wrap;
    }

    .media-header {
        display: flex;
        gap: 1.5rem;
        align-items: flex-start;
        width: 100%;
        max-width: 800px;
        margin-top:1rem;
    }
    
    .cover {
        flex-shrink: 0;
        aspect-ratio: 2 / 3;
        width: 130px;
        border-radius: 10px;
        overflow: hidden;
        background: var(--surface1, #313244);
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
    
    .media-info {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        text-align: left;
        gap: 0.3rem;
        padding-top: 0.4rem;
    }
    
    .media-title {
        font-size: 1.6rem;
        font-weight: 700;
        margin: 0;
    }
    
    .media-meta {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        flex-wrap: wrap;
    }
    
    .status {
        display: flex;
        align-items: center;
        gap: 0.4rem;
        font-size: 0.85rem;
        color: var(--theme-textSecondary, #b3b3b3);
        text-transform: capitalize;
    }
    
    .status-dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background: var(--dot-color, var(--theme-textSecondary, #b3b3b3));
        flex-shrink: 0;
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
    
    .sentence-window {
        width: 100%;
        max-width: 800px;
        min-height: 200px;
        margin-top: 2rem;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 16px;
        display: flex;
        align-items: top;
        justify-content: center;
        padding: 2rem 2.5rem;
        box-sizing: border-box;
    }

    .sentence-text {
        font-family: "Noto Sans JP", Inter, sans-serif;
        color: var(--theme-text, #f6f6f6);
        font-size: 1.9rem;
        font-weight: 700;
        line-height: 1.6;
        text-align: left;
        margin: 0;
        width: 100%;
    }
    
    .sentence-placeholder {
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 1rem;
        text-align: center;
        margin: 0;
    }
</style>