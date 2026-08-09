/** Default Discord Rich Presence payload shown when the timer is idle. */
export const PRESENCE_ICONS = {
  statsIcon: "",
  settingsIcon: "",
  immersionIcon: "https://i.imgur.com/yIObFbV.png",
  dictionaryIcon: "https://i.imgur.com/JeVqrIZ.png",
  logo: "https://i.imgur.com/mHL9UhJ.png",
  smallLogo: "https://i.imgur.com/fJB2Acw.png"
};

/** Refers to the first line */
export const PRESENCE_DETAILS = {
  homeDetails: "On the home page",
  mediaDetails: "Viewing media page",
  immersionDetails: "Immersing",
  statsDetails: "Viewing statistics",
  settingsDetails: "Editing Settings",
  dictionaryDetails: "Browsing the dictionary",
  reviewDetailsWords: "Reviewing words",
  reviewDetailsSentences: "Reviewing sentences"
};

export const PRESENCE_DEFAULTS = {
  details: PRESENCE_DETAILS.homeDetails,
  largeImage: PRESENCE_ICONS.logo,
  largeText: "tomoyo - made by 46dai",
  startTimestamp: undefined,
  endTimestamp: undefined,
};
