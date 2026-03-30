import type { ConfigResponse, NodeView, SolveStatusResponse, SpotsResponse } from './types'

const BASE = '/api'

async function request<T>(url: string, body?: unknown): Promise<T> {
  const opts: RequestInit = body !== undefined
    ? { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }
    : { method: 'GET' }
  const resp = await fetch(BASE + url, opts)
  const data = await resp.json()
  if (!resp.ok || data.error)
    throw new Error(data.error || `HTTP ${resp.status}`)
  return data as T
}

export interface ConfigureParams {
  oop_range: string
  ip_range: string
  board: string
  starting_pot: number
  effective_stack: number
  flop_bet_oop: string
  flop_raise_oop: string
  flop_bet_ip: string
  flop_raise_ip: string
  turn_bet_oop: string
  turn_raise_oop: string
  turn_bet_ip: string
  turn_raise_ip: string
  river_bet_oop: string
  river_raise_oop: string
  river_bet_ip: string
  river_raise_ip: string
}

export interface SolveParams {
  max_iterations: number
  target_exploitability_pct: number
}

export const api = {
  configure: (config: ConfigureParams) => request<ConfigResponse>('/configure', config),
  solve: (params: SolveParams) => request<{ message: string }>('/solve', params),
  solveStop: () => request<{ message: string }>('/solve/stop', {}),
  solveStatus: () => request<SolveStatusResponse>('/solve/status'),
  getNode: () => request<NodeView>('/node'),
  play: (body: { action?: number, card?: string }) => request<NodeView>('/play', body),
  back: () => request<NodeView>('/back', {}),
  root: () => request<NodeView>('/root', {}),
  validateRange: (range: string) => request<{ valid: boolean, error?: string }>('/validate-range', { range }),
  listSpots: () => request<SpotsResponse>('/spots'),
  loadSpot: (id: number) => request<NodeView>('/spots/load', { id }),
}
