import { CheckCircle, AlertTriangle, Activity, Wifi, Pencil, Trash2, Power } from "lucide-react";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card, CardContent } from "../ui/Card";
import type { UpbitAccount } from "../../types";

interface Props {
  account: UpbitAccount;
  onTest: (id: number) => void;
  onEdit: (id: number) => void;
  onDelete: (id: number) => void;
  onToggleEnabled: (id: number, next: boolean) => void;
  testing?: boolean;
  testResult?: string | null;
}

export function AccountCard({ account, onTest, onEdit, onDelete, onToggleEnabled, testing, testResult }: Props) {
  const keysOk = account.has_access_key && account.has_secret_key;
  const locked = account.has_running_session;

  return (
    <Card>
      <CardContent className="space-y-3">
        {/* Top row: label badge + enabled chip + running indicator */}
        <div className="flex items-center gap-2 flex-wrap">
          <Badge variant="amber">{account.label}</Badge>
          {account.enabled ? (
            <Badge variant="green">활성</Badge>
          ) : (
            <Badge variant="default">비활성</Badge>
          )}
          {locked && (
            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-xs font-medium bg-sky-500/15 text-sky-400 border border-sky-500/20">
              <Activity size={11} />
              세션 실행 중
            </span>
          )}
        </div>

        {/* Key status */}
        <div className="flex items-center gap-1.5 text-xs">
          {keysOk ? (
            <>
              <CheckCircle size={13} className="text-emerald-400" />
              <span className="text-emerald-400">API 키 등록됨</span>
            </>
          ) : (
            <>
              <AlertTriangle size={13} className="text-amber-400" />
              <span className="text-amber-400">
                API 키 {!account.has_access_key ? "Access Key" : "Secret Key"} 없음
              </span>
            </>
          )}
        </div>

        {/* Discord webhook status — 없으면 글로벌 fallback 사용 명시. */}
        <div className="text-xs text-zinc-500">
          {account.discord_webhook_url
            ? <span className="text-violet-400">Discord: 계정 전용 채널</span>
            : <span>Discord: 글로벌 fallback</span>}
        </div>

        {/* Test result */}
        {testResult != null && (
          <p className={`text-xs break-all ${
            testResult.startsWith("✓") ? "text-emerald-400"
            : testResult.startsWith("✗") ? "text-rose-400"
            : "text-sky-400"
          }`}>
            {testResult}
          </p>
        )}

        {/* Action buttons */}
        <div className="flex flex-wrap gap-2 pt-1">
          <Button
            size="sm"
            variant="secondary"
            onClick={() => onTest(account.id)}
            disabled={testing || !keysOk}
          >
            <Wifi size={13} />
            {testing ? "테스트 중…" : "Test"}
          </Button>
          <Button
            size="sm"
            variant="secondary"
            onClick={() => onEdit(account.id)}
          >
            <Pencil size={13} />
            Edit
          </Button>
          <Button
            size="sm"
            variant={account.enabled ? "ghost" : "success"}
            onClick={() => onToggleEnabled(account.id, !account.enabled)}
            disabled={locked}
            title={locked ? "실행 중인 세션이 있어 변경 불가" : undefined}
          >
            <Power size={13} />
            {account.enabled ? "비활성화" : "활성화"}
          </Button>
          <Button
            size="sm"
            variant="danger"
            onClick={() => onDelete(account.id)}
            disabled={locked}
            title={locked ? "실행 중인 세션이 있어 삭제 불가" : undefined}
          >
            <Trash2 size={13} />
            Delete
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
