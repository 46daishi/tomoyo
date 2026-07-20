import Database from '@tauri-apps/plugin-sql';
import { open } from '@tauri-apps/plugin-dialog';
import { appDataDir, join } from '@tauri-apps/api/path';
import { copyFile, mkdir, exists } from '@tauri-apps/plugin-fs';
import { convertFileSrc } from '@tauri-apps/api/core';

let dbInstance = null;

export async function getDb() {
    if (!dbInstance) {
        dbInstance = await Database.load('sqlite:immersion.db');
    }
    return dbInstance;
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