<script lang="ts">
  import type { ActionView, NodeView } from '../types'
  import { api } from '../api'
  import { formatActionLabel, formatActionPotPercent, getActionColor, isActionUsed } from '../helpers'
  import { actionColors, breadcrumb, currentNode } from '../stores'

  $: node = $currentNode as NodeView | null
  $: colors = $actionColors
  $: actionViews = node?.actions || [] as ActionView[]
  $: totalPot = node?.total_pot || 0

  function actionName(action: ActionView): string {
    if (action.action_type === 'allin')
      return 'All-in'
    return action.action_type.charAt(0).toUpperCase() + action.action_type.slice(1)
  }

  $: visibleActions = actionViews
    .map((action, index) => ({
      action,
      index,
      label: actionName(action),
      breadcrumbLabel: formatActionLabel(action, totalPot),
      size: formatActionPotPercent(action, totalPot),
      color: colors[index] || getActionColor(action, index),
    }))
    .filter(({ action }) => isActionUsed(action))

  async function playAction(index: number, label: string): Promise<void> {
    if (!node || !node.actions || !node.actions[index])
      return
    const n = await api.play({ action: index })
    $breadcrumb = [...$breadcrumb, label]
    $currentNode = n
    $actionColors = (n.actions || []).map((a, i) => getActionColor(a, i))
  }
</script>

{#if node && visibleActions.length > 0}
  <div class="actions-bar">
    {#each visibleActions as item (item.action.index)}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <div
        class="action-btn"
        on:click={() => playAction(item.index, item.breadcrumbLabel)}
        style="border-bottom: 3px solid {item.color}"
      >
        <div class="a-head">
          <div class="a-label">{item.label}</div>
          {#if item.size}
            <div class="a-size">{item.size}</div>
          {/if}
        </div>
        <div class="a-freq" style="color:{item.color}">{(item.action.frequency * 100).toFixed(1)}%</div>
        <div class="a-ev">EV: {item.action.ev.toFixed(1)}</div>
        <div class="a-bar">
          <div class="fill" style="width:{(item.action.frequency * 100).toFixed(1)}%;background:{item.color}"></div>
        </div>
      </div>
    {/each}
  </div>
{/if}
