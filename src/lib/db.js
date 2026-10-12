import Database from '@tauri-apps/plugin-sql';
import { open } from '@tauri-apps/plugin-dialog';
import { appDataDir, join } from '@tauri-apps/api/path';
import { copyFile, mkdir, exists, readFile, writeFile } from '@tauri-apps/plugin-fs';
import { readImage } from '@tauri-apps/plugin-clipboard-manager';

/** @type {import('@tauri-apps/plugin-sql').default | null} */
let dbInstance = null;

export async function getDb() {
    if (!dbInstance) {
        dbInstance = await Database.load('sqlite:immersion.db');
    }
    return dbInstance;
}

export async function closeDb() {
    if (dbInstance) {
        await dbInstance.close();
        dbInstance = null;
    }
}

export async function pickCoverImage() {
    const selected = await open({
        multiple: false,
        filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'webp'] }]
    });
    if (!selected) return null;

    const dataDir = await appDataDir();
    const coversDir = await join(dataDir, 'covers');
    if (!(await exists(coversDir))) {
        await mkdir(coversDir, { recursive: true });
    }

    const ext = selected.split('.').pop();
    const filename = `${crypto.randomUUID()}.${ext}`;
    const destPath = await join(coversDir, filename);

    await copyFile(selected, destPath);
    return destPath; // store this in the DB
}

/** @type {Object<string, string>} */
const IMAGE_MIME_TYPES = {
    png: 'image/png',
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    webp: 'image/webp',
    gif: 'image/gif',
    bmp: 'image/bmp',
    svg: 'image/svg+xml',
};

// Session cache of loaded images (path -> data URL, or null when the file
// could not be read, so failures don't retry on every render).
const imageUrlCache = new Map();
const imageUrlPending = new Map();

/**
 * @param {Uint8Array} bytes
 * @returns {string}
 */
function bytesToBase64(bytes) {
    // Manual byte->char mapping (never TextDecoder: the 'latin1' label is
    // windows-1252 in browsers, which maps 0x80-0x9F above U+00FF and makes
    // btoa throw InvalidCharacterError on real image files).
    const parts = [];
    const chunk = 0x1000;
    for (let i = 0; i < bytes.length; i += chunk) {
        parts.push(String.fromCharCode.apply(null, Array.from(bytes.subarray(i, i + chunk))));
    }
    return btoa(parts.join(''));
}

/**
 * Load a stored image file (covers, word images, profile pictures) as a
 * same-origin `data:` URL for `<img>` tags.
 *
 * File URLs must NOT be used here: `asset://` subresource loads are subject
 * to the WebView's origin check, which newer WebKitGTK versions reject
 * ("Unsafe attempt to load URL ... Domains, protocols and ports must
 * match"), breaking every image. `data:` URLs carry no origin, so they
 * render everywhere — including inside `<canvas>` pixel reads.
 *
 * @param {string | null | undefined} path absolute file path, as stored in the DB
 * @returns {Promise<string | null>} the data URL, or null when unreadable
 */
export function loadImageSrc(path) {
    if (!path) return Promise.resolve(null);
    if (imageUrlCache.has(path)) return Promise.resolve(imageUrlCache.get(path));
    if (imageUrlPending.has(path)) return imageUrlPending.get(path);
    const promise = readFile(path)
        .then((bytes) => {
            const ext = path.split('.').pop()?.toLowerCase() ?? '';
            const mime = IMAGE_MIME_TYPES[ext] ?? 'image/png';
            const url = `data:${mime};base64,${bytesToBase64(bytes)}`;
            imageUrlCache.set(path, url);
            imageUrlPending.delete(path);
            return url;
        })
        .catch((err) => {
            console.warn(`Could not load image: ${path}`, err);
            imageUrlCache.set(path, null);
            imageUrlPending.delete(path);
            return null;
        });
    imageUrlPending.set(path, promise);
    return promise;
}

export async function pickProfilePicture() {
    const selected = await open({
        multiple: false,
        filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'webp'] }]
    });
    if (!selected) return null;

    const dataDir = await appDataDir();
    const profileDir = await join(dataDir, 'profile');
    if (!(await exists(profileDir))) {
        await mkdir(profileDir, { recursive: true });
    }

    const ext = selected.split('.').pop();
    const filename = `${crypto.randomUUID()}.${ext}`;
    const destPath = await join(profileDir, filename);

    await copyFile(selected, destPath);
    return destPath; // store this in the settings
}

/**
 * Copies a user-picked image into the app's word-images directory.
 * @returns {Promise<string | null>} stored path (to save in words.image_path), or null
 */
export async function pickWordImage() {
    const selected = await open({
        multiple: false,
        filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'webp'] }]
    });
    if (!selected) return null;

    const dataDir = await appDataDir();
    const imagesDir = await join(dataDir, 'word-images');
    if (!(await exists(imagesDir))) {
        await mkdir(imagesDir, { recursive: true });
    }

    const ext = selected.split('.').pop();
    const filename = `${crypto.randomUUID()}.${ext}`;
    const destPath = await join(imagesDir, filename);

    await copyFile(selected, destPath);
    return destPath; // store this in the DB
}

/**
 * Saves the current clipboard image (if any) as PNG into word-images.
 * @returns {Promise<string | null>} stored path, or null when the clipboard holds no image
 */
export async function pasteWordImageFromClipboard() {
    let image;
    try {
        image = await readImage();
    } catch {
        return null;
    }
    if (!image) return null;

    const { width, height } = await image.size();
    if (!width || !height) return null;
    const rgba = await image.rgba();
    if (!rgba || rgba.length === 0) return null;

    const pngBytes = await new Promise((resolve, reject) => {
        const canvas = document.createElement('canvas');
        canvas.width = width;
        canvas.height = height;
        const ctx = canvas.getContext('2d');
        if (!ctx) {
            reject(new Error('no 2d context'));
            return;
        }
        ctx.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
        canvas.toBlob((blob) => {
            if (!blob) {
                reject(new Error('png encode failed'));
                return;
            }
            blob.arrayBuffer().then(
                (buf) => resolve(new Uint8Array(buf)),
                reject
            );
        }, 'image/png');
    });

    const dataDir = await appDataDir();
    const imagesDir = await join(dataDir, 'word-images');
    if (!(await exists(imagesDir))) {
        await mkdir(imagesDir, { recursive: true });
    }

    const destPath = await join(imagesDir, `${crypto.randomUUID()}.png`);
    await writeFile(destPath, pngBytes);
    return destPath;
}