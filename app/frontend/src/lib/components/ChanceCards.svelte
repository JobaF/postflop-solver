<script lang="ts">
  import type { NodeView } from '../types'
  import { api } from '../api'
  import { formatCardSmall, getActionColor, suitClass } from '../helpers'
  import { actionColors, breadcrumb, currentNode } from '../stores'

  $: node = $currentNode as NodeView | null

  async function dealCard(cardStr: string): Promise<void> {
    if (!node || !node.is_chance)
      return
    const n = await api.play({ card: cardStr })
    $breadcrumb = [...$breadcrumb, cardStr]
    $currentNode = n
    $actionColors = (n.actions || []).map((a, i) => getActionColor(a, i))
  }
</script>

{#if node && node.is_chance}
  <div class="chance-section">
    <div class="chance-label">Select a card to deal:</div>
    <div class="chance-cards">
      {#each node.possible_cards as c (c)}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
        <div
          class="chance-card {suitClass(c)}"
          on:click={() => dealCard(c)}
        >{formatCardSmall(c)}</div>
      {/each}
    </div>
  </div>
{/if}
