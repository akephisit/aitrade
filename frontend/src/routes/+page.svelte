<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    connectWs, disconnectWs,
    wsStatus, strategy, position, history, tickCount, tradeCount, eventLog
  } from '$lib/stores';

  onMount(connectWs);
  onDestroy(disconnectWs);

  // ── Computed ────────────────────────────────────────────────────────────
  $: dir = $strategy?.direction ?? null;
  $: dirClass = dir === 'BUY' ? 'buy' : dir === 'SELL' ? 'sell' : 'none';

  $: pipsFromEntry = $position
    ? ($position.direction === 'BUY'
        ? 67032 - $position.entry_price   // placeholder — replace with live price
        : $position.entry_price - 67032)
    : 0;

  function fmt(n: number, decimals = 2) {
    return n?.toFixed(decimals) ?? '—';
  }

  function fmtTime(iso: string) {
    return new Date(iso).toLocaleTimeString('en-US', { hour12: false });
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
      {#if $wsStatus === 'connected'}
        LIVE
      {:else if $wsStatus === 'connecting'}
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
        <div class="stat-value {$strategy ? 'green' : ''}" style="font-size:1rem; padding-top:4px">
          {$strategy ? `${$strategy.direction} ${$strategy.symbol}` : 'NONE'}
        </div>
      </div>
      <div class="stat-card">
        <div class="stat-label">Open Position</div>
        <div class="stat-value {$position ? 'green' : ''}" style="font-size:1rem; padding-top:4px">
          {$position ? `${$position.direction} @ ${fmt($position.entry_price)}` : 'FLAT'}
        </div>
      </div>
    </div>

    <!-- ── Active Strategy ─────────────────────────────────────────── -->
    <div class="card strategy-card">
      <div class="card-header">
        <div class="card-title">Active Strategy</div>
        <span class="card-badge {$strategy ? 'badge-green' : 'badge-dim'}">
          {$strategy ? 'ARMED' : 'DISARMED'}
        </span>
      </div>

      {#if $strategy}
        <div class="direction-badge {dirClass}">
          {dir === 'BUY' ? '▲' : dir === 'SELL' ? '▼' : '—'}
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

        <div class="price-display" style="margin-top:14px; font-size:0.75rem; color:var(--text-dim)">
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
          {$position ? $position.direction : 'FLAT'}
        </span>
      </div>

      {#if $position}
        <div class="pnl-display {pipsFromEntry >= 0 ? 'positive' : 'negative'}">
          {pipsFromEntry >= 0 ? '+' : ''}{fmt(pipsFromEntry, 1)} pips
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

        <div class="price-display" style="margin-top:14px; color: var(--text-dim); font-size:0.72rem;">
          Ticket: {$position.mt5_ticket ?? 'Pending'} &nbsp;|&nbsp;
          Lots: {$position.lot_size} &nbsp;|&nbsp;
          {fmtTime($position.opened_at)}
        </div>
      {:else}
        <div class="no-data">No open position</div>
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
                <th>Direction</th>
                <th>Entry</th>
                <th>TP</th>
                <th>SL</th>
                <th>Lots</th>
                <th>Ticket</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {#each $history as r (r.trade_id)}
                <tr>
                  <td>{fmtTime(r.fired_at)}</td>
                  <td>{r.symbol}</td>
                  <td class="td-direction {r.direction.toLowerCase()}">{r.direction}</td>
                  <td>{fmt(r.entry_price)}</td>
                  <td style="color:var(--green)">{fmt(r.take_profit)}</td>
                  <td style="color:var(--red)">{fmt(r.stop_loss)}</td>
                  <td>{r.lot_size}</td>
                  <td>{r.mt5_ticket ?? '—'}</td>
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
