import { createContext, useContext, useState, useEffect, useCallback, type ReactNode } from 'react'
import { api, type User } from './api'

interface AuthContextValue {
  user: User | null
  loading: boolean
  login: (email: string, password: string) => Promise<void>
  register: (email: string, password: string, displayName?: string) => Promise<void>
  logout: () => Promise<void>
  /** Replace the cached user (e.g. after a profile update). */
  setUser: (user: User) => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    api.auth.me()
      .then((res) => setUser(res.user))
      .catch(() => setUser(null))
      .finally(() => setLoading(false))
  }, [])

  const login = useCallback(async (email: string, password: string) => {
    const res = await api.auth.login({ email, password })
    setUser(res.user)
  }, [])

  const register = useCallback(async (email: string, password: string, displayName?: string) => {
    const res = await api.auth.register({ email, password, display_name: displayName })
    setUser(res.user)
  }, [])

  const logout = useCallback(async () => {
    await api.auth.logout()
    setUser(null)
  }, [])

  const handleSetUser = useCallback((u: User) => {
    setUser(u)
  }, [])

  return (
    <AuthContext.Provider value={{ user, loading, login, register, logout, setUser: handleSetUser }}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be used within AuthProvider')
  return ctx
}
