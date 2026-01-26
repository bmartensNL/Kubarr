import React, { useState, useEffect } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useAuth } from '../contexts/AuthContext';
import UserList from '../components/users/UserList';
import UserForm from '../components/users/UserForm';
import InviteLinkModal from '../components/users/InviteLinkModal';
import {
  User,
  Invite,
  getUsers,
  getPendingUsers,
  createUser,
  updateUser,
  deleteUser,
  approveUser,
  rejectUser,
  getInvites,
  createInvite,
  deleteInvite,
  CreateUserRequest,
  UpdateUserRequest,
} from '../api/users';
import {
  Role,
  getRoles,
  // createRole,  // Commented out - role management UI not yet implemented
  // deleteRole,
} from '../api/roles';
import { getCatalog, App } from '../api/apps';
import { getSettings, updateSetting, Setting } from '../api/settings';
import { Users, Link, UserPlus, Settings, Sliders, Lock, Menu, X } from 'lucide-react';
import PermissionMatrix from '../components/permissions/PermissionMatrix';

type ViewMode = 'list' | 'create' | 'edit';
type SettingsSection = 'general' | 'users' | 'pending' | 'invites' | 'permissions';

const SettingsPage: React.FC = () => {
  const { isAdmin } = useAuth();
  const [searchParams, setSearchParams] = useSearchParams();
  const [users, setUsers] = useState<User[]>([]);
  const [pendingUsers, setPendingUsers] = useState<User[]>([]);
  const [invites, setInvites] = useState<Invite[]>([]);
  const [roles, setRoles] = useState<Role[]>([]);
  const [_apps, setApps] = useState<App[]>([]);
  const [systemSettings, setSystemSettings] = useState<Record<string, Setting>>({});
  const [loading, setLoading] = useState(true);
  const [savingSettings, setSavingSettings] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Get state from URL params
  const activeSection = (searchParams.get('section') as SettingsSection) || 'general';
  const viewMode = (searchParams.get('view') as ViewMode) || 'list';
  const selectedUserId = searchParams.get('user');

  // Find selected user from users list
  const selectedUser = selectedUserId ? users.find(u => u.id === parseInt(selectedUserId)) || null : null;

  // URL state setters
  const setActiveSection = (section: SettingsSection) => {
    // Don't include view when it's 'list' (default)
    setSearchParams({ section });
    setMobileSidebarOpen(false); // Close mobile sidebar when selecting
  };

  const setViewMode = (mode: ViewMode) => {
    if (mode === 'list') {
      // Remove view and user params when going back to list
      setSearchParams({ section: activeSection });
    } else {
      // Set view mode (create or edit)
      setSearchParams({ section: activeSection, view: mode });
    }
  };

  const setSelectedUser = (user: User | null) => {
    if (user) {
      setSearchParams({ section: activeSection, view: 'edit', user: user.id.toString() });
    } else {
      setSearchParams({ section: activeSection, view: 'list' });
    }
  };
  const [creatingInvite, setCreatingInvite] = useState(false);
  const [copiedInviteId, setCopiedInviteId] = useState<number | null>(null);
  const [newInvite, setNewInvite] = useState<Invite | null>(null);
  // Role editing state - commented out until UI is implemented
  // const [newRoleName, setNewRoleName] = useState('');
  // const [newRoleDescription, setNewRoleDescription] = useState('');
  // const [creatingRole, setCreatingRole] = useState(false);
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false);

  useEffect(() => {
    if (isAdmin) {
      loadUsers();
      loadPendingUsers();
      loadInvites();
      loadRoles();
      loadApps();
      loadSettings();
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

  const loadInvites = async () => {
    try {
      const data = await getInvites();
      setInvites(data);
    } catch (err: any) {
      console.error('Failed to load invites:', err);
    }
  };

  const loadRoles = async () => {
    try {
      const data = await getRoles();
      setRoles(data);
    } catch (err: any) {
      console.error('Failed to load roles:', err);
    }
  };

  const loadApps = async () => {
    try {
      const data = await getCatalog();
      setApps(data);
    } catch (err: any) {
      console.error('Failed to load apps:', err);
    }
  };

  const loadSettings = async () => {
    try {
      const data = await getSettings();
      setSystemSettings(data);
    } catch (err: any) {
      console.error('Failed to load settings:', err);
    }
  };

  const handleToggleSetting = async (key: string) => {
    try {
      setSavingSettings(true);
      const currentValue = systemSettings[key]?.value === 'true';
      const newValue = (!currentValue).toString();
      await updateSetting(key, newValue);
      await loadSettings();
    } catch (err: any) {
      setError(err.response?.data?.detail || `Failed to update setting ${key}`);
    } finally {
      setSavingSettings(false);
    }
  };

  const handleCreateUser = async (data: CreateUserRequest | UpdateUserRequest) => {
    await createUser(data as CreateUserRequest);
    await loadUsers();
    setSearchParams({ section: activeSection });
  };

  const handleUpdateUser = async (data: CreateUserRequest | UpdateUserRequest) => {
    if (selectedUser) {
      await updateUser(selectedUser.id, data as UpdateUserRequest);
      await loadUsers();
      setSearchParams({ section: activeSection });
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
    // setSelectedUser already sets view to 'edit'
    setSelectedUser(user);
  };

  const handleCancel = () => {
    // Go back to list view, clearing user selection
    setSearchParams({ section: activeSection });
  };

  const handleCreateInvite = async () => {
    try {
      setCreatingInvite(true);
      const invite = await createInvite({ expires_in_days: 7 });
      setNewInvite(invite);
      await loadInvites();
    } catch (err: any) {
      setError(err.response?.data?.detail || 'Failed to create invite');
    } finally {
      setCreatingInvite(false);
    }
  };

  const handleCloseInviteModal = () => {
    setNewInvite(null);
  };

  const handleDeleteInvite = async (invite: Invite) => {
    if (window.confirm('Are you sure you want to delete this invite?')) {
      try {
        await deleteInvite(invite.id);
        await loadInvites();
      } catch (err: any) {
        setError(err.response?.data?.detail || 'Failed to delete invite');
      }
    }
  };

  const copyInviteLink = (invite: Invite) => {
    const baseUrl = window.location.origin;
    const inviteUrl = `${baseUrl}/auth/register?invite=${invite.code}`;
    navigator.clipboard.writeText(inviteUrl);
    setCopiedInviteId(invite.id);
    setTimeout(() => setCopiedInviteId(null), 2000);
  };

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const isExpired = (expiresAt: string | null) => {
    if (!expiresAt) return false;
    return new Date(expiresAt) < new Date();
  };

  // Role management functions - commented out until UI is added
  // const handleCreateRole = async () => {
  //   if (!newRoleName.trim()) {
  //     setError('Role name is required');
  //     return;
  //   }
  //   try {
  //     setCreatingRole(true);
  //     await createRole({
  //       name: newRoleName,
  //       description: newRoleDescription || undefined,
  //     });
  //     setNewRoleName('');
  //     setNewRoleDescription('');
  //     await loadRoles();
  //   } catch (err: any) {
  //     setError(err.response?.data?.detail || 'Failed to create role');
  //   } finally {
  //     setCreatingRole(false);
  //   }
  // };

  // const handleDeleteRole = async (role: Role) => {
  //   if (role.is_system) {
  //     setError('Cannot delete system roles');
  //     return;
  //   }
  //   if (window.confirm(`Are you sure you want to delete role "${role.name}"?`)) {
  //     try {
  //       await deleteRole(role.id);
  //       await loadRoles();
  //     } catch (err: any) {
  //       setError(err.response?.data?.detail || 'Failed to delete role');
  //     }
  //   }
  // };

  if (!isAdmin) {
    return (
      <div className="max-w-7xl mx-auto px-4 py-8">
        <div className="bg-red-100 dark:bg-red-900 border border-red-300 dark:border-red-700 text-red-800 dark:text-white px-4 py-3 rounded">
          You do not have permission to access this page. Admin privileges required.
        </div>
      </div>
    );
  }

  const accessManagementItems = [
    { id: 'general' as SettingsSection, label: 'General', icon: Sliders },
    { id: 'users' as SettingsSection, label: 'All Users', icon: Users, count: users.length },
    { id: 'pending' as SettingsSection, label: 'Pending Approval', icon: UserPlus, count: pendingUsers.length },
    { id: 'invites' as SettingsSection, label: 'Invite Links', icon: Link, count: invites.filter(i => !i.is_used && !isExpired(i.expires_at)).length },
    { id: 'permissions' as SettingsSection, label: 'Permissions', icon: Lock },
  ];

  return (
    <div className="flex h-full w-full relative">
      {/* Mobile Sidebar Toggle Button */}
      <button
        onClick={() => setMobileSidebarOpen(!mobileSidebarOpen)}
        className="md:hidden fixed bottom-4 right-4 z-50 flex items-center justify-center w-14 h-14 bg-blue-600 hover:bg-blue-700 text-white rounded-full shadow-lg transition-colors"
        aria-label="Toggle settings menu"
      >
        {mobileSidebarOpen ? <X size={24} /> : <Menu size={24} />}
      </button>

      {/* Mobile Sidebar Overlay */}
      {mobileSidebarOpen && (
        <div
          className="md:hidden fixed inset-0 bg-black/50 z-40"
          onClick={() => setMobileSidebarOpen(false)}
        />
      )}

      {/* Left Sidebar */}
      <div className={`
        ${mobileSidebarOpen ? 'translate-x-0' : '-translate-x-full'}
        md:translate-x-0
        fixed md:relative
        inset-y-0 left-0
        z-40 md:z-auto
        w-64 bg-white dark:bg-gray-800 border-r border-gray-200 dark:border-gray-700 flex-shrink-0 flex flex-col
        transition-transform duration-200 ease-in-out
      `}>
        <div className="p-4 border-b border-gray-200 dark:border-gray-700 flex-shrink-0">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <Settings size={20} className="text-gray-500 dark:text-gray-400" />
              <h2 className="text-lg font-semibold text-gray-900 dark:text-white">Settings</h2>
            </div>
            <button
              onClick={() => setMobileSidebarOpen(false)}
              className="md:hidden p-1 rounded-md text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-700"
            >
              <X size={20} />
            </button>
          </div>
        </div>
        <nav className="p-2 flex-1 overflow-auto">
          {/* Access Management Section */}
          <div className="mb-2">
            <div className="px-3 py-2 text-xs font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider">
              Access Management
            </div>
            {accessManagementItems.map((item) => {
              const Icon = item.icon;
              return (
                <button
                  key={item.id}
                  onClick={() => setActiveSection(item.id)}
                  className={`w-full flex items-center justify-between px-3 py-3 md:py-2 rounded-md mb-1 transition-colors ${
                    activeSection === item.id
                      ? 'bg-blue-600 text-white'
                      : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-gray-900 dark:hover:text-white'
                  }`}
                >
                  <div className="flex items-center space-x-3">
                    <Icon size={18} />
                    <span>{item.label}</span>
                  </div>
                  {item.count !== undefined && item.count > 0 && (
                    <span className={`px-2 py-0.5 text-xs rounded-full ${
                      activeSection === item.id
                        ? 'bg-blue-500 text-white'
                        : 'bg-gray-200 dark:bg-gray-700 text-gray-600 dark:text-gray-300'
                    }`}>
                      {item.count}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </nav>
      </div>

      {/* Main Content */}
      <div className="flex-1 overflow-auto p-4 md:p-6 min-w-0">
        {/* Mobile Section Header */}
        <div className="md:hidden mb-4 pb-4 border-b border-gray-200 dark:border-gray-700">
          <button
            onClick={() => setMobileSidebarOpen(true)}
            className="flex items-center gap-2 text-gray-600 dark:text-gray-300 hover:text-gray-900 dark:hover:text-white"
          >
            <Menu size={20} />
            <span className="text-sm">Settings Menu</span>
          </button>
        </div>

        {error && (
          <div className="bg-red-100 dark:bg-red-900 border border-red-300 dark:border-red-700 text-red-800 dark:text-white px-4 py-3 rounded mb-4 flex justify-between items-center">
            <span>{error}</span>
            <button
              onClick={() => setError(null)}
              className="text-red-600 dark:text-white hover:text-red-800 dark:hover:text-gray-300"
            >
              <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clipRule="evenodd" />
              </svg>
            </button>
          </div>
        )}

        {loading ? (
          <div className="flex justify-center items-center py-12">
            <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
          </div>
        ) : (
          <>
            {/* General Section */}
            {activeSection === 'general' && (
              <div className="space-y-6">
                <div>
                  <h3 className="text-2xl font-bold text-gray-900 dark:text-white">General Settings</h3>
                  <p className="text-gray-500 dark:text-gray-400 text-sm mt-1">
                    Configure general access management settings.
                  </p>
                </div>

                <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6 space-y-6">
                  {/* Registration Settings */}
                  <div>
                    <h4 className="text-lg font-medium text-gray-900 dark:text-white mb-4">User Registration</h4>

                    {/* Registration Enabled Toggle */}
                    <div className="flex items-center justify-between py-4 border-b border-gray-200 dark:border-gray-700">
                      <div className="flex-1">
                        <div className="font-medium text-gray-900 dark:text-white">Allow Open Registration</div>
                        <div className="text-sm text-gray-500 dark:text-gray-400 mt-1">
                          When disabled, users can only register using invite links.
                        </div>
                      </div>
                      <button
                        onClick={() => handleToggleSetting('registration_enabled')}
                        disabled={savingSettings}
                        className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-white dark:focus:ring-offset-gray-800 ${
                          systemSettings.registration_enabled?.value === 'true'
                            ? 'bg-blue-600'
                            : 'bg-gray-300 dark:bg-gray-600'
                        } ${savingSettings ? 'opacity-50 cursor-not-allowed' : ''}`}
                      >
                        <span
                          className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                            systemSettings.registration_enabled?.value === 'true'
                              ? 'translate-x-6'
                              : 'translate-x-1'
                          }`}
                        />
                      </button>
                    </div>

                    {/* Require Approval Toggle */}
                    <div className="flex items-center justify-between py-4">
                      <div className="flex-1">
                        <div className="font-medium text-gray-900 dark:text-white">Require Admin Approval</div>
                        <div className="text-sm text-gray-500 dark:text-gray-400 mt-1">
                          New registrations require admin approval before users can log in. Users with invite links are auto-approved.
                        </div>
                      </div>
                      <button
                        onClick={() => handleToggleSetting('registration_require_approval')}
                        disabled={savingSettings}
                        className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-white dark:focus:ring-offset-gray-800 ${
                          systemSettings.registration_require_approval?.value === 'true'
                            ? 'bg-blue-600'
                            : 'bg-gray-300 dark:bg-gray-600'
                        } ${savingSettings ? 'opacity-50 cursor-not-allowed' : ''}`}
                      >
                        <span
                          className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                            systemSettings.registration_require_approval?.value === 'true'
                              ? 'translate-x-6'
                              : 'translate-x-1'
                          }`}
                        />
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* Users Section */}
            {activeSection === 'users' && viewMode === 'list' && (
              <div className="space-y-4">
                <div className="flex justify-between items-center">
                  <h3 className="text-2xl font-bold text-gray-900 dark:text-white">All Users</h3>
                  <button
                    className="px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-md font-medium transition-colors"
                    onClick={() => setViewMode('create')}
                  >
                    Create New User
                  </button>
                </div>
                <UserList
                  users={users}
                  onEdit={handleEditUser}
                  onDelete={handleDeleteUser}
                  onApprove={handleApproveUser}
                  showActions={true}
                />
              </div>
            )}

            {/* Pending Users Section */}
            {activeSection === 'pending' && viewMode === 'list' && (
              <div className="space-y-4">
                <h3 className="text-2xl font-bold text-gray-900 dark:text-white">Pending Approval</h3>
                <p className="text-gray-500 dark:text-gray-400">Users waiting for approval to access the system.</p>
                <UserList
                  users={pendingUsers}
                  onApprove={handleApproveUser}
                  onReject={handleRejectUser}
                  onDelete={handleDeleteUser}
                  showActions={true}
                />
              </div>
            )}

            {/* Invites Section */}
            {activeSection === 'invites' && viewMode === 'list' && (
              <div className="space-y-4">
                <div className="flex justify-between items-center">
                  <div>
                    <h3 className="text-2xl font-bold text-gray-900 dark:text-white">Invite Links</h3>
                    <p className="text-gray-500 dark:text-gray-400 text-sm mt-1">
                      Create invite links to share with users. Each link can only be used once.
                    </p>
                  </div>
                  <button
                    onClick={handleCreateInvite}
                    disabled={creatingInvite}
                    className="px-4 py-2 bg-green-600 hover:bg-green-700 disabled:bg-gray-600 disabled:cursor-not-allowed rounded-md font-medium transition-colors"
                  >
                    {creatingInvite ? 'Creating...' : 'Create Invite'}
                  </button>
                </div>

                {invites.length === 0 ? (
                  <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-8 text-center">
                    <p className="text-gray-500 dark:text-gray-400">No invites created yet. Create one to get started.</p>
                  </div>
                ) : (
                  <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 overflow-x-auto">
                    <table className="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                      <thead className="bg-gray-50 dark:bg-gray-700">
                        <tr>
                          <th className="px-4 md:px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">Status</th>
                          <th className="px-4 md:px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">Created By</th>
                          <th className="px-4 md:px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider hidden sm:table-cell">Created At</th>
                          <th className="px-4 md:px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider hidden sm:table-cell">Expires At</th>
                          <th className="px-4 md:px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider hidden md:table-cell">Used By</th>
                          <th className="px-4 md:px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">Actions</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-gray-200 dark:divide-gray-700">
                        {invites.map((invite) => (
                          <tr key={invite.id} className={invite.is_used || isExpired(invite.expires_at) ? 'opacity-50' : ''}>
                            <td className="px-4 md:px-6 py-4 whitespace-nowrap">
                              {invite.is_used ? (
                                <span className="px-2 py-1 text-xs bg-gray-200 dark:bg-gray-600 text-gray-600 dark:text-gray-300 rounded">Used</span>
                              ) : isExpired(invite.expires_at) ? (
                                <span className="px-2 py-1 text-xs bg-red-600 text-white rounded">Expired</span>
                              ) : (
                                <span className="px-2 py-1 text-xs bg-green-600 text-white rounded">Active</span>
                              )}
                            </td>
                            <td className="px-4 md:px-6 py-4 whitespace-nowrap text-sm text-gray-600 dark:text-gray-300">{invite.created_by_username}</td>
                            <td className="px-4 md:px-6 py-4 whitespace-nowrap text-sm text-gray-500 dark:text-gray-400 hidden sm:table-cell">{formatDate(invite.created_at)}</td>
                            <td className="px-4 md:px-6 py-4 whitespace-nowrap text-sm text-gray-500 dark:text-gray-400 hidden sm:table-cell">{invite.expires_at ? formatDate(invite.expires_at) : 'Never'}</td>
                            <td className="px-4 md:px-6 py-4 whitespace-nowrap text-sm text-gray-600 dark:text-gray-300 hidden md:table-cell">{invite.used_by_username || '-'}</td>
                            <td className="px-4 md:px-6 py-4 whitespace-nowrap text-right text-sm space-x-2">
                              {!invite.is_used && !isExpired(invite.expires_at) && (
                                <button
                                  onClick={() => copyInviteLink(invite)}
                                  className="px-3 py-1 bg-blue-600 hover:bg-blue-700 rounded text-white transition-colors"
                                >
                                  {copiedInviteId === invite.id ? 'Copied!' : 'Copy Link'}
                                </button>
                              )}
                              <button
                                onClick={() => handleDeleteInvite(invite)}
                                className="px-3 py-1 bg-red-600 hover:bg-red-700 rounded text-white transition-colors"
                              >
                                Delete
                              </button>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            )}

            {/* Permissions Section */}
            {activeSection === 'permissions' && (
              <div className="space-y-6">
                <div>
                  <h3 className="text-2xl font-bold text-gray-900 dark:text-white">Permissions</h3>
                  <p className="text-gray-500 dark:text-gray-400 text-sm mt-1">
                    Configure fine-grained access control for each role, including which applications users can access.
                  </p>
                </div>
                <PermissionMatrix />
              </div>
            )}

            {/* Create/Edit User Forms */}
            {viewMode === 'create' && (
              <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
                <UserForm
                  roles={roles}
                  onSubmit={handleCreateUser}
                  onCancel={handleCancel}
                  isEdit={false}
                />
              </div>
            )}

            {viewMode === 'edit' && selectedUser && (
              <div className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
                <UserForm
                  user={selectedUser}
                  roles={roles}
                  onSubmit={handleUpdateUser}
                  onCancel={handleCancel}
                  isEdit={true}
                />
              </div>
            )}
          </>
        )}

        {newInvite && (
          <InviteLinkModal invite={newInvite} onClose={handleCloseInviteModal} />
        )}
      </div>
    </div>
  );
};

export default SettingsPage;
