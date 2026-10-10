import './style.css';
import { App } from './app';
import { wsUrl, type SocketFactory } from './connection';

const start = async () => {
  const mount = document.getElementById('app')!;
  let factory: SocketFactory | undefined;
  // `npm run dev` without a /s/<id> path: run against the in-browser fake daemon.
  if (import.meta.env.DEV && !location.pathname.startsWith('/s/')) {
    const { mockFactory } = await import('./dev/mock');
    factory = mockFactory;
  }
  new App(mount, wsUrl(), factory);
};

void start();
