import { BrowserRouter, Routes, Route, NavLink, Navigate } from "react-router-dom";
import {
  Database,
  LineChart,
  Settings,
  Zap,
  TrendingUp,
  Users,
  LogOut,
} from "lucide-react";
import { useEffect, useState } from "react";
import { onOptimizationEvent } from "./lib/api";
import {
  useOptimizationStore,
  type CompletionEvent,
  type GenerationEvent,
} from "./stores/optimizationStore";
import DataLoadPage from "./pages/DataLoadPage";
import SimulationPage from "./pages/SimulationPage";
import OptimizationPage from "./pages/OptimizationPage";
import LiveTradingPage from "./pages/LiveTradingPage";
import SettingsPage from "./pages/SettingsPage";
import AdminPage from "./pages/AdminPage";
import { useAuthStore } from "./stores/authStore";
import { Button } from "./components/ui/Button";

const NAV_ITEMS = [
  { to: "/", icon: Database, label: "Data" },
  { to: "/simulation", icon: LineChart, label: "Simulation" },
  { to: "/optimization", icon: TrendingUp, label: "Optimize" },
  { to: "/live", icon: Zap, label: "Live" },
  { to: "/settings", icon: Settings, label: "Settings" },
] as const;

function LoginForm() {
  const { login } = useAuthStore();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError("");
    try {
      await login(username, password);
    } catch (err) {
      setError(`Login failed: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex items-center justify-center h-screen bg-[#09090b] relative overflow-hidden">
      {/* Background gradient mesh */}
      <div className="absolute inset-0">
        <div className="absolute top-1/4 left-1/4 w-96 h-96 bg-amber-500/5 rounded-full blur-3xl" />
        <div className="absolute bottom-1/4 right-1/4 w-80 h-80 bg-amber-500/3 rounded-full blur-3xl" />
      </div>

      <form
        onSubmit={handleSubmit}
        className="relative bg-[#0c0c0f] border border-[#1e1e26] rounded-2xl p-8 w-[340px] space-y-5 animate-fade-in"
        style={{ boxShadow: "inset 0 1px 0 rgba(255,255,255,0.03), 0 0 40px -10px rgba(245, 158, 11, 0.1)" }}
      >
        <div className="text-center space-y-2">
          <div className="inline-flex items-center justify-center w-14 h-14 rounded-2xl bg-amber-500/10 border border-amber-500/20 mb-1">
            <span className="text-amber-500 font-bold text-2xl font-data">B</span>
          </div>
          <h1 className="text-lg font-semibold text-zinc-100">Bitcoin Trader</h1>
          <p className="text-xs text-zinc-500">Algorithmic Trading Terminal</p>
        </div>

        {error && (
          <div className="bg-rose-500/10 border border-rose-500/20 rounded-lg p-2.5 text-rose-400 text-xs">
            {error}
          </div>
        )}

        <div className="space-y-1.5">
          <label className="block text-xs font-medium text-zinc-500">Username</label>
          <input
            type="text"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            className="w-full bg-[#141419] border border-[#1e1e26] rounded-lg px-3 py-2.5 text-sm text-zinc-100 placeholder:text-zinc-600 focus:outline-none focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 transition-colors"
            placeholder="Enter username"
            autoFocus
          />
        </div>

        <div className="space-y-1.5">
          <label className="block text-xs font-medium text-zinc-500">Password</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="w-full bg-[#141419] border border-[#1e1e26] rounded-lg px-3 py-2.5 text-sm text-zinc-100 placeholder:text-zinc-600 focus:outline-none focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 transition-colors"
            placeholder="Enter password"
          />
        </div>

        <Button type="submit" disabled={loading} className="w-full" size="lg">
          {loading ? "Signing in..." : "Sign In"}
        </Button>
      </form>
    </div>
  );
}

// Global optimizer event listener — lives as long as the logged-in app,
// so tab switches don't miss `opt:gen` / `opt:done` events. Store handlers
// append to genHistory / clear `running`, ensuring state survives unmounts.
function useOptimizationEventBridge() {
  const onGen = useOptimizationStore((s) => s.onGenerationEvent);
  const onDone = useOptimizationStore((s) => s.onCompletionEvent);
  useEffect(() => {
    let alive = true;
    let unlistenGen: (() => void) | null = null;
    let unlistenDone: (() => void) | null = null;
    (async () => {
      const un1 = await onOptimizationEvent<GenerationEvent>("opt:gen", (ev) => {
        if (alive) onGen(ev);
      });
      const un2 = await onOptimizationEvent<CompletionEvent>("opt:done", (ev) => {
        if (alive) onDone(ev);
      });
      unlistenGen = un1;
      unlistenDone = un2;
    })();
    return () => {
      alive = false;
      unlistenGen?.();
      unlistenDone?.();
    };
  }, [onGen, onDone]);
}

function App() {
  const { user, logout, isAdmin } = useAuthStore();
  useOptimizationEventBridge();

  if (!user) {
    return <LoginForm />;
  }

  return (
    <BrowserRouter>
      <div className="flex h-screen bg-[#09090b] text-zinc-100">
        {/* Sidebar */}
        <nav className="w-16 bg-[#0c0c0f] border-r border-[#1e1e26] flex flex-col items-center py-4 gap-1 shrink-0">
          {/* Logo */}
          <div className="mb-4 flex items-center justify-center w-10 h-10 rounded-xl bg-amber-500/10 border border-amber-500/20">
            <span className="text-amber-500 font-bold text-lg font-data">B</span>
          </div>

          {NAV_ITEMS.map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              className={({ isActive }) =>
                `relative flex flex-col items-center justify-center w-12 h-12 rounded-xl transition-all duration-200 group ${
                  isActive
                    ? "bg-amber-500/10 text-amber-500"
                    : "text-zinc-600 hover:text-zinc-300 hover:bg-[#141419]"
                }`
              }
              title={label}
            >
              {({ isActive }) => (
                <>
                  {isActive && (
                    <div className="absolute left-0 top-2 bottom-2 w-0.5 bg-amber-500 rounded-r" />
                  )}
                  <Icon size={20} />
                  <span className="text-[9px] mt-0.5 font-medium">{label}</span>
                </>
              )}
            </NavLink>
          ))}

          {isAdmin() && (
            <NavLink
              to="/admin"
              className={({ isActive }) =>
                `relative flex flex-col items-center justify-center w-12 h-12 rounded-xl transition-all duration-200 ${
                  isActive
                    ? "bg-amber-500/10 text-amber-500"
                    : "text-zinc-600 hover:text-zinc-300 hover:bg-[#141419]"
                }`
              }
              title="Admin"
            >
              {({ isActive }) => (
                <>
                  {isActive && (
                    <div className="absolute left-0 top-2 bottom-2 w-0.5 bg-amber-500 rounded-r" />
                  )}
                  <Users size={20} />
                  <span className="text-[9px] mt-0.5 font-medium">Admin</span>
                </>
              )}
            </NavLink>
          )}

          {/* Spacer */}
          <div className="flex-1" />

          {/* User info + logout */}
          <div className="flex flex-col items-center gap-1.5 mb-2">
            <div
              className="w-8 h-8 rounded-full bg-amber-500/15 flex items-center justify-center text-amber-500 text-xs font-bold border border-amber-500/20"
              title={user.username}
            >
              {user.username[0].toUpperCase()}
            </div>
            <span className="text-[9px] text-zinc-600 truncate w-14 text-center font-medium">
              {user.username}
            </span>
            <button
              onClick={logout}
              className="text-zinc-600 hover:text-rose-400 transition-colors p-1 rounded-lg hover:bg-rose-500/10"
              title="Logout"
            >
              <LogOut size={14} />
            </button>
          </div>
        </nav>

        {/* Main content */}
        <main className="flex-1 overflow-y-auto p-6">
          <Routes>
            <Route path="/" element={<DataLoadPage />} />
            <Route path="/simulation" element={<SimulationPage />} />
            <Route path="/optimization" element={<OptimizationPage />} />
            <Route path="/live" element={<LiveTradingPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route
              path="/admin"
              element={isAdmin() ? <AdminPage /> : <Navigate to="/" />}
            />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}

export default App;
