import { useState } from "react";
import { Settings, Key, Database, Info, Save, Check } from "lucide-react";

export default function SettingsPage() {
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [defaultStrategy, setDefaultStrategy] = useState("V0");
  const [saved, setSaved] = useState(false);

  const strategies = [
    { key: "V0", name: "Volume Decay" },
    { key: "V1", name: "Enhanced Volume" },
    { key: "V2", name: "Multi Indicator" },
    { key: "V3", name: "Regime Adaptive" },
    { key: "V4", name: "Machine Learning" },
    { key: "V5", name: "Enhanced Adaptive" },
  ];

  const handleSaveKeys = () => {
    // TODO: invoke Tauri command to save keys securely
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div className="space-y-6 max-w-2xl">
      <h1 className="text-2xl font-bold flex items-center gap-2">
        <Settings size={24} />
        Settings
      </h1>

      {/* API Keys */}
      <section className="bg-gray-900 rounded-lg p-6 space-y-4">
        <h2 className="text-lg font-semibold flex items-center gap-2">
          <Key size={18} className="text-violet-400" />
          API Keys
        </h2>
        <p className="text-sm text-gray-400">
          Upbit API keys for live trading. Keys are stored locally.
        </p>
        <div className="space-y-3">
          <div>
            <label className="block text-sm text-gray-400 mb-1">
              Access Key
            </label>
            <input
              type="password"
              value={accessKey}
              onChange={(e) => setAccessKey(e.target.value)}
              placeholder="Enter Upbit Access Key"
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
            />
          </div>
          <div>
            <label className="block text-sm text-gray-400 mb-1">
              Secret Key
            </label>
            <input
              type="password"
              value={secretKey}
              onChange={(e) => setSecretKey(e.target.value)}
              placeholder="Enter Upbit Secret Key"
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
            />
          </div>
          <button
            onClick={handleSaveKeys}
            className="flex items-center gap-2 bg-violet-600 hover:bg-violet-700 px-4 py-2 rounded text-sm transition-colors"
          >
            {saved ? (
              <>
                <Check size={16} /> Saved
              </>
            ) : (
              <>
                <Save size={16} /> Save Keys
              </>
            )}
          </button>
        </div>
      </section>

      {/* Default Strategy */}
      <section className="bg-gray-900 rounded-lg p-6 space-y-4">
        <h2 className="text-lg font-semibold flex items-center gap-2">
          <Database size={18} className="text-violet-400" />
          Trading Configuration
        </h2>
        <div>
          <label className="block text-sm text-gray-400 mb-1">
            Default Strategy
          </label>
          <select
            value={defaultStrategy}
            onChange={(e) => setDefaultStrategy(e.target.value)}
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
          >
            {strategies.map((s) => (
              <option key={s.key} value={s.key}>
                {s.key} - {s.name}
              </option>
            ))}
          </select>
        </div>
      </section>

      {/* Data Management */}
      <section className="bg-gray-900 rounded-lg p-6 space-y-4">
        <h2 className="text-lg font-semibold flex items-center gap-2">
          <Database size={18} className="text-violet-400" />
          Data Management
        </h2>
        <p className="text-sm text-gray-400">
          Import CSV data files for backtesting and analysis.
        </p>
        <button className="bg-gray-700 hover:bg-gray-600 px-4 py-2 rounded text-sm transition-colors">
          Import CSV File
        </button>
      </section>

      {/* About */}
      <section className="bg-gray-900 rounded-lg p-6 space-y-2">
        <h2 className="text-lg font-semibold flex items-center gap-2">
          <Info size={18} className="text-violet-400" />
          About
        </h2>
        <div className="text-sm text-gray-400 space-y-1">
          <p>Bitcoin Trader v0.1.0</p>
          <p>Upbit-based algorithmic trading system</p>
          <p>Strategies: V0-V5 (Volume Decay, Enhanced Volume, Multi Indicator, Regime Adaptive, ML, Enhanced Adaptive)</p>
        </div>
      </section>
    </div>
  );
}
