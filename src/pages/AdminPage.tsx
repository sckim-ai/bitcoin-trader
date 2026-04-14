import { useEffect, useState } from "react";
import { useAuthStore } from "../stores/authStore";
import { listUsers, register, deleteUser, type UserInfo } from "../lib/api";

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
      <div className="flex items-center justify-center h-full text-gray-400">
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
    <div className="max-w-4xl mx-auto space-y-6">
      <h1 className="text-2xl font-bold">User Management</h1>

      {error && (
        <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-3 text-red-400 text-sm">
          {error}
        </div>
      )}

      {/* Add User Form */}
      <form onSubmit={handleRegister} className="bg-gray-900 rounded-lg p-4 border border-gray-800">
        <h2 className="text-lg font-semibold mb-3">Add User</h2>
        <div className="flex gap-3 items-end">
          <div className="flex-1">
            <label className="block text-xs text-gray-400 mb-1">Username</label>
            <input
              type="text"
              value={newUser.username}
              onChange={(e) => setNewUser({ ...newUser, username: e.target.value })}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm"
              placeholder="username"
            />
          </div>
          <div className="flex-1">
            <label className="block text-xs text-gray-400 mb-1">Password</label>
            <input
              type="password"
              value={newUser.password}
              onChange={(e) => setNewUser({ ...newUser, password: e.target.value })}
              className="w-full bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm"
              placeholder="password"
            />
          </div>
          <div>
            <label className="block text-xs text-gray-400 mb-1">Role</label>
            <select
              value={newUser.role}
              onChange={(e) => setNewUser({ ...newUser, role: e.target.value })}
              className="bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm"
            >
              <option value="trader">Trader</option>
              <option value="admin">Admin</option>
            </select>
          </div>
          <button
            type="submit"
            disabled={loading}
            className="px-4 py-2 bg-violet-600 hover:bg-violet-700 rounded text-sm font-medium disabled:opacity-50"
          >
            Add
          </button>
        </div>
      </form>

      {/* User List */}
      <div className="bg-gray-900 rounded-lg border border-gray-800">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-gray-800 text-gray-400">
              <th className="text-left px-4 py-3">ID</th>
              <th className="text-left px-4 py-3">Username</th>
              <th className="text-left px-4 py-3">Role</th>
              <th className="text-left px-4 py-3">Created</th>
              <th className="text-right px-4 py-3">Actions</th>
            </tr>
          </thead>
          <tbody>
            {users.map((u) => (
              <tr key={u.id} className="border-b border-gray-800/50 hover:bg-gray-800/30">
                <td className="px-4 py-3 text-gray-400">{u.id}</td>
                <td className="px-4 py-3 font-medium">{u.username}</td>
                <td className="px-4 py-3">
                  <span
                    className={`px-2 py-0.5 rounded text-xs ${
                      u.role === "admin"
                        ? "bg-violet-600/20 text-violet-400"
                        : "bg-gray-700 text-gray-300"
                    }`}
                  >
                    {u.role}
                  </span>
                </td>
                <td className="px-4 py-3 text-gray-400">{u.created_at}</td>
                <td className="px-4 py-3 text-right">
                  <button
                    onClick={() => handleDelete(u.id, u.username)}
                    className="text-red-400 hover:text-red-300 text-xs"
                  >
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {users.length === 0 && (
          <div className="text-center text-gray-500 py-8">No users found</div>
        )}
      </div>
    </div>
  );
}
