<script lang="ts">
  import BrowserPanel from './lib/components/BrowserPanel.svelte'
  import ConfigPanel from './lib/components/ConfigPanel.svelte'
  import { statusText } from './lib/stores'
</script>

<header>
  <h1>Postflop Solver</h1>
  <div id="status-bar">{$statusText}</div>
</header>

<div class="main">
  <ConfigPanel />
  <BrowserPanel />
</div>
