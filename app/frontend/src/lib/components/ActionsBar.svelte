<script lang="ts">
  import type { NodeView } from '../types'
  import { api } from '../api'
  import { getActionColor } from '../helpers'
  import { actionColors, breadcrumb, currentNode } from '../stores'

  $: node = $currentNode as NodeView | null
  $: colors = $actionColors

  async function playAction(index: number): Promise<void> {
    if (!node || !node.actions || !node.actions[index])
      return
    const label = node.actions[index].label
    const n = await api.play({ action: index })
    $breadcrumb = [...$breadcrumb, label]
    $currentNode = n
    $actionColors = (n.actions || []).map((a, i) => getActionColor(a, i))
  }
</script>

{#if node && node.actions && node.actions.length > 0}
  <div class="actions-bar">
    {#each node.actions as a, i (a.label)}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        class="action-btn"
        on:click={() => playAction(i)}
        style="border-bottom: 3px solid {colors[i]}"
      >
        <div class="a-label">{a.label}</div>
        <div class="a-freq" style="color:{colors[i]}">{(a.frequency * 100).toFixed(1)}%</div>
        <div class="a-ev">EV: {a.ev.toFixed(1)}</div>
        <div class="a-bar">
          <div class="fill" style="width:{(a.frequency * 100).toFixed(1)}%;background:{colors[i]}"></div>
        </div>
      </div>
    {/each}
  </div>
{/if}
