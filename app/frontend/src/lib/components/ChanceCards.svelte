<script lang="ts">
  import type { NodeView } from '../types'
  import { api } from '../api'
  import { formatCardSmall, getActionColor, suitClass } from '../helpers'
  import { actionColors, activePath, breadcrumb, cardToPathIndex, currentNode, pushComboCoverageSnapshot } from '../stores'

  const SUIT_ORDER = ['c', 'd', 'h', 's']
  const SUIT_LABELS: Record<string, string> = {
    c: 'Clubs',
    d: 'Diamonds',
    h: 'Hearts',
    s: 'Spades',
  }
  const RANK_ORDER = 'AKQJT98765432'

  $: node = $currentNode as NodeView | null
  $: stageLabel = node?.board.length === 3 ? 'Turn' : node?.board.length === 4 ? 'River' : 'Next Card'
  $: groupedCards = SUIT_ORDER
    .map(suit => ({
      suit,
      label: SUIT_LABELS[suit],
      cards: (node?.possible_cards || [])
        .filter(card => card[1] === suit)
        .sort((a, b) => RANK_ORDER.indexOf(a[0]) - RANK_ORDER.indexOf(b[0])),
    }))
    .filter(group => group.cards.length > 0)

  async function dealCard(cardStr: string): Promise<void> {
    if (!node || !node.is_chance)
      return
    const n = await api.play({ card: cardStr })
    pushComboCoverageSnapshot()
    $breadcrumb = [...$breadcrumb, cardStr]
    const pathStep = cardToPathIndex(cardStr)
    if (pathStep !== null)
      $activePath = [...$activePath, pathStep]
    $currentNode = n
    $actionColors = (n.actions || []).map((a, i) => getActionColor(a, i))
  }
</script>

{#if node && node.is_chance}
  <div class="chance-section">
    <div class="chance-head">
      <div>
        <div class="chance-title">Select {stageLabel} Card</div>
        <div class="chance-label">{node.possible_cards.length} available cards</div>
      </div>
      <div class="chance-stage">{stageLabel}</div>
    </div>
    <div class="chance-groups">
      {#each groupedCards as group (group.suit)}
        <div class="chance-group">
          <div class="chance-group-title">{group.label}</div>
          <div class="chance-cards">
            {#each group.cards as c (c)}
              <button
                type="button"
                class="chance-card playing-card {suitClass(c)}"
                on:click={() => dealCard(c)}
              >
                {formatCardSmall(c)}
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  </div>
{/if}
