import { useEffect, useState } from "react";
import { Users, Plus } from "lucide-react";
import { Button } from "../components/ui/Button";
import { AccountCard } from "../components/accounts/AccountCard";
import { AddAccountDialog } from "../components/accounts/AddAccountDialog";
import { EditAccountDialog } from "../components/accounts/EditAccountDialog";
import { confirmDialog } from "../components/ui/ConfirmDialog";
import {
  listUpbitAccounts,
  testUpbitAccountConnection,
  testAccountDiscord,
  setUpbitAccountEnabled,
  deleteUpbitAccount,
} from "../lib/live";
import type { UpbitAccount } from "../types";

export default function AccountsPage() {
  const [accounts, setAccounts] = useState<UpbitAccount[]>([]);
  const [loading, setLoading] = useState(true);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);

  // Per-account test state — 키 연결 테스트와 Discord 테스트가 같은 result line을
  // 공유한다(카드 공간 절약). 마지막 액션의 결과만 표시되고, 메시지 prefix("연결 …" /
  // "Discord …")로 어떤 액션인지 구분된다.
  const [testing, setTesting] = useState<Record<number, boolean>>({});
  const [testingDiscord, setTestingDiscord] = useState<Record<number, boolean>>({});
  const [testResults, setTestResults] = useState<Record<number, string | null>>({});

  const refresh = async () => {
    setLoading(true);
    try {
      setAccounts(await listUpbitAccounts());
    } catch (e) {
      console.error("listUpbitAccounts failed:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { refresh(); }, []);

  const handleTest = async (id: number) => {
    setTesting((s) => ({ ...s, [id]: true }));
    setTestResults((s) => ({ ...s, [id]: "⏳ 연결 테스트 중…" }));
    try {
      const n = await testUpbitAccountConnection(id);
      setTestResults((s) => ({ ...s, [id]: `✓ 연결 성공 — 보유 통화 ${n}개` }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setTestResults((s) => ({ ...s, [id]: `✗ 실패: ${msg}` }));
    } finally {
      setTesting((s) => ({ ...s, [id]: false }));
    }
  };

  const handleTestDiscord = async (id: number) => {
    setTestingDiscord((s) => ({ ...s, [id]: true }));
    setTestResults((s) => ({ ...s, [id]: "⏳ Discord 전송 중…" }));
    try {
      const msg = await testAccountDiscord(id);
      setTestResults((s) => ({ ...s, [id]: msg }));
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setTestResults((s) => ({ ...s, [id]: `✗ Discord 실패: ${msg}` }));
    } finally {
      setTestingDiscord((s) => ({ ...s, [id]: false }));
    }
  };

  const handleEdit = (id: number) => {
    setEditingId(id);
  };

  const handleToggleEnabled = async (id: number, next: boolean) => {
    try {
      await setUpbitAccountEnabled(id, next);
      await refresh();
    } catch (e) {
      console.error("setUpbitAccountEnabled failed:", e);
    }
  };

  const handleDelete = async (id: number) => {
    const account = accounts.find((a) => a.id === id);
    const ok = await confirmDialog({
      title: "계정 삭제",
      severity: "danger",
      confirmLabel: "삭제",
      body: (
        <div className="space-y-2">
          <p>
            <span className="font-semibold text-zinc-200">{account?.label ?? `계정 #${id}`}</span>을 삭제합니다.
          </p>
          <p className="text-xs text-zinc-500">OS 키체인의 API 키도 함께 삭제됩니다. 이 작업은 되돌릴 수 없습니다.</p>
        </div>
      ),
    });
    if (!ok) return;
    try {
      await deleteUpbitAccount(id);
      setTestResults((s) => { const next = { ...s }; delete next[id]; return next; });
      await refresh();
    } catch (e) {
      console.error("deleteUpbitAccount failed:", e);
    }
  };

  return (
    <div className="space-y-6 max-w-3xl animate-fade-in">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold text-zinc-100 flex items-center gap-2">
          <Users size={22} className="text-zinc-500" />
          Upbit 계정 관리
        </h1>
        <Button size="sm" onClick={() => setShowAddDialog(true)}>
          <Plus size={14} />
          계정 추가
        </Button>
      </div>

      {/* Content */}
      {loading ? (
        <p className="text-sm text-zinc-500">불러오는 중…</p>
      ) : accounts.length === 0 ? (
        <div className="text-center py-16 text-zinc-500 space-y-2">
          <Users size={40} className="mx-auto text-zinc-700" />
          <p className="text-sm">등록된 계정이 없습니다.</p>
          <p className="text-xs">
            "+ 계정 추가" 버튼으로 Upbit API 키를 등록하세요.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          {accounts.map((account) => (
            <AccountCard
              key={account.id}
              account={account}
              onTest={handleTest}
              onTestDiscord={handleTestDiscord}
              onEdit={handleEdit}
              onDelete={handleDelete}
              onToggleEnabled={handleToggleEnabled}
              testing={testing[account.id]}
              testingDiscord={testingDiscord[account.id]}
              testResult={testResults[account.id]}
            />
          ))}
        </div>
      )}

      <AddAccountDialog
        open={showAddDialog}
        onClose={() => setShowAddDialog(false)}
        onAdded={refresh}
      />

      {editingId !== null && (() => {
        const acc = accounts.find((a) => a.id === editingId);
        if (!acc) return null;
        return (
          <EditAccountDialog
            account={acc}
            onClose={() => setEditingId(null)}
            onUpdated={refresh}
          />
        );
      })()}
    </div>
  );
}
