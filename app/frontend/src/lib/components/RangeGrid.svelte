<script lang="ts">
  import type { ActionView, GridCell, NodeView } from '../types'
  import { ACTION_HOVER_MATCH_THRESHOLD, buildGradient, formatActionLabel, getActionColor, isActionUsed } from '../helpers'
  import { actionColors, currentNode, hoveredActionIndex, hoveredMatrixLabels } from '../stores'

  const RANKS = 'AKQJT98765432'

  $: node = $currentNode as NodeView | null
  $: colors = $actionColors
  $: grid = node?.grid || []
  $: actions = node?.actions || [] as ActionView[]
  $: hoveredAction = $hoveredActionIndex
  $: hoveredLabels = $hoveredMatrixLabels
  $: totalPot = node?.total_pot || 0
  $: visibleActions = actions
    .map((action, index) => ({
      action,
      index,
      label: formatActionLabel(action, totalPot),
      color: colors[index] || getActionColor(action, index),
    }))
    .filter(({ action }) => isActionUsed(action))

  function cellStyle(cell: GridCell, actionIndex: number | null): string {
    if (cell.combos < 0.001)
      return ''
    if (actionIndex !== null && actionIndex < actions.length) {
      const freq = Math.max(0, Math.min(1, cell.strategy[actionIndex] || 0))
      const pct = (freq * 100).toFixed(1)
      const color = colors[actionIndex] || getActionColor(actions[actionIndex], actionIndex)
      return `background: linear-gradient(to right, ${color} 0% ${pct}%, var(--bg3) ${pct}% 100%)`
    }
    return `background: ${buildGradient(cell.strategy, colors)}`
  }

  function cellTitle(cell: GridCell, actionIndex: number | null): string {
    let tip = `${cell.label} (${cell.combos.toFixed(1)} combos)\n`
    if (actionIndex !== null && actionIndex < actions.length) {
      const item = visibleActions.find(a => a.index === actionIndex)
      const freq = cell.strategy[actionIndex] || 0
      if (item)
        tip += `${item.label}: ${(freq * 100).toFixed(0)}%\n`
      return tip
    }
    for (const item of visibleActions) {
      const freq = cell.strategy[item.index] || 0
      if (freq > 0.001)
        tip += `${item.label}: ${(freq * 100).toFixed(0)}%\n`
    }
    return tip
  }

  function isDimmed(cell: GridCell, labels: string[] | null, actionIndex: number | null): boolean {
    const dimByCategory = !!labels && labels.length > 0 && !labels.includes(cell.label)
    const dimByAction = actionIndex !== null && (cell.strategy[actionIndex] || 0) < ACTION_HOVER_MATCH_THRESHOLD
    return dimByCategory || dimByAction
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
              class:dimmed={isDimmed(cell, hoveredLabels, hoveredAction)}
              style={cellStyle(cell, hoveredAction)}
              title={cellTitle(cell, hoveredAction)}
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
