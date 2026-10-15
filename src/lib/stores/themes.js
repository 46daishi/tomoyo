import { writable } from "svelte/store";

/** @type {Record<string, { label: string, colors: Record<string, string> }>} */
export const themes = {
  // ── Dark themes ────────────────────────────────────────────────────────────
  "tomoyo": {
    label: "Tomoyo",
    colors: {
      background:    "#12213a",
      surface:       "#1c2f4f",
      text:          "#eef2fa",
      textSecondary: "#8ba3c7",
      border:        "#2c4468",
      shadow:        "rgba(0,0,0,0.45)",
      button:        "#1c2f4f",
      buttonText:    "#eef2fa",
      primary:       "#5b8dd6",
      primaryHover:  "#4874bb",
      accent:        "#8fb2e8",
    },
  },

  "dusk-violet": {
    label: "Dusk Violet",
    colors: {
      background:    "#1c1526",
      surface:       "#2a1f3d",
      text:          "#f1e9fb",
      textSecondary: "#9d8bb8",
      border:        "#3d2e57",
      shadow:        "rgba(0,0,0,0.5)",
      button:        "#2a1f3d",
      buttonText:    "#f1e9fb",
      primary:       "#a374e0",
      primaryHover:  "#8c5cc9",
      accent:        "#c9a6f0",
    },
  },

  "forest-canopy": {
    label: "Forest Canopy",
    colors: {
      background:    "#14201a",
      surface:       "#1e2f26",
      text:          "#e6f1ea",
      textSecondary: "#84a693",
      border:        "#2e4638",
      shadow:        "rgba(0,0,0,0.45)",
      button:        "#1e2f26",
      buttonText:    "#e6f1ea",
      primary:       "#5cb87f",
      primaryHover:  "#47a068",
      accent:        "#8fd6a8",
    },
  },

  "amber-glow": {
    label: "Amber Glow",
    colors: {
      background:    "#201a12",
      surface:       "#33281a",
      text:          "#f7ecd9",
      textSecondary: "#b39a76",
      border:        "#4a3a24",
      shadow:        "rgba(0,0,0,0.45)",
      button:        "#33281a",
      buttonText:    "#f7ecd9",
      primary:       "#e8a53d",
      primaryHover:  "#cc8e2c",
      accent:        "#f0c47a",
    },
  },

  "crimson-night": {
    label: "Crimson Night",
    colors: {
      background:    "#1f1113",
      surface:       "#33181c",
      text:          "#f7e6e8",
      textSecondary: "#b3838a",
      border:        "#4a2429",
      shadow:        "rgba(0,0,0,0.5)",
      button:        "#33181c",
      buttonText:    "#f7e6e8",
      primary:       "#e0495a",
      primaryHover:  "#c33547",
      accent:        "#f08a95",
    },
  },

  "slate-graphite": {
    label: "Slate Graphite",
    colors: {
      background:    "#1a1c1f",
      surface:       "#26292e",
      text:          "#eceef0",
      textSecondary: "#8d949c",
      border:        "#383d44",
      shadow:        "rgba(0,0,0,0.45)",
      button:        "#26292e",
      buttonText:    "#eceef0",
      primary:       "#7c93a8",
      primaryHover:  "#647c92",
      accent:        "#a4b8c9",
    },
  },

  "plum-orchid": {
    label: "Plum Orchid",
    colors: {
      background:    "#1e1420",
      surface:       "#301f34",
      text:          "#f5e8f5",
      textSecondary: "#a486a6",
      border:        "#4a2f4e",
      shadow:        "rgba(0,0,0,0.5)",
      button:        "#301f34",
      buttonText:    "#f5e8f5",
      primary:       "#c467bd",
      primaryHover:  "#ab4ea3",
      accent:        "#e29bd9",
    },
  },

  "arctic-cyan": {
    label: "Arctic Cyan",
    colors: {
      background:    "#0d1a1f",
      surface:       "#152730",
      text:          "#e3f3f8",
      textSecondary: "#7c9fab",
      border:        "#22404c",
      shadow:        "rgba(0,0,0,0.45)",
      button:        "#152730",
      buttonText:    "#e3f3f8",
      primary:       "#3fc1e0",
      primaryHover:  "#2ea8c6",
      accent:        "#7fdcf0",
    },
  },

  "copper-forge": {
    label: "Copper Forge",
    colors: {
      background:    "#1e1610",
      surface:       "#332419",
      text:          "#f7ebe0",
      textSecondary: "#b3927a",
      border:        "#4a3626",
      shadow:        "rgba(0,0,0,0.45)",
      button:        "#332419",
      buttonText:    "#f7ebe0",
      primary:       "#d97a45",
      primaryHover:  "#bd6534",
      accent:        "#eda06f",
    },
  },

  "midnight-indigo": {
    label: "Midnight Indigo",
    colors: {
      background:    "#12142b",
      surface:       "#1c2044",
      text:          "#e6e8f7",
      textSecondary: "#8286b3",
      border:        "#2d3260",
      shadow:        "rgba(0,0,0,0.5)",
      button:        "#1c2044",
      buttonText:    "#e6e8f7",
      primary:       "#6b74e0",
      primaryHover:  "#555ec7",
      accent:        "#9fa6f0",
    },
  },

  "tomoyo-noir": {
    label: "Tomoyo Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#5b8dd6",
      primaryHover:  "#4874bb",
      accent:        "#8fb2e8",
    },
  },

  "dusk-violet-noir": {
    label: "Dusk Violet Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#a374e0",
      primaryHover:  "#8c5cc9",
      accent:        "#c9a6f0",
    },
  },
  
  "forest-canopy-noir": {
    label: "Forest Canopy Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#5cb87f",
      primaryHover:  "#47a068",
      accent:        "#8fd6a8",
    },
  },
  
  "amber-glow-noir": {
    label: "Amber Glow Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#e8a53d",
      primaryHover:  "#cc8e2c",
      accent:        "#f0c47a",
    },
  },
  
  "crimson-night-noir": {
    label: "Crimson Night Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#e0495a",
      primaryHover:  "#c33547",
      accent:        "#f08a95",
    },
  },
  
  "slate-graphite-noir": {
    label: "Slate Graphite Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#7c93a8",
      primaryHover:  "#647c92",
      accent:        "#a4b8c9",
    },
  },
  
  "plum-orchid-noir": {
    label: "Plum Orchid Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#c467bd",
      primaryHover:  "#ab4ea3",
      accent:        "#e29bd9",
    },
  },
  
  "arctic-cyan-noir": {
    label: "Arctic Cyan Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#3fc1e0",
      primaryHover:  "#2ea8c6",
      accent:        "#7fdcf0",
    },
  },
  
  "copper-forge-noir": {
    label: "Copper Forge Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#d97a45",
      primaryHover:  "#bd6534",
      accent:        "#eda06f",
    },
  },
  
  "midnight-indigo-noir": {
    label: "Midnight Indigo Noir",
    colors: {
      background:    "#0d0d0f",
      surface:       "#18181b",
      text:          "#e8e9ec",
      textSecondary: "#7a7c82",
      border:        "#2a2a2e",
      shadow:        "rgba(0,0,0,0.55)",
      button:        "#18181b",
      buttonText:    "#e8e9ec",
      primary:       "#6b74e0",
      primaryHover:  "#555ec7",
      accent:        "#9fa6f0",
    },
  },

  // ── Light themes ───────────────────────────────────────────────────────────
  // Same main colours as the dark set; only the chrome flips to
  // hue-tinted light surfaces with dark text.

  "tomoyo-light": {
    label: "Tomoyo Light",
    colors: {
      background:    "#eef3fa",
      surface:       "#ffffff",
      text:          "#1a2436",
      textSecondary: "#5a6d8a",
      border:        "#d3deeb",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#e8eef7",
      buttonText:    "#1a2436",
      primary:       "#5b8dd6",
      primaryHover:  "#4874bb",
      accent:        "#8fb2e8",
    },
  },

  "dusk-violet-light": {
    label: "Dusk Violet Light",
    colors: {
      background:    "#f1ebf9",
      surface:       "#ffffff",
      text:          "#241a33",
      textSecondary: "#6f5f87",
      border:        "#ddd2ec",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#efe7f8",
      buttonText:    "#241a33",
      primary:       "#a374e0",
      primaryHover:  "#8c5cc9",
      accent:        "#c9a6f0",
    },
  },

  "forest-canopy-light": {
    label: "Forest Canopy Light",
    colors: {
      background:    "#e9f3ee",
      surface:       "#ffffff",
      text:          "#16281f",
      textSecondary: "#57766a",
      border:        "#cfdfd5",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#e6f1ea",
      buttonText:    "#16281f",
      primary:       "#5cb87f",
      primaryHover:  "#47a068",
      accent:        "#8fd6a8",
    },
  },

  "amber-glow-light": {
    label: "Amber Glow Light",
    colors: {
      background:    "#faf3e6",
      surface:       "#ffffff",
      text:          "#2f2413",
      textSecondary: "#7a6a4d",
      border:        "#e8dcc2",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#f5eddc",
      buttonText:    "#2f2413",
      primary:       "#e8a53d",
      primaryHover:  "#cc8e2c",
      accent:        "#f0c47a",
    },
  },

  "crimson-night-light": {
    label: "Crimson Night Light",
    colors: {
      background:    "#faecee",
      surface:       "#ffffff",
      text:          "#33191d",
      textSecondary: "#8a5f66",
      border:        "#ecd3d7",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#f7e9eb",
      buttonText:    "#33191d",
      primary:       "#e0495a",
      primaryHover:  "#c33547",
      accent:        "#f08a95",
    },
  },

  "slate-graphite-light": {
    label: "Slate Graphite Light",
    colors: {
      background:    "#eef0f2",
      surface:       "#ffffff",
      text:          "#1d2126",
      textSecondary: "#5f6b76",
      border:        "#d5dade",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#e9edf0",
      buttonText:    "#1d2126",
      primary:       "#7c93a8",
      primaryHover:  "#647c92",
      accent:        "#a4b8c9",
    },
  },

  "plum-orchid-light": {
    label: "Plum Orchid Light",
    colors: {
      background:    "#f8ecf7",
      surface:       "#ffffff",
      text:          "#2e1c2e",
      textSecondary: "#7e5f7e",
      border:        "#e9d4e8",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#f4e7f3",
      buttonText:    "#2e1c2e",
      primary:       "#c467bd",
      primaryHover:  "#ab4ea3",
      accent:        "#e29bd9",
    },
  },

  "arctic-cyan-light": {
    label: "Arctic Cyan Light",
    colors: {
      background:    "#e9f4f8",
      surface:       "#ffffff",
      text:          "#14252c",
      textSecondary: "#54707b",
      border:        "#ccdee6",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#e4f0f5",
      buttonText:    "#14252c",
      primary:       "#3fc1e0",
      primaryHover:  "#2ea8c6",
      accent:        "#7fdcf0",
    },
  },

  "copper-forge-light": {
    label: "Copper Forge Light",
    colors: {
      background:    "#f9efe8",
      surface:       "#ffffff",
      text:          "#2f2115",
      textSecondary: "#7c6650",
      border:        "#e9d6c3",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#f5ebe0",
      buttonText:    "#2f2115",
      primary:       "#d97a45",
      primaryHover:  "#bd6534",
      accent:        "#eda06f",
    },
  },

  "midnight-indigo-light": {
    label: "Midnight Indigo Light",
    colors: {
      background:    "#ebecf8",
      surface:       "#ffffff",
      text:          "#1b1e35",
      textSecondary: "#5d6189",
      border:        "#d2d5ec",
      shadow:        "rgba(30,40,60,0.15)",
      button:        "#e7e9f6",
      buttonText:    "#1b1e35",
      primary:       "#6b74e0",
      primaryHover:  "#555ec7",
      accent:        "#9fa6f0",
    },
  },
};

/** Flat list for ThemeSelector to render swatches. */
export const themeList = Object.entries(themes).map(([value, t]) => ({
  value,
  label: t.label,
  colors: t.colors,
}));

// ─── Stores ───────────────────────────────────────────────────────────────────

export const currentTheme = writable("tomoyo");

// ─── Apply / init ─────────────────────────────────────────────────────────────

/**
 * Apply a theme by key: writes all CSS custom properties on :root,
 * updates the store, and persists to localStorage.
 * @param {string} themeKey  key in `themes`
 */
  export function applyTheme(themeKey) {
      const theme = themes[themeKey];
      if (!theme) return;
  
      const root = document.documentElement;
      for (const [k, v] of Object.entries(theme.colors))
          root.style.setProperty(`--theme-${k}`, v);
      // Native controls/scrollbars follow the theme family; dark themes
      // keep the browser default (unchanged existing behavior). The mode
      // attribute also scopes light-only contrast overrides in app.css.
      if (themeKey.endsWith("-light")) {
          root.style.colorScheme = "light";
          root.dataset.themeMode = "light";
      } else {
          root.style.removeProperty("color-scheme");
          root.dataset.themeMode = "dark";
      }
  
     currentTheme.set(themeKey);
     localStorage.setItem("tomoyo-theme", themeKey);
 }

/** Read persisted theme and apply it. Call once in the root layout's onMount. */
export function initializeTheme() {
  const themeKey = localStorage.getItem("tomoyo-theme") ?? "tomoyo";
  applyTheme(themeKey);
}

/**
 * Paired-theme counterpart of a theme key: light themes strip `-light`,
 * dark themes gain it. Noir (near-black) variants pair with the plain
 * light theme (`tomoyo-noir` -> `tomoyo-light`). Null when the
 * counterpart doesn't exist.
 * @param {string|null|undefined} themeKey
 * @returns {string|null}
 */
export function oppositeThemeKey(themeKey) {
  if (!themeKey || typeof themeKey !== "string") return null;
  let base = themeKey;
  if (base.endsWith("-noir")) base = base.slice(0, -"-noir".length);
  const flipped = base.endsWith("-light")
    ? base.slice(0, -"-light".length)
    : `${base}-light`;
  return themes[flipped] ? flipped : null;
}

/**
 * Inline CSS variables painting a subtree in the given theme family.
 * Empty string when the key is unknown (caller renders normally).
 * @param {string|null|undefined} themeKey
 * @returns {string}
 */
export function themeVars(themeKey) {
  const theme = themeKey ? themes[themeKey] : null;
  if (!theme) return "";
  return Object.entries(theme.colors)
    .map(([k, v]) => `--theme-${k}: ${v};`)
    .join(" ");
}

/**
 * Inline CSS variables painting a subtree in the opposite theme family
 * (sentence-window inversion): same main colours, flipped chrome.
 * Empty string when no counterpart exists (caller renders normally).
 * Root-level light/dark contrast overrides still follow the global mode.
 * @param {string|null|undefined} themeKey
 * @returns {string}
 */
export function invertedThemeVars(themeKey) {
  return themeVars(oppositeThemeKey(themeKey));
}