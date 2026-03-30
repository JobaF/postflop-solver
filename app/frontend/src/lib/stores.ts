import type { AppView, NodeView, SolveStatusResponse } from './types'
import { writable } from 'svelte/store'

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
