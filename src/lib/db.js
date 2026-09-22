import Database from '@tauri-apps/plugin-sql';
import { open } from '@tauri-apps/plugin-dialog';
import { appDataDir, join } from '@tauri-apps/api/path';
import { copyFile, mkdir, exists, writeFile } from '@tauri-apps/plugin-fs';
import { readImage } from '@tauri-apps/plugin-clipboard-manager';
import { convertFileSrc } from '@tauri-apps/api/core';

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

export function coverSrc(path) {
    return path ? convertFileSrc(path) : null;
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