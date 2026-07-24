import { invoke } from "@tauri-apps/api/core";

const DISCORD_CLIENT_ID = "1530192543529304065";

/**
 * Thin wrapper around the Tauri `discord_rpc` commands.
 * All methods return Promises and throw on failure.
 */
class DiscordRPC {
	constructor() {
		this.connected = false;
	}

	async connect() {
		await invoke("connect_discord", { clientId: DISCORD_CLIENT_ID });
		this.connected = true;
	}

	async updatePresence(options) {
		if (!this.connected) throw new Error("Discord RPC not connected");
		await invoke("update_discord_presence", {
			details:        options.details,
			status:         options.status,
			largeImage:     options.largeImage,
			largeText:      options.largeText,
			smallImage:     options.smallImage,
			smallText:      options.smallText,
			startTimestamp: options.startTimestamp,
			endTimestamp:   options.endTimestamp,
		});
	}

	async disconnect() {
		if (!this.connected) return;
		await invoke("disconnect_discord");
		this.connected = false;
	}
}

/** Singleton — import this everywhere instead of constructing a new one. */
export const discordRPC = new DiscordRPC();
