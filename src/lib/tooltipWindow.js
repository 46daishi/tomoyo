import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getCurrentWindow, LogicalPosition } from '@tauri-apps/api/window';
import { emit } from '@tauri-apps/api/event';

let tooltipWindow = null;

function getTooltipWindow() {
    if (!tooltipWindow) {
        tooltipWindow = WebviewWindow.getByLabel('tooltip');
    }
    return tooltipWindow;
}

export async function showTooltipAt(clientX, clientY, spanData) {
    const tooltip = await getTooltipWindow();
    if (!tooltip) return;

    const mainWindow = getCurrentWindow();
    const mainPos = await mainWindow.outerPosition();
    const scale = await mainWindow.scaleFactor();

    const screenX = mainPos.x + clientX * scale;
    const screenY = mainPos.y + clientY * scale;

    await tooltip.setPosition(new LogicalPosition(screenX / scale, screenY / scale));
    await emit('tooltip-content', spanData);
    await tooltip.show();
}

export async function hideTooltip() {
    const tooltip = await getTooltipWindow();
    if (tooltip) await tooltip.hide();
}