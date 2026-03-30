<script lang="ts">
  import type { SpotMeta } from '../types'
  import { onMount } from 'svelte'
  import { api } from '../api'
  import { getActionColor } from '../helpers'
  import {
    actionColors,
    appView,
    breadcrumb,
    canSolve,
    currentNode,
    errorMsg,
    solveInfo,
    statusText,
  } from '../stores'

  let spots: SpotMeta[] = []
  let activeId: number | null = null

  onMount(() => {
    refresh()
  })

  export async function refresh(): Promise<void> {
    try {
      const data = await api.listSpots()
      spots = data.spots
      activeId = data.active_id
    }
    catch { /* ignore */ }
  }

  async function loadSpot(id: number): Promise<void> {
    $errorMsg = ''
    try {
      const node = await api.loadSpot(id)
      $currentNode = node
      $breadcrumb = ['Root']
      $actionColors = (node.actions || []).map((a, i) => getActionColor(a, i))
      $appView = 'browser'
      $statusText = 'Browsing saved spot'
      const status = await api.solveStatus()
      if (status.status === 'Done') {
        const pot = node.pot || 1
        const expPct = `${(status.exploitability! / pot * 100).toFixed(2)}%`
        $solveInfo = `Solved in ${status.iterations} iterations | Exploitability: ${expPct} pot`
      }
      $canSolve = true
      await refresh()
    }
    catch (e) {
      $errorMsg = (e as Error).message
    }
  }

</script>

<div class="section">
  <h3>Solved Spots</h3>
  <div class="spot-list">
    {#if spots.length === 0}
      <span class="spot-empty">No solved spots yet</span>
    {:else}
      {#each spots as s (s.id)}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div
          class="spot-item"
          class:active={s.id === activeId}
          on:click={() => loadSpot(s.id)}
          title="{s.oop_range} vs {s.ip_range}"
        >
          <span class="spot-label">{s.label}</span>
          <span class="spot-exp">exp:{(s.exploitability / s.pot * 100).toFixed(1)}%</span>
        </div>
      {/each}
    {/if}
  </div>
</div>
