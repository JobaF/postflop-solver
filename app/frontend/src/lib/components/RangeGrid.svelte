<script lang="ts">
  import type { ActionView, GridCell, NodeView } from '../types'
  import { ACTION_HOVER_MATCH_THRESHOLD, buildGradient, formatActionLabel, formatCardSmall, getActionColor, isActionUsed, suitClass } from '../helpers'
  import {
    actionColors,
    comboCoverage,
    currentNode,
    handsPanelTab,
    hoveredActionIndex,
    hoveredMatrixLabels,
    hoveredRunoutCard,
    runoutChartBars,
    runoutChartError,
    runoutChartLegend,
    runoutChartLoading,
  } from '../stores'

  const RANKS = 'AKQJT98765432'

  $: node = $currentNode as NodeView | null
  $: colors = $actionColors
  $: grid = node?.grid || []
  $: actions = node?.actions || [] as ActionView[]
  $: hoveredAction = $hoveredActionIndex
  $: hoveredLabels = $hoveredMatrixLabels
  $: coverage = $comboCoverage
  $: totalPot = node?.total_pot || 0
  let runoutLoading = false
  let runoutError = ''
  let runoutLegend = [] as { key: string, label: string, color: string }[]
  let runoutBars = [] as { card: string, segments: { key: string, value: number, color: string }[], tooltip: string }[]
  $: visibleActions = actions
    .map((action, index) => ({
      action,
      index,
      label: formatActionLabel(action, totalPot),
      color: colors[index] || getActionColor(action, index),
    }))
    .filter(({ action }) => isActionUsed(action))
  $: isRunoutMode = $handsPanelTab === 'runouts'
  $: runoutLoading = $runoutChartLoading
  $: runoutError = $runoutChartError
  $: runoutLegend = $runoutChartLegend
  $: runoutBars = $runoutChartBars

  function cellBackgroundStyle(cell: GridCell, actionIndex: number | null): string {
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

  function cellReachShare(cell: GridCell): number {
    const player = node?.current_player
    if (player !== 0 && player !== 1)
      return 1
    const byLabel = coverage[player]
    const share = byLabel?.[cell.label]
    if (share === undefined)
      return 1
    return Math.max(0, Math.min(1, share))
  }

  function cellFillStyle(cell: GridCell, actionIndex: number | null): string {
    const fillPct = (cellReachShare(cell) * 100).toFixed(1)
    return `height: ${fillPct}%; ${cellBackgroundStyle(cell, actionIndex)}`
  }

  function cellTitle(cell: GridCell, actionIndex: number | null): string {
    let tip = `${cell.label} (${cell.combos.toFixed(1)} combos)\n`
    const reach = cellReachShare(cell)
    if (reach < 0.999)
      tip += `Reach: ${(reach * 100).toFixed(1)}%\n`
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

  function hoverRunoutCard(card: string): void {
    $hoveredRunoutCard = card
  }
</script>

{#if grid.length > 0}
  <div class="grid-panel">
    {#if isRunoutMode}
      <div class="runout-grid-view">
        <div class="runout-head">
          <div class="runout-title">Alternative Card Strategies</div>
          <div class="runout-subtitle">Hover a card to inspect details in the right panel.</div>
        </div>
        {#if runoutLegend.length > 0}
          <div class="filters-legend compact">
            {#each runoutLegend as item (item.key)}
              <span class="legend-chip">
                <span class="legend-dot" style="background:{item.color}"></span>{item.label}
              </span>
            {/each}
          </div>
        {/if}
        {#if runoutLoading}
          <div class="runout-empty">Loading runout strategies...</div>
        {:else if runoutError}
          <div class="runout-empty">{runoutError}</div>
        {:else if runoutBars.length === 0}
          <div class="runout-empty">No runout strategy data found.</div>
        {:else}
          <div class="runout-chart-wrap runout-chart-wrap-large">
            <div class="runout-y-axis">
              <span>100%</span>
              <span>75%</span>
              <span>50%</span>
              <span>25%</span>
              <span>0%</span>
            </div>
            <div class="runout-chart-scroll">
              <div class="runout-chart runout-chart-large">
                {#each runoutBars as bar (bar.card)}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div class="runout-col runout-col-large" title={bar.tooltip} on:mouseenter={() => hoverRunoutCard(bar.card)}>
                    <div class="runout-bar runout-bar-large" class:active={bar.card === $hoveredRunoutCard}>
                      {#if bar.segments.length === 0}
                        <div class="runout-bar-empty"></div>
                      {:else}
                        {#each bar.segments as segment (segment.key)}
                          <div
                            class="runout-seg"
                            style="height:{(segment.value * 100).toFixed(1)}%;background:{segment.color}"
                          ></div>
                        {/each}
                      {/if}
                    </div>
                    <div class="runout-card-wrap">
                      <span class="runout-card playing-card {suitClass(bar.card)}">{formatCardSmall(bar.card)}</span>
                    </div>
                  </div>
                {/each}
              </div>
            </div>
          </div>
        {/if}
      </div>
    {:else}
      <div class="range-grid-area">
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
                  title={cellTitle(cell, hoveredAction)}
                >
                  <div class="grid-cell-fill" style={cellFillStyle(cell, hoveredAction)}></div>
                  <span class="grid-cell-label">{cell.label}</span>
                </div>
              {/if}
            {/each}
          {/each}
        </div>
      </div>
      <div class="legend">
        {#each visibleActions as item (item.action.index)}
          <div class="legend-item">
            <div class="legend-swatch" style="background:{item.color}"></div>
            {item.label}
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}
