/**
 * chartFormatters.js
 * Shared formatting utilities for all chart components.
 * Eliminates duplication across BarChart, LineChart, DonutChart, and HeatMap.
 */

/**
 * Format a date as "Jan 15" style.
 * @param {string | number} dateInput - ISO date string or timestamp
 * @returns {string}
 */
export function formatDate(dateInput) {
    return new Date(dateInput).toLocaleDateString("en-US", {
        month: "short",
        day: "numeric",
    });
}

/**
 * Format a full date including weekday and year.
 * @param {Date} date
 * @returns {string}
 */
export function formatDateFull(date) {
    return date.toLocaleDateString("ja-JP", {
        weekday: "short",
        month: "short",
        day: "numeric",
        year: "numeric",
    });
}

/**
 * Format minutes as human-readable time.
 * @param {number} m - Minutes
 * @returns {string} e.g. "1h 30m", "45m", "0m"
 */
export function formatMinutes(m) {
    if (!m) return "0m";
    const h = Math.floor(m / 60);
    const min = m % 60;
    return h > 0 ? (min > 0 ? `${h}h ${min}m` : `${h}h`) : `${min}m`;
}

/**
 * Format a Y-axis tick for charts (uses formatMinutes for minute units).
 * @param {number} tick
 * @param {string} unit - "m" for minutes, anything else for plain numbers
 * @returns {string}
 */
export function formatTick(tick, unit = "") {
    if (unit !== "m") return String(tick);
    if (tick === 0) return "0";
    if (tick % 60 === 0) return `${tick / 60}h`;
    return tick >= 60
        ? `${Math.floor(tick / 60)}h${tick % 60}m`
        : `${tick}m`;
}

/**
 * Generic formatter that delegates to formatMinutes or String.
 * @param {number} val
 * @param {string} unit - "m" for minutes
 * @returns {string}
 */
export function formatValue(val, unit = "") {
    return unit === "m" ? formatMinutes(val) : String(val);
}

/**
 * Calculate a "nice" maximum for Y-axis based on the data max.
 * Used by BarChart and LineChart for consistent scale behavior.
 * @param {number} v - The actual max value from data
 * @returns {number} A rounded-up value suitable for axis max
 */
export function calculateNiceMax(v) {
    if (v <= 0) return 10;
    const candidates = [
        1, 2, 5, 10, 15, 20, 25, 30, 60, 90, 120, 150, 180, 240, 300, 360, 480,
    ];
    const rawStep = v / 4;
    const step =
        candidates.find((c) => c >= rawStep) ??
        Math.ceil(rawStep / 60) * 60;
    return step * 4;
}

/**
 * Calculate which X-axis labels to show based on data length.
 * @param {number} dataLength
 * @returns {number} The label step (show every Nth label)
 */
export function calculateLabelStep(dataLength) {
    if (dataLength <= 7) return 1;
    if (dataLength <= 14) return 2;
    if (dataLength <= 21) return 3;
    return Math.ceil(dataLength / 7);
}