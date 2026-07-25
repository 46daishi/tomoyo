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