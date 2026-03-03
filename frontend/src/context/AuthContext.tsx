import { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import type {
  User,
  UserSettings,
  SetupStatusResponse,
  DismissedWarranties,
  LoginTotpRequiredResponse,
} from '@/types';
import { authApi } from '@/services/api';

// Storage keys - similar to Humidor
const TOKEN_KEY = 'home_registry_token';
const USER_KEY = 'home_registry_user';

interface AuthContextType {
  user: User | null;
  token: string | null;
  settings: UserSettings | null;
  isLoading: boolean;
  isAuthenticated: boolean;
  needsSetup: boolean | null;
  totpRequired: LoginTotpRequiredResponse | null;
  login: (
    username: string,
    password: string
  ) => Promise<{ success: boolean; error?: string; totpRequired?: boolean }>;
  completeTotpLogin: (
    partialToken: string,
    code: string
  ) => Promise<{ success: boolean; error?: string }>;
  clearTotpRequired: () => void;
  logout: () => void;
  checkSetupStatus: () => Promise<SetupStatusResponse | null>;
  refreshUser: () => Promise<void>;
  refreshSettings: () => Promise<void>;
  updateSettings: (settings: Partial<UserSettings>) => Promise<boolean>;
  getDismissedWarranties: () => DismissedWarranties;
  dismissNotification: (itemId: number, warrantyExpiry: string) => Promise<boolean>;
  clearAllDismissals: () => Promise<boolean>;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [token, setToken] = useState<string | null>(null);
  const [settings, setSettings] = useState<UserSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [needsSetup, setNeedsSetup] = useState<boolean | null>(null);
  const [totpRequired, setTotpRequired] = useState<LoginTotpRequiredResponse | null>(null);

  // Check for existing auth on mount
  useEffect(() => {
    const initAuth = async () => {
      const storedToken = localStorage.getItem(TOKEN_KEY);
      const storedUser = localStorage.getItem(USER_KEY);

      if (storedToken && storedUser) {
        try {
          const parsedUser = JSON.parse(storedUser) as User;
          setToken(storedToken);
          setUser(parsedUser);

          // Verify token is still valid by fetching profile
          const profileResult = await authApi.getProfile(storedToken);
          if (profileResult.success && profileResult.data) {
            setUser(profileResult.data);
            localStorage.setItem(USER_KEY, JSON.stringify(profileResult.data));

            // Fetch user settings
            const settingsResult = await authApi.getSettings(storedToken);
            if (settingsResult.success && settingsResult.data) {
              setSettings(settingsResult.data);
            }
          } else {
            // Token invalid, clear auth
            logout();
          }
        } catch (error) {
          console.error('Error restoring auth:', error);
          logout();
        }
      } else {
        // Check if setup is needed
        await checkSetupStatus();
      }
      setIsLoading(false);
    };

    void initAuth();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const checkSetupStatus = useCallback(async (): Promise<SetupStatusResponse | null> => {
    try {
      const result = await authApi.checkSetupStatus();
      if (result.success && result.data) {
        setNeedsSetup(result.data.needs_setup);
        return result.data;
      }
    } catch (error) {
      console.error('Error checking setup status:', error);
    }
    return null;
  }, []);

  const login = useCallback(
    async (
      username: string,
      password: string
    ): Promise<{ success: boolean; error?: string; totpRequired?: boolean }> => {
      try {
        const result = await authApi.login({ username, password });

        if (result.success && result.data) {
          // Check if TOTP verification is required
          // The backend returns either LoginResponse or LoginTotpRequiredResponse
          // TypeScript narrows via the 'requires_totp' discriminant property
          if ('requires_totp' in result.data) {
            // TOTP required - store the response for the login page to use
            setTotpRequired(result.data);
            return { success: true, totpRequired: true };
          }

          const { token: newToken, user: newUser } = result.data;

          // Store auth data
          localStorage.setItem(TOKEN_KEY, newToken);
          localStorage.setItem(USER_KEY, JSON.stringify(newUser));

          setToken(newToken);
          setUser(newUser);
          setNeedsSetup(false);
          setTotpRequired(null);

          // Fetch user settings
          const settingsResult = await authApi.getSettings(newToken);
          if (settingsResult.success && settingsResult.data) {
            setSettings(settingsResult.data);
          }

          return { success: true };
        } else {
          return { success: false, error: result.error ?? 'Login failed' };
        }
      } catch (error) {
        console.error('Login error:', error);
        return { success: false, error: 'Network error. Please try again.' };
      }
    },
    []
  );

  const completeTotpLogin = useCallback(
    async (partialToken: string, code: string): Promise<{ success: boolean; error?: string }> => {
      try {
        const result = await authApi.verifyTotp(partialToken, code);

        if (result.success && result.data) {
          const { token: newToken, user: newUser } = result.data;

          // Store auth data
          localStorage.setItem(TOKEN_KEY, newToken);
          localStorage.setItem(USER_KEY, JSON.stringify(newUser));

          setToken(newToken);
          setUser(newUser);
          setNeedsSetup(false);
          setTotpRequired(null);

          // Fetch user settings
          const settingsResult = await authApi.getSettings(newToken);
          if (settingsResult.success && settingsResult.data) {
            setSettings(settingsResult.data);
          }

          return { success: true };
        } else {
          return { success: false, error: result.error ?? 'Invalid TOTP code' };
        }
      } catch (error) {
        console.error('TOTP verification error:', error);
        return { success: false, error: 'Network error. Please try again.' };
      }
    },
    []
  );

  const clearTotpRequired = useCallback(() => {
    setTotpRequired(null);
  }, []);

  const logout = useCallback(() => {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(USER_KEY);
    sessionStorage.removeItem('home_registry_instructions_dismissed');
    // Clear auto-navigation flag so default inventory navigation works on next login
    sessionStorage.removeItem('home_registry_auto_navigated');
    setToken(null);
    setUser(null);
    setSettings(null);
  }, []);

  const refreshUser = useCallback(async () => {
    if (!token) {
      return;
    }

    try {
      const result = await authApi.getProfile(token);
      if (result.success && result.data) {
        setUser(result.data);
        localStorage.setItem(USER_KEY, JSON.stringify(result.data));
      }
    } catch (error) {
      console.error('Error refreshing user:', error);
    }
  }, [token]);

  const refreshSettings = useCallback(async () => {
    if (!token) {
      return;
    }

    try {
      const result = await authApi.getSettings(token);
      if (result.success && result.data) {
        setSettings(result.data);
      }
    } catch (error) {
      console.error('Error refreshing settings:', error);
    }
  }, [token]);

  const updateSettings = useCallback(
    async (newSettings: Partial<UserSettings>): Promise<boolean> => {
      if (!token) {
        return false;
      }

      try {
        const result = await authApi.updateSettings(token, newSettings);
        if (result.success && result.data) {
          setSettings(result.data);
          return true;
        }
      } catch (error) {
        console.error('Error updating settings:', error);
      }
      return false;
    },
    [token]
  );

  // Enhancement 3: Dismissal functions
  const getDismissedWarranties = useCallback((): DismissedWarranties => {
    const dismissed = settings?.settings_json.dismissedWarranties;
    return (dismissed as DismissedWarranties | undefined) ?? {};
  }, [settings]);

  const dismissNotification = useCallback(
    async (itemId: number, warrantyExpiry: string): Promise<boolean> => {
      if (!token) {
        return false;
      }

      try {
        const dismissed = getDismissedWarranties();
        dismissed[String(itemId)] = {
          dismissedAt: new Date().toISOString(),
          warrantyExpiry,
        };

        const result = await authApi.updateSettings(token, {
          settings_json: {
            ...settings?.settings_json,
            dismissedWarranties: dismissed,
          },
        });

        if (result.success && result.data) {
          setSettings(result.data);
          return true;
        }
      } catch (error) {
        console.error('Error dismissing notification:', error);
      }
      return false;
    },
    [token, settings, getDismissedWarranties]
  );

  const clearAllDismissals = useCallback(async (): Promise<boolean> => {
    if (!token) {
      return false;
    }

    try {
      const result = await authApi.updateSettings(token, {
        settings_json: {
          ...settings?.settings_json,
          dismissedWarranties: {},
        },
      });

      if (result.success && result.data) {
        setSettings(result.data);
        return true;
      }
    } catch (error) {
      console.error('Error clearing dismissals:', error);
    }
    return false;
  }, [token, settings]);

  return (
    <AuthContext.Provider
      value={{
        user,
        token,
        settings,
        isLoading,
        isAuthenticated: !!token && !!user,
        needsSetup,
        totpRequired,
        login,
        completeTotpLogin,
        clearTotpRequired,
        logout,
        checkSetupStatus,
        refreshUser,
        refreshSettings,
        updateSettings,
        getDismissedWarranties,
        dismissNotification,
        clearAllDismissals,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function useAuth() {
  const context = useContext(AuthContext);
  if (context === undefined) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}

// Helper function to get the current token for API calls
// eslint-disable-next-line react-refresh/only-export-components
export function getAuthToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

// Helper function to get auth headers for fetch calls
// eslint-disable-next-line react-refresh/only-export-components
export function getAuthHeaders(): Record<string, string> {
  const token = getAuthToken();
  if (token) {
    return {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
    };
  }
  return {
    'Content-Type': 'application/json',
  };
}
