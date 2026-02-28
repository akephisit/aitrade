<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    connectWs,
    disconnectWs,
    wsStatus,
    strategy,
    position,
    history,
    tickCount,
    tradeCount,
    eventLog,
    riskStatus,
    activateKillSwitch,
    rearmSystem,
  } from "$lib/stores";

  onMount(connectWs);
  onDestroy(disconnectWs);

  // ── Computed ────────────────────────────────────────────────────────────
  $: dir = $strategy?.direction ?? null;
  $: dirClass = dir === "BUY" ? "buy" : dir === "SELL" ? "sell" : "none";
  $: killed = $riskStatus?.is_killed ?? false;

  $: pipsFromEntry = $position
    ? $position.direction === "BUY"
      ? 67032 - $position.entry_price // placeholder — replace with live price
      : $position.entry_price - 67032
    : 0;

  function fmt(n: number | null | undefined, decimals = 2) {
    if (n == null) return "—";
    return n.toFixed(decimals);
  }

  function fmtTime(iso: string | null | undefined) {
    if (!iso) return "—";
    return new Date(iso).toLocaleTimeString("en-US", { hour12: false });
  }

  async function handleKill() {
    if (!confirm("Activate Kill Switch? This will block all new trades."))
      return;
    await activateKillSwitch("Manual kill from Dashboard");
  }

  async function handleRearm() {
    if (!confirm("Re-arm the system? Trading will be enabled.")) return;
    await rearmSystem();
  }
</script>

<svelte:head>
  <title>Antigravity — Trading Dashboard</title>
  <meta name="description" content="Antigravity real-time trading monitor" />
</svelte:head>

<div class="app-layout">
  <!-- ── Header ─────────────────────────────────────────────────────── -->
  <header class="header">
    <div class="header-brand">
      <div class="brand-dot"></div>
      ANTIGRAVITY
    </div>

    <div class="ws-status">
      <div class="status-dot {$wsStatus}"></div>
      {#if $wsStatus === "connected"}
        LIVE
      {:else if $wsStatus === "connecting"}
        CONNECTING...
      {:else}
        DISCONNECTED
      {/if}
    </div>
  </header>

  <!-- ── Dashboard ──────────────────────────────────────────────────── -->
  <main class="dashboard">
    <!-- Stats Row -->
    <div class="stats-row">
      <div class="stat-card">
        <div class="stat-label">Tick Count</div>
        <div class="stat-value blue">{$tickCount.toLocaleString()}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Trades Fired</div>
        <div class="stat-value purple">{$tradeCount}</div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Strategy</div>
        <div
          class="stat-value {$strategy ? 'green' : ''}"
          style="font-size:1rem; padding-top:4px"
        >
          {$strategy ? `${$strategy.direction} ${$strategy.symbol}` : "NONE"}
        </div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Open Position</div>
        <div
          class="stat-value {$position ? 'green' : ''}"
          style="font-size:1rem; padding-top:4px"
        >
          {$position
            ? `${$position.direction} @ ${fmt($position.entry_price)}`
            : "FLAT"}
        </div>
      </div>
      <div class="stat-card {killed ? 'stat-danger' : ''}">
        <div class="stat-label">Risk Status</div>
        <div
          class="stat-value {killed ? 'red' : 'green'}"
          style="font-size:1rem; padding-top:4px"
        >
          {killed ? "⛔ KILLED" : "✅ ARMED"}
        </div>
      </div>
    </div>

    <!-- ── Active Strategy ─────────────────────────────────────────── -->
    <div class="card strategy-card">
      <div class="card-header">
        <div class="card-title">Active Strategy</div>
        <span class="card-badge {$strategy ? 'badge-green' : 'badge-dim'}">
          {$strategy ? "ARMED" : "DISARMED"}
        </span>
      </div>

      {#if $strategy}
        <div class="direction-badge {dirClass}">
          {dir === "BUY" ? "▲" : dir === "SELL" ? "▼" : "—"}
          {dir}
        </div>

        <div class="levels-grid">
          <div class="level-item">
            <div class="level-label">Take Profit</div>
            <div class="level-price tp">{fmt($strategy.take_profit)}</div>
          </div>
          <div class="level-item">
            <div class="level-label">Entry Zone</div>
            <div class="level-price entry">
              {fmt($strategy.entry_zone.low)} – {fmt($strategy.entry_zone.high)}
            </div>
          </div>
          <div class="level-item">
            <div class="level-label">Stop Loss</div>
            <div class="level-price sl">{fmt($strategy.stop_loss)}</div>
          </div>
        </div>

        <div class="entry-zone-bar"><div class="entry-zone-fill"></div></div>

        <div
          class="price-display"
          style="margin-top:14px; font-size:0.75rem; color:var(--text-dim)"
        >
          💬 {$strategy.rationale}
        </div>
      {:else}
        <div class="no-data">No strategy. Waiting for OpenClaw...</div>
      {/if}
    </div>

    <!-- ── Open Position ──────────────────────────────────────────── -->
    <div class="card position-card">
      <div class="card-header">
        <div class="card-title">Open Position</div>
        <span class="card-badge {$position ? 'badge-green' : 'badge-dim'}">
          {$position ? $position.direction : "FLAT"}
        </span>
      </div>

      {#if $position}
        <div class="pnl-display {pipsFromEntry >= 0 ? 'positive' : 'negative'}">
          {pipsFromEntry >= 0 ? "+" : ""}{fmt(pipsFromEntry, 1)} pips
        </div>

        <div class="levels-grid">
          <div class="level-item">
            <div class="level-label">Entry</div>
            <div class="level-price entry">{fmt($position.entry_price)}</div>
          </div>
          <div class="level-item">
            <div class="level-label">TP</div>
            <div class="level-price tp">{fmt($position.take_profit)}</div>
          </div>
          <div class="level-item">
            <div class="level-label">SL</div>
            <div class="level-price sl">{fmt($position.stop_loss)}</div>
          </div>
        </div>

        <div
          class="price-display"
          style="margin-top:14px; color: var(--text-dim); font-size:0.72rem;"
        >
          Ticket: {$position.mt5_ticket ?? "Pending"} &nbsp;|&nbsp; Lots: {$position.lot_size}
          &nbsp;|&nbsp;
          {fmtTime($position.opened_at)}
        </div>
      {:else}
        <div class="no-data">No open position</div>
      {/if}
    </div>

    <!-- ── Risk Management Panel ─────────────────────────────────────── -->
    <div class="card risk-card {killed ? 'risk-killed' : ''}">
      <div class="card-header">
        <div class="card-title">🛡️ Risk Management</div>
        <span class="card-badge {killed ? 'badge-red' : 'badge-green'}">
          {killed ? "KILLED" : "ACTIVE"}
        </span>
      </div>

      <div class="risk-grid">
        <div class="risk-stat">
          <div class="risk-label">Trades Today</div>
          <div class="risk-value">
            {$riskStatus?.trades_today ?? 0} / {$riskStatus?.config
              .max_trades_per_day ?? "∞"}
          </div>
        </div>
        <div class="risk-stat">
          <div class="risk-label">Consec. Failures</div>
          <div
            class="risk-value {($riskStatus?.consecutive_failures ?? 0) > 0
              ? 'red-text'
              : ''}"
          >
            {$riskStatus?.consecutive_failures ?? 0} / {$riskStatus?.config
              .max_consecutive_failures ?? "off"}
          </div>
        </div>
        <div class="risk-stat">
          <div class="risk-label">Cooldown</div>
          <div
            class="risk-value {$riskStatus?.in_cooldown
              ? 'red-text'
              : 'green-text'}"
          >
            {$riskStatus?.in_cooldown ? "⏳ ACTIVE" : "NONE"}
          </div>
        </div>
        <div class="risk-stat">
          <div class="risk-label">Kill Reason</div>
          <div class="risk-value" style="font-size:0.72rem">
            {$riskStatus?.kill_reason ?? "—"}
          </div>
        </div>
      </div>

      <div class="risk-actions">
        {#if killed}
          <button id="btn-rearm" class="btn btn-green" on:click={handleRearm}>
            ✅ Re-arm System
          </button>
        {:else}
          <button id="btn-kill" class="btn btn-red" on:click={handleKill}>
            ⛔ Emergency Kill
          </button>
        {/if}
      </div>
    </div>

    <!-- ── Open Position ──────────────────────────────────────────────── -->
    <div class="card position-card">
      <div class="card-header">
        <div class="card-title">Open Position</div>
        <span class="card-badge {$position ? 'badge-green' : 'badge-dim'}">
          {$position ? $position.direction : "FLAT"}
        </span>
      </div>

      {#if $position}
        <div class="levels-grid">
          <div class="level-item">
            <div class="level-label">Entry</div>
            <div class="level-price entry">{fmt($position.entry_price)}</div>
          </div>
          <div class="level-item">
            <div class="level-label">TP</div>
            <div class="level-price tp">{fmt($position.take_profit)}</div>
          </div>
          <div class="level-item">
            <div class="level-label">SL</div>
            <div class="level-price sl">{fmt($position.stop_loss)}</div>
          </div>
        </div>

        <div
          class="price-display"
          style="margin-top:14px; color: var(--text-dim); font-size:0.72rem;"
        >
          Ticket: {$position.mt5_ticket ?? "Pending"} &nbsp;|&nbsp; Lots: {$position.lot_size}
          &nbsp;|&nbsp;
          {fmtTime($position.opened_at)}
        </div>
      {:else}
        <div class="no-data">No open position — Reflex Loop ready</div>
      {/if}
    </div>

    <!-- ── Event Log ──────────────────────────────────────────────── -->
    <div class="card event-log">
      <div class="card-header">
        <div class="card-title">Event Log</div>
        <span class="card-badge badge-dim">{$eventLog.length}</span>
      </div>

      <div class="log-container">
        {#each $eventLog as entry (entry.id)}
          <div class="log-entry {entry.type}">
            <span class="log-time">{entry.time}</span>
            <span class="log-msg">{entry.event}: {entry.message}</span>
          </div>
        {/each}

        {#if $eventLog.length === 0}
          <div class="no-data">Waiting for events...</div>
        {/if}
      </div>
    </div>

    <!-- ── Trade History ──────────────────────────────────────────── -->
    <div class="card history-card">
      <div class="card-header">
        <div class="card-title">Trade History</div>
        <span class="card-badge badge-dim">{$history.length} trades</span>
      </div>

      {#if $history.length > 0}
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Time</th>
                <th>Symbol</th>
                <th>Dir</th>
                <th>Entry</th>
                <th>TP</th>
                <th>SL</th>
                <th>Close</th>
                <th>Profit</th>
                <th>Reason</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {#each $history as r (r.trade_id)}
                <tr>
                  <td>{fmtTime(r.fired_at)}</td>
                  <td>{r.symbol}</td>
                  <td class="td-direction {r.direction.toLowerCase()}"
                    >{r.direction}</td
                  >
                  <td>{fmt(r.entry_price)}</td>
                  <td style="color:var(--green)">{fmt(r.take_profit)}</td>
                  <td style="color:var(--red)">{fmt(r.stop_loss)}</td>
                  <td>{fmt(r.close_price)}</td>
                  <td
                    class={(r.profit_pips ?? 0) >= 0 ? "td-profit" : "td-loss"}
                  >
                    {r.profit_pips != null
                      ? `${r.profit_pips >= 0 ? "+" : ""}${r.profit_pips.toFixed(1)}`
                      : "—"}
                  </td>
                  <td>{r.close_reason ?? "—"}</td>
                  <td class="td-status {r.status.toLowerCase()}">{r.status}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <div class="no-data">No trades yet</div>
      {/if}
    </div>
  </main>
</div>
