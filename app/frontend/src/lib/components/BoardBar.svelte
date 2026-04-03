<script lang="ts">
  import type { NodeView } from '../types'
  import { api } from '../api'
  import { getActionColor, suitClass, suitSymbol } from '../helpers'
  import { actionColors, breadcrumb, currentNode } from '../stores'

  $: node = $currentNode as NodeView | null

  async function goRoot(): Promise<void> {
    const n = await api.root()
    $currentNode = n
    $breadcrumb = ['Root']
    $actionColors = (n.actions || []).map((a, i) => getActionColor(a, i))
  }

  async function goBack(): Promise<void> {
    const n = await api.back()
    if ($breadcrumb.length > 1)
      $breadcrumb = $breadcrumb.slice(0, -1)
    $currentNode = n
    $actionColors = (n.actions || []).map((a, i) => getActionColor(a, i))
  }

  async function goToDepth(depth: number): Promise<void> {
    let n = $currentNode
    while (n && n.history_depth > depth) {
      n = await api.back()
    }
    $currentNode = n
    $breadcrumb = $breadcrumb.slice(0, depth + 1)
    $actionColors = ((n?.actions) || []).map((a, i) => getActionColor(a, i))
  }
</script>

{#if node}
  <div class="board-bar">
    <div class="board-cards">
      {#each node.board as c (c)}
        <span class="card playing-card {suitClass(c)}">{c[0]}{suitSymbol(c[1])}</span>
      {/each}
    </div>
    <div class="nav-info">
      {#if !node.is_terminal && !node.is_chance}
        <span class="player-badge {node.player_name.toLowerCase()}">{node.player_name}</span>
        <span>Equity: <span class="stat-val">{(node.range_equity * 100).toFixed(1)}%</span></span>
        <span>EV: <span class="stat-val">{node.range_ev.toFixed(1)}</span></span>
      {/if}
      <span>Pot: <span class="stat-val">{node.total_pot}</span></span>
      {#if node.is_terminal}
        <span class="stat-val terminal">Terminal</span>
      {/if}
      {#if node.is_chance}
        <span class="stat-val chance">Chance Node</span>
      {/if}
    </div>
    <div class="nav-btns">
      <button class="btn btn-sm btn-primary" on:click={goRoot} disabled={node.history_depth === 0}>Root</button>
      <button class="btn btn-sm btn-primary" on:click={goBack} disabled={node.history_depth === 0}>Back</button>
    </div>
  </div>

  <div class="breadcrumb">
    {#each $breadcrumb as b, i (i)}
      {#if i > 0}<span class="sep">&rsaquo;</span>{/if}
      {#if i === 0}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <span on:click={goRoot}>{b}</span>
      {:else if i === $breadcrumb.length - 1}
        <span>{b}</span>
      {:else}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <span on:click={() => goToDepth(i)}>{b}</span>
      {/if}
    {/each}
  </div>
{/if}
