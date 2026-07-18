import { writable } from "svelte/store";

/** @type {Record<string, { label: string, colors: Record<string, string> }>} */
export const themes = {
  // ── Dark themes ────────────────────────────────────────────────────────────
  "tomoyo": {
    label: "tomoyo",
    colors: {
      background: "#1a1a1a",
      surface: "#2d2d2d",
      text: "#ffffff",
      textSecondary: "#b3b3b3",
      border: "#404040",
      shadow: "rgba(0,0,0,0.3)",
      button: "#2d2d2d",
      buttonText: "#ffffff",
      primary: "#1ea2e0",
      primaryHover: "#178fc5",
      accent: "#63c8f0"
    },
  },
  
  "tokyo-night": {
    label: "Tokyo Night",
    colors: {
      background:    "#1a1b2e",
      surface:       "#24253f",
      text:          "#c0caf5",
      textSecondary: "#565f89",
      border:        "#32334a",
      shadow:        "rgba(0,0,0,0.4)",
      button:        "#24253f",
      buttonText:    "#c0caf5",
      primary:       "#7aa2f7",
      primaryHover:  "#5d87f5",
      accent:        "#bb9af7",
    },
  },
 
  "catppuccin-mocha": {
    label: "Catppuccin Mocha",
    colors: {
      background:    "#1e1e2e",
      surface:       "#313244",
      text:          "#cdd6f4",
      textSecondary: "#6c7086",
      border:        "#45475a",
      shadow:        "rgba(0,0,0,0.4)",
      button:        "#313244",
      buttonText:    "#cdd6f4",
      primary:       "#cba6f7",
      primaryHover:  "#b48ef5",
      accent:        "#89b4fa",
    },
  },
 
  "gruvbox-dark": {
      "label": "Gruvbox Dark",
      "colors": {
        "background": "#282828",
        "surface": "#3c3836",
        "text": "#ebdbb2",
        "textSecondary": "#a89984",
        "border": "#504945",
        "shadow": "rgba(0,0,0,0.4)",
        "button": "#504945",
        "buttonText": "#ebdbb2",
        "primary": "#fabd2f",
        "primaryHover": "#d79921",
        "accent": "#d79921"
      }
    },
 
  "nord": {
    label: "Nord",
    colors: {
      background:    "#2e3440",
      surface:       "#3b4252",
      text:          "#eceff4",
      textSecondary: "#4c566a",
      border:        "#434c5e",
      shadow:        "rgba(0,0,0,0.4)",
      button:        "#3b4252",
      buttonText:    "#eceff4",
      primary:       "#88c0d0",
      primaryHover:  "#6fafc2",
      accent:        "#81a1c1",
    },
  },
 
  "cyber-pink": {
      "label": "Cyber Pink (Dark)",
      "colors": {
        "background": "#1a161d",
        "surface": "#2d242f",
        "text": "#fdeef4",
        "textSecondary": "#a18894",
        "border": "#4a3b45",
        "shadow": "rgba(0,0,0,0.5)",
        "button": "#2d242f",
        "buttonText": "#fdeef4",
        "primary": "#ff75a8",
        "primaryHover": "#ff5293",
        "accent": "#ff9ec2"
      }
  },

  "crimson-ruby": {
      "label": "Crimson Ruby (Dark)",
      "colors": {
        "background": "#1a0a0a",
        "surface": "#2d1212",
        "text": "#f5e6e6",
        "textSecondary": "#a07a7a",
        "border": "#4a1c1c",
        "shadow": "rgba(0,0,0,0.6)",
        "button": "#2d1212",
        "buttonText": "#f5e6e6",
        "primary": "#d93030",
        "primaryHover": "#b02626",
        "accent": "#ff6b6b"
      }
  },

  "everforest": {
    "label": "Everforest",
    "colors": {
      "background": "#2d353b",
      "surface": "#374145",
      "text": "#d3c6aa",
      "textSecondary": "#859289",
      "border": "#475258",
      "shadow": "rgba(0,0,0,0.3)",
      "button": "#374145",
      "buttonText": "#d3c6aa",
      "primary": "#a7c080",
      "primaryHover": "#91a870",
      "accent": "#8DC767"
    }
  },

 
  // ── Light themes ───────────────────────────────────────────────────────────
  
    "github-light": {
        "label": "GitHub Light",
        "colors": {
          "background": "#ffffff",
          "surface": "#f6f8fa",
          "text": "#24292f",
          "textSecondary": "#57606a",
          "border": "#d0d7de",
          "shadow": "rgba(0,0,0,0.08)",
          "button": "#f6f8fa",
          "buttonText": "#24292f",
          "primary": "#0969da",
          "primaryHover": "#0857b3",
          "accent": "#54aeff"
        }
  },

  "warm-sand": {
      "label": "Warm Sand (Light)",
      "colors": {
        "background": "#fdf6e3",
        "surface": "#fbf1d3",
        "text": "#5d554a",
        "textSecondary": "#9e917d",
        "border": "#e4d9c0",
        "shadow": "rgba(0,0,0,0.05)",
        "button": "#fbf1d3",
        "buttonText": "#5d554a",
        "primary": "#b88835",
        "primaryHover": "#9a722c",
        "accent": "#d6a54f"
      }
  },

  "sage-meadow": {
      "label": "Sage Meadow (Light)",
      "colors": {
        "background": "#f4f7f2",
        "surface": "#eaf0e6",
        "text": "#3d4a3d",
        "textSecondary": "#7a8a7a",
        "border": "#d2dbd0",
        "shadow": "rgba(0,0,0,0.05)",
        "button": "#eaf0e6",
        "buttonText": "#3d4a3d",
        "primary": "#6c8a6c",
        "primaryHover": "#566e56",
        "accent": "#9ab59a"
      }
  },

  "terracotta-clay": {
      "label": "Terracotta Clay (Light)",
      "colors": {
        "background": "#fffaf7",
        "surface": "#f5ece7",
        "text": "#4a3b35",
        "textSecondary": "#8c766d",
        "border": "#e8d8d0",
        "shadow": "rgba(0,0,0,0.06)",
        "button": "#f5ece7",
        "buttonText": "#4a3b35",
        "primary": "#cc6b49",
        "primaryHover": "#a8593d",
        "accent": "#e69275"
      }
  },
  "lavender-mist": {
      "label": "Lavender Mist (Light)",
      "colors": {
        "background": "#f8f7ff",
        "surface": "#efedfc",
        "text": "#484063",
        "textSecondary": "#8b85a3",
        "border": "#dcd8f2",
        "shadow": "rgba(0,0,0,0.05)",
        "button": "#efedfc",
        "buttonText": "#484063",
        "primary": "#8676c9",
        "primaryHover": "#6d5db5",
        "accent": "#a99ce0"
      }
    },
    "berry-blush": {
      "label": "Berry Blush (Light)",
      "colors": {
        "background": "#fff5f5",
        "surface": "#ffecec",
        "text": "#633d3d",
        "textSecondary": "#a37a7a",
        "border": "#f2dcdc",
        "shadow": "rgba(0,0,0,0.05)",
        "button": "#ffecec",
        "buttonText": "#633d3d",
        "primary": "#d94e4e",
        "primaryHover": "#b53d3d",
        "accent": "#f08585"
      }
    }
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