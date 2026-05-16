import { useState } from "react";
import {
  Settings,
  Wallet,
  Database,
  Info,
  Save,
  Bell,
  Send,
} from "lucide-react";
import { Link } from "react-router-dom";
import {
  saveNotificationConfig,
  testNotification,
  testTradeNotifications,
} from "../lib/api";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Input } from "../components/ui/Input";
import { Select } from "../components/ui/Select";
import { Button } from "../components/ui/Button";

export default function SettingsPage() {
  const [defaultStrategy, setDefaultStrategy] = useState("V3");

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
      await saveNotificationConfig(channel, config, enabled);
      const note = enabled ? "Saved!" : "Saved (disabled — 알림 안 감)";
      setNotifStatus((s) => ({ ...s, [channel]: note }));
      setTimeout(() => setNotifStatus((s) => ({ ...s, [channel]: "" })), 3000);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setNotifStatus((s) => ({ ...s, [channel]: `Error: ${msg}` }));
    }
  };

  const handleTestNotif = async (channel: string) => {
    try {
      setNotifStatus((s) => ({ ...s, [`${channel}_test`]: "⏳ Sending..." }));
      const result = await testNotification(channel);
      // backend가 채널별 구체 결과 문자열 반환 (e.g. "Discord 전송 완료 — 채널을 확인하세요.")
      setNotifStatus((s) => ({ ...s, [`${channel}_test`]: `✓ ${result}` }));
      setTimeout(() => setNotifStatus((s) => ({ ...s, [`${channel}_test`]: "" })), 5000);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setNotifStatus((s) => ({ ...s, [`${channel}_test`]: `✗ ${msg}` }));
      // 에러는 사용자가 복사해 디버깅할 수 있도록 자동 사라지지 않게
    }
  };

  const handleTestTrades = async () => {
    try {
      setNotifStatus((s) => ({ ...s, trade_test: "⏳ 6개 메시지 전송 중..." }));
      const result = await testTradeNotifications();
      setNotifStatus((s) => ({ ...s, trade_test: `✓ ${result}` }));
      setTimeout(() => setNotifStatus((s) => ({ ...s, trade_test: "" })), 8000);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setNotifStatus((s) => ({ ...s, trade_test: `✗ ${msg}` }));
    }
  };

  return (
    <div className="space-y-6 max-w-2xl animate-fade-in">
      <h1 className="text-xl font-semibold text-zinc-100 flex items-center gap-2">
        <Settings size={22} className="text-zinc-500" />
        Settings
      </h1>

      {/* Upbit 계정 — Accounts 페이지로 이동 */}
      <Card>
        <CardHeader className="flex items-center gap-2">
          <Wallet size={16} className="text-amber-500" />
          <h2 className="text-sm font-semibold text-zinc-300">Upbit 계정</h2>
        </CardHeader>
        <CardContent>
          <p className="text-zinc-400 text-sm">
            계정 관리는 Accounts 페이지로 이동했습니다.
          </p>
          <Link to="/accounts">
            <Button className="mt-3" size="sm" variant="secondary">
              Accounts 페이지로 이동
            </Button>
          </Link>
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

          {/* Trade-notification format check — 6개 변형을 enabled 채널에 모두 전송 */}
          <div className="pt-4 border-t border-zinc-800">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <div>
                <h3 className="text-xs font-semibold text-zinc-300">Trade notifications 포맷 검증</h3>
                <p className="text-[11px] text-zinc-500 mt-0.5">
                  매수대기·매도대기·매수·매도·주문등록·늦은체결 6종을 enabled 채널로 전송 (~5초)
                </p>
              </div>
              <Button onClick={handleTestTrades} size="sm" variant="secondary">
                <Send size={12} /> Send 6 samples
              </Button>
            </div>
            {notifStatus.trade_test && (
              <p className={`text-xs mt-2 break-all ${
                notifStatus.trade_test.startsWith("✓") ? "text-emerald-400"
                : notifStatus.trade_test.startsWith("✗") ? "text-rose-400"
                : "text-sky-400"
              }`}>
                {notifStatus.trade_test}
              </p>
            )}
          </div>
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
      <div className="flex flex-wrap gap-2 items-center">
        <Button onClick={onSave} size="sm" variant="secondary">
          <Save size={12} /> Save
        </Button>
        <Button onClick={onTest} size="sm" variant="ghost">
          <Send size={12} /> Test
        </Button>
        {status && (
          <span className={`text-xs ${status.includes("disabled") ? "text-amber-400" : "text-emerald-400"}`}>
            {status}
          </span>
        )}
        {testStatus && (
          <span
            className={`text-xs break-all ${
              testStatus.startsWith("✓") ? "text-emerald-400"
              : testStatus.startsWith("✗") ? "text-rose-400"
              : "text-sky-400"
            }`}
          >
            {testStatus}
          </span>
        )}
      </div>
    </div>
  );
}
