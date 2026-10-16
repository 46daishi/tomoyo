export const STATUS_COLORS = {
    active: '#a6e3a1',
    planned: '#89b4fa',
    paused: '#fab387',
    dropped: '#f38ba8',
    completed: '#cba6f7'
};

export const FILTER_OPTIONS = [
        { value: 'all', label: 'All' },
        { value: 'active', label: 'Active' },
        { value: 'paused', label: 'Paused' },
        { value: 'planned', label: 'Planned' },
        { value: 'completed', label: 'Completed' },
        { value: 'dropped', label: 'Dropped' }
];

export const STATUS_OPTIONS = FILTER_OPTIONS.filter((o) => o.value !== 'all');

export const MEDIA_TYPE_OPTIONS = [
    { value: 'visual_novel', label: 'Visual Novel' },
    { value: 'game', label: 'Game' },
    { value: 'light_novel', label: 'Light Novel' },
    { value: 'book', label: 'Book' },
    { value: 'other', label: 'Other' },
];

export const STATUS_LEVELS = [
    { label: 'New', color: '#6c7086' },
    { label: 'Recognized', color: '#89b4fa' },
    { label: 'Familiar', color: '#cba6f7' },
    { label: 'Learned', color: '#a6e3a1' },
    { label: 'Known', color: '#40a02b' },
];

// Word-level colours are user-customizable (settings → general → word
// level colours, stored as settings.word_level_colors, index-aligned).
// These helpers resolve them with per-level fallback, so partial, stale,
// or missing custom data degrades gracefully to the built-ins.
/**
 * @param {any} settings settings object (or null/undefined)
 * @param {number} level 0-4 word level
 * @returns {string} hex colour
 */
export function statusLevelColor(settings, level) {
    const custom = settings?.word_level_colors;
    if (Array.isArray(custom) && typeof custom[level] === 'string' && custom[level]) {
        return custom[level];
    }
    return STATUS_LEVELS[level]?.color ?? '#f38ba8';
}

/**
 * @param {any} settings settings object (or null/undefined)
 * @returns {Array<{ label: string, color: string }>} levels with resolved colours
 */
export function resolveStatusLevels(settings) {
    return STATUS_LEVELS.map((level, i) => ({ ...level, color: statusLevelColor(settings, i) }));
}