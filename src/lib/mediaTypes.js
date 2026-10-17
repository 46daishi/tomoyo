import { ICONS } from './icons.js';

/**
 * Single source of truth for media types. The home category pills, the
 * cover type badge, and the add/edit media modal all read this list, so
 * a new type only needs to be added here.
 *
 * `importers` lists the ids of import sources offered for the type (see
 * IMPORT_SOURCES). An empty list means the type can only be added
 * manually.
 */
export const MEDIA_TYPES = [
    { value: 'visual_novel', label: 'Visual Novel', abr: 'VN', icon: ICONS.heart, importers: ['vndb'] },
    { value: 'game', label: 'Game', icon: ICONS.gamepad, importers: [] },
    { value: 'light_novel', label: 'Light Novel', abr: 'LN', icon: ICONS.book_open, importers: [] },
    { value: 'book', label: 'Book', icon: ICONS.book, importers: [] },
    { value: 'other', label: 'Other', icon: ICONS.text, importers: [] },
];

export const MEDIA_TYPE_BY_VALUE = Object.fromEntries(MEDIA_TYPES.map((t) => [t.value, t]));

/**
 * Resolve a media type definition, defaulting to Visual Novel for
 * missing/unknown values (legacy rows without a media_type column).
 * @param {string|null|undefined} value
 * @returns {{ value: string, label: string, abr?: string, icon: string, importers: string[] }}
 */
export function mediaTypeOf(value) {
    return MEDIA_TYPE_BY_VALUE[value ?? ''] ?? MEDIA_TYPE_BY_VALUE.visual_novel;
}

/**
 * Import sources keyed by id. Data-only descriptors: a type opts in via
 * its `importers` list, so the modal never hardcodes type checks. Add a
 * new source here (plus its fetch routine) to offer it to more types.
 * @type {Record<string, { id: string, label: string, placeholder: string, hint: string }>}
 */
export const IMPORT_SOURCES = {
    vndb: {
        id: 'vndb',
        label: 'VNDB',
        placeholder: 'VNDB ID or title',
        hint: 'Title and cover are fetched from VNDB automatically.',
    },
};
