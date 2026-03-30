import type { ActionView, Preset } from './types'

const SUIT_SYMBOLS: Record<string, string> = { c: '\u2663', d: '\u2666', h: '\u2665', s: '\u2660' }

export function suitSymbol(s: string): string {
  return SUIT_SYMBOLS[s] || s
}

export function formatCardSmall(c: string): string {
  if (!c || c.length < 2)
    return c
  return c[0] + suitSymbol(c[1])
}

export function suitClass(c: string): string {
  if (!c || c.length < 2)
    return ''
  return c[1]
}

const BET_COLORS = ['#ffa726', '#ef5350', '#ab47bc', '#ff7043', '#e91e63', '#00bcd4']

export function getActionColor(action: ActionView, index: number): string {
  const t = action.action_type
  if (t === 'check')
    return '#4caf50'
  if (t === 'call')
    return '#66bb6a'
  if (t === 'fold')
    return '#42a5f5'
  if (t === 'allin')
    return '#e91e63'
  return BET_COLORS[index % BET_COLORS.length]
}

export function buildGradient(strategy: number[], colors: string[]): string {
  const stops: string[] = []
  let cum = 0
  for (let a = 0; a < strategy.length; a++) {
    const s = strategy[a]
    if (s > 0.001) {
      stops.push(`${colors[a]} ${(cum * 100).toFixed(1)}% ${((cum + s) * 100).toFixed(1)}%`)
    }
    cum += s
  }
  return stops.length > 0 ? `linear-gradient(to right, ${stops.join(', ')})` : 'var(--bg3)'
}

export const PRESETS: Record<string, Preset> = {
  'srp': {
    oop: '66+,A2s+,K9s+,Q9s+,J9s+,T8s+,97s+,86s+,76s,65s,54s,ATo+,KTo+,QTo+,JTo',
    ip: 'QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+',
    pot: 60,
    stack: 970,
    bet: '33%, 75%, 150%',
    raise: '3x',
  },
  '3bp': {
    oop: 'QQ+,AKs,AQs,AJs,ATs,A5s-A2s,KQs,KJs,76s,65s,54s,AKo',
    ip: 'JJ-22,AQs-A2s,KQs-K9s,QJs-Q9s,JTs-J9s,T9s-T8s,98s-97s,87s-86s,76s-75s,65s,AQo-AJo,KQo',
    pot: 200,
    stack: 900,
    bet: '33%, 75%, 150%',
    raise: '3x',
  },
  '4bp': {
    oop: 'AA,KK,QQ,AKs,AQs,AKo',
    ip: 'KK-JJ,AKs,AQs,AJs,AKo,AQo',
    pot: 480,
    stack: 760,
    bet: '33%, 75%, 150%',
    raise: '3x',
  },
}
