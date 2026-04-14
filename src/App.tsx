import { BrowserRouter, Routes, Route, NavLink } from "react-router-dom";
import {
  Database,
  LineChart,
  Settings,
  Zap,
  TrendingUp,
} from "lucide-react";
import DataLoadPage from "./pages/DataLoadPage";
import SimulationPage from "./pages/SimulationPage";
import OptimizationPage from "./pages/OptimizationPage";
import LiveTradingPage from "./pages/LiveTradingPage";
import SettingsPage from "./pages/SettingsPage";

const NAV_ITEMS = [
  { to: "/", icon: Database, label: "Data" },
  { to: "/simulation", icon: LineChart, label: "Simulation" },
  { to: "/optimization", icon: TrendingUp, label: "Optimization" },
  { to: "/live", icon: Zap, label: "Live" },
  { to: "/settings", icon: Settings, label: "Settings" },
] as const;

function App() {
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
        </nav>

        {/* Main content */}
        <main className="flex-1 overflow-y-auto p-6">
          <Routes>
            <Route path="/" element={<DataLoadPage />} />
            <Route path="/simulation" element={<SimulationPage />} />
            <Route path="/optimization" element={<OptimizationPage />} />
            <Route path="/live" element={<LiveTradingPage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}

export default App;
