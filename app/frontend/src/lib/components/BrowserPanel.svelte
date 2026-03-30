<script lang="ts">
  import type { NodeView } from '../types'
  import { appView, currentNode } from '../stores'
  import ActionsBar from './ActionsBar.svelte'
  import BoardBar from './BoardBar.svelte'
  import ChanceCards from './ChanceCards.svelte'
  import HandsTable from './HandsTable.svelte'
  import RangeGrid from './RangeGrid.svelte'
  import SolveOverlay from './SolveOverlay.svelte'

  $: node = $currentNode as NodeView | null
  $: view = $appView
</script>

<div class="browser-panel">
  {#if view === 'empty'}
    <div class="browser-empty">
      Configure a spot and solve to start browsing
    </div>
  {:else if view === 'solving'}
    <SolveOverlay />
  {:else if view === 'browser' && node}
    <BoardBar />

    {#if node.is_terminal}
      <div class="terminal-msg">Showdown / Fold &mdash; navigate back to continue</div>
    {:else if node.is_chance}
      <ChanceCards />
    {:else}
      <ActionsBar />
      <div class="browser-content">
        <RangeGrid />
        <HandsTable />
      </div>
    {/if}
  {/if}
</div>
