import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)

// Register service worker for PWA support — but NOT inside Tauri webview,
// where SW navigation interception breaks route transitions.
const isTauri = '__TAURI_INTERNALS__' in window;
if ('serviceWorker' in navigator) {
  if (isTauri) {
    // Clean up any SW previously registered from a browser session.
    navigator.serviceWorker.getRegistrations().then(regs => {
      regs.forEach(r => r.unregister());
    }).catch(() => {});
  } else {
    navigator.serviceWorker.register('/sw.js').catch(() => {});
  }
}
