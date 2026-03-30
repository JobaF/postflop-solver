export interface ActionView {
  index: number
  label: string
  action_type: string
  amount: number | null
  frequency: number
  ev: number
}

export interface HandView {
  hand: string
  equity: number
  ev: number
  weight: number
  strategy: number[]
  ev_detail: number[]
}

export interface GridCell {
  label: string
  combos: number
  strategy: number[]
}

export interface NodeView {
  is_terminal: boolean
  is_chance: boolean
  current_player: number
  player_name: string
  board: string[]
  pot: number
  bets: [number, number]
  total_pot: number
  effective_stack: number
  history_depth: number
  actions: ActionView[]
  range_equity: number
  range_ev: number
  hands: HandView[]
  grid: GridCell[][]
  possible_cards: string[]
}

export interface SpotMeta {
  id: number
  label: string
  board: string
  oop_range: string
  ip_range: string
  pot: number
  stack: number
  exploitability: number
  iterations: number
}

export interface SpotsResponse {
  spots: SpotMeta[]
  active_id: number | null
}

export interface ConfigResponse {
  success: boolean
  message: string
  memory_mb: number
  num_hands_oop: number
  num_hands_ip: number
}

export interface SolveStatusResponse {
  status: 'Idle' | 'Ready' | 'Solving' | 'Done'
  iteration?: number
  max_iterations?: number
  exploitability?: number
  iterations?: number
  memory_mb?: number
}

export interface Preset {
  oop: string
  ip: string
  pot: number
  stack: number
  bet: string
  raise: string
}

export type AppView = 'empty' | 'solving' | 'browser'
export type SortCol = 'hand' | 'equity' | 'ev' | 'weight'
