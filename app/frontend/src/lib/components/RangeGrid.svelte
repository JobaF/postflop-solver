<script lang="ts">
  import type { ActionView, GridCell, NodeView } from '../types'
  import { buildGradient, formatActionLabel, getActionColor, isActionUsed } from '../helpers'
  import { actionColors, currentNode } from '../stores'

  const RANKS = 'AKQJT98765432'

  $: node = $currentNode as NodeView | null
  $: colors = $actionColors
  $: grid = node?.grid || []
  $: actions = node?.actions || [] as ActionView[]
  $: totalPot = node?.total_pot || 0
  $: visibleActions = actions
    .map((action, index) => ({
      action,
      index,
      label: formatActionLabel(action, totalPot),
      color: colors[index] || getActionColor(action, index),
    }))
    .filter(({ action }) => isActionUsed(action))

  function cellStyle(cell: GridCell): string {
    if (cell.combos < 0.001)
      return ''
    return `background: ${buildGradient(cell.strategy, colors)}`
  }

  function cellTitle(cell: GridCell): string {
    let tip = `${cell.label} (${cell.combos.toFixed(1)} combos)\n`
    for (const item of visibleActions) {
      const freq = cell.strategy[item.index] || 0
      if (freq > 0.001)
        tip += `${item.label}: ${(freq * 100).toFixed(0)}%\n`
    }
    return tip
  }
</script>

{#if grid.length > 0}
  <div class="grid-panel">
    <div class="range-grid">
      <div class="grid-header"></div>
      {#each RANKS as c (c)}
        <div class="grid-header">{c}</div>
      {/each}
      {#each grid as row, r (r)}
        <div class="grid-header">{RANKS[r]}</div>
        {#each row as cell, c (c)}
          {#if cell.combos < 0.001}
            <div class="grid-cell empty" title={cell.label}>{cell.label}</div>
          {:else}
            <div
              class="grid-cell"
              class:pair={r === c}
              style={cellStyle(cell)}
              title={cellTitle(cell)}
            >{cell.label}</div>
          {/if}
        {/each}
      {/each}
    </div>
    <div class="legend">
      {#each visibleActions as item (item.action.index)}
        <div class="legend-item">
          <div class="legend-swatch" style="background:{item.color}"></div>
          {item.label}
        </div>
      {/each}
    </div>
  </div>
{/if}
