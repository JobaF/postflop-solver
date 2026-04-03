import type { AppView, NodeView, SolveStatusResponse } from './types'
import { derived, writable } from 'svelte/store'

export const currentNode = writable<NodeView | null>(null)
export const breadcrumb = writable<string[]>(['Root'])
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
