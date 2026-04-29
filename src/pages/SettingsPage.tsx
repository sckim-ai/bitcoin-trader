import { useEffect, useState } from "react";
import {
  Settings,
  Key,
  Database,
  Info,
  Save,
  Bell,
  Send,
  Trash2,
  Wifi,
} from "lucide-react";
import {
  saveNotificationConfig,
  testNotification,
  saveUpbitKeys,
  getUpbitKeyStatus,
  clearUpbitKeys,
  testUpbitConnection,
  type UpbitKeyStatus,
} from "../lib/api";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Input } from "../components/ui/Input";
import { Select } from "../components/ui/Select";
import { Button } from "../components/ui/Button";

export default function SettingsPage() {
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [defaultStrategy, setDefaultStrategy] = useState("V3");
  const [keyStatus, setKeyStatus] = useState<UpbitKeyStatus | null>(null);
  const [keyMsg, setKeyMsg] = useState<{ tone: "ok" | "err" | "info"; text: string } | null>(null);
  const [keyBusy, setKeyBusy] = useState(false);

  const refreshKeyStatus = async () => {
    try {
      setKeyStatus(await getUpbitKeyStatus());
    } catch (e) {
      // PWA fallback path — keep the badge in null/loading state but surface
      // the reason once so a developer can spot the protocol mismatch.
      console.warn("getUpbitKeyStatus failed:", e);
    }
  };
  useEffect(() => { refreshKeyStatus(); }, []);

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
    { key: "V3", name: "Regime Adaptive" },
  ];

  const handleSaveKeys = async () => {
    if (!accessKey.trim() || !secretKey.trim()) {
      setKeyMsg({ tone: "err", text: "Both keys are required." });
      return;
    }
    setKeyBusy(true);
    setKeyMsg(null);
    try {
      await saveUpbitKeys(accessKey.trim(), secretKey.trim());
      setAccessKey("");
      setSecretKey("");
      await refreshKeyStatus();
      setKeyMsg({ tone: "ok", text: "Saved to OS keychain." });
    } catch (e) {
      setKeyMsg({ tone: "err", text: `Save failed: ${e instanceof Error ? e.message : String(e)}` });
    } finally {
      setKeyBusy(false);
    }
  };

  const handleTestKeys = async () => {
    setKeyBusy(true);
    setKeyMsg({ tone: "info", text: "Testing connection..." });
    try {
      const n = await testUpbitConnection();
      setKeyMsg({ tone: "ok", text: `Connection OK — account holds ${n} currencies.` });
    } catch (e) {
      setKeyMsg({ tone: "err", text: `Test failed: ${e instanceof Error ? e.message : String(e)}` });
    } finally {
      setKeyBusy(false);
    }
  };

  const handleClearKeys = async () => {
    if (!window.confirm("Remove the saved Upbit API keys from the OS keychain?")) return;
    setKeyBusy(true);
    try {
      await clearUpbitKeys();
      await refreshKeyStatus();
      setKeyMsg({ tone: "ok", text: "Keys cleared." });
    } catch (e) {
      setKeyMsg({ tone: "err", text: `Clear failed: ${e instanceof Error ? e.message : String(e)}` });
    } finally {
      setKeyBusy(false);
    }
  };

  const token = localStorage.getItem("auth_token") || "";

  const handleSaveNotif = async (channel: string) => {
    try {
      let config = "";
      let enabled = false;
      if (channel === "fcm") {
        config = JSON.stringify({ server_key: fcmServerKey, device_token: fcmDeviceToken });
        enabled = fcmEnabled;
      } else if (channel === "discord") {
        config = JSON.stringify({ webhook_url: discordWebhook });
        enabled = discordEnabled;
      } else if (channel === "telegram") {
        config = JSON.stringify({ bot_token: telegramBotToken, chat_id: telegramChatId });
        enabled = telegramEnabled;
      }
      await saveNotificationConfig(token, channel, config, enabled);
      setNotifStatus((s) => ({ ...s, [channel]: "Saved!" }));
      setTimeout(() => setNotifStatus((s) => ({ ...s, [channel]: "" })), 2000);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setNotifStatus((s) => ({ ...s, [channel]: `Error: ${msg}` }));
    }
  };

  const handleTestNotif = async (channel: string) => {
    try {
      setNotifStatus((s) => ({ ...s, [`${channel}_test`]: "Sending..." }));
      await testNotification(token, channel);
      setNotifStatus((s) => ({ ...s, [`${channel}_test`]: "Sent!" }));
      setTimeout(() => setNotifStatus((s) => ({ ...s, [`${channel}_test`]: "" })), 2000);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setNotifStatus((s) => ({ ...s, [`${channel}_test`]: `Error: ${msg}` }));
    }
  };

  return (
    <div className="space-y-6 max-w-2xl animate-fade-in">
      <h1 className="text-xl font-semibold text-zinc-100 flex items-center gap-2">
        <Settings size={22} className="text-zinc-500" />
        Settings
      </h1>

      {/* Upbit API Keys — OS keychain via Tauri keyring crate */}
      <Card>
        <CardHeader className="flex items-center gap-2">
          <Key size={16} className="text-amber-500" />
          <h2 className="text-sm font-semibold text-zinc-300">Upbit API Keys</h2>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-xs text-zinc-500">
            실거래 자동매매에 사용. OS 키체인(Windows Credential Manager / macOS Keychain / Linux Secret Service)에 저장되며 코드/설정 파일에는 남지 않습니다.
          </p>

          {/* Status badge + diagnostic */}
          <div className="space-y-1 text-xs">
            <div className="flex items-center gap-2">
              {keyStatus == null ? (
                <span className="text-zinc-500">Loading…</span>
              ) : keyStatus.has_access && keyStatus.has_secret ? (
                <span className="px-2 py-0.5 rounded bg-emerald-900/40 text-emerald-300 border border-emerald-800">
                  Configured · source: <span className="font-data">{keyStatus.source}</span>
                </span>
              ) : (
                <span className="px-2 py-0.5 rounded bg-zinc-800 text-zinc-400 border border-zinc-700">
                  Not configured
                </span>
              )}
            </div>
            {keyStatus?.access_error && (
              <div className="text-rose-400 break-all">access read error: {keyStatus.access_error}</div>
            )}
            {keyStatus?.secret_error && (
              <div className="text-rose-400 break-all">secret read error: {keyStatus.secret_error}</div>
            )}
          </div>

          <Input
            label="Access Key"
            type="password"
            passwordToggle
            value={accessKey}
            onChange={(e) => setAccessKey(e.target.value)}
            placeholder={keyStatus?.has_access ? "(saved — enter new value to replace)" : "Enter Upbit Access Key"}
            disabled={keyBusy}
          />
          <Input
            label="Secret Key"
            type="password"
            passwordToggle
            value={secretKey}
            onChange={(e) => setSecretKey(e.target.value)}
            placeholder={keyStatus?.has_secret ? "(saved — enter new value to replace)" : "Enter Upbit Secret Key"}
            disabled={keyBusy}
          />

          <div className="flex flex-wrap gap-2">
            <Button onClick={handleSaveKeys} size="sm" disabled={keyBusy || (!accessKey && !secretKey)}>
              <Save size={14} /> Save
            </Button>
            <Button
              onClick={handleTestKeys}
              size="sm"
              variant="secondary"
              disabled={keyBusy || !keyStatus?.has_access || !keyStatus?.has_secret}
            >
              <Wifi size={14} /> Test connection
            </Button>
            <Button
              onClick={handleClearKeys}
              size="sm"
              variant="danger"
              disabled={keyBusy || !keyStatus?.has_access}
            >
              <Trash2 size={14} /> Clear
            </Button>
          </div>

          {keyMsg && (
            <p className={`text-xs ${
              keyMsg.tone === "ok" ? "text-emerald-400" :
              keyMsg.tone === "err" ? "text-rose-400" : "text-zinc-400"
            }`}>{keyMsg.text}</p>
          )}
        </CardContent>
      </Card>

      {/* Default Strategy */}
      <Card>
        <CardHeader className="flex items-center gap-2">
          <Database size={16} className="text-amber-500" />
          <h2 className="text-sm font-semibold text-zinc-300">Trading Configuration</h2>
        </CardHeader>
        <CardContent>
          <Select
            label="Default Strategy"
            value={defaultStrategy}
            onChange={(e) => setDefaultStrategy(e.target.value)}
            options={strategies.map((s) => ({ value: s.key, label: `${s.key} - ${s.name}` }))}
          />
        </CardContent>
      </Card>

      {/* Notifications */}
      <Card>
        <CardHeader className="flex items-center gap-2">
          <Bell size={16} className="text-amber-500" />
          <h2 className="text-sm font-semibold text-zinc-300">Notifications</h2>
        </CardHeader>
        <CardContent className="space-y-6">
          {/* FCM */}
          <NotifSection
            title="FCM Push Notification"
            enabled={fcmEnabled}
            onToggle={setFcmEnabled}
            status={notifStatus.fcm}
            testStatus={notifStatus.fcm_test}
            onSave={() => handleSaveNotif("fcm")}
            onTest={() => handleTestNotif("fcm")}
          >
            <Input type="password" passwordToggle value={fcmServerKey} onChange={(e) => setFcmServerKey(e.target.value)} placeholder="Server Key" />
            <Input value={fcmDeviceToken} onChange={(e) => setFcmDeviceToken(e.target.value)} placeholder="Device Token" />
          </NotifSection>

          {/* Discord */}
          <NotifSection
            title="Discord Webhook"
            enabled={discordEnabled}
            onToggle={setDiscordEnabled}
            status={notifStatus.discord}
            testStatus={notifStatus.discord_test}
            onSave={() => handleSaveNotif("discord")}
            onTest={() => handleTestNotif("discord")}
          >
            <Input value={discordWebhook} onChange={(e) => setDiscordWebhook(e.target.value)} placeholder="Webhook URL" />
          </NotifSection>

          {/* Telegram */}
          <NotifSection
            title="Telegram Bot"
            enabled={telegramEnabled}
            onToggle={setTelegramEnabled}
            status={notifStatus.telegram}
            testStatus={notifStatus.telegram_test}
            onSave={() => handleSaveNotif("telegram")}
            onTest={() => handleTestNotif("telegram")}
          >
            <Input type="password" passwordToggle value={telegramBotToken} onChange={(e) => setTelegramBotToken(e.target.value)} placeholder="Bot Token" />
            <Input value={telegramChatId} onChange={(e) => setTelegramChatId(e.target.value)} placeholder="Chat ID" />
          </NotifSection>
        </CardContent>
      </Card>

      {/* Data Management */}
      <Card>
        <CardHeader className="flex items-center gap-2">
          <Database size={16} className="text-amber-500" />
          <h2 className="text-sm font-semibold text-zinc-300">Data Management</h2>
        </CardHeader>
        <CardContent>
          <p className="text-xs text-zinc-500 mb-3">Import CSV data files for backtesting and analysis.</p>
          <Button variant="secondary">Import CSV File</Button>
        </CardContent>
      </Card>

      {/* About */}
      <Card>
        <CardHeader className="flex items-center gap-2">
          <Info size={16} className="text-amber-500" />
          <h2 className="text-sm font-semibold text-zinc-300">About</h2>
        </CardHeader>
        <CardContent>
          <div className="text-xs text-zinc-500 space-y-1">
            <p>Bitcoin Trader v0.1.0</p>
            <p>Upbit-based algorithmic trading system</p>
            <p>Strategy: V3 Regime Adaptive</p>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function NotifSection({
  title,
  enabled,
  onToggle,
  children,
  status,
  testStatus,
  onSave,
  onTest,
}: {
  title: string;
  enabled: boolean;
  onToggle: (v: boolean) => void;
  children: React.ReactNode;
  status?: string;
  testStatus?: string;
  onSave: () => void;
  onTest: () => void;
}) {
  return (
    <div className="space-y-3 border-b border-[#1e1e26] pb-5 last:border-b-0 last:pb-0">
      <div className="flex items-center justify-between">
        <h3 className="text-xs font-semibold text-zinc-300">{title}</h3>
        <button
          onClick={() => onToggle(!enabled)}
          className={`toggle-switch ${enabled ? "active" : ""}`}
          aria-label={`Toggle ${title}`}
        />
      </div>
      <div className="space-y-2">
        {children}
      </div>
      <div className="flex gap-2 items-center">
        <Button onClick={onSave} size="sm" variant="secondary">
          <Save size={12} /> Save
        </Button>
        <Button onClick={onTest} size="sm" variant="ghost">
          <Send size={12} /> Test
        </Button>
        {status && <span className="text-xs text-emerald-400">{status}</span>}
        {testStatus && <span className="text-xs text-sky-400">{testStatus}</span>}
      </div>
    </div>
  );
}
