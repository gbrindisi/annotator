import { parseServerMsg, type ClientMsg, type ServerMsg } from './protocol';

export type ConnState = 'connecting' | 'open' | 'closed';

/** Anything shaped like a browser WebSocket (the dev mock implements this). */
export interface SocketLike {
  readyState: number;
  send(data: string): void;
  close(): void;
  onopen: ((ev: Event) => void) | null;
  onclose: ((ev: CloseEvent) => void) | null;
  onerror: ((ev: Event) => void) | null;
  onmessage: ((ev: MessageEvent) => void) | null;
}

export type SocketFactory = (url: string) => SocketLike;

export const backoffDelay = (attempt: number): number => Math.min(10_000, 500 * 2 ** Math.min(attempt, 5));

/** WebSocket with reconnect + backoff. The daemon sends a fresh Doc on every connect. */
export class Connection {
  private socket: SocketLike | null = null;
  private attempt = 0;
  private timer: number | undefined;
  state: ConnState = 'connecting';

  constructor(
    private url: string,
    private handlers: { onMessage: (m: ServerMsg) => void; onState: (s: ConnState, retryIn?: number) => void },
    private factory: SocketFactory = u => new WebSocket(u) as unknown as SocketLike,
  ) {}

  connect(): void {
    window.clearTimeout(this.timer);
    this.setState('connecting');
    const s = this.factory(this.url);
    this.socket = s;
    s.onopen = () => {
      this.attempt = 0;
      this.setState('open');
    };
    s.onmessage = ev => {
      const m = parseServerMsg(String(ev.data));
      if (m) this.handlers.onMessage(m);
    };
    s.onclose = () => {
      if (this.socket !== s) return;
      this.socket = null;
      const delay = backoffDelay(this.attempt++);
      this.setState('closed', delay);
      this.timer = window.setTimeout(() => this.connect(), delay);
    };
    s.onerror = () => {};
  }

  /** Reconnect immediately (e.g. user clicked the indicator). */
  retryNow(): void {
    if (this.state === 'closed') this.connect();
  }

  send(m: ClientMsg): boolean {
    if (!this.socket || this.socket.readyState !== 1) return false;
    this.socket.send(JSON.stringify(m));
    return true;
  }

  private setState(s: ConnState, retryIn?: number): void {
    this.state = s;
    this.handlers.onState(s, retryIn);
  }
}

export const wsUrl = (loc: Location = window.location): string => {
  const m = loc.pathname.match(/^\/s\/([^/]+)/);
  const id = m ? m[1] : 'dev';
  return `${loc.protocol === 'https:' ? 'wss:' : 'ws:'}//${loc.host}/ws/${id}`;
};
