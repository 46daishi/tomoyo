import Database from '@tauri-apps/plugin-sql';

let dbInstance = null;

export async function getDb() {
    if (!dbInstance) {
        dbInstance = await Database.load('sqlite:immersion.db');
    }
    return dbInstance;
}