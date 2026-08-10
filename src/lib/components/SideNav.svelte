<!-- SideNav.svelte -->
<script>
  // Add any shared nav logic or props here if needed
</script>

<div class="logo">
    <img src="/tomoyo_full.png" alt="tomoyo" />
</div>
<nav class="side-nav" aria-label="App navigation">
  <div class="nav-actions">
    <slot />
  </div>
</nav>