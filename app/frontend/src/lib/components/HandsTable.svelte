<script lang="ts">
  import type { ActionView, HandView, NodeView, SortCol } from '../types'
  import { formatActionLabel, isActionUsed } from '../helpers'
  import { actionColors, currentNode } from '../stores'

  $: node = $currentNode as NodeView | null
  $: colors = $actionColors
  $: actions = node?.actions || [] as ActionView[]
  $: totalPot = node?.total_pot || 0
  $: actionLabels = actions.map(action => formatActionLabel(action, totalPot))

  let filter = ''
  let sortCol: SortCol = 'ev'
  let sortDir = -1

  $: hands = sortAndFilter(node?.hands || [], filter, sortCol, sortDir)

  function sortAndFilter(allHands: HandView[], f: string, col: SortCol, dir: number): HandView[] {
    let h = allHands
    if (f) {
      const upper = f.toUpperCase()
      h = h.filter(hand => hand.hand.toUpperCase().includes(upper))
    }
    return [...h].sort((a, b) => {
      if (col === 'hand')
        return dir * a.hand.localeCompare(b.hand)
      const va = a[col]
      const vb = b[col]
      return dir * (va - vb)
    })
  }

  function doSort(col: SortCol): void {
    if (sortCol === col) {
      sortDir *= -1
    }
    else {
      sortCol = col
      sortDir = col === 'hand' ? 1 : -1
    }
  }
</script>

<div class="hands-panel">
  <div class="hand-filter">
    <input type="text" bind:value={filter} placeholder="Filter hands (e.g. AK, QQ)">
  </div>
  <table class="hand-table">
    <thead>
      <tr>
        <th on:click={() => doSort('hand')}>Hand</th>
        <th on:click={() => doSort('equity')}>Equity</th>
        <th on:click={() => doSort('ev')}>EV</th>
        <th on:click={() => doSort('weight')}>Combos</th>
        <th>Strategy</th>
        <th>Details</th>
      </tr>
    </thead>
    <tbody>
      {#each hands as h (h.hand)}
        <tr>
          <td><strong>{h.hand}</strong></td>
          <td>{(h.equity * 100).toFixed(1)}%</td>
          <td>{h.ev.toFixed(1)}</td>
          <td>{h.weight.toFixed(1)}</td>
          <td>
            <div class="strat-bar">
              {#each h.strategy as s, a (a)}
                {#if s > 0.001 && a < actions.length && isActionUsed(actions[a])}
                  <div
                    style="width:{(s * 100).toFixed(1)}%;background:{colors[a]}"
                    title="{actionLabels[a]}: {(s * 100).toFixed(0)}%"
                  ></div>
                {/if}
              {/each}
            </div>
          </td>
          <td class="details">
            {#each h.strategy as s, a (a)}
              {#if s > 0.005 && a < actions.length && isActionUsed(actions[a])}
                <span>{actionLabels[a]}: {(s * 100).toFixed(0)}% ({h.ev_detail[a].toFixed(1)})</span>
              {/if}
            {/each}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
