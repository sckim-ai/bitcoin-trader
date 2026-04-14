import { useState } from "react";
import {
  Settings,
  Key,
  Database,
  Info,
  Save,
  Check,
  Bell,
  Send,
} from "lucide-react";
import {
  saveNotificationConfig,
  testNotification,
} from "../lib/api";

export default function SettingsPage() {
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [defaultStrategy, setDefaultStrategy] = useState("V0");
  const [saved, setSaved] = useState(false);

  // Notification state
  const [fcmServerKey, setFcmServerKey] = useState("");
  const [fcmDeviceToken, setFcmDeviceToken] = useState("");
  const [fcmEnabled, setFcmEnabled] = useState(false);

  const [discordWebhook, setDiscordWebhook] = useState("");
  const [discordEnabled, setDiscordEnabled] = useState(false);

  const [telegramBotToken, setTelegramBotToken] = useState("");
  const [telegramChatId, setTelegramChatId] = useState("");
  const [telegramEnabled, setTelegramEnabled] = useState(false);

  const [notifStatus, setNotifStatus] = useState<Record<string, string>>({});

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

  const token = localStorage.getItem("auth_token") || "";

  const handleSaveNotif = async (channel: string) => {
    try {
      let config = "";
      let enabled = false;
      if (channel === "fcm") {
        config = JSON.stringify({
          server_key: fcmServerKey,
          device_token: fcmDeviceToken,
        });
        enabled = fcmEnabled;
      } else if (channel === "discord") {
        config = JSON.stringify({ webhook_url: discordWebhook });
        enabled = discordEnabled;
      } else if (channel === "telegram") {
        config = JSON.stringify({
          bot_token: telegramBotToken,
          chat_id: telegramChatId,
        });
        enabled = telegramEnabled;
      }
      await saveNotificationConfig(token, channel, config, enabled);
      setNotifStatus((s) => ({ ...s, [channel]: "Saved!" }));
      setTimeout(
        () => setNotifStatus((s) => ({ ...s, [channel]: "" })),
        2000
      );
    } catch (e: any) {
      setNotifStatus((s) => ({ ...s, [channel]: `Error: ${e.message}` }));
    }
  };

  const handleTestNotif = async (channel: string) => {
    try {
      setNotifStatus((s) => ({ ...s, [`${channel}_test`]: "Sending..." }));
      await testNotification(token, channel);
      setNotifStatus((s) => ({ ...s, [`${channel}_test`]: "Sent!" }));
      setTimeout(
        () => setNotifStatus((s) => ({ ...s, [`${channel}_test`]: "" })),
        2000
      );
    } catch (e: any) {
      setNotifStatus((s) => ({
        ...s,
        [`${channel}_test`]: `Error: ${e.message}`,
      }));
    }
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

      {/* Notifications */}
      <section className="bg-gray-900 rounded-lg p-6 space-y-6">
        <h2 className="text-lg font-semibold flex items-center gap-2">
          <Bell size={18} className="text-violet-400" />
          Notifications
        </h2>

        {/* FCM */}
        <div className="space-y-3 border-b border-gray-800 pb-4">
          <div className="flex items-center justify-between">
            <h3 className="font-medium text-sm">FCM Push Notification</h3>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={fcmEnabled}
                onChange={(e) => setFcmEnabled(e.target.checked)}
                className="accent-violet-500"
              />
              Enable
            </label>
          </div>
          <input
            type="password"
            value={fcmServerKey}
            onChange={(e) => setFcmServerKey(e.target.value)}
            placeholder="Server Key"
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
          />
          <input
            type="text"
            value={fcmDeviceToken}
            onChange={(e) => setFcmDeviceToken(e.target.value)}
            placeholder="Device Token"
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
          />
          <div className="flex gap-2 items-center">
            <button
              onClick={() => handleSaveNotif("fcm")}
              className="bg-violet-600 hover:bg-violet-700 px-3 py-1.5 rounded text-xs transition-colors"
            >
              <Save size={14} className="inline mr-1" />
              Save
            </button>
            <button
              onClick={() => handleTestNotif("fcm")}
              className="bg-gray-700 hover:bg-gray-600 px-3 py-1.5 rounded text-xs transition-colors"
            >
              <Send size={14} className="inline mr-1" />
              Test
            </button>
            {notifStatus.fcm && (
              <span className="text-xs text-green-400">{notifStatus.fcm}</span>
            )}
            {notifStatus.fcm_test && (
              <span className="text-xs text-blue-400">
                {notifStatus.fcm_test}
              </span>
            )}
          </div>
        </div>

        {/* Discord */}
        <div className="space-y-3 border-b border-gray-800 pb-4">
          <div className="flex items-center justify-between">
            <h3 className="font-medium text-sm">Discord Webhook</h3>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={discordEnabled}
                onChange={(e) => setDiscordEnabled(e.target.checked)}
                className="accent-violet-500"
              />
              Enable
            </label>
          </div>
          <input
            type="text"
            value={discordWebhook}
            onChange={(e) => setDiscordWebhook(e.target.value)}
            placeholder="Webhook URL"
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
          />
          <div className="flex gap-2 items-center">
            <button
              onClick={() => handleSaveNotif("discord")}
              className="bg-violet-600 hover:bg-violet-700 px-3 py-1.5 rounded text-xs transition-colors"
            >
              <Save size={14} className="inline mr-1" />
              Save
            </button>
            <button
              onClick={() => handleTestNotif("discord")}
              className="bg-gray-700 hover:bg-gray-600 px-3 py-1.5 rounded text-xs transition-colors"
            >
              <Send size={14} className="inline mr-1" />
              Test
            </button>
            {notifStatus.discord && (
              <span className="text-xs text-green-400">
                {notifStatus.discord}
              </span>
            )}
            {notifStatus.discord_test && (
              <span className="text-xs text-blue-400">
                {notifStatus.discord_test}
              </span>
            )}
          </div>
        </div>

        {/* Telegram */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="font-medium text-sm">Telegram Bot</h3>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={telegramEnabled}
                onChange={(e) => setTelegramEnabled(e.target.checked)}
                className="accent-violet-500"
              />
              Enable
            </label>
          </div>
          <input
            type="password"
            value={telegramBotToken}
            onChange={(e) => setTelegramBotToken(e.target.value)}
            placeholder="Bot Token"
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
          />
          <input
            type="text"
            value={telegramChatId}
            onChange={(e) => setTelegramChatId(e.target.value)}
            placeholder="Chat ID"
            className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm focus:outline-none focus:border-violet-500"
          />
          <div className="flex gap-2 items-center">
            <button
              onClick={() => handleSaveNotif("telegram")}
              className="bg-violet-600 hover:bg-violet-700 px-3 py-1.5 rounded text-xs transition-colors"
            >
              <Save size={14} className="inline mr-1" />
              Save
            </button>
            <button
              onClick={() => handleTestNotif("telegram")}
              className="bg-gray-700 hover:bg-gray-600 px-3 py-1.5 rounded text-xs transition-colors"
            >
              <Send size={14} className="inline mr-1" />
              Test
            </button>
            {notifStatus.telegram && (
              <span className="text-xs text-green-400">
                {notifStatus.telegram}
              </span>
            )}
            {notifStatus.telegram_test && (
              <span className="text-xs text-blue-400">
                {notifStatus.telegram_test}
              </span>
            )}
          </div>
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
          <p>
            Strategies: V0-V5 (Volume Decay, Enhanced Volume, Multi Indicator,
            Regime Adaptive, ML, Enhanced Adaptive)
          </p>
        </div>
      </section>
    </div>
  );
}
