import type { AppView, NodeView, SolveStatusResponse } from './types'
import { derived, writable } from 'svelte/store'

export const currentNode = writable<NodeView | null>(null)
export const breadcrumb = writable<string[]>(['Root'])
export const activePath = writable<number[]>([])
export const activeSpotId = writable<number | null>(null)

export type HandsPanelTab = 'hands' | 'filters' | 'runouts'
export interface RunoutActionDef {
  key: string
  label: string
  color: string
}
export interface RunoutSegment {
  key: string
  value: number
  color: string
}
export interface RunoutCardBar {
  card: string
  segments: RunoutSegment[]
  tooltip: string
}

export const handsPanelTab = writable<HandsPanelTab>('hands')
export const runoutChartLoading = writable(false)
export const runoutChartError = writable('')
export const runoutChartLegend = writable<RunoutActionDef[]>([])
export const runoutChartBars = writable<RunoutCardBar[]>([])
export const hoveredRunoutCard = writable<string | null>(null)

export const actionColors = writable<string[]>([])
export const statusText = writable('Ready')
export const errorMsg = writable('')
export const solveInfo = writable('')
export const solveStatus = writable<SolveStatusResponse>({ status: 'Idle' })
export const isSolving = writable(false)
export const canSolve = writable(false)
export const appView = writable<AppView>('empty')
export const hoveredMatrixLabels = writable<string[] | null>(null)
export const hoveredActionIndex = writable<number | null>(null)

const CARD_RANKS = '23456789TJQKA'
const CARD_SUITS = 'cdhs'

export function cardToPathIndex(card: string): number | null {
  if (!card || card.length < 2)
    return null
  const rank = CARD_RANKS.indexOf(card[0])
  const suit = CARD_SUITS.indexOf(card[1])
  if (rank < 0 || suit < 0)
    return null
  return rank * 4 + suit
}

type PlayerIndex = 0 | 1
type LabelCoverage = Record<string, number>

interface CoverageState {
  0: LabelCoverage
  1: LabelCoverage
}

function makeEmptyCoverageState(): CoverageState {
  return { 0: {}, 1: {} }
}

function cloneCoverageState(state: CoverageState): CoverageState {
  return {
    0: { ...state[0] },
    1: { ...state[1] },
  }
}

export const comboCoverageHistory = writable<CoverageState[]>([makeEmptyCoverageState()])
export const comboCoverage = derived(comboCoverageHistory, ($history) => {
  const last = $history.at(-1)
  return last ? cloneCoverageState(last) : makeEmptyCoverageState()
})

export function resetComboCoverage(): void {
  comboCoverageHistory.set([makeEmptyCoverageState()])
}

export function pushComboCoverageFromAction(node: NodeView, actionIndex: number): void {
  comboCoverageHistory.update((history) => {
    const previous = history.at(-1) || makeEmptyCoverageState()
    const next = cloneCoverageState(previous)
    if (node.current_player === 0 || node.current_player === 1) {
      const player = node.current_player as PlayerIndex
      const playerCoverage = { ...next[player] }
      for (const row of node.grid || []) {
        for (const cell of row) {
          if (cell.combos <= 0.001)
            continue
          const frequency = Math.max(0, Math.min(1, cell.strategy[actionIndex] || 0))
          const prevCoverage = playerCoverage[cell.label] ?? 1
          playerCoverage[cell.label] = prevCoverage * frequency
        }
      }
      next[player] = playerCoverage
    }
    return [...history, next]
  })
}

export function pushComboCoverageSnapshot(): void {
  comboCoverageHistory.update((history) => {
    const previous = history.at(-1) || makeEmptyCoverageState()
    return [...history, cloneCoverageState(previous)]
  })
}

export function popComboCoverageSnapshot(): void {
  comboCoverageHistory.update((history) => {
    if (history.length <= 1)
      return history
    return history.slice(0, -1)
  })
}

export function trimComboCoverageToDepth(depth: number): void {
  const targetLength = Math.max(1, depth + 1)
  comboCoverageHistory.update((history) => {
    if (history.length >= targetLength)
      return history.slice(0, targetLength)
    const previous = history.at(-1) || makeEmptyCoverageState()
    const next = [...history]
    while (next.length < targetLength)
      next.push(cloneCoverageState(previous))
    return next
  })
}
