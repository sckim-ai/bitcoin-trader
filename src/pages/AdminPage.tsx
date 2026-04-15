import { useEffect, useState } from "react";
import { useAuthStore } from "../stores/authStore";
import { listUsers, register, deleteUser, type UserInfo } from "../lib/api";
import { Card, CardContent, CardHeader } from "../components/ui/Card";
import { Input } from "../components/ui/Input";
import { Select } from "../components/ui/Select";
import { Button } from "../components/ui/Button";
import { Badge } from "../components/ui/Badge";
import { Trash2 } from "lucide-react";

export default function AdminPage() {
  const { token, isAdmin } = useAuthStore();
  const [users, setUsers] = useState<UserInfo[]>([]);
  const [error, setError] = useState("");
  const [newUser, setNewUser] = useState({ username: "", password: "", role: "trader" });
  const [loading, setLoading] = useState(false);

  const fetchUsers = async () => {
    if (!token) return;
    try {
      const data = await listUsers(token);
      setUsers(data);
      setError("");
    } catch (e) {
      setError(`Failed to load users: ${e}`);
    }
  };

  useEffect(() => {
    if (isAdmin()) fetchUsers();
  }, [token]);

  if (!isAdmin()) {
    return (
      <div className="flex items-center justify-center h-full text-zinc-600">
        Admin access required
      </div>
    );
  }

  const handleRegister = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!token || !newUser.username || !newUser.password) return;
    setLoading(true);
    try {
      await register(token, newUser.username, newUser.password, newUser.role);
      setNewUser({ username: "", password: "", role: "trader" });
      await fetchUsers();
    } catch (err) {
      setError(`Failed to create user: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (userId: number, username: string) => {
    if (!token) return;
    if (!confirm(`Delete user "${username}"?`)) return;
    try {
      await deleteUser(token, userId);
      await fetchUsers();
    } catch (err) {
      setError(`Failed to delete user: ${err}`);
    }
  };

  return (
    <div className="max-w-4xl mx-auto space-y-6 animate-fade-in">
      <h1 className="text-xl font-semibold text-zinc-100">User Management</h1>

      {error && (
        <div className="bg-rose-500/10 border border-rose-500/20 rounded-xl px-4 py-3 text-rose-400 text-sm">
          {error}
        </div>
      )}

      {/* Add User Form */}
      <Card>
        <CardHeader>
          <h2 className="text-sm font-semibold text-zinc-300">Add User</h2>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleRegister} className="flex gap-3 items-end">
            <div className="flex-1">
              <Input
                label="Username"
                value={newUser.username}
                onChange={(e) => setNewUser({ ...newUser, username: e.target.value })}
                placeholder="username"
              />
            </div>
            <div className="flex-1">
              <Input
                label="Password"
                type="password"
                value={newUser.password}
                onChange={(e) => setNewUser({ ...newUser, password: e.target.value })}
                placeholder="password"
              />
            </div>
            <div className="w-36">
              <Select
                label="Role"
                value={newUser.role}
                onChange={(e) => setNewUser({ ...newUser, role: e.target.value })}
                options={[
                  { value: "trader", label: "Trader" },
                  { value: "admin", label: "Admin" },
                ]}
              />
            </div>
            <Button type="submit" disabled={loading} size="md">
              Add
            </Button>
          </form>
        </CardContent>
      </Card>

      {/* User List */}
      <Card>
        <CardContent className="p-0">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-[#1e1e26]">
                <th className="text-left px-4 py-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">ID</th>
                <th className="text-left px-4 py-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Username</th>
                <th className="text-left px-4 py-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Role</th>
                <th className="text-left px-4 py-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Created</th>
                <th className="text-right px-4 py-3 text-[11px] font-semibold uppercase tracking-wider text-zinc-500">Actions</th>
              </tr>
            </thead>
            <tbody>
              {users.map((u) => (
                <tr key={u.id} className="border-b border-[#1e1e26]/50 hover:bg-[#141419] transition-colors">
                  <td className="px-4 py-3 font-data text-zinc-500">{u.id}</td>
                  <td className="px-4 py-3 font-medium text-zinc-200">{u.username}</td>
                  <td className="px-4 py-3">
                    <Badge variant={u.role === "admin" ? "amber" : "default"}>
                      {u.role}
                    </Badge>
                  </td>
                  <td className="px-4 py-3 text-zinc-500 text-xs">{u.created_at}</td>
                  <td className="px-4 py-3 text-right">
                    <Button
                      variant="danger"
                      size="sm"
                      onClick={() => handleDelete(u.id, u.username)}
                    >
                      <Trash2 size={12} /> Delete
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {users.length === 0 && (
            <div className="text-center text-zinc-600 py-8 text-sm">No users found</div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
