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

  "deep-teal": {
    label: "Deep Teal",
    colors: {
      background:    "#0f2020",
      surface:       "#173030",
      text:          "#e2f4f2",
      textSecondary: "#7ba8a4",
      border:        "#264948",
      shadow:        "rgba(0,0,0,0.45)",
      button:        "#173030",
      buttonText:    "#e2f4f2",
      primary:       "#2fb3a6",
      primaryHover:  "#25998e",
      accent:        "#6fd4c6",
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

  // ── Light themes ───────────────────────────────────────────────────────────

  "sakura-light": {
    label: "Sakura (Light)",
    colors: {
      background:    "#fff6f8",
      surface:       "#fce9ed",
      text:          "#4a2c33",
      textSecondary: "#96707a",
      border:        "#f2d3da",
      shadow:        "rgba(0,0,0,0.06)",
      button:        "#fce9ed",
      buttonText:    "#4a2c33",
      primary:       "#d6597e",
      primaryHover:  "#b8446a",
      accent:        "#ea8ba6",
    },
  },

  "mist-blue": {
    label: "Mist Blue (Light)",
    colors: {
      background:    "#f5f9fc",
      surface:       "#e7eff7",
      text:          "#2c3b4a",
      textSecondary: "#748a9e",
      border:        "#d3e0ec",
      shadow:        "rgba(0,0,0,0.05)",
      button:        "#e7eff7",
      buttonText:    "#2c3b4a",
      primary:       "#3d7ab3",
      primaryHover:  "#2f6494",
      accent:        "#6fa3d1",
    },
  },

  "linen-cream": {
    label: "Linen Cream (Light)",
    colors: {
      background:    "#faf6ef",
      surface:       "#f1ead9",
      text:          "#4a4335",
      textSecondary: "#8f8570",
      border:        "#e2d7bf",
      shadow:        "rgba(0,0,0,0.05)",
      button:        "#f1ead9",
      buttonText:    "#4a4335",
      primary:       "#a8823e",
      primaryHover:  "#8c6c30",
      accent:        "#c9a35f",
    },
  },

  "matcha-light": {
    label: "Matcha (Light)",
    colors: {
      background:    "#f5f8f0",
      surface:       "#e8f0dd",
      text:          "#354a2c",
      textSecondary: "#7d9270",
      border:        "#d3e2c3",
      shadow:        "rgba(0,0,0,0.05)",
      button:        "#e8f0dd",
      buttonText:    "#354a2c",
      primary:       "#6f9c47",
      primaryHover:  "#5a8236",
      accent:        "#9bc178",
    },
  },

  "clay-light": {
    label: "Clay (Light)",
    colors: {
      background:    "#fff8f5",
      surface:       "#faeae2",
      text:          "#4a352c",
      textSecondary: "#9c7c6c",
      border:        "#efd9cb",
      shadow:        "rgba(0,0,0,0.05)",
      button:        "#faeae2",
      buttonText:    "#4a352c",
      primary:       "#c9673b",
      primaryHover:  "#ab532c",
      accent:        "#e0906a",
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