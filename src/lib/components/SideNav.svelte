<!-- SideNav.svelte -->
<script>
  // Add any shared nav logic or props here if needed
</script>

<div class="logo">
    <a href="https://x.com/46daishi" target="_blank" rel="noopener noreferrer"><img src="/tomoyo_full.png" alt="tomoyo" /></a>
</div>
<nav class="side-nav" aria-label="App navigation">
  <div class="nav-actions">
    <slot />
  </div>
</nav>