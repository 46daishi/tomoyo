import { invoke } from '@tauri-apps/api/core';

let cached = null;

export async function loadSettings() {
    cached = await invoke('get_settings');
    return cached;
}

export async function saveSettings(settings) {
    await invoke('save_settings', { settings });
    cached = settings;
}

export function getCachedSettings() {
    return cached;
}

// SCHEMA
export const FONT_OPTIONS = [
    { value: 'Noto Sans JP', label: 'Noto Sans JP' },
    { value: 'M PLUS 1p', label: 'M PLUS 1p' },
    { value: 'Zen Kaku Gothic New', label: 'Zen Kaku Gothic New' },
];

export const LOOKUP_MODE_OPTIONS = [
    { value: 'click', label: 'Click' },
    { value: 'hover', label: 'Hover' },
    { value: 'hotkey', label: 'Hotkey' },
];

export const INPUT_MODE_OPTIONS = [
    { value: 'clipboard', label: 'Clipboard' },
    { value: 'websocket', label: 'Websocket' },
];

export const REVIEW_MODE_OPTIONS = [
    { value: 'normal', label: 'Normal' },
    { value: 'flashcard', label: 'Flashcard' },
];

// Each option: { key, label, type, subRow?, showIf?, ...type-specific fields }
// type-specific fields:
//   select      -> options
//   slider      -> min, max, step, unit (string appended to displayed value), percent (bool)
//   number      -> min, max
//   text        -> placeholder
//   checkbox    -> (nothing extra)
//   theme       -> (nothing extra — renders <ThemeGrid />)
//   hotkey      -> (nothing extra)
//   actions     -> buttons: [{ label, onClick }]
//   placeholder -> text

export const SETTINGS_SCHEMA = [
    {
        id: 'general',
        label: 'General',
        options: [
          { key: 'theme', label: 'Theme', type: 'theme' },
          { key: 'input_mode', label: 'Input source', type: 'select', options: INPUT_MODE_OPTIONS },
          {
              key: 'websocket_address',
              label: 'Websocket address',
              type: 'text',
              subRow: true,
              showIf: (s) => s.input_mode === 'websocket',
          },
            { key: 'resume_last_session', label: 'Resume last session on startup', type: 'checkbox' },
          { key: 'discord_rpc_enabled', label: 'Discord Rich Presence', type: 'checkbox' },
            {
                key: 'data_actions',
                label: 'Data',
                type: 'actions',
                buttons: [
                    { label: 'Export', action: 'export' },
                    { label: 'Import', action: 'import' },
                ],
            },
        ],
    },
    {
        id: 'lookup',
        label: 'Lookup',
        options: [
            { key: 'lookup_mode', label: 'Lookup trigger', type: 'select', options: LOOKUP_MODE_OPTIONS },
            { key: 'lookup_limit_enabled', label: 'Limit lookups per hour', type: 'checkbox' },
            {
                key: 'lookup_limit_per_hour',
                label: 'Max lookups / hour',
                type: 'slider',
                min: 5, max: 200,
                subRow: true,
                showIf: (s) => s.lookup_limit_enabled,
            },
            { key: 'show_related_entries', label: 'Show related entries in tooltip', type: 'checkbox' },
            { key: 'track_unknown_words', label: 'Track commonly looked-up words not in dictionary', type: 'checkbox' },
        ],
  },
  {
      id: 'window',
      label: 'Sentence window',
    options: [
          { key: 'font_family', label: 'Font family', type: 'select', options: FONT_OPTIONS },
          { key: 'font_size', label: 'Font size', type: 'slider', min: 12, max: 48, unit: 'px' },
          { key: 'cycle_key', label: 'Cycle key', type: 'hotkey' },
          { key: 'word_highlight_enabled', label: 'Highlight words on hover', type: 'checkbox' },
      { key: 'show_related_entries', label: 'Show related entries in tooltip', type: 'checkbox' },
      { key: 'history_enabled', label: 'Enable history', type: 'checkbox' },
      {
          key: 'history_span',
          label: 'History span',
          type: 'slider',
          min: 10, max: 200, step: 10,
          unit: ' sentences',
          subRow: true,
          showIf: (s) => s.history_enabled,
      },
      { key: 'mini_mode_enabled', label: 'Enable mini mode', type: 'checkbox' },
      {
          key: 'mini_mode_enter_height',
          label: 'Enter threshold (height)',
          type: 'number',
          subRow: true,
          showIf: (s) => s.mini_mode_enabled,
      },
      {
          key: 'mini_mode_transparency',
          label: 'Transparency',
          type: 'slider',
          min: 0, max: 0.9, step: 0.05,
          percent: true,
          subRow: true,
          showIf: (s) => s.mini_mode_enabled,
      },
      ],
  },
    {
        id: 'dictionaries',
        label: 'Dictionaries',
        options: [
            { key: 'dict_placeholder', type: 'placeholder', text: 'Dictionary management is coming soon.', sub: 'JMdict is currently built in and always active.' },
        ],
    },
    {
        id: 'review',
        label: 'Review',
        options: [
            { key: 'default_review_mode', label: 'Default review mode', type: 'select', options: REVIEW_MODE_OPTIONS },
            { key: 'difficulty_estimation_enabled', label: 'Estimate media difficulty from lookup frequency', type: 'checkbox' },
            { key: 'review_placeholder', type: 'placeholder', sub: 'Full review/flashcard functionality is coming soon.' },
        ],
  },
  {
      id: 'advanced',
      label: 'Advanced',
      options: [
          { key: 'jp_detection_threshold', label: 'Japanese detection sensitivity', type: 'slider', min: 0.1, max: 0.9, step: 0.05, percent: true },
      ],
  },
];