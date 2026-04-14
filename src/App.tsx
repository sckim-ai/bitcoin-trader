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
import { useState } from "react";
import DataLoadPage from "./pages/DataLoadPage";
import SimulationPage from "./pages/SimulationPage";
import OptimizationPage from "./pages/OptimizationPage";
import LiveTradingPage from "./pages/LiveTradingPage";
import SettingsPage from "./pages/SettingsPage";
import AdminPage from "./pages/AdminPage";
import { useAuthStore } from "./stores/authStore";

const NAV_ITEMS = [
  { to: "/", icon: Database, label: "Data" },
  { to: "/simulation", icon: LineChart, label: "Simulation" },
  { to: "/optimization", icon: TrendingUp, label: "Optimization" },
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
    <div className="flex items-center justify-center h-screen bg-gray-950">
      <form
        onSubmit={handleSubmit}
        className="bg-gray-900 border border-gray-800 rounded-xl p-8 w-80 space-y-4"
      >
        <div className="text-center">
          <div className="text-violet-400 font-bold text-2xl mb-1">BT</div>
          <div className="text-gray-400 text-sm">Bitcoin Trader</div>
        </div>

        {error && (
          <div className="bg-red-500/10 border border-red-500/30 rounded p-2 text-red-400 text-xs">
            {error}
          </div>
        )}

        <div>
          <label className="block text-xs text-gray-400 mb-1">Username</label>
          <input
            type="text"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm"
            autoFocus
          />
        </div>

        <div>
          <label className="block text-xs text-gray-400 mb-1">Password</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm"
          />
        </div>

        <button
          type="submit"
          disabled={loading}
          className="w-full py-2 bg-violet-600 hover:bg-violet-700 rounded font-medium text-sm disabled:opacity-50"
        >
          {loading ? "Logging in..." : "Login"}
        </button>
      </form>
    </div>
  );
}

function App() {
  const { user, logout, isAdmin } = useAuthStore();

  if (!user) {
    return <LoginForm />;
  }

  return (
    <BrowserRouter>
      <div className="flex h-screen bg-gray-950 text-white">
        {/* Sidebar */}
        <nav className="w-16 bg-gray-900 border-r border-gray-800 flex flex-col items-center py-4 gap-2">
          <div className="text-violet-400 font-bold text-lg mb-4">BT</div>
          {NAV_ITEMS.map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              className={({ isActive }) =>
                `flex flex-col items-center justify-center w-12 h-12 rounded-lg transition-colors ${
                  isActive
                    ? "bg-violet-600/20 text-violet-400"
                    : "text-gray-500 hover:text-gray-300 hover:bg-gray-800"
                }`
              }
              title={label}
            >
              <Icon size={20} />
              <span className="text-[10px] mt-0.5">{label}</span>
            </NavLink>
          ))}

          {isAdmin() && (
            <NavLink
              to="/admin"
              className={({ isActive }) =>
                `flex flex-col items-center justify-center w-12 h-12 rounded-lg transition-colors ${
                  isActive
                    ? "bg-violet-600/20 text-violet-400"
                    : "text-gray-500 hover:text-gray-300 hover:bg-gray-800"
                }`
              }
              title="Admin"
            >
              <Users size={20} />
              <span className="text-[10px] mt-0.5">Admin</span>
            </NavLink>
          )}

          {/* Spacer */}
          <div className="flex-1" />

          {/* User info + logout */}
          <div className="flex flex-col items-center gap-1 mb-2">
            <div
              className="w-8 h-8 rounded-full bg-violet-600/30 flex items-center justify-center text-violet-400 text-xs font-bold"
              title={user.username}
            >
              {user.username[0].toUpperCase()}
            </div>
            <span className="text-[9px] text-gray-500 truncate w-14 text-center">
              {user.username}
            </span>
            <button
              onClick={logout}
              className="text-gray-500 hover:text-red-400 transition-colors"
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
