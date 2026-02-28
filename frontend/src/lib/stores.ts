// WebSocket connection manager + Svelte stores

import { writable, derived } from 'svelte/store';

// ── Types ──────────────────────────────────────────────────────────────────

export interface EntryZone { low: number; high: number; }

export interface ActiveStrategy {
    strategy_id: string;
    symbol: string;
    direction: 'BUY' | 'SELL' | 'NO_TRADE';
    entry_zone: EntryZone;
    take_profit: number;
    stop_loss: number;
    lot_size: number;
    rationale: string;
    created_at: string;
    expires_at: string | null;
}

export interface OpenPosition {
    position_id: string;
    strategy_id: string;
    symbol: string;
    direction: 'BUY' | 'SELL';
    entry_price: number;
    lot_size: number;
    take_profit: number;
    stop_loss: number;
    mt5_ticket: number | null;
    opened_at: string;
}

export interface TradeRecord {
    trade_id: string;
    strategy_id: string;
    symbol: string;
    direction: 'BUY' | 'SELL';
    entry_price: number;
    lot_size: number;
    take_profit: number;
    stop_loss: number;
    mt5_ticket: number | null;
    status: 'PENDING' | 'CONFIRMED' | 'FAILED';
    status_message: string;
    fired_at: string;
}

export interface LogEntry {
    id: number;
    time: string;
    event: string;
    message: string;
    type: string;
}

// ── Stores ─────────────────────────────────────────────────────────────────

export const wsStatus = writable<'connecting' | 'connected' | 'disconnected'>('disconnected');
export const strategy = writable<ActiveStrategy | null>(null);
export const position = writable<OpenPosition | null>(null);
export const history = writable<TradeRecord[]>([]);
export const tickCount = writable<number>(0);
export const tradeCount = writable<number>(0);
export const eventLog = writable<LogEntry[]>([]);

let logIdCounter = 0;

function addLog(event: string, message: string, type = 'default') {
    const entry: LogEntry = {
        id: ++logIdCounter,
        time: new Date().toLocaleTimeString('en-US', { hour12: false }),
        event,
        message,
        type: type.toLowerCase().replace(' ', '_'),
    };
    eventLog.update(logs => [entry, ...logs].slice(0, 100)); // keep last 100
}

// ── WebSocket Manager ─────────────────────────────────────────────────────

const WS_URL = 'ws://localhost:3000/ws/monitor';
const API_URL = 'http://localhost:3000';

let ws: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

export function connectWs() {
    if (ws?.readyState === WebSocket.OPEN) return;

    wsStatus.set('connecting');
    ws = new WebSocket(WS_URL);

    ws.onopen = () => {
        wsStatus.set('connected');
        addLog('CONNECTED', `WebSocket connected to ${WS_URL}`, 'default');
        if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
        // Load history from REST
        fetchHistory();
    };

    ws.onclose = () => {
        wsStatus.set('disconnected');
        addLog('DISCONNECTED', 'WebSocket closed — reconnecting in 3s...', 'default');
        reconnectTimer = setTimeout(connectWs, 3000);
    };

    ws.onerror = () => {
        addLog('ERROR', 'WebSocket error', 'default');
    };

    ws.onmessage = (e: MessageEvent) => {
        try {
            const data = JSON.parse(e.data);
            handleEvent(data);
        } catch {
            console.warn('WS parse error:', e.data);
        }
    };
}

export function disconnectWs() {
    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
    ws?.close();
    ws = null;
}

// ── Event Handler ─────────────────────────────────────────────────────────

function handleEvent(data: Record<string, unknown>) {
    const event = data.event as string;

    switch (event) {
        case 'SNAPSHOT':
            strategy.set((data.strategy as ActiveStrategy) ?? null);
            position.set((data.position as OpenPosition) ?? null);
            tickCount.set((data.tick_count as number) ?? 0);
            tradeCount.set((data.trade_count as number) ?? 0);
            addLog('SNAPSHOT', 'State snapshot loaded', 'default');
            break;

        case 'STRATEGY_UPDATED':
            strategy.set(data.strategy as ActiveStrategy);
            addLog('STRATEGY_UPDATED',
                `New strategy: ${(data.strategy as ActiveStrategy).direction} ${(data.strategy as ActiveStrategy).symbol}`,
                'strategy_updated');
            break;

        case 'STRATEGY_CLEARED':
            strategy.set(null);
            addLog('STRATEGY_CLEARED', 'Strategy cleared — Reflex Loop disarmed', 'default');
            break;

        case 'TRADE_FIRING':
            addLog('TRADE_FIRING',
                `Firing trade: ${(data.record as TradeRecord).direction} @ ${(data.record as TradeRecord).entry_price}`,
                'trade_firing');
            break;

        case 'POSITION_OPENED':
            position.set(data.position as OpenPosition);
            tradeCount.update(n => n + 1);
            addLog('POSITION_OPENED',
                `Position opened: ${(data.position as OpenPosition).direction} @ ${(data.position as OpenPosition).entry_price} | Ticket: ${(data.position as OpenPosition).mt5_ticket ?? 'pending'}`,
                'position_opened');
            fetchHistory();  // Refresh trade history
            break;

        case 'TRADE_FAILED':
            addLog('TRADE_FAILED',
                `Trade failed: ${(data.record as TradeRecord).status_message}`,
                'trade_failed');
            fetchHistory();
            break;

        case 'SERVER_STATS':
            tickCount.set((data.tick_count as number) ?? 0);
            tradeCount.set((data.trade_count as number) ?? 0);
            break;
    }
}

// ── REST Helpers ──────────────────────────────────────────────────────────

async function fetchHistory() {
    try {
        const resp = await fetch(`${API_URL}/api/monitor/history`);
        const data = await resp.json();
        history.set(data.records ?? []);
    } catch {
        // Silently ignore — UI will show empty state
    }
}
