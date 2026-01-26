import React, { useState, useEffect } from 'react';
import { User, CreateUserRequest, UpdateUserRequest } from '../../api/users';
import { Role } from '../../api/roles';

interface UserFormProps {
  user?: User | null;
  roles?: Role[];
  onSubmit: (data: CreateUserRequest | UpdateUserRequest) => Promise<void>;
  onCancel: () => void;
  isEdit?: boolean;
}

const UserForm: React.FC<UserFormProps> = ({ user, roles = [], onSubmit, onCancel, isEdit = false }) => {
  const [formData, setFormData] = useState<CreateUserRequest>({
    username: '',
    email: '',
    password: '',
    role_ids: [],
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (user && isEdit) {
      setFormData({
        username: user.username,
        email: user.email,
        password: '',
        role_ids: user.roles?.map(r => r.id) || [],
      });
    }
  }, [user, isEdit]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);

    try {
      if (isEdit) {
        // Only send fields that can be updated
        const updateData: UpdateUserRequest = {
          role_ids: formData.role_ids,
        };
        await onSubmit(updateData);
      } else {
        // Validate password for new users
        if (!formData.password || formData.password.length < 8) {
          setError('Password must be at least 8 characters');
          setLoading(false);
          return;
        }
        await onSubmit(formData);
      }
    } catch (err: any) {
      setError(err.response?.data?.detail || 'Failed to save user');
      setLoading(false);
    }
  };

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const { name, value, type, checked } = e.target;
    setFormData((prev) => ({
      ...prev,
      [name]: type === 'checkbox' ? checked : value,
    }));
  };

  const handleRoleToggle = (roleId: number) => {
    setFormData((prev) => {
      const currentRoles = prev.role_ids || [];
      if (currentRoles.includes(roleId)) {
        return { ...prev, role_ids: currentRoles.filter(id => id !== roleId) };
      } else {
        return { ...prev, role_ids: [...currentRoles, roleId] };
      }
    });
  };

  return (
    <div>
      <h3 className="text-2xl font-bold mb-6">{isEdit ? 'Edit User' : 'Create New User'}</h3>
      {error && (
        <div className="bg-red-900 border border-red-700 text-white px-4 py-3 rounded mb-4" role="alert">
          {error}
        </div>
      )}
      <form onSubmit={handleSubmit} className="space-y-4">
        <div>
          <label htmlFor="username" className="block text-sm font-medium text-gray-300 mb-2">
            Username
          </label>
          <input
            type="text"
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-md text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent disabled:opacity-50 disabled:cursor-not-allowed"
            id="username"
            name="username"
            value={formData.username}
            onChange={handleChange}
            required
            disabled={isEdit}
          />
        </div>

        <div>
          <label htmlFor="email" className="block text-sm font-medium text-gray-300 mb-2">
            Email
          </label>
          <input
            type="email"
            className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-md text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent disabled:opacity-50 disabled:cursor-not-allowed"
            id="email"
            name="email"
            value={formData.email}
            onChange={handleChange}
            required
            disabled={isEdit}
          />
        </div>

        {!isEdit && (
          <div>
            <label htmlFor="password" className="block text-sm font-medium text-gray-300 mb-2">
              Password
            </label>
            <input
              type="password"
              className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-md text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              id="password"
              name="password"
              value={formData.password}
              onChange={handleChange}
              required
              minLength={8}
            />
            <p className="mt-1 text-xs text-gray-400">
              Minimum 8 characters
            </p>
          </div>
        )}

        {/* Roles selection */}
        {roles.length > 0 && (
          <div>
            <label className="block text-sm font-medium text-gray-300 mb-2">
              Roles
            </label>
            <div className="space-y-2">
              {roles.map((role) => (
                <div key={role.id} className="flex items-start">
                  <input
                    type="checkbox"
                    className="h-4 w-4 mt-1 bg-gray-700 border border-gray-600 rounded text-blue-600 focus:ring-2 focus:ring-blue-500"
                    id={`role-${role.id}`}
                    checked={formData.role_ids?.includes(role.id) || false}
                    onChange={() => handleRoleToggle(role.id)}
                  />
                  <label className="ml-2 text-sm text-gray-300" htmlFor={`role-${role.id}`}>
                    <span className="font-medium">{role.name}</span>
                    {role.description && (
                      <span className="text-gray-500 ml-2">- {role.description}</span>
                    )}
                    {role.app_names && role.app_names.length > 0 && (
                      <div className="text-xs text-gray-500 mt-1">
                        Apps: {role.app_names.join(', ')}
                      </div>
                    )}
                  </label>
                </div>
              ))}
            </div>
          </div>
        )}

        <div className="flex space-x-3 pt-4">
          <button
            type="submit"
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-md font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            disabled={loading}
          >
            {loading ? 'Saving...' : isEdit ? 'Update User' : 'Create User'}
          </button>
          <button
            type="button"
            className="px-4 py-2 bg-gray-600 hover:bg-gray-700 text-white rounded-md font-medium transition-colors"
            onClick={onCancel}
          >
            Cancel
          </button>
        </div>
      </form>
    </div>
  );
};

export default UserForm;
