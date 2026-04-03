<script lang="ts">
  import type { ActionView, HandView, NodeView, SortCol } from '../types'
  import { onDestroy } from 'svelte'
  import { formatActionLabel, isActionUsed } from '../helpers'
  import { actionColors, currentNode, hoveredMatrixLabels } from '../stores'

  interface ActionMeta {
    index: number
    label: string
    color: string
  }

  interface FilterBucket {
    key: string
    label: string
    combos: number
    sharePct: number
    strategy: number[]
    matrixLabels: string[]
  }

  interface CategoryDef {
    key: string
    label: string
  }

  interface Card {
    rank: number
    suit: string
  }

  interface HandFacts {
    handCategory: string
    drawCategory: string
  }

  const RANKS = '23456789TJQKA'

  const HAND_CATEGORY_DEFS: CategoryDef[] = [
    { key: 'set', label: 'Set' },
    { key: 'overpair', label: 'Overpair' },
    { key: 'top_pair', label: 'Top pair' },
    { key: 'underpair', label: 'Underpair' },
    { key: 'second_pair', label: 'Second pair' },
    { key: 'third_pair', label: 'Third pair' },
    { key: 'ace_high', label: 'Ace high' },
    { key: 'king_high', label: 'King high' },
    { key: 'no_made_hand', label: 'No made hand' },
  ]

  const DRAW_CATEGORY_DEFS: CategoryDef[] = [
    { key: 'combo_draw', label: 'Combo draw' },
    { key: 'flush_draw', label: 'Flush draw' },
    { key: 'straight_draw', label: 'Straight draw' },
    { key: 'bdfd_2cards', label: 'BDFD 2 cards' },
    { key: 'no_draw', label: 'No draw' },
  ]

  const EQUITY_BUCKET_DEFS = [
    { key: 'eq90_100', label: 'Equity 90-100', low: 90, high: 101 },
    { key: 'eq80_90', label: 'Equity 80-90', low: 80, high: 90 },
    { key: 'eq70_80', label: 'Equity 70-80', low: 70, high: 80 },
    { key: 'eq60_70', label: 'Equity 60-70', low: 60, high: 70 },
    { key: 'eq50_60', label: 'Equity 50-60', low: 50, high: 60 },
    { key: 'eq25_50', label: 'Equity 25-50', low: 25, high: 50 },
    { key: 'eq0_25', label: 'Equity 0-25', low: 0, high: 25 },
  ]

  $: node = $currentNode as NodeView | null
  $: colors = $actionColors
  $: actions = node?.actions || [] as ActionView[]
  $: totalPot = node?.total_pot || 0
  $: actionLabels = actions.map(action => formatActionLabel(action, totalPot))
  $: visibleActions = actions
    .map((action, index) => ({ action, index }))
    .filter(({ action }) => isActionUsed(action))
    .map(({ index }) => ({
      index,
      label: actionLabels[index],
      color: colors[index] || 'var(--text2)',
    })) as ActionMeta[]

  let filter = ''
  let sortCol: SortCol = 'ev'
  let sortDir = -1
  let activeTab: 'hands' | 'filters' = 'hands'
  let filterMode: 'include' | 'exclude' = 'include'
  let hoveredBucketKey: string | null = null
  let lockedBucketKey: string | null = null

  $: hands = sortAndFilter(node?.hands || [], filter, sortCol, sortDir)
  $: totalWeight = (node?.hands || []).reduce((sum, hand) => sum + hand.weight, 0)
  $: boardCards = parseBoard(node?.board || [])
  $: handFacts = buildHandFacts(node?.hands || [], boardCards)
  $: handCategoryBuckets = summarizeCategoryBuckets(HAND_CATEGORY_DEFS, node?.hands || [], handFacts, 'handCategory', actions.length, totalWeight)
  $: drawCategoryBuckets = summarizeCategoryBuckets(DRAW_CATEGORY_DEFS, node?.hands || [], handFacts, 'drawCategory', actions.length, totalWeight)
  $: equityBuckets = buildEquityBuckets(node?.hands || [], actions.length, totalWeight)
  $: allFilterBuckets = [...handCategoryBuckets, ...drawCategoryBuckets, ...equityBuckets]
  $: activeBucketKey = lockedBucketKey || hoveredBucketKey
  $: activeBucket = allFilterBuckets.find(bucket => bucket.key === activeBucketKey) || null
  $: isFiltersView = !!node && activeTab === 'filters'
  $: if (!isFiltersView) {
    hoveredBucketKey = null
    lockedBucketKey = null
  }
  $: $hoveredMatrixLabels = isFiltersView && activeBucket ? activeBucket.matrixLabels : null

  onDestroy(() => {
    $hoveredMatrixLabels = null
  })

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

  function parseCard(card: string): Card | null {
    if (!card || card.length < 2)
      return null
    const rank = RANKS.indexOf(card[0]) + 2
    if (rank < 2)
      return null
    return { rank, suit: card[1] }
  }

  function parseBoard(board: string[]): Card[] {
    return board.map(parseCard).filter(c => c !== null) as Card[]
  }

  function parseHandCards(label: string): [Card, Card] | null {
    if (!label || label.length < 4)
      return null
    const c1 = parseCard(label.slice(0, 2))
    const c2 = parseCard(label.slice(2, 4))
    if (!c1 || !c2)
      return null
    return [c1, c2]
  }

  function rankToChar(rank: number): string {
    return RANKS[rank - 2] || ''
  }

  function handToMatrixLabel(label: string): string | null {
    const cards = parseHandCards(label)
    if (!cards)
      return null

    const [c1, c2] = cards
    if (c1.rank === c2.rank) {
      const rank = rankToChar(c1.rank)
      return `${rank}${rank}`
    }

    const first = c1.rank >= c2.rank ? c1 : c2
    const second = c1.rank >= c2.rank ? c2 : c1
    const suffix = c1.suit === c2.suit ? 's' : 'o'
    return `${rankToChar(first.rank)}${rankToChar(second.rank)}${suffix}`
  }

  function hasStraight(rankPresence: boolean[]): boolean {
    for (let start = 1; start <= 10; start++) {
      let ok = true
      for (let d = 0; d < 5; d++) {
        if (!rankPresence[start + d]) {
          ok = false
          break
        }
      }
      if (ok)
        return true
    }
    return false
  }

  function toStraightPresence(cards: Card[]): boolean[] {
    const rankPresence = Array.from({ length: 15 }).fill(false) as boolean[]
    for (const card of cards) {
      rankPresence[card.rank] = true
      if (card.rank === 14)
        rankPresence[1] = true
    }
    return rankPresence
  }

  function straightOutRanks(cards: Card[]): number {
    const base = toStraightPresence(cards)
    if (hasStraight(base))
      return 0

    let outRanks = 0
    for (let rank = 2; rank <= 14; rank++) {
      const withOut = [...base]
      withOut[rank] = true
      if (rank === 14)
        withOut[1] = true
      if (hasStraight(withOut))
        outRanks++
    }
    return outRanks
  }

  function classifyHandCategory(cards: [Card, Card], board: Card[]): string {
    const [c1, c2] = cards
    const isPocketPair = c1.rank === c2.rank
    const boardRanks = board.map(c => c.rank)
    const boardDistinct = boardRanks.filter((rank, index, arr) => arr.indexOf(rank) === index).sort((a, b) => b - a)
    const topRank = boardDistinct[0] || 0
    const secondRank = boardDistinct[1] || 0
    const thirdRank = boardDistinct[2] || 0
    const boardHasRank = (rank: number): boolean => boardRanks.includes(rank)

    if (isPocketPair && boardHasRank(c1.rank))
      return 'set'
    if (isPocketPair && c1.rank > topRank)
      return 'overpair'

    const matched: number[] = []
    if (boardHasRank(c1.rank))
      matched.push(c1.rank)
    if (boardHasRank(c2.rank))
      matched.push(c2.rank)

    const bestMatch = matched.length > 0 ? Math.max(...matched) : 0
    if (bestMatch === topRank && topRank > 0)
      return 'top_pair'
    if (bestMatch === secondRank && secondRank > 0)
      return 'second_pair'
    if (bestMatch === thirdRank && thirdRank > 0)
      return 'third_pair'

    if (isPocketPair)
      return 'underpair'

    const highHoleRank = Math.max(c1.rank, c2.rank)
    if (highHoleRank === 14)
      return 'ace_high'
    if (highHoleRank === 13)
      return 'king_high'
    return 'no_made_hand'
  }

  function classifyDrawCategory(cards: [Card, Card], board: Card[]): string {
    const allCards = [...board, cards[0], cards[1]]
    const suitCounts: Record<string, number> = {}
    for (const card of allCards)
      suitCounts[card.suit] = (suitCounts[card.suit] || 0) + 1

    const maxSuitCount = Math.max(...Object.values(suitCounts))
    const madeFlush = maxSuitCount >= 5
    const flushDraw = !madeFlush && maxSuitCount === 4
    const straightDraw = straightOutRanks(allCards) >= 1

    if (flushDraw && straightDraw)
      return 'combo_draw'
    if (flushDraw)
      return 'flush_draw'
    if (straightDraw)
      return 'straight_draw'

    const [c1, c2] = cards
    const isFlop = board.length === 3
    const suitedHole = c1.suit === c2.suit
    const boardSameSuit = board.filter(c => c.suit === c1.suit).length
    if (isFlop && suitedHole && boardSameSuit === 1)
      return 'bdfd_2cards'

    return 'no_draw'
  }

  function buildHandFacts(allHands: HandView[], board: Card[]): HandFacts[] {
    return allHands.map((hand) => {
      const parsed = parseHandCards(hand.hand)
      if (!parsed) {
        return {
          handCategory: 'no_made_hand',
          drawCategory: 'no_draw',
        }
      }
      return {
        handCategory: classifyHandCategory(parsed, board),
        drawCategory: classifyDrawCategory(parsed, board),
      }
    })
  }

  function summarizeCategoryBuckets(
    defs: CategoryDef[],
    allHands: HandView[],
    facts: HandFacts[],
    field: 'handCategory' | 'drawCategory',
    numActions: number,
    total: number,
  ): FilterBucket[] {
    return defs.map((def) => {
      let combos = 0
      const strategySums = Array.from({ length: numActions }).fill(0) as number[]
      const matrixLabelLookup: Record<string, true> = {}

      for (let i = 0; i < allHands.length; i++) {
        if (facts[i]?.[field] !== def.key)
          continue

        const hand = allHands[i]
        combos += hand.weight
        const matrixLabel = handToMatrixLabel(hand.hand)
        if (matrixLabel)
          matrixLabelLookup[matrixLabel] = true
        for (let a = 0; a < numActions; a++) {
          strategySums[a] += (hand.strategy[a] || 0) * hand.weight
        }
      }

      const strategy = strategySums.map(sum => combos > 0 ? sum / combos : 0)
      return {
        ...def,
        combos,
        sharePct: total > 0 ? combos / total * 100 : 0,
        strategy,
        matrixLabels: Object.keys(matrixLabelLookup),
      }
    }).filter(bucket => bucket.combos > 0.001)
  }

  function buildEquityBuckets(allHands: HandView[], numActions: number, total: number): FilterBucket[] {
    return EQUITY_BUCKET_DEFS.map((def) => {
      let combos = 0
      const strategySums = Array.from({ length: numActions }).fill(0) as number[]
      const matrixLabelLookup: Record<string, true> = {}

      for (const hand of allHands) {
        const equityPct = hand.equity * 100
        if (equityPct < def.low || equityPct >= def.high)
          continue

        combos += hand.weight
        const matrixLabel = handToMatrixLabel(hand.hand)
        if (matrixLabel)
          matrixLabelLookup[matrixLabel] = true
        for (let a = 0; a < numActions; a++) {
          strategySums[a] += (hand.strategy[a] || 0) * hand.weight
        }
      }

      const strategy = strategySums.map(sum => combos > 0 ? sum / combos : 0)
      return {
        key: def.key,
        label: def.label,
        combos,
        sharePct: total > 0 ? combos / total * 100 : 0,
        strategy,
        matrixLabels: Object.keys(matrixLabelLookup),
      }
    }).filter(bucket => bucket.combos > 0.001)
  }

  function hoverBucket(bucket: FilterBucket): void {
    if (lockedBucketKey)
      return
    hoveredBucketKey = bucket.key
  }

  function clearBucketHover(): void {
    if (lockedBucketKey)
      return
    hoveredBucketKey = null
  }

  function toggleBucketLock(bucket: FilterBucket): void {
    if (lockedBucketKey === bucket.key) {
      lockedBucketKey = null
      hoveredBucketKey = null
      return
    }
    lockedBucketKey = bucket.key
    hoveredBucketKey = null
  }
</script>

<div class="hands-panel">
  <div class="hand-tabs">
    <button
      class="tab-btn"
      class:active={activeTab === 'hands'}
      on:click={() => activeTab = 'hands'}
    >
      Hands
    </button>
    <button
      class="tab-btn"
      class:active={activeTab === 'filters'}
      on:click={() => activeTab = 'filters'}
    >
      Filters
    </button>
  </div>

  {#if activeTab === 'hands'}
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
  {:else}
    <div class="filters-panel">
      <div class="filters-topbar">
        <div class="mode-tabs">
          <button class="mode-tab" class:active={filterMode === 'include'} on:click={() => filterMode = 'include'}>
            Include
          </button>
          <button class="mode-tab" class:active={filterMode === 'exclude'} on:click={() => filterMode = 'exclude'}>
            Exclude
          </button>
        </div>
        <div class="suit-hints">
          <span>Offsuit ♠ ♥ ♦ ♣</span>
          <span>Suited ♠♠ ♥♥ ♦♦ ♣♣</span>
        </div>
      </div>

      <div class="filters-legend compact">
        {#each visibleActions as item (item.index)}
          <span class="legend-chip">
            <span class="legend-dot" style="background:{item.color}"></span>{item.label}
          </span>
        {/each}
      </div>
      {#if lockedBucketKey}
        <div class="filter-lock-note">Filter locked. Click the active category again to unlock.</div>
      {/if}
      <div class="filters-grid">
        {#if handCategoryBuckets.length > 0}
          <div class="filters-section filters-section-hands">
            <div class="filters-title">Hands</div>
            {#each handCategoryBuckets as bucket (bucket.key)}
              <button
                type="button"
                class="filter-row"
                class:active={bucket.key === activeBucketKey}
                class:dimmed={!!activeBucketKey && bucket.key !== activeBucketKey}
                on:mouseenter={() => hoverBucket(bucket)}
                on:mouseleave={clearBucketHover}
                on:click={() => toggleBucketLock(bucket)}
              >
                <span class="filter-label">{bucket.label}</span>
                <span class="filter-meta" title="{bucket.combos.toFixed(1)} combos">{bucket.sharePct.toFixed(1)}%</span>
                <div class="filter-bar">
                  {#each visibleActions as item (item.index)}
                    {#if bucket.strategy[item.index] > 0.001}
                      <div
                        class="seg"
                        style="width:{(bucket.strategy[item.index] * 100).toFixed(1)}%;background:{item.color}"
                        title="{item.label}: {(bucket.strategy[item.index] * 100).toFixed(1)}%"
                      ></div>
                    {/if}
                  {/each}
                </div>
              </button>
            {/each}
          </div>
        {/if}

        {#if drawCategoryBuckets.length > 0}
          <div class="filters-section filters-section-draws">
            <div class="filters-title">Draws</div>
            {#each drawCategoryBuckets as bucket (bucket.key)}
              <button
                type="button"
                class="filter-row"
                class:active={bucket.key === activeBucketKey}
                class:dimmed={!!activeBucketKey && bucket.key !== activeBucketKey}
                on:mouseenter={() => hoverBucket(bucket)}
                on:mouseleave={clearBucketHover}
                on:click={() => toggleBucketLock(bucket)}
              >
                <span class="filter-label">{bucket.label}</span>
                <span class="filter-meta" title="{bucket.combos.toFixed(1)} combos">{bucket.sharePct.toFixed(1)}%</span>
                <div class="filter-bar">
                  {#each visibleActions as item (item.index)}
                    {#if bucket.strategy[item.index] > 0.001}
                      <div
                        class="seg"
                        style="width:{(bucket.strategy[item.index] * 100).toFixed(1)}%;background:{item.color}"
                        title="{item.label}: {(bucket.strategy[item.index] * 100).toFixed(1)}%"
                      ></div>
                    {/if}
                  {/each}
                </div>
              </button>
            {/each}
          </div>
        {/if}

        {#if equityBuckets.length > 0}
          <div class="filters-section filters-section-equity">
            <div class="filters-title">EQ buckets - Advanced</div>
            {#each equityBuckets as bucket (bucket.key)}
              <button
                type="button"
                class="filter-row"
                class:active={bucket.key === activeBucketKey}
                class:dimmed={!!activeBucketKey && bucket.key !== activeBucketKey}
                on:mouseenter={() => hoverBucket(bucket)}
                on:mouseleave={clearBucketHover}
                on:click={() => toggleBucketLock(bucket)}
              >
                <span class="filter-label">{bucket.label}</span>
                <span class="filter-meta" title="{bucket.combos.toFixed(1)} combos">{bucket.sharePct.toFixed(1)}%</span>
                <div class="filter-bar">
                  {#each visibleActions as item (item.index)}
                    {#if bucket.strategy[item.index] > 0.001}
                      <div
                        class="seg"
                        style="width:{(bucket.strategy[item.index] * 100).toFixed(1)}%;background:{item.color}"
                        title="{item.label}: {(bucket.strategy[item.index] * 100).toFixed(1)}%"
                      ></div>
                    {/if}
                  {/each}
                </div>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
