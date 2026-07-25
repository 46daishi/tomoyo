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

  currentTheme.set(themeKey);
  localStorage.setItem("tomoyo-theme", themeKey);
}

/** Read persisted theme and apply it. Call once in the root layout's onMount. */
export function initializeTheme() {
  const themeKey = localStorage.getItem("tomoyo-theme") ?? "tomoyo";
  applyTheme(themeKey);
}