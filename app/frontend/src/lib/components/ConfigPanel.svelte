<script lang="ts">
  import { api } from '../api'
  import { getActionColor, PRESETS } from '../helpers'
  import {
    actionColors,
    appView,
    breadcrumb,
    canSolve,
    currentNode,
    errorMsg,
    isSolving,
    solveInfo,
    statusText,
  } from '../stores'
  import SpotList from './SpotList.svelte'

  let oopRange = ''
  let ipRange = ''
  let board = ''
  let startingPot = 200
  let effectiveStack = 900

  let betSizes = '33%, 75%, 150%'
  let raiseSizes = '3x'

  let maxIter = 1000
  let targetExp = 0.5
  let building = false
  let pollInterval: ReturnType<typeof setInterval> | null = null

  function applyPreset(name: string): void {
    const p = PRESETS[name]
    if (!p)
      return
    oopRange = p.oop
    ipRange = p.ip
    startingPot = p.pot
    effectiveStack = p.stack
    betSizes = p.bet
    raiseSizes = p.raise
  }

  async function buildGame(): Promise<void> {
    $errorMsg = ''
    building = true
    try {
      const data = await api.configure({
        oop_range: oopRange,
        ip_range: ipRange,
        board,
        starting_pot: Math.round(startingPot),
        effective_stack: Math.round(effectiveStack),
        flop_bet_oop: betSizes,
        flop_raise_oop: raiseSizes,
        flop_bet_ip: betSizes,
        flop_raise_ip: raiseSizes,
        turn_bet_oop: betSizes,
        turn_raise_oop: raiseSizes,
        turn_bet_ip: betSizes,
        turn_raise_ip: raiseSizes,
        river_bet_oop: betSizes,
        river_raise_oop: raiseSizes,
        river_bet_ip: betSizes,
        river_raise_ip: raiseSizes,
      })
      $statusText = data.message
      $canSolve = true
      $solveInfo = `Memory: ${data.memory_mb.toFixed(0)} MB | OOP: ${data.num_hands_oop} hands | IP: ${data.num_hands_ip} hands`
    }
    catch (e) {
      $errorMsg = (e as Error).message
    }
    building = false
  }

  async function startSolve(): Promise<void> {
    $errorMsg = ''
    try {
      await api.solve({
        max_iterations: maxIter,
        target_exploitability_pct: targetExp,
      })
      $isSolving = true
      $canSolve = false
      $appView = 'solving'
      pollInterval = setInterval(pollSolve, 2000)
    }
    catch (e) {
      $errorMsg = (e as Error).message
    }
  }

  async function pollSolve(): Promise<void> {
    try {
      const data = await api.solveStatus()
      if (data.status === 'Solving') {
        const expPct = data.exploitability != null && data.exploitability >= 0 && startingPot > 0
          ? `${(data.exploitability / startingPot * 100).toFixed(2)}%`
          : '...'
        $solveInfo = `Iteration ${data.iteration}/${data.max_iterations} | Exploitability: ${expPct} pot`
      }
      else if (data.status === 'Done') {
        if (pollInterval)
          clearInterval(pollInterval)
        pollInterval = null
        const expPctDone = startingPot > 0 ? `${(data.exploitability! / startingPot * 100).toFixed(2)}%` : data.exploitability!.toFixed(4)
        $solveInfo = `Solved in ${data.iterations} iterations | Exploitability: ${expPctDone} pot`
        $isSolving = false
        $canSolve = true
        $statusText = 'Solved'
        $breadcrumb = ['Root']
        const node = await api.getNode()
        $currentNode = node
        $actionColors = (node.actions || []).map((a, i) => getActionColor(a, i))
        $appView = 'browser'
      }
    }
    catch (e) {
      if (pollInterval)
        clearInterval(pollInterval)
      pollInterval = null
      $errorMsg = (e as Error).message
    }
  }

  async function stopSolve(): Promise<void> {
    try {
      await api.solveStop()
    }
    catch { /* ignore */ }
  }
</script>

<div class="config-panel">
  <div class="section">
    <h3>Presets</h3>
    <div class="preset-btns">
      <button class="btn btn-sm btn-primary" on:click={() => applyPreset('srp')}>SRP</button>
      <button class="btn btn-sm btn-primary" on:click={() => applyPreset('3bp')}>3-Bet Pot</button>
      <button class="btn btn-sm btn-primary" on:click={() => applyPreset('4bp')}>4-Bet Pot</button>
    </div>
  </div>

  <SpotList />

  <div class="section">
    <h3>Ranges</h3>
    <label for="oop-range">OOP Range</label>
    <textarea id="oop-range" bind:value={oopRange} rows="2" placeholder="e.g. QQ+,AKs,AQs,AJs,A5s-A2s,KQs,AKo"></textarea>
    <label for="ip-range">IP Range</label>
    <textarea id="ip-range" bind:value={ipRange} rows="2" placeholder="e.g. JJ-22,AQs-A2s,KQs-K9s,AQo-AJo"></textarea>
  </div>

  <div class="section">
    <h3>Board & Stacks</h3>
    <label for="board">Board (e.g. Qs7h2c or Qs7h2cAd)</label>
    <input id="board" type="text" bind:value={board} placeholder="Qs7h2c">
    <div class="row">
      <div>
        <label for="starting-pot">Starting Pot</label>
        <input id="starting-pot" type="number" bind:value={startingPot}>
      </div>
      <div>
        <label for="effective-stack">Effective Stack</label>
        <input id="effective-stack" type="number" bind:value={effectiveStack}>
      </div>
    </div>
  </div>

  <div class="section">
    <h3>Bet Sizes</h3>
    <div class="row">
      <div><label for="bet-sizes">Bet</label><input id="bet-sizes" type="text" bind:value={betSizes} placeholder="33%, 75%, 150%"></div>
      <div><label for="raise-sizes">Raise</label><input id="raise-sizes" type="text" bind:value={raiseSizes} placeholder="3x"></div>
    </div>
  </div>

  <div class="section">
    <h3>Solve Settings</h3>
    <div class="row">
      <div>
        <label for="max-iter">Max Iterations</label>
        <input id="max-iter" type="number" bind:value={maxIter}>
      </div>
      <div>
        <label for="target-exp">Target Exploit. %</label>
        <input id="target-exp" type="number" bind:value={targetExp} step="0.1">
      </div>
    </div>
  </div>

  <div>
    <button class="btn btn-primary btn-block" on:click={buildGame} disabled={building || $isSolving}>
      {building ? 'Building...' : 'Build Game'}
    </button>
    {#if !$isSolving}
      <button class="btn btn-green btn-block" on:click={startSolve} disabled={!$canSolve}>Solve</button>
    {:else}
      <button class="btn btn-red btn-block" on:click={stopSolve}>Stop Solve</button>
    {/if}
    {#if $solveInfo}
      <div id="solve-info">{$solveInfo}</div>
    {/if}
    {#if $errorMsg}
      <div id="error-msg">{$errorMsg}</div>
    {/if}
  </div>
</div>
