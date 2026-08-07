/**
 * Extracts an ambient accent color from an image, boosted for use as a
 * glow/gradient light (rather than a flat, washed-out average).
 *
 * @param {string} imageSrc
 * @returns {Promise<string|null>} an "rgb(r, g, b)" string, or null on failure
 */
export async function extractDominantColor(imageSrc) {
    if (!imageSrc) return null;

    try {
        const img = await loadImage(imageSrc);

        const size = 32; // downscale — we only need an approximate average
        const canvas = document.createElement('canvas');
        canvas.width = size;
        canvas.height = size;
        const ctx = canvas.getContext('2d', { willReadFrequently: true });
        ctx.drawImage(img, 0, 0, size, size);

        const { data } = ctx.getImageData(0, 0, size, size);

        let r = 0, g = 0, b = 0, count = 0;
        for (let i = 0; i < data.length; i += 4) {
            const alpha = data[i + 3];
            if (alpha < 125) continue; // skip transparent pixels
            r += data[i];
            g += data[i + 1];
            b += data[i + 2];
            count++;
        }

        if (count === 0) return null;

        r = Math.round(r / count);
        g = Math.round(g / count);
        b = Math.round(b / count);

        // Plain pixel averages tend to come out muddy/grey. Push saturation
        // and clamp lightness so the result reads as a "glow" rather than a
        // flat wash.
        const [h, s, l] = rgbToHsl(r, g, b);
        const boostedS = Math.min(1, Math.max(s, 0.45));
        const clampedL = Math.min(0.62, Math.max(0.32, l));
        const [br, bg, bb] = hslToRgb(h, boostedS, clampedL);

        return `rgb(${br}, ${bg}, ${bb})`;
    } catch (err) {
        console.warn('Could not extract cover color:', err);
        return null;
    }
}

function loadImage(src) {
    return new Promise((resolve, reject) => {
        const img = new Image();
        img.crossOrigin = 'anonymous';
        img.onload = () => resolve(img);
        img.onerror = reject;
        img.src = src;
    });
}

function rgbToHsl(r, g, b) {
    r /= 255; g /= 255; b /= 255;
    const max = Math.max(r, g, b), min = Math.min(r, g, b);
    let h = 0, s = 0;
    const l = (max + min) / 2;

    if (max !== min) {
        const d = max - min;
        s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
        switch (max) {
            case r: h = (g - b) / d + (g < b ? 6 : 0); break;
            case g: h = (b - r) / d + 2; break;
            case b: h = (r - g) / d + 4; break;
        }
        h /= 6;
    }
    return [h, s, l];
}

function hslToRgb(h, s, l) {
    let r, g, b;
    if (s === 0) {
        r = g = b = l;
    } else {
        const hue2rgb = (p, q, t) => {
            if (t < 0) t += 1;
            if (t > 1) t -= 1;
            if (t < 1 / 6) return p + (q - p) * 6 * t;
            if (t < 1 / 2) return q;
            if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
            return p;
        };
        const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
        const p = 2 * l - q;
        r = hue2rgb(p, q, h + 1 / 3);
        g = hue2rgb(p, q, h);
        b = hue2rgb(p, q, h - 1 / 3);
    }
    return [Math.round(r * 255), Math.round(g * 255), Math.round(b * 255)];
}