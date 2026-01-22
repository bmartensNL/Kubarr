import React, { useState, useEffect } from 'react';
import { useAuth } from '../contexts/AuthContext';
import UserList from '../components/users/UserList';
import UserForm from '../components/users/UserForm';
import {
  User,
  getUsers,
  getPendingUsers,
  createUser,
  updateUser,
  deleteUser,
  approveUser,
  rejectUser,
  CreateUserRequest,
  UpdateUserRequest,
} from '../api/users';

type ViewMode = 'list' | 'create' | 'edit';

const UsersPage: React.FC = () => {
  const { isAdmin } = useAuth();
  const [users, setUsers] = useState<User[]>([]);
  const [pendingUsers, setPendingUsers] = useState<User[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>('list');
  const [selectedUser, setSelectedUser] = useState<User | null>(null);
  const [activeTab, setActiveTab] = useState<'all' | 'pending'>('all');

  useEffect(() => {
    if (isAdmin) {
      loadUsers();
      loadPendingUsers();
    }
  }, [isAdmin]);

  const loadUsers = async () => {
    try {
      setLoading(true);
      const data = await getUsers();
      setUsers(data);
      setError(null);
    } catch (err: any) {
      setError(err.response?.data?.detail || 'Failed to load users');
    } finally {
      setLoading(false);
    }
  };

  const loadPendingUsers = async () => {
    try {
      const data = await getPendingUsers();
      setPendingUsers(data);
    } catch (err: any) {
      console.error('Failed to load pending users:', err);
    }
  };

  const handleCreateUser = async (data: CreateUserRequest | UpdateUserRequest) => {
    await createUser(data as CreateUserRequest);
    await loadUsers();
    setViewMode('list');
  };

  const handleUpdateUser = async (data: CreateUserRequest | UpdateUserRequest) => {
    if (selectedUser) {
      await updateUser(selectedUser.id, data as UpdateUserRequest);
      await loadUsers();
      setViewMode('list');
      setSelectedUser(null);
    }
  };

  const handleDeleteUser = async (user: User) => {
    if (window.confirm(`Are you sure you want to delete user "${user.username}"?`)) {
      try {
        await deleteUser(user.id);
        await loadUsers();
        await loadPendingUsers();
      } catch (err: any) {
        setError(err.response?.data?.detail || 'Failed to delete user');
      }
    }
  };

  const handleApproveUser = async (user: User) => {
    try {
      await approveUser(user.id);
      await loadUsers();
      await loadPendingUsers();
    } catch (err: any) {
      setError(err.response?.data?.detail || 'Failed to approve user');
    }
  };

  const handleRejectUser = async (user: User) => {
    if (window.confirm(`Are you sure you want to reject user "${user.username}"?`)) {
      try {
        await rejectUser(user.id);
        await loadPendingUsers();
      } catch (err: any) {
        setError(err.response?.data?.detail || 'Failed to reject user');
      }
    }
  };

  const handleEditUser = (user: User) => {
    setSelectedUser(user);
    setViewMode('edit');
  };

  const handleCancel = () => {
    setViewMode('list');
    setSelectedUser(null);
  };

  if (!isAdmin) {
    return (
      <div className="max-w-7xl mx-auto px-4 py-8">
        <div className="bg-red-900 border border-red-700 text-white px-4 py-3 rounded">
          You do not have permission to access this page. Admin privileges required.
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <h1 className="text-3xl font-bold">User Management</h1>
        {viewMode === 'list' && (
          <button
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-md font-medium transition-colors"
            onClick={() => setViewMode('create')}
          >
            Create New User
          </button>
        )}
      </div>

      {error && (
        <div className="bg-red-900 border border-red-700 text-white px-4 py-3 rounded flex justify-between items-center">
          <span>{error}</span>
          <button
            onClick={() => setError(null)}
            className="text-white hover:text-gray-300"
            aria-label="Close"
          >
            <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 20 20">
              <path fillRule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clipRule="evenodd" />
            </svg>
          </button>
        </div>
      )}

      {viewMode === 'list' && (
        <>
          <div className="border-b border-gray-700">
            <nav className="flex space-x-8">
              <button
                className={`py-4 px-1 border-b-2 font-medium text-sm transition-colors ${
                  activeTab === 'all'
                    ? 'border-blue-500 text-blue-500'
                    : 'border-transparent text-gray-400 hover:text-gray-300 hover:border-gray-300'
                }`}
                onClick={() => setActiveTab('all')}
              >
                All Users ({users.length})
              </button>
              <button
                className={`py-4 px-1 border-b-2 font-medium text-sm transition-colors ${
                  activeTab === 'pending'
                    ? 'border-blue-500 text-blue-500'
                    : 'border-transparent text-gray-400 hover:text-gray-300 hover:border-gray-300'
                }`}
                onClick={() => setActiveTab('pending')}
              >
                Pending Approval ({pendingUsers.length})
              </button>
            </nav>
          </div>

          {loading ? (
            <div className="flex justify-center items-center py-12">
              <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
            </div>
          ) : (
            <>
              {activeTab === 'all' && (
                <UserList
                  users={users}
                  onEdit={handleEditUser}
                  onDelete={handleDeleteUser}
                  onApprove={handleApproveUser}
                  showActions={true}
                />
              )}
              {activeTab === 'pending' && (
                <UserList
                  users={pendingUsers}
                  onApprove={handleApproveUser}
                  onReject={handleRejectUser}
                  onDelete={handleDeleteUser}
                  showActions={true}
                />
              )}
            </>
          )}
        </>
      )}

      {viewMode === 'create' && (
        <div className="bg-gray-800 rounded-lg border border-gray-700 p-6">
          <UserForm onSubmit={handleCreateUser} onCancel={handleCancel} isEdit={false} />
        </div>
      )}

      {viewMode === 'edit' && selectedUser && (
        <div className="bg-gray-800 rounded-lg border border-gray-700 p-6">
          <UserForm
            user={selectedUser}
            onSubmit={handleUpdateUser}
            onCancel={handleCancel}
            isEdit={true}
          />
        </div>
      )}
    </div>
  );
};

export default UsersPage;
