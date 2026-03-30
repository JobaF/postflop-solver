<script lang="ts">
  import { api } from '../api'
  import { solveInfo } from '../stores'

  async function stopSolve(): Promise<void> {
    try {
      await api.solveStop()
    }
    catch { /* ignore */ }
  }
</script>

<div class="solve-overlay">
  <div class="solve-title">Solving...</div>
  <div class="solve-stats">{$solveInfo}</div>
  <div class="solve-pbar">
    <div class="fill"></div>
  </div>
  <button class="btn btn-red" on:click={stopSolve}>Stop</button>
</div>
