// Moves a node to document.body on mount and restores nothing on destroy
// (the node is removed from the DOM entirely, which is what Svelte expects
// when the {#if} block that rendered it goes away).
//
// Needed for floating UI like the status menu: word-card has
// `transform: translateZ(0)` (for compositing-layer promotion, see hover
// flicker fix), and any `position: fixed` descendant of a transformed
// ancestor is positioned relative to that ancestor instead of the viewport.
// Combined with dict-content's `overflow-y: scroll`, a menu positioned
// "fixed" from inside a card would still get clipped to the card/scroll
// bounds. Portaling to <body> sidesteps both issues.
export function portal(node, target = document.body) {
    target.appendChild(node);

    return {
        destroy() {
            if (node.parentNode) {
                node.parentNode.removeChild(node);
            }
        },
    };
}